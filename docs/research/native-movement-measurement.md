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
Crouched jumps, positive-stamina takeoff, effective surface friction and full
surfing remain separate measurements. The crouched branch is unchanged. The
subsequent standing-running measurements below qualify the first supported
height and ground recovery for that narrower case.

## Standing running jump, landing and coast

Separate controlled P2000 trials retain all 512 native command pairs at each
actual rate, with exact post-to-next-pre continuity, TickBase advancement,
unchanged typed weapon/scope and all 43 configuration values. Standing hull
routes were qualified by three independent native placement/neutral/query
windows before running. The captures have zero lifecycle events, frozen
flags, mouse deltas or angle drift. Ordinary forward and jump input remained
held through landing and at least 32 supported recovery commands, then released
for at least 32 coast commands. Host pacing does not establish command counts.
The measured first forward wish was 225, followed by 450; the inputs remain
unchanged. The installed P2000 declaration and measured plateau establish its
240 speed; the observed player max-speed property of 260 is not substituted.

| Actual rate | First forward speed | First supported height above settled floor | Landing stamina | Next supported speed |
| --- | --- | --- | --- | --- |
| 64 | 20.625001907 | 1.464447022 | 13.962831497 | 177.657485961 |
| 128 | 10.312500953 | 1.478576661 | 14.431581497 | 175.726898193 |

For this standing ground run, observed transitions fit a wish and horizontal
speed cap `W*(1-pre_stamina/100)^2`, with full weapon speed `W=240` supplying
the ground acceleration base even on the initial 225 wish. Stamina then
recovers at 60 per second. Friction uses the recorded 5.2 and stop speed 80.
The relation fits 465 supported-to-supported command transitions at 64 and
417 at 128, with maximum speed residual below 0.000022. Entire coast windows
fit nominal friction within 0.000008. These are observed black-box relations,
not claims about an engine body or effective friction on other surfaces.

First supported stamina matches `0.05*abs(pre_command_vz)` at both rates.
That pre-state VZ differs from the mover's existing half-gravity contact
impact bookkeeping. The stamina correction keeps impact events and
`last_fall_speed` separate. The first supported post remains above the settled
floor; the next supported step snaps. Earlier Rust snapped immediately,
charged the extra half-gravity to stamina and repeatedly damped horizontal
velocity. Its private synthetic-floor baseline reached 127.140732 after eight
recovery commands where native 64 reached 205.792755.

The ground mover now uses these scoped cap, acceleration and stamina targets.
Private per-command caches preserve the serialized state shape. Existing
stance multipliers, air acceleration, crouched jump and bunnyhop laws remain;
native proof here is standing running only. Numeric regressions replay the
actual 512-input timelines on an analytic floor and check runup, plateau,
takeoff, first support, all 32 held recovery commands and 32 coast commands at
both rates. Protocol 8 rejects version 7 and older prediction semantics.

The original gate's constant-wish host rejection remains preserved. A separate
host analyzer accepts exactly the observed initial 225 or 450 forward edge,
then requires all subsequent held wishes to be 450 and released wishes zero.
Malformed later wishes, side/up input, duplicate intervals and wrong edges
remain refusals; no raw data was rewritten. Negative spawn, drift, lifecycle
and query receipts remain retained as well.

Private proof identifiers are
`controlled-running-quiet-held-64-20261013-123714-93b26a0b` and
`controlled-running-quiet-held-128-20261013-130607-a52e30da`. The same frozen
observer source/SMX above and quiet fixture gate were used at both rates. Each
native process exited zero, restored twelve Steam preference targets and the
89-file configuration snapshot, disabled private loaders and preserved all
raw copies. The fixture ended warmup once with the recognized ordinary
`mp_warmup_end` command and waited for recorded freezetime before strict
nonfrozen qualification. This is a documented fixture rule, not default-server
parity. Requested mouse-isolation launch flags do not prove flag consumption;
actual zero mouse/angle fields qualify these windows.

This synthetic-floor comparison does not admit an installed-map
`sim::World` replay, an internal physics callback phase, 2015 behavior, slopes,
surfing, moving surfaces, crouch or fatigued takeoff. Those remain open.

## Route qualification and remaining comparison

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

Stable neutral state is a prerequisite, not movement parity. The controlled
trials above now retain running, continued input, recovery and coast separately.
A first supported snapshot is not automatically the exact physical impact time
or a causal attribution to friction/stamina.

Moving-state evidence alone does not qualify native pre/post correspondence with
the existing [simulation replay](movement-replay.md). Match actual map/catalog
identities, initial state, configuration and inputs before admitting that replay.
Repeated-crouch fatigue remains a later measurement task.

## Ground correction validation checkpoint

On 2026-10-13, production leaf `c85b621` passed 31 movement and 85 netcode
tests, including protocol-7 refusal. An isolated copy of our old `220b2b3`
mover failed four independent measured regressions: partial-input runup,
landing cost, first supported height and the following ground recovery cap.
The disabled weapon-speed acceleration option passed against both versions.
The public implementation was never temporarily replaced for that RED run.

Separate private binaries replayed all 512 actual inputs at each rate using
the corrected mover. Every raw command/state/reference pair remained unchanged
in the outputs. All supported/airborne states matched. Maximum horizontal
velocity residual was 0.00001526 (one `f32` ULP during runup); entire position
trajectories matched within 0.000000001 and stamina within 0.000000000493,
consistent with nine-decimal CSV serialization. All 32 supported recovery and
complete coast speeds matched within 0.000000001. Both unobserved Rust-counter
variants remained equal. Independent review verified source, executable,
input/output hashes and all raw pairs. These error bounds apply to the scoped
analytic-floor comparison and do not broaden its admission.

The complete workspace passed 619 tests with six existing ignored tests,
including a tightened 0.0001 first-support height tolerance. Required release
`probe de_dust2` and `sim de_dust2` each reported zero failures. The sound
decoder check retained 1,425 decodable sounds and zero failures; the asset
guard passed. Actual process exit codes and private before/after logs remain
retained alongside the comparison provenance.

## Surf-map geometry and entry fall checkpoint

The installed `surf_ski_3_nova_kg_v6` map is the next measured fixture,
not an assumed surf preset. Its BSP SHA1 is
`d53036167dc30ac976ee4144d0faf8ef7a08b417`. A private read-only helper
queried 36 fixed standing-player hull sweeps, excluding only the observed
player. Whole-query before/after context and entity inventories matched.
The admitted native run used Legacy 1.38.8.1 at 64 Hz and retained all
command, lifecycle, module, configuration and preference receipts.

An independent private executable used the current `Level` and
`CollisionWorld` against that exact installed map, with the same endpoints,
standing hull and player-solid mask. All 36 hit/start-solid/all-solid flags
matched. Maximum endpoint residual was 0.00006104 units, normal residual
0.000000000966 and fraction residual 0.000000007952. After reconstructing
`f32` values from native nine-decimal text, endpoint differences were at
most two ULPs, normal differences zero and fraction differences one ULP.
This checks those sampled traces; it does not prove a continuous route,
authored brush identity, trigger effects or moving-player physics.

A separate ordinary placement recorded the inherited incoming velocity,
then qualified a fresh 64-command stationary entry window. Actual feet
settled at `(3496.5, 0, 1226.03125)`, with zero velocity, base velocity,
stamina, duck and input throughout. The held weapon was an unscoped AWP,
definition 9. It must not be reported as a P2000 entry or assigned the
observed `player_maxspeed` property as its weapon speed.

Replaying the preceding 62 zero-input fall commands through the current
Rust mover and actual map collision matched all recorded post positions,
velocities and stamina exactly at the retained text precision. Initial
feet were `(3496.5, 0, 1240)` and incoming VZ was -37.5. First support
occurred at command 358, height 1226.81640625 and stamina 6.875; the next command
settled at 1226.03125. The private helper resolved AWP speed from the
installed item catalog and used the actual relevant movement cvars.
All 62 ground classifications matched the available ground-handle
interpretation. Raw flags contained a stale ground bit while that handle
was -1; the interpretation remains explicit rather than a claimed native
internal phase. No production movement tuning followed this comparison.

Private evidence is retained in
`movement-reference-contract/ramp-geometry-source3/serialization1-review`,
`surf-trace-comparator-preparation`,
`movement-reference-contract/ramp-entry-diagnostic-only-64-20261013-183239-5661635a`
and `ramp-entry-fall-replay`. The fall fixture SHA256 is
`32e4305a072263a35645aab213f518d19e5d7c89fa139cddbbd099f12fe1afbe`;
its Rust output SHA256 is
`f5b41ccfa60e9b326c148867c930b262b30d38cbc41ee9c7a07faa09f494e0e2`.
Native lifecycle and preservation checks passed; the private corrected
build and replay both exited zero. These results do not establish general
surfing, ramp ingress or recovery, 128 Hz, raw-bit equality or 2015 parity.

The first bounded backward trial subsequently qualified three complete
64-command diagnostic/support/moving windows with an unchanged, unscoped
FAMAS, definition 10, and an explicitly resolved catalog speed of 220.
Actual input was 27 backward commands: first wish -225, then -450, followed
by release and coast. Feet moved from X3496.5 to X3408.230957031, retaining
Y0 and Z1226.03125; all callbacks remained supported and final velocity was
zero. All 43 cvars matched between windows. The game exited zero and
preserved twelve preference targets, installed configuration and raw copies.
The short pulse did not reach a ramp and must not be described as surfing.

Earlier selector-only and give/use fixtures retained AUG and M4A1 respectively
and were rejected before movement input; their causes remain unestablished.
The accepted successor sends no weapon selector or provision commands. It
requires the same observed typed class/definition and unscoped state across
every complete window, with an explicit speed from the installed catalog.
It never substitutes `player_maxspeed` or a default weapon speed. Private
evidence is
`movement-reference-contract/ramp-back64-exploratory-only-64-20261013-193921-a3147fba`.

The independent private `ramp-back64-map-replay1` then replayed all 64
commands through the current mover and actual map collision. It resolved
the same FAMAS definition/speed through the repository's weapon catalog and
retained every wish, including the initial -225. Source's directional
`IN_BACK` bit maps through the wish vector; it is not copied into Rust's
unrelated button flags. Maximum post-position residual was 0.000000000875,
velocity residual 0.000000000993 and stamina residual zero. All reconstructed
`f32` values and ground-handle classifications matched, independently
recomputed against the original raw CSV. No physics or collision change
was justified. The short hold reached speed 207.089569091, not a measured
220 plateau; release and complete coast remained separate observations.

The qualified private build and runtime exited zero, with clean source
identity captured before that build. An earlier wrapper's unavailable exit
code remains preserved as unqualified evidence. Fixture SHA256 is
`7C492C84035D6DD69FC95FE5B898EFC0703648D7E71213B8C4BDB7B75A5B3481`;
Rust output SHA256 is
`4718C8D0D209C3B4647134ADF396D867FFD67B075D3B5BFD78E4121B2E6BFC75`.
This is a movement replay of the scoped ground sequence, not full simulation,
native internal-phase proof, ramp contact, a complete route or surfing parity.

A separately qualified 128-command moving window at the same 64 Hz used a
stable unscoped AUG, definition 8/catalog speed 220. Its 88 actual held
commands and coast ended at `(3196.900390625, 0, 1226.03125)` with zero
velocity. Every post remained supported; this extended observation also
did not reach a ramp. The installed BSP explains the result: adjacent
world deck faces continue to X3186.5, and the standing hull still overlaps
that deck at the final center. Further ingress work starts with actual
downward hull queries at the edge, rather than attributing the constant
height to faulty ramp physics. Private raw evidence is
`movement-reference-contract/ramp-back64-exploratory-only-64-20261013-195523-33943187`;
its actual-map replay remains a separate prepared, uncompiled fixture.
