# Server bot population follow-up

Removing a bot releases its server-only brain, team role, money reservation,
enemy contact records and pending coordinator defuser selection. Surviving
brains, current team plans, navigation and skill remain intact. Existing
disconnect gear/C4 handling and monotonically allocated player IDs are
unchanged. This does not implement bot filling or change static/offline bots.

The next Track A population step needs an explicit, server-owned policy. It
can use the existing replicated player ID, `is_bot`, team and life epoch;
clients and prediction must never create, remove or decide for bots. An
opt-in policy could reconcile once per authoritative tick and replace a bot
only after validating a new human's admission. Discovery and invalid/duplicate
Hello messages must never change population. The default should preserve
existing static bots, and host Preparing must preserve its frozen admission
boundary. No new wire fields are needed for that first seam.

## Evidence and unresolved behavior

Read-only inspection of the installed CS:GO Legacy 1.38.8.1 config data found:

| Config | Quota | Mode | Difficulty |
| --- | ---: | --- | ---: |
| `cfg/gamemode_competitive.cfg` | 1 | competitive | 2 |
| `cfg/gamemode_competitive_offline.cfg` | 10 | fill | inherited |
| `cfg/gamemode_deathmatch.cfg` | 10 | fill | 2 |
| `cfg/gamemode_casual.cfg` | 10 | fill | 1 |

The installed `gamemodes.txt` difficulty metadata maps None to quota 0;
Medium's UI value 3 maps to bot difficulty 1. The numeric UI selection is
therefore not itself the difficulty cvar. These are data observations, not
proof of the population algorithm or an instruction to execute arbitrary
config commands. In particular, the local/offline competitive override must
not silently become the default for a multiplayer host.

The Valve Developer Community's [game-mode documentation](https://developer.valvesoftware.com/wiki/Counter-Strike:_Global_Offensive/Game_Modes)
distinguishes the CS:GO offline config override from ordinary server configs.
That page also contains newer CS2 information; only the explicitly CS:GO
config distinction is relevant here. No SDK implementation is used.

Before implementing fill, verify the original game's total versus per-team
quota, spectator counting, bot-vacate selection, join/disconnect timing,
competitive mid-round spawning and quota-kick inventory behavior. The
current disconnect path drops competitive guns, C4 and kits; using it for
repeated quota kicks without measuring the original could multiply ground
gear within a round. The `competitive` quota mode, bot takeover and team
balancing are separate behavior, not aliases for an assumed global fill
formula. Installed values alone do not resolve these questions.

Useful future checks include validated join/disconnect replacement with
fresh IDs, actor/connection bounds, spectator behavior, atomic rejection,
exactly-once objective/drop events, bounded AI bookkeeping through churn,
and actual two-peer UDP snapshots/prediction at both tick rates. Those are
proposal scope, not completed population features.

## Measured explicit fill in installed Legacy

On 2026-10-12 an owned, isolated CS:GO Legacy 1.38.8.1 (build 8802)
listen-server session on `de_dust2` in Casual measured runtime cvars,
timestamped `status` replies and private scoreboard captures. It used
`-insecure` and the existing fidelity net-console/session helpers. No config,
exec or game-log files were written by the observer, and no install assets
were edited. The owned process exited; existing files were preserved and
only newly created known session byproducts were removed by the harness.

The settled fixture used `bot_quota_mode=fill`, quota 4,
`bot_join_after_player=1`, `bot_auto_vacate=1`, and disabled
`mp_autoteambalance`/`mp_limitteams` to isolate population counting. It waited
for team/kick effects, reasserted quota, then allowed population maintenance
before reading the effective quota and status. All rows below retained an
effective quota of 4 except the explicit zero-quota row.

| Local human's assigned team | Bot team setting | Bots | Scoreboard observation |
| --- | --- | ---: | --- |
| Spectator | any | 4 | Four bots; human in spectator list |
| Counter-Terrorist | any | 3 | Two CTs including human, two T bots |
| Terrorist | any | 3 | Two CT bots, one T bot plus human |
| Spectator again | any | 4 | Two CT bots and two T bots |
| Counter-Terrorist | T | 3 | Human alone on CT, three T bots |
| Counter-Terrorist, quota 0 | T | 0 | No bots |

The CT and spectator scoreboard counts were corroborated by the earlier
controlled captures with the same effective quota and team settings. The
settled run separately captured the T and forced-T rows.

An earlier fresh **unassigned** human with quota 4 and after-player enabled
had no bots. Explicitly assigning that human to Spectator allowed four bots,
even with after-player still enabled. Spectator therefore satisfied the join
gate in this fixture but did not subtract from the playing-player fill
target. The playing-team rows are consistent with a total target minus T/CT
humans. The `any` scoreboard observations include humans in the balanced
roster, while a forced bot team is a separate policy. Odd targets, tie order
and additional humans have not been measured.

The preliminary team-change probe also showed that the engine could change
quota 4 to 5 after a CT-to-T change, including after an immediately queued
quota assertion. Console command acknowledgment did not make the subsequent
team/population effects atomic. That diagnostic run is not a constant-quota
comparison; the settled fixture deliberately separated those boundaries.

### Limits and historical policy

This is one local human in **2023 Legacy Casual with explicit fill**, not a
second human joining/disconnecting, a competitive live-round test or a
2018 reference. Vacancy selection, quota-kick inventory/C4 behavior, bot
takeover, zero-human dedicated behavior and full connection-slot admission
remain unmeasured. No bot-fill implementation or exact-fidelity claim follows
from this checkpoint.

Valve's [2021-01-07 release notes](https://blog.counter-strike.net/ro/2021/01/32369/)
changed competitive/wingman disconnect handling to stop replacement bots.
The installed competitive quota mode therefore cannot establish the owner's
2018 target behavior. Keep that historical policy separate from the explicit
fill measurements and the installed offline override.

### Private provenance

Raw console logs contain identifiers and screenshots contain game content;
they remain outside the repository under
`%USERPROFILE%\csr-fidelity\legacy-bot-fill-settled`. Public evidence is
limited to these aggregate observations and SHA-256 fingerprints:

| Private file | SHA-256 |
| --- | --- |
| `observe.ps1` | `ef364ed455a279c98827a99bec0c2a845ba0bb710a4ae3d8da93836f16392574` |
| `console-private.log` | `6c2d22bb316e752785bcb1cb73ff6b649e4f196cdf35ef5b811e511f81aac0c6` |
| `t-scoreboard.png` | `649f079483532bb2fafa03373ca049aa4a0e90e997ccc84337c15558eb616202` |
| `forced-t-scoreboard.png` | `eeb67b8e00245d3ef650e38d4a4660ba38e6d035e7299b5a801b9dbc042016e0` |
| `../legacy-bot-fill-controlled/ct-scoreboard.png` | `2805e7a68f55cac6431dad7710e2d2cd4dcbbfc347bfd99f2c7b076c1911455d` |
| `../legacy-bot-fill-controlled/spectator-scoreboard.png` | `5ce2dc359e48d7225954f8c632cc9ed43227ad52d84eb9967e0825ccf8644a64` |

Both settled and the corroborating controlled captures were independently inspected. Earlier
diagnostic probes remain private in `legacy-bot-fill-observer` and
`legacy-bot-fill-controlled`; their transient totals are superseded by the
settled comparison, not additional evidence for a fixed-quota formula.
