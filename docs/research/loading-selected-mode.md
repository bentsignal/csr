# Original Loading mode text

Status: source candidate stacked on PR189. Rendered label verification is
pending the root integration build. This does not replace the custom Cancel.

The loader previously retained only the current generation and map name for
the original Loading movie. Its `setMapInfo` call therefore passed null for
game type and mode, and the bridge hid both authored mode/scenario fields.
Known offline and hosted selections now retain their actual Competitive or
Deathmatch mode with that generation. Remote discovery has no mode field;
remote loads, unknown modes and the grenade sandbox keep no mode description.
There is no protocol change or assumption that a remote match is Competitive.

The original movie's public runtime members identified `infoGameType` and
`infoGameMode` at the fourth/fifth `setMapInfo` positions. Installed
`gamemodes.txt` and the already observed offline GO requests use
`classic/competitive` and `gungame/deathmatch`. These keys are passed only if
the selected installed metadata supplies both `nameID` and `descID`. The
original movie resolves and renders its own ModeText; no custom label/font or
copied description is added. Scenario metadata remains unobserved and hidden.

The same ownership snapshot and label rule apply in both original movie-ready
and subsequent context-change paths. Reselection/cancellation still invalidates
the generation and its art presentation. One focused regression switches the
same map from offline Competitive to hosted Deathmatch, then grenade practice,
remote unknown mode and cancellation, ensuring the previous description cannot
bleed into another selection. Cargo and graphical validation remain pending.

## Cancel investigation boundary

A private read-only authoring-tag audit of the installed `loading.swf` found
`Panel.ContinueButton`, but no declared Cancel placement/export. Prior public
runtime member observations expose `ShowContinueButton` and `LoadingNav`; they
do not establish a Cancel callback. The audit skipped all AVM action and
instruction payloads and did not decompile the movie. The existing custom
Cancel and Escape remain functional fallbacks. Continue is not relabeled or
repurposed. An original Cancel route needs additional actual public callback
evidence before replacement is justified.
