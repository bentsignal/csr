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
Release during pin pull is latched until the declared windup finishes. Holding
never starts the projectile fuse. Secondary uses ACT_VM_RELEASE; combined input
is retained separately and uses the overhand clip. Bots release from Holding
through their regular TickInput producer. Switching cancels an uncommitted pin;
switching/dropping is blocked during committed playback to avoid duplication.

The viewmodel follows those authority ticks and holds the last pin-pull frame
until release. Existing model-event sound playback schedules pin sounds from
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
math are retained. Numeric underhand/combined launch strength, charge blend,
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
final pull-pin pose without cloning the animation every render frame.

Protocol11 admits the new serialized WeaponState field and rejects older
peers. Snapshot admission checks producer ticks and phase invariants before
exposing throw state. Catalogs compare timing-header identities too.

Verification: focused sim/level/netcode tests passed after replacing the old
instant-throw fixtures. Additional tests cover both tick rates, every button
mode, indefinite holding, exact once-only release, tap windup, cancellation,
and snapshot restoration. Native ballistic calibration and rendered phase
captures remain integration checks; public metadata is not engine behavior.
