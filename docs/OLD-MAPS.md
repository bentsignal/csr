# Classic map archives

`csr maps` lists the installed maps and fourteen pinned CS:GO compatibility
snapshots published by Valve. `csr fetch-map <name|all>` prefetches archives;
`csr remove-map <name|all>` removes their local copies. Loading a snapshot
with `probe`, `sim` or `play` fetches it automatically. These commands support
development and fidelity checks. The classic map picker also offers
**Dust II (2017)**, the pinned pre-rework archive, as an experimental map.
Other archives remain hidden pending their gameplay/dependency work.

Choose Dust II (2017) in the original map picker and press GO. The existing
loading screen reports contacting Steam, download percentage, verification
and preparation before normal map loading. Escape or Cancel returns home;
acquisition stops at the next callback and retains resumable download bytes.
Only one map worker owns the archive until it exits, so immediately choosing
GO again waits for cancellation rather than racing two downloads. Outstanding
network I/O has connection and response-header deadlines, but no total-body
deadline. Cancellation does not interrupt a blocking socket read. A failed acquisition reports a friendly
retry message and keeps the main menu usable.

## Runtime data

The `workshop` crate first searches every Steam library for the item's
`*_legacy.bin`. It never reads CS2 Workshop VPKs. Otherwise it obtains the
download address through Steam's public
[GetPublishedFileDetails API](https://partner.steamgames.com/doc/webapi/ISteamRemoteStorage#GetPublishedFileDetails).
The item ID, Valve creator, app, original filename, CDN host, size and SHA-1
must match the pinned table. Downloaded bytes must also pass SHA-1, and ZIP
extraction checks CRC before publishing the BSP atomically.

Maps live in `<player data>/workshop/maps`, outside installed application
files. Interrupted downloads resume from `<player data>/workshop/download`;
a server that ignores Range restarts the download instead of appending an
entire file to partial bytes. Complete partials can finish offline.
The Legacy install is read only. No map data belongs in this repository,
public releases or CI downloads; `/workshop` is ignored for source builds.

Each archive contains one BSP whose embedded pakfile includes the map's
navigation, overview and radar under the **original** name. For example,
`de_dust2_2017.bsp` contains `maps/de_dust2.nav`. The loader resolves archived
navigation and overview from that pakfile alone. It cannot silently fall
back to the installed, newer Dust II metadata. Embedded materials and
models take priority over the Legacy install's files.

Every listed map and loaded level carries a `content::ContentId`, for
example `csgo_map_de_dust2_2017`. `Level::load_from_vfs` accepts an already
mounted file system and preserves the supplied origin and identity. Pack
maps therefore use the same BSP/collision loader without Steam detection,
Workshop downloads or an install dependency. Their BSP filename can differ
from their stable ID. A synthetic ZIP-pack test verifies this seam; the
full `.csrpack` manifest and installation workflow remain Track E work.

## Which version?

Names use Valve's archive year. Upload date and build version do **not**
establish August 2018 parity. Compare intervening update notes and reference
captures before making that claim. Train's rebuild was
[December 10, 2014](https://blog.counter-strike.net/2014/12/11049/), so the
2015 archive is after that rebuild.

| Load name | Workshop item | Archived game build |
|---|---|---|
| `de_dust2_2017` | 1164734849 | 1.36.0.6, before the October 2017 rework |
| `de_inferno_2016` | 778803540 | 1.35.5.2, before the October 2016 rework |
| `de_nuke_2016` | 626513993 | 1.35.2.0, before the February 2016 rework |
| `de_train_2015` | 538091991 | 1.35.0.6 |
| `de_vertigo_2017` | 1193870504 | 1.36.1.2, before the 2019 rework |
| `de_dust2_2019` | 1773333823 | 1.37.0.1 |
| `de_inferno_2020` | 2054468706 | 1.37.4.6 |
| `de_nuke_2019` | 1885085042 | 1.37.1.8 |
| `de_cache_2019` | 1891729882 | 1.37.1.9 |
| `de_train_2020` | 2126172171 | 1.37.5.5 |
| `de_mirage_2019` | 1935967249 | 1.37.3.2 |
| `de_overpass_2019` | 1821117172 | 1.37.0.7 |
| `de_cbble_2018` | 1542127528 | 1.36.5.6 |
| `de_canals_2018` | 1534022416 | 1.36.5.5 |

## Verification and remaining work

On 2026-10-11 all fourteen snapshots downloaded from Valve's CDN and passed
their pinned archive size and SHA-1 checks. All fourteen BSP pakfiles contain
version 16, subversion 1 navigation, overview descriptions and radar images.
The initial loader missed those NAV files because it looked up the renamed
archive path; a regression test now covers the original-name lookup and
prohibits fallback to newer installed metadata.

All fourteen release-profile simulations pass movement, bot combat and
ten-round competitive checks at 128 tick, including purchases, kills and
bomb plants. Ten asset probes pass; four report one unresolved material each:

| Archive | NAV areas | Probe failures | Sim failures |
|---|---:|---:|---:|
| `de_cache_2019` | 738 | 1 | 0 |
| `de_canals_2018` | 1086 | 0 | 0 |
| `de_cbble_2018` | 1554 | 0 | 0 |
| `de_dust2_2017` | 1413 | 1 | 0 |
| `de_dust2_2019` | 1120 | 0 | 0 |
| `de_inferno_2016` | 1471 | 0 | 0 |
| `de_inferno_2020` | 990 | 0 | 0 |
| `de_mirage_2019` | 900 | 0 | 0 |
| `de_nuke_2016` | 738 | 0 | 0 |
| `de_nuke_2019` | 1205 | 1 | 0 |
| `de_overpass_2019` | 1880 | 1 | 0 |
| `de_train_2015` | 770 | 0 | 0 |
| `de_train_2020` | 773 | 0 | 0 |
| `de_vertigo_2017` | 877 | 0 | 0 |

Unresolved dependencies:

- Cache: `models/de_cache/midfloor3.mdl` references
  `models/de_cache/midfloor3/darkmetal3`. The archive has
  `materials/de_cache/darkmetal3.vmt`, but that is a different lookup path;
  do not invent an alias without checking the original game's behavior.
- Classic Dust II: `models/props_mill/millwall_01.mdl` references
  `models/props_mill/greygrid`. Read-only inspection on 2026-10-14 confirms
  that neither model nor material is embedded in the archive; the Legacy
  model's second mesh uses that unavailable material on one three-vertex
  triangle. The archive places one instance, at `-128 -1024 69`, skin 0.
  Its main mesh has 417 vertices and resolves `building_concrete`. The
  renderer retains its existing missing-material fallback for the small
  triangle; no alias, replacement or silent omission is introduced, and
  the archived-map probe still reports the dependency.
- Nuke 2019: `tools/toolsblockbullets_cs` is absent. The renderer skips tool
  surfaces, but the dependency probe still reports the missing material.
- Overpass: `models/props_urban/park_fence_128.mdl` references
  `models/props_urban/gates001`.

The probe now checks the skins used by actual prop instances rather than
assuming skin zero, and reports missing material names and candidate paths.
Screenshots of classic Inferno, Nuke and Dust II were inspected at 1920x1080;
their geometry and archived radars load. These are smoke checks, not proof
of visual parity. Material/lighting and sky fidelity remain Track D work.
Local logs, CSV results and captures are under
`%USERPROFILE%\csr-fidelity\workshop-review`, outside the repository.

The Dust II menu pilot uses the existing Track C loading worker and original
Scaleform card/loading surface. Other archive menu integration still needs
dependency resolution and matched reference captures. The archive year does
not establish 2015 parity; the existing material and lighting gaps remain.
Content packs remain separate Track E work; this archive loader does not
complete `.csrpack`, file associations, drag-and-drop or server-driven downloads.
