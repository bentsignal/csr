# Native movement measurement checkpoints

The owner still reports excess slowing after landing. Tuning remains open.
The current reference is CS:GO Legacy 1.38.8.1 (October 2023), accepted by the
owner for movement observation. These receipts do not establish 2015 parity.

## Stable native state at both tick rates

A private, passive SourceMod observer records unchanged command arguments,
pre/post player state, server-frame receipts, lifecycle events and per-command
context. Public API reflection qualifies each property. Missing values stay
empty; callback names alone do not establish a physics-step correspondence.
No Source SDK, leaked implementation or decompiled code is used.

The controlled fixture is installed Dust II, Deathmatch type 1/mode 2, with
the observed bot quota explicitly changed to zero. Physics cvars are retained
at their observed values. This is a quiet measurement fixture, not an assertion
that its entire configuration matches a competitive or surf server.

| Accepted neutral window | Actual interval | Held definition | Actual player scope | Weapon zoom property |
| --- | --- | --- | --- | --- |
| 64 command pairs at 64 tick | 0.015625 | P90, 19 | false | available, 0 |
| 64 command pairs at 128 tick | 0.0078125 | M4A4 (`weapon_m4a1`), 16 | false | available, 0 |

Each accepted window has 128 context rows, 64 server-frame receipts, zero
recorded lifecycle transitions and exact adjacent command/state/context
continuity. Standing hull bounds are (-16,-16,0)..(16,16,72); base velocity is
zero. The named friction and movement modifiers are 1, and the reflected
movement type is the public WALK value. TickBase advances within each command;
command number, client tick, server tick and TickBase remain distinct domains.

Player scope is read only after actual SendProp reflection reports an Integer
with exactly one bit. A preceding Nova capture lacked a weapon zoom property;
the observer preserved that absence instead of inventing zero. The subsequent
scope observer adds an independently typed player property and retains the
weapon property's separate availability.

The named player `m_flMaxspeed` value is 260. An installed Nova declaration
specifies 220 for its normal and alternate speed. Therefore the player property
cannot simply substitute for a held weapon's configured or observed speed.
Likewise, named `m_flFriction` is not automatically proof of effective ground
surface friction. Running-speed and surface observations must resolve these
semantics before comparison.

## Invalid samples are retained

An earlier neutral receipt had an origin discontinuity and remains rejected.
The current observer hooks round start/end and target spawn/death, including
the interval after the command limit but before deferred file flush. The real
64-tick gate rejected a target spawn during capture and a round start during
that deferred interval. The 128-tick gate also rejected a target spawn before
its accepted second attempt. No failed window was cropped or normalized.

Each gate exited zero, restored all twelve snapshotted Steam preference
targets exactly, preserved the installed configuration inventory and disabled
the private loaders. Independent audits check actual module paths/hashes and
byte-identical copies of all six raw artifacts. Game data, screenshots, raw
receipts and the private observer/compiler remain outside the repository.

Private proof identifiers on 2026-10-13 are
`scope6-neutral-64-20261013-041844-149e4160` and
`scope6-neutral-128-20261013-042429-46fcc992`. The frozen observer source SHA256
is `1af043b2ad1f1397d42998ea9dc5a4ebfa8beeb2ded32adfd3891738da985d5f`;
its compiled SMX is
`b5741f66de0fe3d7507a3419aa828c6ccc18163e230e8a5520fced79e83d992b`.

## Measured standing takeoff

Separate genuine stationary jumps have now completed at both rates. Each
retained all 512 command pairs and their state/context joins, with exactly one
raw jump onset, takeoff and later supported post-state. TickBase advanced once
within every command, and each post-state exactly matched the next pre-state.
Both traces retained unchanged configuration/context and no lifecycle events.
The initial state was supported, standing, unscoped and unfatigued, with zero
velocity, directional input and base velocity. Held weapons differed (MAG-7
and SSG 08); no running-speed claim follows from a stationary trial.

| Actual rate | First airborne VZ | First command height gain | First post stamina |
| --- | --- | --- | --- |
| 64 | 283.243377685 | 4.523330689 | 23.659469604 |
| 128 | 292.618377685 | 2.310497284 | 23.909469604 |

With recorded impulse `J=301.993377`, gravity `g=800` and interval `dt`, the
observed first post velocity matches `J-1.5*g*dt`; displacement matches
`(post_vz+g*dt/2)*dt` within position serialization precision. Post stamina
matches `0.08*(J-g*dt/2)`. Subsequent airborne velocity differences match
`-g*dt`; stamina recovery matches 60 units per second. These are black-box
command transition targets, not copied or inferred engine implementation.

Rust previously published standing takeoff VZ `J-g*dt`, used a larger first
displacement, and charged stamina from the full impulse. Its standing branch
now applies the additional half-gravity before displacement and adjusts the
jump cost. Regression tests compare measured first-state targets and subsequent
free flight on a synthetic flat floor at both rates. They vary Rust-only
`ground_ticks` and `last_fall_speed`, which the mover does not read, so missing
native counterparts are not silently assigned equivalent values. The old
implementation fails the measured first-transition test. Network protocol 7
rejects prior movement versions, preventing mixed prediction behavior.

Both gates exited zero, preserved twelve preference targets and the installed
configuration inventory, and disabled private loaders. Independent reviews
verified twelve actual module paths/hashes and eighteen byte-identical raw
artifact copies per run. Proof identifiers are
`scope6-stationary-64-20261013-045656-00075352` and
`scope6-stationary-128-20261013-050534-afd8c197`. A mistaken compressed tick-rate
argument produced an additional 64-tick run; it remains labeled 64 and is not
used as 128 evidence. Neutral preflight failures remain preserved.

This scoped observable transition comparison does not upgrade the raw
observer's internal physics-phase flag or admit an entire installed-map replay.
Crouched jumps, positive-stamina takeoff, running speed, effective surface
friction and full surfing remain separate measurements. The crouched branch
is unchanged. The first supported landing post also remained above its later
settled floor position; support/snap and landing slowdown are still open.

## Next comparison

An initial untouched-spawn forward-route preflight was rejected before moving
input. The private helper performed one standing/jump-height envelope sweep
and 65 support probes over 512 units using the public player-solid trace API,
excluding only the observed player. Actual traces reported an obstruction,
uneven support and solid samples; all results remain retained. The game exited
zero with preferences and installed configuration preserved. A flat-looking
navigation surface is insufficient: offline inspection also found nonzero
authored displacement offsets beneath another candidate's flat NAV heights.
The next fixture uses separately recorded placement and actual native terrain
qualification rather than treating those navigation heights as collision proof.

Stable neutral state is a prerequisite, not movement parity. Next collect
forward-running jumps, separating continued input
from airborne release/coasting. Observe clearance, actual input transitions,
held speed and scope, takeoff, first supported post-state and subsequent speed
decay. A first supported snapshot is not automatically the exact physical
impact time or a causal attribution to friction/stamina.

Only moving-state evidence can qualify native pre/post correspondence with
the existing [simulation replay](movement-replay.md). Match actual map/catalog
identities, initial state, configuration and inputs before correcting Rust.
Repeated-crouch fatigue remains a later measurement task.
