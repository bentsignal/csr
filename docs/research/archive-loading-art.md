# Original archive loading art (candidate)

The map picker offers pinned pre-rework Dust II and Inferno, but the loader
explicitly excluded archive maps from its original LoadingScreen context. The
movie player's filesystem mounts only the installed Legacy game, so renaming
the selected map did not expose its embedded older overview. A generic progress
surface consequently remained the only supported archive presentation.

Read-only inspection of the two installed compatibility BSPs found an original
normal radar, a distinct spectate radar and an overview text file in each BSP's
pakfile. Their paths use the map's original base name, as the public workshop
catalog already records. No historical game data is included in the repository.

| Selected content | BSP SHA256 | Embedded spectate DDS SHA256 |
| --- | --- | --- |
| `csgo_map_de_dust2_2017` | `51502850271C3A8C50320094DA669C430CFA81B3C411511F22FA8A27AA52247D` | `0477A8D97BCE2AA740E9A5DB6F74BFD631597944510075328363569B38B94079` |
| `csgo_map_de_inferno_2016` | `55BF80411C395623FC46D8EFD9FBF1C23DCE89A92D464050AAD7DBC0D0834588` | `D84FD5B80D8B18A1C1DBC2EB55C2601173A8CFBF7C8E3A6A1B8E124718598CAC` |

The candidate lets these two selected archives initialize the existing authored
LoadingScreen. An opaque image URL includes the selected ContentId and loader
generation. The existing engine image boundary rejects a different map,
generation, inactive worker epoch or a request outside Loading. The movie's
root-owned compatibility listener redirects only that root's exact overview
and collection-icon requests. The overview comes from the selected archive's
embedded spectate DDS; the collection emblem comes from its explicitly
catalogued base map in the Legacy install. The archive title remains selected.

Extraction runs lazily on the movie worker outside the frontend snapshot lock.
Before reading, the source file must have its exact expected size. The bounded
read and full BSP hash must match the pinned record. The existing BSP header
and ZIP readers admit the uncompressed pakfile lump with checked offset/length
and a 64 MiB lump bound. Existing loading-image DDS decoding keeps its 4096-side,
32 MiB input and bounded PNG output limits. Only the two resulting images are
retained in that loading context. A replaced, absent or unsupported source
returns failure and clears the context back to the existing progress/cancel
surface; it never falls through to a remake radar.

This retains the current measured OverviewBG aspect fit. It does not guess an
inset/crop or claim native loading-screen layout parity. Actual Nuke, Mirage,
Dust II and Inferno install records currently have normal radars and collection
icons; their optional older `_loading.jpg` and spectate DDS are absent. The
owner's earlier generic Nuke report is not established as a current missing
asset gate. Their authored inset/icon-position text is a separate unsupported
bridge contract and remains open.

Verification is pending: only source/syntax review has been performed so far.
The focused identity test and opt-in installed two-archive import have not been
run. Required workspace/PLAY/RELEASE checks and actual original-movie captures
of both archive loading panels, map switches and cancellation remain pending.
The exact icon request/target spelling in the new redirect also needs actual
public runtime admission. The existing custom progress strip remains present;
removing it safely requires an acknowledged authored-loading raster. UI11 is
not complete.
