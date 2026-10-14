# LoadingScreen context observations

Observed on 2026-10-14 by running the installed CS:GO Legacy movie through the
existing offscreen `menu_probe`. Only public runtime properties, callbacks,
getter reads and resource requests were inspected; no authored instructions
were decoded.

The actual loaded root is `_level1000`. It exposes `setMapInfo`,
`updateMapInfo`, `showLoadinginfo` and `setLoadingThumbnail`. The frontend
currently calls `showLoadinginfo` without initializing the map fields.

One call with distinct scalar labels established these assignments:

| Argument | Public property |
|---|---|
| 1 | `mapName` |
| 3 | `infoMapName` |
| 4 | `infoGameType` |
| 5 | `infoGameMode` |
| 6 | `infoScenario` |
| 7 | `infoThumbPath` |

Arguments 2 and 8 remain unknown. This identifies assignments, not the valid
domain or semantics of every argument. An object in argument 1 was assigned
directly to `mapName`; no record fields were read.

A bounded call with installed map `de_dust2`, localized title `Dust II`,
installed KV keys `classic` and `competitive`, null unknown arguments 2/6,
and omitted argument 7 replaced all four `undefined` resource names.
`collection_icon_de_dust2.png` loaded. Three real resource requests still
failed: `../../maps/de_dust2_loading.jpg`,
`../overviews/de_dust2_radar_spectate.dds` and
`../overviews/de_dust2_radar.dds`. The probe exited 0; these raw loader errors
remain failures of the presentation. No getter reads from `gameModeData`
occurred, so this does not verify the mode/scenario argument domains.

The installed normal radar DDS exists, but the navigator rejects parent
segments before looking in the VFS. The installed loading JPG has the name
`maps/de_dust2.jpg`; the older `_loading.jpg` name and spectate DDS are absent.
Supplying selected context alone therefore does not finish loading art.
A future adapter must keep generic path restrictions and use explicit owned
content identities for supported images. Missing art must not be represented
as successful loading of an unrelated resource.

The follow-up adapter recognizes only the two observed parent-path URL
families with canonical map identifiers. It reads only the exact `_loading.jpg`
for that family. The spectate DDS family may use the same map's normal radar
only when the preferred spectate file is absent. This is an explicit
compatibility fallback, not a claim that the original engine substituted it.
Corrupt preferred data is an error. JPEG and DXT1/3/5 DDS are converted to PNG
in memory with a 4096-pixel side limit, 64 MiB RGBA limit, 32 MiB encoded
input limit and bounded encoded output/cache. No parent-path access is enabled
outside these families.

The frontend gets its selected map and generation from the owned loader.
Only Legacy identities with installed metadata, radar and collection icon
initialize the movie. Map title comes from installed localization. Unknown
mode/scenario fields are null; their native domains remain unverified.
Unsupported pack/archive art uses the existing progress/cancel surface.
Home and cancellation clear the context, and cached roots receive the new
map before being shown. Runtime verification of this follow-up is pending.

Private logs, captures and owned-process receipts are under
`csr-fidelity/overnight-feedback-20261014/loading-*` outside the repository.
Two earlier probe preflight failures are retained there alongside the actual
public observations; they are not counted as successful observations.

## Presentation correction

The first converted-art run exited normally with no raw loading errors, but
the modern plain JPG overflowed the authored panel. Public settled bounds
showed that `Panel.Overview` received the 1920 by 1080 photo at the origin of
the 479 by 479 `Panel.OverviewBG`; `MapHolder.ImageThumbnail` stayed empty.
A final opt-in public MovieClipLoader observation confirmed that the movie
itself requested `_loading.jpg` into `Panel.Overview`, and both Complete and
Init reported that same clip. No radar request followed. This is not a
misrouted asynchronous image: substituting the modern plain JPG makes the
optional older loading image falsely succeed and suppresses the movie's
observed radar fallback. That plain-JPG substitution is not a valid fidelity
fix. It has been removed; modern photo compatibility is deferred.

The corrected compatibility surface uses the actual normal radar in the
measured authored overview region. The frontend enables it only when the
exact optional `_loading.jpg` is absent and the same map's installed radar
and icon exist. A root/map/generation-owned MovieClipLoader hook redirects
only that exact optional-photo URL/target pair to the normal radar URL.
The original call, return and authored callbacks remain in place. After
initialization a one-shot listener fits the radar in the measured sibling
OverviewBG bounds without resampling its source or distorting its aspect.
This is an explicit compatibility route, not native fallback parity. A
preferred real loading image keeps its original request unchanged. Context
replacement, unsupported art, Home and cancellation remove the owned listener
and request unloading of its clip; stale callback identities cannot apply
the old layout. The pinned Ruffle implementation notes unfinished unload
cancellation semantics, so this does not prove that a mid-load cancellation
prevents every pending asset write. That edge remains unverified.
Runtime verification of the corrected candidate is pending.

Calling the observed `setHintText("")` removes the absent hint. Hiding only
the observed `Panel.MainText.ModeText` and `ScenarioText` after
`showLoadinginfo` removes unknown labels without inventing game metadata.
The same order was verified in the current offscreen probe. Clearing the
cached root with `hidePanel` then six null `setMapInfo` arguments made no
null or undefined image requests; subsequent Inferno context did request its
own art. This is a movie-level check, not a claim of an actual mid-loading
frontend cancellation test.
