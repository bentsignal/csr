# Private content packs

`.csrpack` version 1 is a bounded ZIP of data. It supports named BSP maps and
model, material and sound resources through stable `ContentId`s. Recipe packs
can name maps in the player's own CS:GO Legacy install. Importers for other
games, weapon overrides and sandboxed mechanics are subsequent Track E work.
No pack code runs, and no pack entry is extracted onto the player's filesystem.

Private asset packs belong outside this repository, public releases and CI.
The repository ignores `.csrpack` files and runtime `packs/` storage.

## Current developer workflow

`csr check-pack <file>` validates without installing. `csr install-pack <file>`
checks the pack and publishes its validated bytes under
`<player data>/packs/<id>.csrpack`. The launcher selects the player data folder
before these commands run; installed copies use
`%LocalAppData%/CSGO-Rewrite/userdata`, portable copies use their adjacent
`userdata/`, and source builds use the checkout. `CSR_DATA_DIR` overrides it.
Relative input filenames are resolved before the launcher changes directory.

`csr packs` lists installed definitions and reports invalid packs.
`csr remove-pack <id>` also removes corrupt packs with a safe filename.
`csr probe mod_map_arena`, `csr sim mod_map_arena` and
`csr play mod_map_arena --direct` load a map by its declared ContentId. Only
the selected pack is mounted; its assets have priority over the read-only
Legacy install, and the BSP's embedded pakfile retains highest priority.
The BSP filename is independent of the id and needs no `de_`/`cs_` prefix.
Asset-map NAV and overview metadata come only from the selected pack or its
BSP's embedded pakfile. Missing metadata cannot silently use another pack or
the newer installed map with the same basename. Recipe aliases read Legacy.

Updates atomically replace the installed file without deleting the old copy
first. Validation errors, conflicts and failed publication preserve that copy.
An already loaded level keeps its immutable content until the next load.
Install/remove operations use a file lock; simultaneous installation returns
a retryable error. Links and Windows reparse points in pack storage are refused.

These are developer commands. Window drop handling, double-click association,
visible progress/errors, cancellation and server delivery are not implemented
yet; the shared `Installed::install` seam is ready for those flows. Do not ask
friends to use the developer commands as the final setup experience.

## Format

One UTF-8 `pack.toml`, plus optional asset files at game-relative paths:

```toml
format_version = 1
id = "arena_pack"
name = "Private arena"
version = "1.0.0"
origin = "mod"
kind = "assets" # or "recipe"
min_game_version = "0.3.0"
provides = ["mod_map_arena"]
content_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"

[[maps]]
id = "mod_map_arena"
bsp = "arena_file" # maps/arena_file.bsp
name = "Arena"
source = "pack" # recipe maps use "legacy" and carry no asset bytes
```

The authoring helper fills the payload hash:
`cargo run -p packs --example packtool -- build <private folder> <new output.csrpack>`.
It refuses to overwrite an existing output. Put inputs and output outside Git.

`cargo run -p level --example packcheck -- <file.csrpack> <map ContentId>
<private data folder> [Legacy game folder]` verifies installation, identity,
BSP/collision/catalog/NAV loading without building the renderer. This is also
a useful asset-pack proof with an empty VFS when the pack supplies its own
dependencies. It never writes to the Legacy install.

Optional `[[resources]]` entries have `id` and `path`, for example
`id = "mod_material_arena_wall"`, `path = "materials/arena/wall.vmt"`.
Supported kinds are `model` (`.mdl`), `material` (`.vmt`) and `sound`
(`.wav`, `.mp3`, `.ogg`). Every provided id must have exactly one definition;
the declared map/resource must exist. Dependency files need no separate id.
The origin is `mod` or a custom lowercase component. Version 1 refuses built-in
game namespaces (`csgo`, `t4`, `iw4`, `t5`, `iw5`, `t6`) and `mw2`, `minecraft`
and `cs2`; private definitions cannot shadow engine-owned stable IDs. Legacy
recipe aliases get new IDs such as `mod_map_dust2_recipe`. A future explicit
override/importer API must define how built-in namespaces can be used.
Unknown manifest fields, definitions and unsupported kinds are errors.

Asset files are limited to `maps/` BSP/NAV, `materials/` VMT/VTF,
`models/` MDL/VVD/VTX/PHY/ANI, `sound/` WAV/MP3/OGG and
`resource/overviews/` TXT. Scripts, SWFs, native libraries, WASM and arbitrary
files are refused. Recipe packs contain only the manifest and map definitions.
No importer instructions can access arbitrary filesystem paths.

ZIP entries use slash-separated ASCII relative paths. Traversal, drive paths,
alternate streams, device names, links, dot segments and case aliases are
refused. Only stored/deflate compression is accepted. Default limits are
512 MiB compressed, 512 MiB per entry, 1 GiB expanded, 8192 entries and
256 KiB manifest. CRC, exact expanded length and hashes are checked before
anything is mounted or installed. These checks bound the container, not a
complete audit of all inherited BSP/model/audio parsers.
Directory counts and sizes are preflighted before ZIP's eager metadata parser;
the directory budget is 8 MiB. Version 1 refuses ZIP64, split archives,
self-extracting prefixes and trailing bytes. These restrictions fit its
sub-4-GiB budgets and keep malformed headers from allocating unbounded tables.

The payload `content_hash` is SHA-256 over files other than `pack.toml`, sorted
by canonical lowercase path: path bytes, one zero byte, 8-byte little-endian
length, then file bytes. `Pack::archive_hash()` hashes the full ZIP including
the manifest. Server advertisements must use the full archive hash and size;
the payload hash alone does not bind metadata and is not a trust signature.

`Registry` maps `ContentId` to an install, Workshop item or exact pack archive.
Conflicts are errors and adding a pack is transactional. It is a foundation
for combined catalogs; the existing built-in catalogs have not all migrated.

## Next integration seams

- Installed Windows copies register `.csrpack` per user during install/update.
  Opening one validates and atomically installs it with visible progress and
  cancellation, reports errors in a dialog, then opens the game if it is not
  already running. A successful install into a running game shows a notification
  and exits this launcher. Content is stored in the existing userdata root;
  updateable app folders and the Legacy install remain read-only.
  `csr --open-pack FILE` and `csr FILE.csrpack` use the same path.
- Track C can queue dropped files through `packs::InstallJob::start(store,path)`.
  `progress()` returns the stage and bounded byte counts; `cancel()` requests
  cancellation; `poll()` returns an installed manifest/hash receipt, cancellation
  or an error. Wait for that terminal result before ending a process or starting
  the next queued job. Only an installed receipt permits success feedback/map-list
  refresh. Cancellation before publication preserves the old pack and removes
  the pending file; successful publication always reports success. Loading
  snapshots stay separate from a running match. Window drops, catalog refresh
  and selected ContentId session loading are frontend work still in progress.
- Track A can advertise exact archive hashes/sizes, stage downloads outside
  active installed packs, validate bytes, then mount an explicit session set.
  A server's files must not replace the player's global pack set automatically.
- Mechanics require a versioned sandbox API with budgets and deterministic
  tick inputs/outputs. No native module may auto-execute on server join.
