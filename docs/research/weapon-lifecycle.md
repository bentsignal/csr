# Burst mode and empty-magazine lifecycle

The installed CS:GO Legacy 1.38.8.1 was observed through ordinary local-player
console inputs, its rendered weapon/ammo HUD, and `snd_showstart 2` output.
These observations establish the cases below, not every weapon's lifecycle or
an exact 2015 timing match. No game implementation code was inspected.

## Admitted native observations

The private `weapon-lifecycle-20261015/native-lifecycle-20261015-020816` run
completed normally. Both guns were positively identified by their visible
models, HUD names and starting ammo, then by their own near-shot waves from
the local player (`src 1`), excluding distant layers.

| Input/case | Glock-18 | FAMAS |
| --- | --- | --- |
| Loaded before switch | 20/120 | 25/90 |
| Secondary mode change | Visible burst-mode notice and `auto_semiauto_switch.wav` | Same notice and cue |
| Primary held through one burst and beyond | Three near shots; HUD 17/120 | Three near shots; HUD 22/90 |
| Last magazine emptied, then trigger released | Reload animation/cues without another click; full magazine restored | Same |
| Last magazine emptied with primary kept held | Remained at 0 ammo through the observation window; reload began after release | Same |

The held-mode observations lasted approximately 2.3 seconds; the held-empty
observation windows lasted approximately 4 seconds. Console receipt and image
capture times are coarse evidence of order and sustained state, **not exact
server shot, switch-lockout or reload-start timestamps**.

An earlier run on the CT team correctly failed admission: giving a Glock while
the player still held the P2000 did not equip the requested gun. Its near waves
and HUD remained P2000, so no ammo-exhaustion case or lifecycle conclusion was
accepted. The retry used the ordinary T spawn loadout. Both runs restored the
same 12 preference files exactly and closed only the owned game process.

Installed declarative model metadata shows no separate mode-switch sequence
in either viewmodel. The Glock's sequence labelled `glock_firelast` has activity
`ACT_VM_DRYFIRE`; that label alone does not establish an empty-trigger animation.
No mode animation is invented. The visible native mode notice and single held
burst are retained, despite the playtest recollection suggesting otherwise.

## Implementation scope

- A successful authoritative mode change emits an appended
  `BurstModeChanged` event; clients play the installed
  `Weapon.AutoSemiAutoSwitch` soundscript cue. Rejected changes do not emit it.
- An empty burst-mode weapon reloads when both attack inputs are released,
  the existing weapon recovery/deploy deadline has passed, and reload is
  otherwise admitted. This stays in `World::step` through `TickInput`.
- Existing first-shot burst recovery deadlines are preserved. Their exact
  native relationship to reload start has not been measured here.
- Protocol 21 rejects older peers before admitting the new event schema.

This does not establish normal-mode AK/M4 empty-magazine behavior,
secondary-held edge cases, switch lockouts, exhausted-reserve dry-click
timing/cadence, last-shot animation choice, or original Scaleform notice
placement. Those remain open. No blanket automatic-repeat change is made.

## Verification

Native evidence includes 155 actual 1600x900 screenshots, command receipts,
local near-shot/mode/reload sound logs and exact preference-restoration
receipts; the implementing agent reviewed the decisive images. Raw game
captures and metadata remain private. Local Rust checks and the rewritten
client's cue/reload runtime verification are pending the shared build slot.
