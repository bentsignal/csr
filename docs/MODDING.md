# Modding: content packs

Mods and crossed-in content from other games reach players as **content
packs**: single `.csrpack` files. This is the first cut of the connection
layer from [VISION.md](VISION.md) §4 and §6 and ROADMAP Track E. Code:
`crates/packs`.

Nothing from other games lives in this repository. Packs are made and
shared by whoever owns the content; the game only loads them.

## Installing a pack

Any of these, no other steps:

- **Drag** the `.csrpack` onto the game window. A notice confirms it.
- **Double-click** it. Installed copies register the `.csrpack` file type
  for the current user (no administrator) when they install or update, and
  remove it when uninstalled (`crates/launcher/src/fileassoc.rs`).
- `csr install-pack <file>`.

The pack is checked (manifest, content hash, game version, rules below) and
copied to `<data>\packs\<id>.csrpack`, replacing any installed pack with
the same id. `<data>` is the player data folder
(`%LocalAppData%\CSGO-Rewrite\userdata` for installed copies). Packs take
effect from the next map load. `csr packs` lists them (and what they
provide); `csr remove-pack <id>` deletes one. A broken pack in the folder
is skipped with a warning, never a crash.

## What a pack is

A zip with this layout:

```text
pack.toml                  the manifest
defs/weapons/*.toml        weapon definitions
materials/... models/...   assets at their game paths (asset packs only)
sound/... maps/...
```

`pack.toml`:

```toml
id = "golden_guns"                 # [a-z0-9_]; the installed file name
name = "Golden guns"
version = "1.0.0"
origin = "mod"                     # where the content comes from: csgo, iw4, t5, mod, ...
kind = "recipe"                    # "recipe" or "assets"
min_game_version = "0.2.0"         # oldest CSGO Rewrite that can load it
provides = ["mod_weapon_ak47_gold"]
content_hash = "sha256:..."        # see below
description = "A shiny AK."        # optional
```

Unknown keys are errors, so typos do not pass silently.

- **Recipe packs** (`kind = "recipe"`) hold only `pack.toml` and `defs/`.
  They describe content and tell the game how to read it from installs the
  player already owns. They contain nothing copyrighted and are safe to
  share publicly. The game refuses a recipe pack that carries assets.
- **Asset packs** (`kind = "assets"`) also carry extracted files. They are
  private: never committed here, never attached to public releases, never
  fetched by CI.

**Content hash:** SHA-256 over every file except `pack.toml`, sorted by
lowercase path; for each file the path, a zero byte, its length as 8
little-endian bytes, then its bytes. Written `sha256:<hex>`. A pack that was
damaged or edited after it was built is refused. `packs::build` writes a
pack with the hash filled in.

**Assets** are mounted on the game's file system ahead of the CS:GO Legacy
install (behind only the current map's pakfile), so a pack can add files or
replace them, e.g. `sound/weapons/ak47/ak47_01.wav`.

### Weapons

`defs/weapons/<anything>.toml` adds weapons as variants of existing ones:

```toml
[[weapon]]
id = "mod_weapon_ak47_gold"        # must be listed in provides
base = "csgo_weapon_ak47"          # any weapon already in the catalog
name = "Golden AK-47"              # shown instead of the base's name
view_model = "models/weapons/v_rif_ak47.mdl"     # optional, as are
world_model = "models/weapons/w_rif_ak47.mdl"    # these three
fire_sound = "Weapon_AK47.Single"

[weapon.attributes]                # items_game.txt attribute names
damage = 40
"in game price" = 3500
"cycletime" = 0.09
```

The variant starts as a copy of the base weapon (class, models, sounds,
stats) and the attributes are applied on top, with the same names and
meaning as in `items_game.txt` (`damage`, `armor ratio`, `cycletime`,
`is full auto`, `primary clip size`, `max player speed`, `inaccuracy *`,
`recoil *`, `kill award`...). It gets its own item index (1,000,000 and up),
so it is bought, dropped and picked up as its own weapon, and it appears in
`csr weapons`, the buy menu and the sim catalog.

## Content ids

Every piece of content has a `content::ContentId`, written
`<origin>_<kind>_<name>`:

| Part | Values |
|---|---|
| origin | `csgo`, `t4`, `iw4`, `t5`, `iw5`, `t6`, `mod`, or any other `[a-z0-9]` prefix |
| kind | `weapon`, `map`, `model`, `material`, `sound`, `perk`, `streak`, `equipment`, `character`, `mode` |
| name | lowercase `[a-z0-9_]` |

Examples: `csgo_weapon_ak47` (from the install), `csgo_map_de_dust2_2017`
(an old Workshop map, see [OLD-MAPS.md](OLD-MAPS.md)),
`iw4_weapon_intervention` (MW2 content), `mod_weapon_ak47_gold`. The scheme
matches the COD Legacy Mashup project (see [STITCHING.md](STITCHING.md)),
so ids from several games share one catalog.

`packs::Registry` maps every known id to its source: the install, a Workshop
item, or a pack. Maps a pack provides (`<origin>_map_<name>`, with
`maps/<name>.bsp` in the pack) show in the map list whatever their prefix;
installed maps still need a `de_`/`cs_` prefix (the modes we support).

## Mechanics: the plan

Packs today carry data and assets. Mods also need to bring **mechanics**:
weapon behaviours, movement profiles, game modes, world types (Overworld
will come back this way). Rust has no stable plugin ABI, so code cannot be
loaded the way data is. The options:

| | WASM modules | Native DLLs |
|---|---|---|
| Portability | One `.wasm` for every OS | One build per OS and target |
| ABI | Defined by us (the mod API), versioned | Must match our exact compiler and crate versions; breaks every update |
| Safety | Sandboxed: no files, network or memory outside what we pass in | Full access to the PC |
| Auto-download from servers | Safe | Not safe (running a stranger's native code) |
| Speed | Near native for logic; calls across the boundary cost something | Native |

**Decision: WASM**, with native code reserved for content compiled into the
game itself. Design:

- **Runtime:** `wasmtime` (Cranelift JIT; or `wasmi` where JIT is not
  allowed). A pack gains `mods/*.wasm` and a `[mod]` table in `pack.toml`
  naming the entry points it implements and the API version it targets.
- **Versioned mod API** (`csr-mod-api`, a small crate mod authors depend
  on, compiled to `wasm32-unknown-unknown`). Shaped from `sim`'s seams, so
  mods run inside the deterministic tick and stay server-authoritative:
  - `WeaponBehaviour`: on fire / on hit / on tick for a `WeaponDefinition`
    (e.g. IW4 spread cones instead of CS:GO inaccuracy, STITCHING step 2).
  - `MovementProfile`: parameters or a step function over
    `movement::MoveConfig` and the player's state.
  - `GameMode`: round rules over `sim::RulesConfig`, spawn and win logic.
  - `DynamicWorld`: voxel or procedural collision (the seam left in
    `collision` for Overworld).
- **Data across the boundary:** plain structs serialised with a compact
  binary codec (the same one the netcode uses), passed through linear
  memory. No pointers into the host. Mods are given only the state the hook
  needs, and return intents (damage, impulses, spawns) that `sim` applies.
- **Determinism:** WASM is deterministic apart from NaN bit patterns and
  resource limits; the host canonicalises NaNs and gives every call a fuel
  budget, so client prediction and the server agree.
- **Versioning:** the API version is semver. The host supports the current
  major version and refuses packs built for another, with a clear message.

Not built yet: this section is the design to build against.

## Server-pushed packs (with Track A's networking)

Joining a server that uses packs must just work. Intended handshake (not
implemented; it lands with the netcode):

1. The server's info reply (used by the server browser too) lists the
   packs the server requires: `id`, `version`, `content_hash`, size.
2. On connect the client compares that list with its installed packs by
   `id` and `content_hash`.
3. For each missing or different pack, the client downloads it from the
   server: over the game connection in chunks, or from an HTTP URL the
   server advertises (like Source's `sv_downloadurl`). Downloads go to
   `<data>\packs\cache\<content_hash>.csrpack`, resume, and are checked
   against the advertised hash before use.
4. The client mounts exactly the server's set for that session and goes
   back to the player's own set afterwards. Packs a server pushed are kept
   in the cache for next time, not installed for every game.
5. Only recipe packs and WASM mods are pushed automatically. Asset packs
   are private by design; a server may only push them to players on its own
   list, and the client asks once before accepting an asset pack from a
   server.
