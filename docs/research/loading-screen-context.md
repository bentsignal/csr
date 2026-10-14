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

Private logs, captures and owned-process receipts are under
`csr-fidelity/overnight-feedback-20261014/loading-*` outside the repository.
Two earlier probe preflight failures are retained there alongside the actual
public observations; they are not counted as successful observations.
