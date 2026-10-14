# Grenade pin, hold and release milestone

This source milestone replaces instant trigger-click launches with tick-based
pin-pulling, indefinite holding, release playback and inventory removal after
that playback. It uses the selected installed viewmodel's declarative sequence
duration and throw event, not a guessed windup delay. It is an authored-animation
policy; those declarations alone do not prove the native command-phase law.

[Valve's February2014 update](https://blog.counter-strike.net/2014/02/8505/)
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

Remaining fidelity: existing projectile velocity, fuse, collision and bounce
math are retained. Numeric underhand/combined launch strength, charge blend,
native minimum windup, native inventory-consumption phase, grenade deploy
cooldown, cancellation and airborne/VR release laws need independent observed
targets. The current milestone distinguishes secondary state/animation; it
does not claim the shorter native underhand range. No response constants were
fit to a residual. Retained grenade state during release playback keeps the
viewmodel available; this is explicit presentation policy, not native phase
admission.

Source-only checkpoint: no Cargo, Native, GPU or CI run yet. The new serialized
weapon field requires the next semantic protocol version after PR84 is merged;
this branch must be rebased and that version finalized before publication.
