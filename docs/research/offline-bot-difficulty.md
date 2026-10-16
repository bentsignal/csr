# Offline bot difficulty configuration

The original offline picker and hosted lobby formerly admitted only UI values
0 (No Bots) and 3 (Medium). They discarded every other choice, carried only a
`no_bots` flag, and always constructed the existing AI with skill 0.6.

Both paths now retain the selected original UI value in `ClientOptions` and
trusted map requests. The map worker validates it before entering a match or
admitting a host. `Level` loads typed difficulty metadata from its normal VFS,
including content-pack overrides. No installed files are edited, copied into
the repository, or executed as console commands.

The active `gamemodes.txt` supplies each UI value's `bot_difficulty`,
`bot_dont_shoot` and explicit zero quota. Its named Easy/Normal/Hard/Expert
`BotProfile.db` templates supply Skill and ReactionTime, inheriting Default
fields. Skill percent is normalized to the existing AI's 0..1 field; authored
ReactionTime replaces its existing derived reaction delay only for admitted
profiles. No new skill coefficients are synthesized. Harmless retains ordinary
navigation/perception while suppressing attack inputs and utility attacks;
`aggressive=false` would instead create a stationary target-practice bot.

On the read-only installed Legacy 1.38.8.1 data examined for this change:

| Picker | UI value | Bot difficulty | Named template | Skill | ReactionTime | Shoots |
| --- | ---: | ---: | --- | ---: | ---: | --- |
| No Bots | 0 | 0 | none needed | — | — | no population |
| Harmless | 1 | 0 | Easy | 5 | 0.60 | no |
| Easy | 2 | 0 | Easy | 5 | 0.60 | yes |
| Medium | 3 | 1 | Normal | 50 | 0.60 | yes |
| Hard | 4 | 2 | Hard | 75 | 0.40 | yes |
| Expert | 5 | 3 | Expert | 90 | 0.20 | yes |

Private read-only source pins: gamemodes SHA256
`664A20160282F1C19A18528D790017898CEC7E407205FAC3834C3023D5DB46A4`;
BotProfile SHA256
`F41E0F5348ED62F2792FAC874AD13ADC10B59ED96F58083DDB6CF7A637DC0827`.
These are current Legacy 2023 authoring observations, not proof of 2015 balance.

Unspecified options preserve the old 0.6 construction and existing reaction
formula. An explicit unsupported, missing, nonfinite or incompatible profile
fails visibly; it never relabels default 0.6 as another tier. A valid No Bots
entry remains available even if its personality file is missing or malformed.
No gameplay wire or snapshot fields change: settings belong to authority,
and bot decisions still become normal per-player TickInput.

This is bounded configuration support, not native persona selection or engine
AI parity. Native grouped difficulty pools also contain Fair, Tough, VeryHard
and Elite; their distribution, per-personality selection, AttackDelay,
aim-focus/look parameters, tactics and original algorithms remain unmeasured
and unimplemented here. The existing AI still uses its existing skill-based
turn/aim/recoil heuristics. The dialog opacity/layout portion of UI09 remains
open.

One regression loads owned metadata through a memory content mount, constructs
distinct profiles, verifies invalid explicit profiles fail closed, and runs a
real authoritative bot world: Harmless moves without requesting attacks while
the same Easy profile with shooting enabled actually fires. Compilation,
installed-data and rendered picker verification remain pending root integration.

The narrow ignored installed import check is
`installed_all_six_bot_difficulty_choices_resolve_through_content_vfs`. It uses
the ordinary mounted Legacy VFS, reports the actual mounted source hashes and
all six resolved profiles, and requires distinct admitted skills plus matching
Harmless/Easy profile values with different shooting settings. This guards
against assuming that the synthetic pack witness proves installed parsing.
