# Historical CS:GO map coverage

Research checked 2026-10-18. This records historical identity and private data
inspection, not new map admission. The eight offered archives in
[OLD-MAPS.md](../OLD-MAPS.md) remain unchanged. No game assets belong here.

## What complete coverage means

Cover the requested core maps with authentic, playable representatives of their
meaningful CS:GO generations. Give each selection an explicit archive identity;
do not call a late pre-remake snapshot an exact launch-era or 2015 version.
Compatibility uploads preserve individual versions for demos, not a curated list
of major generations. An upload year alone establishes neither a rework boundary
nor complete dependencies. This is the research criterion for the owner's
maps-only scope, not a claim that all historical versions are already supported.

The present classic set represents pre-remake Dust II, Inferno and Nuke; original
Vertigo; actual October 2015 Cache and Train; and later Mirage/Cobblestone
snapshots with explicit qualifications. Preserve the accepted selections. Early
CS:GO versions that materially differ from these representatives remain coverage
gaps, rather than being silently equated with them. Exact 2015 Mirage and
Cobblestone parity is still unestablished. Installed Legacy provides a later
CS:GO reference; CS2 is outside this content scope.

Overpass requires an early layout and an evolved CS:GO representative. The
October 2015 archive is the existing target for the latter; a January 2015
snapshot does not replace it merely because both carry the same year. Train
before December 2014 is another distinct generation, not the offered Train
(2015). Do not substitute it for the accepted selection.

## Overpass history from Valve

Valve [introduced Overpass on December 18, 2013](https://blog.counter-strike.net/2013/12/9589/).
It initially had community/offline access and restricted official matchmaking;
the [February 5, 2014 update](https://blog.counter-strike.net/2014/02/9585/)
opened it to everyone and changed cover, lighting, the B bridge and site layout.

Valve's [April 4, 2014 account](https://blog.counter-strike.net/2014/04/9284/)
describes seven updates since launch. Changes included A-site vehicles, fences
and hut access; B-site layout and team spawns; canal segmentation; wider
interiors; and new bathroom/park and T-tunnel/canal connectors. This is evidence
for a meaningful early/evolved distinction. It does not prove that every
individual change warrants another picker entry, or that an early 2014 archive
is the untouched December 2013 release.

Changes continued within 2015: [July 9 notes](https://blog.counter-strike.net/2015/07/12140/)
describe predictable first train timing, train-area sound and an A-side texture
fix. Accordingly, neither a January nor October snapshot should be described
as representing all of 2015 without qualification. These notes establish history,
not implementation of those map events in the rewrite.

## Verified archive identities and actual blockers

Valve's keyless [GetPublishedFileDetails endpoint](https://partner.steamgames.com/doc/webapi/ISteamRemoteStorage#GetPublishedFileDetails)
identified each item below as creator `76561198082857351`, creator/consumer app
730, filename `mymaps/de_overpass.bsp`, with the named compatibility version.
App 730 and the account's present CS2 display name do not turn these historical
CS:GO BSPs into CS2 content. Check the item, original filename, version, dates,
size and content hash together.

- [213863953](https://steamcommunity.com/sharedfiles/filedetails/?id=213863953):
  **1.31.6.1**, uploaded January 7, 2014. Prior acquisition received zero bytes
  instead of 14,099,123; SHA-1 pin `50af96a1e796151a50c2426a3903bdde5ed87b79`.
  Metadata is verified, but there is no verified BSP. That failed CDN fetch
  was not repeated in this research.
- [227300574](https://steamcommunity.com/sharedfiles/filedetails/?id=227300574):
  **1.32.1.0**, uploaded February 12, 2014. A new, private download verified
  14,439,685 ZIP bytes and SHA-1 `2ff06f4711693597388f7560c24adbe43c4f888f`.
  Extracted BSP: 32,193,600 bytes, SHA-256
  `c573e2d87957f70ec3519371426cc84caf86fd162220d7a3cbd4b31798ece544`.
  This supplies an acquired early reference, not launch-version proof.
- [383037414](https://steamcommunity.com/sharedfiles/filedetails/?id=383037414):
  **1.34.6.8**, uploaded January 28, 2015. A new, private download verified
  14,478,140 ZIP bytes and SHA-1 `b8a4796b0fbe45482769147c07337d5ad405af7f`.
  Extracted BSP: 33,278,920 bytes, SHA-256
  `bb22eba9ea9a84eccf6ce4c45e550c989c3054355fc3e97970150ed71d94cd54`.
- [534663598](https://steamcommunity.com/sharedfiles/filedetails/?id=534663598):
  **1.35.0.6**, uploaded October 13, 2015. Existing catalog/archive target;
  BSP SHA-256 `ba42342be13c65931d2031c664cf3f0abc020513db8200ad83b10b7b497b3732`.
  Its previously established missing fence dependency remains unresolved.

Both new downloads passed the extracted BSP member's ZIP CRC. They remain
private research files, outside the normal archive store. Read-only inspection
found embedded `maps/de_overpass.nav` (version 16/subversion 1), 1,629 areas in
the February archive and 1,773 in January, plus 20 declared spawns per team in
each. BSP plane/vertex/face/model/brush lump hashes differ between the two and
October 2015; these are distinct compiled maps. Neither spawn/NAV metadata nor
different hashes proves faithful routing, appearance or ten-player playability.

Neither new BSP embeds `models/props_urban/park_fence_128.mdl` or the required
`materials/models/props_urban/gates001.vmt`. Mounted Legacy supplies the model
(MDL49; SHA-256
`45eeea47d61608de47671c8aa512fe029fae1ae467c7c43d00c55466451ad9a7`),
whose sole texture is `gates001`, authored directory is `models/props_urban/`,
and VTX declares no material replacement. The material and root-name fallback
are missing. The February map uses **33** instances, January **22**, and the
existing October archive **19**. Earlier inspection established nonempty fence
mesh geometry. No guessed material alias or fence omission is justified.

No other referenced model file or world VMT was missing in the two new
inspections. A textual dependency check also found the installed
`dev/dev_waterbeneath2` referencing absent `nature/water_frame01_dudv.vtf`;
this is an authored reference, not proof that the renderer selects that shader
parameter. Actual decoding and active shader behavior remain runtime checks.
The prior fourteen-cache/Legacy dependency absence result is retained; it was
not repeated as a broad scan. None of these findings admits a new map.

## Grounded next acquisition paths

For the January 2014 item, Valve documents the legacy
[ISteamRemoteStorage UGCDownload and UGCDownloadToLocation interfaces](https://partner.steamgames.com/doc/api/ISteamRemoteStorage#UGCDownload).
An authenticated Steam consumer can try the recorded content handle
`667956672890059412`, then validate the same pinned ZIP size/SHA-1 and CRC.
This is an unrun alternative transport, not evidence the missing CDN object
can be recovered. Do not assume the newer `ISteamUGC::DownloadItem` will work:
Valve's [Workshop guide](https://partner.steamgames.com/doc/features/workshop/implementation#DownloadItem)
explicitly excludes legacy RemoteStorage items from that method. The web
`GetUGCFileDetails` alternative requires an API authentication key; none was
requested or searched for.

For the fence material, the concrete requirement is an authentic original
CS:GO dependency set containing the exact VMT and its transitive textures,
with the authored historical model context checked. A privately retained,
legitimately installed original CS:GO content depot is a candidate source;
another BSP that relies on the same external model does not supply it. Current
Legacy's installed depot 731 manifest is `1224088799001669801`; it has already
been checked and is not a new dependency source. An older accessible manifest
and its actual material contents have **not** been established. Public
[SteamDB history](https://steamdb.info/depot/731/manifests/) requires sign-in
for older entries, and the current public depot view can show an app-730 stub.
Do not fetch the current app-730 content or invent an old manifest ID. See the
existing [depot research and access limits](2018-era.md#2-old-game-builds-steam-depots).

Only after obtaining and verifying original bytes should shared, explicit
private dependency mounting be considered. Then root must run decode/probe,
simulation, traversal and visual admission checks. No runtime correction can
recover an absent original material from these map metadata alone.

## Distinct early Train, without replacing Train (2015)

Valve [reintroduced Train on December 10, 2014](https://blog.counter-strike.net/2014/12/11053/)
and describes a [ground-up visual/gameplay rebuild](https://www.counter-strike.net/reintroducing_train/),
including changed A entrances, tunnels, sniper positions and B-site routes.
The offered October 2015 snapshot belongs after that change.

New metadata verification for
[353839326](https://steamcommunity.com/sharedfiles/filedetails/?id=353839326)
establishes Valve creator/app/original `mymaps/de_train.bsp`, **1.34.6.1**,
uploaded **December 9, 2014 22:09:50 UTC**, 10,567,128 ZIP bytes, SHA-1
`fcd42d4ebd70086cf80cb6782d5d30189b6aea98`. This is a grounded pre-reintroduction
acquisition candidate. Its bytes, dependencies, exact layout and playability
remain uninspected; it was not downloaded, added to the catalog or substituted
for Train (2015).

## Verification boundary

Source-only work at parent base `7461440107ac5d5fb9855ffdeeac82f386ec106d`.
Only public documentation/metadata and private original-game data were read;
no leaked/decompiled/SDK code, CS2 content, game execution, builds, Cargo or
runtime tests were used. Inspection scripts, raw metadata and byte receipts
are private under `%USERPROFILE%/csr-fidelity/map-playability-20261018/`;
the handoff is `historical-map-coverage-r1.md`. Source changes are documentation
only. Root owns integration, runtime verification and publication.
