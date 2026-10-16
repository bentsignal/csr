# Grenade pin, hold and release milestone

This milestone initially replaced instant trigger-click launches with tick-based
pin-pulling, indefinite holding, release playback and inventory removal after
that playback. It initially used the selected installed viewmodel's declarative sequence
duration and throw event. Native observation below supersedes that timing
policy for identified models; declarations alone do not establish the native
command-phase law.

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
fidelity defects, not accepted behavior. The playtested state machine waited for
the entire declared pull-pin clip and then for event3005 in the release clip;
the renderer uses the same producer tick. Installed animation metadata alone
does not establish the native command-to-release or minimum-preparation law.
The existing movement inheritance coefficient remains provisional. Native
observation must distinguish the command edge, visible throw onset, and actual
projectile release before adjusting either timing or launch coefficients.

The owner also reported missing smoke emission audio. The playtested activation
had no smoke sound route; bounce audio does not substitute for emission.

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

### Bounded physical-release correction

A tighter owned HE session captured both a held throw and a quick tap with
acknowledged host_timescale 0.05. The raw timestamps bound the first visible
HE projectile between approximately 0.105 and 0.120 scaled seconds after the
release command, rather than the authored marker's one-third second. Captures
include their begin/end wall-clock timestamps, so first visibility is not
claimed as the exact server construction tick. The tap's earlier preparation
was visibly interrupted, independently confirming removal of the full-pin gate.

A further owned session used acknowledged host_timescale 0.1 and 13 cases:
primary and secondary throws of HE, flash, smoke, Molotov, incendiary and decoy,
plus combined-button HE. Before-release poses and each grenade's pin/throw
sound identities were checked. Every case visibly released the matching world
projectile by roughly 0.12?0.17 scaled seconds, including underhand bottles and
canisters. Godmode was explicitly acknowledged to protect repeated self-throws;
particles were hidden only for observation. Each flash projectile was killed
with an ordinary console command before its fuse, and subsequent captures
showed no flash blindness. Both sessions restored all 12 preference targets
exactly and left no owned native process. The all-type run contains 234 frames
plus 13 before-release captures; all evidence remains private outside the repo.

The identified six installed MDL header hashes now select a separate observed
command-release delay of 0.1 seconds. At 64Hz its ceiling is 0.109375 seconds,
consistent with the observed visibility interval. This is a bounded empirical
profile, not proof of the internal native delay, server tick, 128-tick behavior,
or historical 2015 parity. Different/custom MDL headers retain authored-event
fallback timing instead of inheriting an unobserved native profile. Catalog
identity includes the changed timing-policy generation. No velocity, movement
inheritance, pitch, spawn-offset, fuse or bounce coefficient was adjusted.

Simulation creates the projectile from this command-relative delay through
TickInput alone. Viewmodel clips retain their natural producer-tick clock and
authored full-duration inventory completion. A private pre-release marker-remap
trial was rejected after rendered comparison: event3005 does not establish the
native visible hand-release phase. Physical release/audio timing are verified
independently; exact hand-phase matching, native animation speed, movement/VR
trajectories and historical parity remain open.

### Public throw-state capability checkpoint

A fresh standing HE observation identified the actual selected weapon and
predicted viewmodel through the ordinary public prediction list. The player
dump showed a settled origin/eye offset, zero movement velocity and zero
punch; the viewmodel's weapon handle independently identified the selected
HE. Entity indices were read from that session rather than reused.

The weapon dump exposes `m_bPinPulled` and `m_flThrowStrength`; the settled
values were false and 1. The viewmodel exposes `m_nSequence`,
`m_flPlaybackRate` and `m_flAnimTime`, showing idle sequence 0 and playback
rate 1. This receiver did not expose the required grenade throw-time or
viewmodel cycle fields. The complete capability gate was therefore rejected
before any throw, preserving five images as negative evidence. All 12
preference targets were restored exactly and no native process remained.
An animation-time field is not silently substituted for an absent cycle.

The existing installed-model format reader separately enumerates HE
sequences in order: idle, pullpin, throw, deploy, underhand and
throwcharge_blend. Throw and underhand each declare 13 frames at 30 fps,
with event 3005 at cycle approximately 0.833. These are authored metadata,
not evidence of a native playback clock or physical release time. Runtime
sequence transitions and the exposed clock fields still need an actual
released throw before they can constrain the rendering discrepancy.

Ordinary public help also describes entity lookup and dump commands. Their
availability alone does not establish an actual projectile position/velocity
receiver. Launch pitch, origin, speed, movement inheritance and throw-strength
blending remain uncalibrated; no numeric launch policy changes follow from
this capability checkpoint. Raw installed-game images and logs remain private.

A subsequent finite packet admitted only the fields actually exposed above,
including the viewmodel's explicit HE weapon handle, and executed one held
primary release. The held viewmodel reports sequence 1, playback rate 1 and
animation time 205.679688. By the first post-release viewmodel capture,
bounded at 0.0652 to 0.0724 scaled seconds, it reports sequence 2, playback
rate 1 and animation time 207.171875. Sequence 2 and that rate/time persist
through the 0.3278 to 0.3349-second capture. The independently read installed
sequence order identifies these as pullpin and throw. A world HE projectile
and throwing arm are visible at 0.1398 to 0.1481 seconds. This supports
retaining the selected natural release clip; it does not identify an exact
spawn tick, establish a cycle field, or justify compressing playback to the
authored event marker. Exact arm-pose comparison also needs matched arm
models and viewmodel settings.

The ordinary server entity dump independently identifies a real
`hegrenade_projectile`, reporting gravity 0.40 and friction 0.20. Two dumps
expose no position or velocity, so this is a negative numeric-trajectory
receiver rather than a launch measurement. A client lookup restricted to
the substring `hegrenade` finds the held weapon only; no projectile entity
index is invented from that result. The packet completed 17 captures,
restored all 12 preferences exactly and left no native process.


A further one-release packet attempted to identify the actual client projectile
through public lookup before inspecting any numeric fields. Fresh selected HE,
player state and prediction-list admission passed. Its additional clock change
to 0.001 was acknowledged, but the following ordinary help/readback and its
acknowledgement arrived about 6.2 seconds later, beyond the inherited four-second
command deadline. The packet therefore aborted before entity lookup or numeric
field capture. The late console output confirms the requested clock value;
it does not retroactively admit the interrupted packet or establish a missing
projectile receiver. All 12 preferences were restored and no process remained.
No launch constants follow from this transport/scheduling negative.


The bounded transport correction used a positively reported 0.01 clock rate
without lengthening any command, admission or global deadline. It completed
one fresh held HE release and discovered exactly one actual non-weapon client
projectile, `C_BaseCSGrenadeProjectile`, before selecting its public dump. The
actual entity index came from that session's lookup. Five captured states
expose changing network origins and nonzero `m_vecVelocity`, duplicated under
`CBaseGrenade`; the generic `m_vecAbsVelocity` remains zero and is not treated
as the moving body's velocity. Server debug text and the visible body move
consistently with the network origin. Render/absolute origin lags the network
origin and is recorded separately.

At actual standing pitch 15 degrees and yaw 0, horizontal projectile velocity
is 670.435852 units/second throughout those samples. Vertical velocity changes
from -123.362717 to -175.862717 as network X advances from 1094.15625 to
1204.15625. Pairwise position/velocity differences are consistent with downward
acceleration 320: conservative one-thirty-second-unit endpoint bounds encompass
approximately 319.25 to 320.79, and trapezoid vertical-position residuals stay
below 0.012 units. Two disjoint background areas remain stable, and ordinary
before/after position and view angles agree. All 13 captures, real timing
bounds and exact restoration of 12 preferences are retained privately.

This admits a numeric trajectory receiver and exposes a real discrepancy with
the retained raw-pitch launch baseline. It does not yet identify native initial
velocity, spawn position, spawn tick, moving-player inheritance or underhand
law. No production coefficient changes follow from this single standing case.
The installed schema separately declares throw velocity 750; that declaration
alone is not the actual velocity law. The target here is installed 2023 Legacy,
with historical 2015 equivalence still unmeasured.

### Grounded primary HE horizontal movement response

A finite stationary/forward/backward Train packet independently repeated the
stationary horizontal projectile velocity, 670.435852 units/second. During the
forward release, the actual player velocity stayed at 244.999985 and the actual
projectile velocity stayed at 976.685791. The third case was rejected because
the newly observed projectile reused an earlier weapon entity index and an old
index exclusion incorrectly discarded it. Two replacement attempts then expired
their manual admission deadline before throwing. Those negative receipts remain
preserved; neither absent backward data nor requested movement was substituted.

One further bounded backward-only packet completed with a fresh personally
reviewed player state and prediction list. Ordinary public lookup uniquely
identified a non-weapon C_BaseCSGrenadeProjectile from that session. Five
independent captures show horizontal projectile velocity 364.185883 throughout.
The player states before release and at the four subsequent capture brackets
retain actual velocity [-244.999985, 0, 0], world ground, the same standing eye
offset and floor, and aim [15, 0, 0]. Network position advances coherently with
the changing vertical velocity: gravity-derived intervals give horizontal
residuals below 0.009 units and vertical trapezoid residuals below 0.005 units.
Generic absolute velocity remains zero and is excluded from this calculation.

Subtracting the independently repeated stationary projectile velocity gives
forward and backward horizontal inheritance ratios 1.24999983 and 1.24999995.
Both independently support 1.25 instead of the retained provisional 0.5. The
backward packet completed 17 captures, restored all 12 preferences exactly and
left no native process. Raw images, timing bounds and separate posthoc numeric
admission remain private under the 20261015-235754 packet; the original raw
receipt's unadmitted numeric flags are retained rather than overwritten.

The importer binds this response to the exact observed item schema and HE model
header. Simulation applies it to the horizontal release velocity for grounded
primary HE throws only. As a horizontal-vector law it also applies after aim
rotation; independent perpendicular movement is not measured. Airborne throws,
vertical inheritance, underhand/combined modes, other grenade definitions and
unmatched private content keep their previous behavior. Base speed, pitch law,
spawn origin, fuse, collision integration and hand-animation timing do not
change. Consequently this fixes a measured contribution to running throws,
not complete trajectory fidelity. This is 128-tick installed Legacy 1.38.8.1
evidence, not a native 64-tick or historical 2015 parity claim.

The catalog timing/launch policy generation advances so a changed launch law
cannot silently join an older peer. Wire state and protocol schema are unchanged.
Source validation checks opposite-direction native velocity differences, content
identity admission, unchanged airborne/vertical/unmeasured-mode behavior and the
installed profile. Compilation, full checks and actual rendered integration are
pending at this checkpoint.
