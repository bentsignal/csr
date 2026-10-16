# Bot crossing recovery: bounded verification

The owner reported bots repeatedly jumping against the Dust II B-window box
and other corners (BO01). [PR #175](https://github.com/bentsignal/global-offensive/pull/175)
addresses two reproducible implementation defects: replanning the same failed
directed crossing, and counting sideways escape motion as waypoint progress.
Its final source is `363ae05ac0b602e471c451251ed5e8ebbad9e106`.

The bounded recovery cache preserves alternate approaches, tactical costs and
wandering. It expires temporary exclusions, avoids repeated identical failed
A* queries, ignores navigation inactivity during combat and respects the next
area's explicit `NO_JUMP`. These are implementation recovery rules, not a
measurement of original CS:GO bot AI. Two focused regressions exercise route
recovery, query reuse/area changes, expiry and flag handling at 64/128 Hz.

## Source and runtime checks

Final source CI passed both jobs in
[run 38883893810](https://github.com/bentsignal/global-offensive/actions/runs/38883893810).
The coordinating agent also validated integrated source
`6ef460efc3668d664cee19da4eed228efeb0f0b4`, crates
`2310df1a0f5cc0f29a46850ae6ba65d48f091f89`, with full workspace and focused
nav/sim tests, a PLAY build and normal/old-map checks. The final sim/nav files
are byte-identical to PR #175. Classic Dust II probe retains its ten existing
baseline failures; it is not reported as passing. These records do not stand
in for separately tracked final RELEASE qualification.

A bounded ordinary nine-bot run then captured the final artifact at actual
simulation time 30.00000. The owned child exited zero, raw logs passed their
error/panic and finite-state gates, and the private classic BSP identity stayed
`8D0BFCABEECC4600208F495308A50681CA7E9EEB`. No bot poses, commands or effects
were injected. The existing watch-camera hook observes the first living bot;
it does not change that bot's gameplay.

Both implementing and coordinating agents personally viewed the final image.
A complete unobscured actor and ordinary weapon/locomotion pose are visible.
The same capture records watched bot 2 at
[-433.29852,498.53064,5.68609], grounded, with velocity
[-9.1694765,214.80438,0], path index 15/40, four route searches and temporary
failed crossing 5104 -> 5106 expiring at 30.71875. Across the nine actual
actors, route-search counts are 1-4. This establishes functioning rendering
and actual navigation/recovery state; it is not a performance comparison.

Private evidence is retained under `bot-stuck-20261016`. The final executable
SHA256 is `BF8D5091C4691BACF85AC76467CDC0120127ED75FFFA9168ED5B95FE0DDBDBC3`;
the final image SHA256 is
`84FCADC5CA9EFE473DEBED01C5F24F3BA72A510AC0DBC0CCAE6301341797B489`.
Separate implementing-agent admission
`posthoc-final-bot363-functional-review.json` has SHA256
`594BC339AB018CFB8C3BC335563BD7A3877E437E00167119448F8817564FAA62`;
the coordinating-agent review is `root-bot-final-visual-review.json`.
Original raw receipts and their false parity flags remain unchanged. No game
assets or screenshots are included in the repository.

## Retained negative limits

An earlier four-capture packet used the prior 9c6d306 feature inside integrated
52ff900. Both prospective fixed B-site camera captures were occluded by crate
geometry and do not admit B-window/box coverage. The earlier watch-15 image
was partly pillar-occluded; watch-30 showed a clear actor. Actual first-alive
selection changed from bot 2 to bot 4 after deaths, so these are not a
continuous same-actor trajectory. The implementer viewed all four images;
the coordinator independently viewed the clear watch-30 and occluded B-site-30.
Those captures do not cover the later failed-query-cache/NO_JUMP delta.

The final clear image and nonzero authoritative velocity still do not prove
smooth continuous movement, absence of brief rubber-banding or original AI
parity. The particular reported B-window jump loop was not reproduced.
BO01's exact corner case and BO02's independent airborne animation/interpolation
checks remain open for targeted observation and owner regression testing.
