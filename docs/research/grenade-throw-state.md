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

### Grounded primary HE speed and pitch model

The retained raw-aim speed750 misses independently observed stationary HE
velocities. A new owned Train packet collected five pre-contact numeric captures
at each of three actual pitches, 0, +45 and -45 degrees, from the same standing
world-ground pose and zero actual player velocity. The first two captures in
each case share a native state; four distinct moving states remain. Every
before/after player bracket preserves origin, eye offset, ground, zero punch and
the actual requested angle. Fresh ordinary public lookup identifies the actual
non-weapon projectile independently in each case, including reused entity
indices with a newly observed class. All cases retain constant horizontal
velocity and gravity320-consistent network positions; maximum endpoint residuals
are below0.023 horizontal and0.017 vertical units. No bounce occurs in the
admitted frames, including the downward45-degree case. All39 images and exact
12-preference restoration are retained privately; no native process remains.

The actual horizontal velocities are 664.745239, 517.080017 and 433.881653
units/second respectively. A bounded empirical candidate uses constant speed
and an upward pitch bias that reduces linearly toward vertical:

`effective_pitch = pitch - bias * (1 - abs(pitch) / 90)`.

Fitting only the 0/+45 horizontal observations gives speed675.000011 and
bias10.0000025 degrees. Rounded675/10 predicts the separately observed -45 and
earlier15-degree observations within0.00006 units/second. This functional family
is not uniquely established by finite horizontal samples. Actual received
vertical signs distinguish upward level/-45 launches from the downward+45
launch; the downward magnitude would need over1.4 seconds of gravity to arise
from a positive initial value, unlike the observed roughly0.156 scaled-wall
command-to-capture interval. The candidate's initial vertical values exceed
the three actual first receive-time velocities by about15 units/second. That
is consistent with gravity but is not an admission of a shared creation tick
or an exactly measured initial vertical velocity. Console timing is not a
substitute for an authoritative native creation timestamp.

A genuinely prospective negative30-degree holdout was then preregistered with
unchanged675/10, predicting horizontal velocity541.4331551 and zero sideways
velocity, with a0.002-units/second tolerance. A separate owned session reports
actual pitch-30/yaw0, the same standing ground/eye/origin, zero player movement
and punch, and a fresh actual projectile. Its five captures retain horizontal
velocity541.433167, residual0.0000119; four distinct states show upward vertical
velocity388.082062 to380.582062 with gravity-consistent network positions.
Maximum position residuals are below0.023 units in either axis. No parameter
or timing was refit to the holdout. All15 images, original unadmitted raw flags,
separate posthoc numeric admission and exact12-preference restoration remain
private under the 20261016-005336 packet; no native process remains.

The exact schema/HE-header-bound grounded primary profile now uses this
observational speed/pitch model for release velocity, alongside the separately
measured horizontal movement inheritance. The spawn origin and its raw-aim
collision trace remain unchanged. Airborne throws, other button modes and other
content retain their previous complete launch vectors. Vertical movement
inheritance remains0.5. Yaw rotation is the existing vector construction;
independent sideways throws, extreme angles, native64Hz, VR and historical2015
equivalence remain unmeasured. Projectile integration, fuse, collision, hand
playback and the bot's preexisting approximate lob planning are unchanged.
This improves the measured ground throw arc; it does not establish complete
trajectory parity or exact native initial vertical values. The catalog policy
generation advances again; the wire schema is unchanged.

Focused source regressions compare actual native horizontal values, the
prospective holdout, conservative received-vertical bounds, unchanged spawn
origin and unchanged unobserved launch scopes. Compilation and integration
verification are pending at this source checkpoint.

### Grounded primary Flash and Decoy angular admission (October 20)

Root independently qualified each kind's stationary grounded PRIMARY samples
at actual pitch 0/-30/+45, yaw 0, in installed Legacy 1.38.8.1 (app 4465480,
128 Hz). For all six cases root personally reviewed the original held weapon,
player before release, first moving projectile and player after release.
Primary strength 1, pulled pin, weapon owner/projectile thrower, unchanged
active weapon, world ground, zero actual player velocity, standing eye 64.062561
and zero punch agree. Per-case review records and original hashes remain
private under `native-ground-families-oct20-r1/{flashbang,decoy}-{level,up30,down45}`.
This source increment relies on root's admissions; it is not another personal
image review or a new native run.

Each kind independently gives horizontal network velocity 664.745239 at 0,
541.433167 at -30 and 517.080017 at +45. The existing weighted-pitch family above
fits each kind's first two angles with rounded speed 675/bias 10; the separate
+45 sample checks it without refitting, residual below 0.00003 units/second.
Received normal-gravity Vz is already aged and is not admitted as initial Vz.
These finite horizontal observations admit a bounded empirical angular model,
not unique engine internals, exact spawn age or complete trajectory parity.

Only the exact observed items schema and each kind's own installed MDL header
receive this grounded primary velocity profile. Horizontal inheritance stays
at its unmeasured prior 0.5; vertical inheritance stays 0.5. Secondary/combined,
all airborne components and missing metadata/mode retain their independent
fallbacks. Smoke's accepted ground/air/button profiles and supported-rest
activation, HE's ground/air responses, Molotov's measured primary horizontal 1.25
with absent angular family, and Incendiary's complete fallback are preserved.
Molotov's known level sample alone does not admit its angular family. Root may
extend the shared grounded-primary profile construction only after separately
qualifying further observations.

The catalog semantic generation advances once from v17 to v18 for the new
grounded families while retaining v17 Smoke activation semantics. No fields,
wire layout, TickInput, spawn trace, gravity, collision or lifecycle law change.
Authored UNRUN regressions exercise ordinary hold/release TickInput at 64/128 Hz,
the three native horizontal targets, retained movement inheritance, absent
angular components, other modes and jump fallbacks. Separate loader tests cover
exact kind/schema/header gating; a missing-mode test uses the production release
seam because ordinary hold/release always supplies a mode. Native 64 Hz and moving,
sideways, extreme-angle, VR and 2015 equivalence remain unmeasured. Root owns
compilation, runtime verification and combined integration/publication.

### Grounded HE button endpoint velocities

A separate owned Train packet preregistered three level, stationary cases:
primary control, secondary-only underhand and both-button combined. Requested
buttons alone were not accepted as mode proof. Fresh public entity lookup and
prediction lists identified the unique current HE and viewmodel for every
case. Actual held player buttons, active weapon handle, weapon owner, viewmodel
owner/weapon handle, pulled-pin state and throw strength independently agree:
primary has strength1, secondary has strength0 and combined has strength0.5.
The player remains on the same world ground at origin[-450,-875,-319.96875],
eye offset[0,0,64.062561], pitch/yaw0, zero movement and zero punch in all six
pose brackets per case. Infinite ammunition preserves this same owned weapon;
each projectile is freshly looked up and verified by its current class.

The untouched horizontal predictions were primary664.745239, secondary
199.4235717 and combined432.08440535 units/second, using ratios1,0.3,0.65.
Actual projectile velocity is respectively664.745239,199.423584,432.084351.
Secondary and combined residuals are +0.0000123 and -0.00005435, both inside
the preregistered0.002 tolerance. All three have zero sideways velocity. Five
captures per case include a duplicated initial state and four distinct
pre-contact states with constant horizontal velocity and gravity320-consistent
positions; maximum position residual is below0.027 units horizontally and
0.021 vertically. Actual first received vertical velocities are102.212517,
20.163757 and61.188133. They are later than construction and do not establish
exact initial vertical velocity or a native spawn timestamp.

The successful private20261016-014903 packet retains48 captures, separate
posthoc mode/ownership/pose/vector admission and exact restoration of all12
preferences. No native process remains. A preceding preflight failed the
existing-process guard before any session or case; that negative receipt is
retained. The successful retry used the same approved helper and guards.
Raw unadmitted numeric flags remain unchanged. All18 pose and15 projectile
numeric images and the actual held ownership/mode fields were personally
reviewed; this is not a claim that every world screenshot was reviewed.

Before observing any angled mode result, a separate three-case session froze
the675/10 pitch family and0.3/0.65 speed ratios. The level primary control
predicts664.7452333; underhand at+45 predicts155.1239997; combined at-30 predicts
351.9315508 units/second horizontally, with the same0.002 tolerance. Actual
velocities are664.745239,155.124008 and351.931519, respectively. Both untouched
angled predictions pass within0.000032 units/second. Actual pin/held button
state, strengths1/0/0.5, ownership, current weapon/viewmodel handles and stable
world-ground zero-movement/zero-punch pose are admitted independently in each
case. All15 projectile and18 player bracket numeric images were personally
reviewed. Four distinct pre-contact states per case have position residuals
below0.02054 horizontal and0.01375 vertical units. Received vertical velocity
is downward for underhand+45 and upward for combined-30, with gravity320
coherence. No parameter was refit and no exact construction time was inferred.
The private20261016-022325 packet retains48 raw captures, separate posthoc
admission and exact12-preference restoration, with no native process remaining.

The content-bound profile uses secondary speed202.5 and combined438.75 with
the primary profile's10-degree pitch bias, matching the observed level and
prospective angled endpoints. Finite samples do not uniquely establish the
global pitch family. Their horizontal and vertical movement inheritance stays0.5;
the measured primary horizontal1.25 response is not generalized to them.
Airborne vectors, raw-aim spawn traces, other content, animation timing and
projectile integration remain unchanged. Continuous strength transitions,
moving or crouched secondary/combined releases, unmeasured angles, native64Hz and
historical2015 equivalence are unmeasured. The catalog policy generation
advances; wire state does not change. Source regressions compare measured
endpoints and preserve those unchanged scopes. Build and integration checks
are pending at this candidate checkpoint.

### Nonprimary HE horizontal movement inheritance

Stationary button velocities do not establish movement inheritance. A separate
three-case packet preregistered a primary stationary control, forward underhand
and backward combined throw, comparing the existing0.5 response against1.25.
The predictions use the independently admitted stationary mode vectors and
actual measured player release velocity, rather than assuming that movement
buttons imply the target speed. All release brackets retain the expected
standing eye/world ground, zero vertical/sideways network movement and punch,
and actual pitch/yaw0. Actual held strengths/buttons and weapon/viewmodel
ownership are independently checked for each fresh current projectile.

The primary control remains664.745239 units/second. Forward underhand reports
actual player network velocity[245,0,0] and projectile[505.673584,0,20.163757]
at the first received state. Subtracting the independent stationary underhand
horizontal199.423584 gives306.25, exactly1.25 times the actual player speed.
The0.5 competitor misses183.75 units/second. The five projectile captures have
four distinct pre-contact states, constant horizontal velocity and
gravity320-consistent network positions, with residuals below0.02328 horizontal
and0.00970 vertical units. All six actual player pose images and actual held
mode/ownership fields were personally reviewed. The observed network movement
field is used; the separate interpolated absolute-velocity field is excluded.

The combined case also reports a1.25 velocity difference, but fails its strict
initial-position admission: actual settled before-hold x74.153938 differs from
requested70 by4.153938, exceeding the preregistered0.01 tolerance. Its actual
moving release brackets are retained as qualified evidence, not relabeled as
an untouched all-gates pass. The separate admission preserves that negative.
An independent combined-only confirmation must admit actual initial x70,
zero movement and world ground before any hold/runup, after ordinary movement
has stopped and settled before the final teleport.

That independent confirmation, private20261016-030238, passes the strict actual
initial image gate at exactly[70,-875,-319.96875], world-ground and zero movement
before holding or runup. There is no later teleport. All five release brackets
report network velocity[-245,0,0], unchanged standing eye/floor, zero punch and
pitch/yaw0. Actual pulled pin, both held buttons, strength0.5 and current owned
HE/viewmodel handles agree. Fresh projectile lookup and all five numeric images
report horizontal125.834351, with four distinct pre-contact states and
gravity320-consistent vertical progression. Position residuals are below0.01951
horizontal and0.02880 vertical units. Subtracting independent stationary
combined432.084351 yields-306.25, exactly1.25 times the actual player velocity;
the0.5 competitor misses-183.75. Both hypotheses and numeric tolerances remained
unchanged. All six player numeric images and actual mode/ownership fields were
personally reviewed. The owned packet completed18 captures and exact restoration
of all12 preferences, with no native process remaining. The earlier strict
negative remains separate and is not overwritten by this clean confirmation.

The private20261016-024633 packet retains48 raw images and separate posthoc
admission; all18 player and15 projectile numeric crops were personally reviewed.
All12 preferences were restored exactly and no native process remains. The
preceding024321 preflight negative launched nothing and touched no preferences:
its process receipt positively identifies a headless CI `sim de_dust2`. The
reviewed retry permits only that exact known CI executable with an anchored
`probe de_dust2` or `sim de_dust2` command; every Legacy or unknown process
still blocks, and no unowned process is stopped. The successful retry observed
no other game, so live exercise of the headless exception is not claimed.

Optional mode-specific horizontal inheritance fields permit independent
admission. The content-bound underhand and combined profiles now use1.25; absent fields
retain the previous0.5 response. Vertical inheritance, air, spawn traces and
other content remain unchanged. Independent reverse-direction underhands,
perpendicular movement, crouch, native64Hz and historical2015 equivalence remain
unmeasured. Independent forward combined throws also remain unmeasured.
Compilation and integration checks are pending for this separate
source checkpoint.
