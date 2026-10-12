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
