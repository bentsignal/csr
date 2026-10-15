# Grenade pin, hold and release milestone

This source milestone replaces instant trigger-click launches with tick-based
pin-pulling, indefinite holding, release playback and inventory removal after
that playback. It uses the selected installed viewmodel's declarative sequence
duration and throw event, not a guessed windup delay. It is an authored-animation
policy; those declarations alone do not prove the native command-phase law.

[Valve's February2014 update](https://blog.counter-strike.net/page/24/?s=update)
describes secondary underhand throws and combined primary/secondary input.
[srctools' public MDL event API](https://srctools.readthedocs.io/en/latest/modules/mdl.html)
identifies event3005 as EVENT_WEAPON_THROW. Only installed model metadata and
public format/API declarations were used, with no engine implementation.

All six installed Legacy grenade viewmodels declare ACT_VM_PULLPIN,
ACT_VM_THROW and ACT_VM_RELEASE. The pull clip lasts29/30seconds except
Molotov34/30. HE's overhand clip lasts12/30seconds; flash/smoke/decoy/incendiary
22/30; Molotov0.55. Underhand clips last12/30. Each release clip has one event3005;
its cycle times the declared duration determines this milestone's projectile
release. The common event is approximately one-third of a second into playback;
Molotov's overhand event is at cycle0. Clips/ANI data remain runtime-owned.

Level loading imports those profiles into normalized weapon definitions and
includes the selected timing-header bytes in the catalog identity. Catalogs
without grenade definitions retain their existing raw-input digest. Missing or
malformed grenade timing metadata yields no timing profile and no guessed
instant-throw fallback. Dedicated include-model grenade timing is not supported.

WeaponState retains the phase, button mode, pin/throw producer ticks, pending
release and whether the projectile was spawned. TickInput alone changes it.
Release during pin pull interrupts it on the final button-release tick. Holding
never starts the projectile fuse. Secondary uses ACT_VM_RELEASE; combined input
is retained separately and uses the overhand clip. Bots release from Holding
through their regular TickInput producer. Switching cancels an uncommitted pin;
switching/dropping is blocked during committed playback to avoid duplication.

The viewmodel follows those authority ticks and holds the last pin-pull frame
as its base pose until release. Existing model-event sound playback schedules pin sounds from
the same producer tick. No render system creates a projectile, consumes a
grenade or mutates authority.

Secondary now makes a shorter toss and combined input an intermediate toss.
The current strength factors (1.0 / 0.3 / 0.65) are explicitly provisional;
0.65 is the arithmetic midpoint of the endpoints, not a native measurement.
The official announcement establishes short and adjustable throws, but does
not supply these numbers. Do not use this milestone as trajectory evidence.
The existing movement-velocity inheritance factor (0.5) is retained, sampling
actual velocity at release rather than at pin-pull. Running, backpedalling and
jumping therefore influence the throw; crouching lowers the actual eye origin.
The amount of each effect is not yet calibrated against native observations.

Remaining fidelity: existing base projectile velocity, fuse, collision and bounce
math are retained. Numeric underhand/combined launch strength, continuous charge blending,
native minimum windup, native inventory-consumption phase, grenade deploy
cooldown, cancellation and airborne/VR release laws need independent observed
targets. No response constants were fit to a residual. Retained grenade state
during release playback keeps the
viewmodel available; this is explicit presentation policy, not native phase
admission.

Practice replenishes on release-clip completion, clears the old throw state,
and draws a fresh grenade. Presentation clears the bone pose when changing
grenade phases so a masked smoke/pin clip cannot leave the old hand pose in
the next throw. Phase clips are decoded as non-looping; Holding clamps the
final pull-pin base pose without cloning the animation every render frame.

## Authored hold-hand poses

All six installed grenade MDLs also declare the blank-activity sequence
`throwcharge_blend`: a 3-by-1 grid on the `throwcharge` pose parameter (0 to 1).
Its three single-frame animations are `throwcharge_low`, `throwcharge_mid`
and `throwcharge_high`. Explicit sequence weights select 18 left forearm,
hand and finger bones, or 27 bones for Molotov. This was previously discarded
because the viewmodel loader retained only activity-bearing sequences.

The renderer now retains this narrowly validated grid and applies its authored
mask after the base clip during authoritative `Holding` only. Right-button
underhand selects low, both-button combined selects mid, and left-button
overhand selects high. This discrete presentation mapping is provisional;
the model data does not establish native button-to-pose interpolation or
blend-in timing. No easing, ballistic-strength mapping or simulation changes
are introduced. Pull-pin and release clips keep ownership outside Holding.
Unsupported/malformed grids retain their prior base-pose behavior. The poses
are read through the same runtime model/VFS path, including content packs;
dedicated include-model hold grids are not yet supported.

Source validation: one focused case checks the three hold modes, preservation
of unmasked bones, and no override during pin-pull/release or absent throw.
Compilation, rendered repeated throws, and native held-mode comparison remain
pending; this is not a native blending or trajectory parity claim.

Protocol11 admits the new serialized WeaponState field and rejects older
peers. Snapshot admission checks producer ticks and phase invariants before
exposing throw state. Catalogs compare timing-header identities too.

Verification: focused sim/level/netcode tests passed after replacing the old
instant-throw fixtures. Additional tests cover both tick rates, every button
mode, indefinite holding, exact once-only release, tap windup, cancellation,
and snapshot restoration. Native ballistic calibration and rendered phase
captures remain integration checks; public metadata is not engine behavior.


### Held-pose comparison checkpoint (2026-10-14)

Rendered smoke primary/both/secondary holds and repeated throw cleanup passed.
A single owned Legacy 1.38.8.1 session on Inferno CT matched the rewrite's SAS
arms, 1600x900 viewport, viewmodel FOV 60, offsets 1/1/-1 and right-handed view.
All three captures show velocity 0.00 and unchanged position/angles. Their
qualitative primary/high, both/mid and secondary/low ordering matches the
rewrite's authored endpoint selection. This does not establish pixel-perfect
lighting, blend-in timing or trajectories. Native `getang` and `cl_mouseenable`
commands were unsupported; `getpos` and the visible position/velocity overlay
provide the actual camera/stationarity observations. Preferences were restored
byte-for-byte after the owned process exited. Captures and raw identifiers
remain private under `csr-fidelity/grenade-hold-native-20261014/held-smoke-20261014-122708`.

### Owner release-timing regression (2026-10-14 playtest)

The owner observed a visible delay between releasing either throw button and
throw playback, with the projectile timing affecting jump throws. Quick taps
also feel unlike native throws, and running throws fall short. These are open
fidelity defects, not accepted behavior. The current state machine waits for
the entire declared pull-pin clip and then for event3005 in the release clip;
the renderer uses the same producer tick. Installed animation metadata alone
does not establish the native command-to-release or minimum-preparation law.
The existing movement inheritance coefficient remains provisional. Native
observation must distinguish the command edge, visible throw onset, and actual
projectile release before adjusting either timing or launch coefficients.

The owner also reported missing smoke emission audio. Activation currently
has no smoke sound route; bounce audio does not substitute for emission.

### First native release correction

An owned Legacy1.38.8.1 session on Inferno used ordinary console button input,
60FPS and acknowledged host_timescale0.25. HE held-overhand, held-underhand,
a short HE tap, held smoke and held Molotov all emitted their throw cue on the
release command edge. The short tap was approximately67ms of wall time (about
17ms of scaled game time); it did not wait for the roughly0.967s pull-pin clip.
First captured HE/underhand/smoke frames already showed the hand out of view.
This disproves the prior full-clip preparation gate. The state now transitions
to Throwing when the last held attack button is released; holding either button
continues preparation. Physical projectile creation still follows the authored
release event pending a tighter native spawn observation, so this is not yet a
claim of exact jumpthrow or running-throw fidelity. No velocity coefficient,
pitch law, origin offset, fuse or bounce constant was adjusted.

Throw foley follows the authoritative release-phase tick, using the selected
VFS's per-kind installed throw entry; the later projectile event no longer
repeats that cue. An interrupted pin-pull cannot play its queued pin sounds
over the release or the next practice grenade. The catalog policy salt changes
so old full-pin-gate clients do not silently join this timing policy.

Native smoke activation logged smoke_emit.wav and sg_explode_distant.wav.
The installed BaseSmokeEffect.Sound entry declares those near/distant cues;
activation now routes that entry through the existing positional sound mixer.
Soundcheck includes both emission entries and all six throw entries. Captures,
logs, executable hashes and exact preference restoration remain private under
csr-fidelity/grenade-release-native-20261015/release-20261014-222257. All12
preference targets matched the pre-launch snapshot after the owned process
exited; no native process remained. First-frame sampling was coarse, so these
captures do not establish an exact projectile spawn time.
