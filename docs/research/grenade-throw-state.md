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
receive this grounded primary velocity profile. Initially horizontal inheritance
stays at its unmeasured prior 0.5; the independent moving admissions below
supersede that component for Flash and Decoy. Vertical inheritance stays 0.5. Secondary/combined,
all airborne components and missing metadata/mode retain their independent
fallbacks. Smoke's accepted ground/air/button profiles and supported-rest
activation and HE's ground/air responses are preserved. The subsequent fire-kind
admission below extends the same construction after independent observations.

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

### Grounded primary Molotov and Incendiary angular admission (October 20)

Root then qualified each fire kind's own stationary grounded primary samples
at pitch 0/-30/+45, yaw 0. Molotov's independently known and freshly checked
level VX is 664.745239; its new -30/+45 VX values are 541.433167/517.080017.
Incendiary independently gives the same three values. Each case's actual held
weapon identifies the kind, with matching owner/thrower and actual
CMolotovProjectile network velocity; the shared projectile class alone cannot
distinguish these grenades. Root personally reviewed the held/before/projectile0/
after originals. Records and hashes remain private alongside the prior kinds
under `native-ground-families-oct20-r1/{molotov,incgrenade}-{level,up30,down45}`.

Both +45 cases have a narrower bracket: the after-release pdump closed. Their
after images establish stable console getpos/eye and zero velocity only;
after-release active weapon, ground and punch fields were unavailable. The
grounded held/before state, pin true, strength 1, standing eye, zero movement
and punch, actual angle and identified first pre-contact projectile VX admit
the angular holdout. They do not establish a complete after-release state
bracket. Received Vz is not initial Vz. The same bounded speed 675/bias 10 family
fits each kind's 0/-30 observations and checks against its independent +45
holdout without refitting.

The existing exact schema/item/header gate now admits grounded primary angular
profiles for all four newly measured kinds. Molotov retains its separately
measured horizontal inheritance 1.25. Incendiary initially retains its
unmeasured prior 0.5; the independent moving admissions below supersede this
component for Incendiary, Flash and Decoy. Grounded vertical
inheritance remains 0.5. Every new kind's secondary/combined and all airborne
components retain their prior independent fallbacks. Smoke/HE and all lifecycle
laws are unchanged. This extends the same pending v18 semantic increment; no
additional salt advance or wire/API change occurs.

The existing identity/installed-loader and ordinary TickInput regressions are
extended to all four kinds, including distinct retained inheritance and absent
angular-component fallback. The Molotov/Smoke moving regression now uses the
admitted Molotov angular profile. All are authored UNRUN; root owns source and
runtime checks and combined integration. These angular observations alone do not
admit movement inheritance, airborne/other-mode families, initial
Vz/spawn age, contact/lifecycle law, native 64 Hz or historical equivalence.

### Grounded primary Flash horizontal inheritance (October 20)

Root separately reviewed all four original images for the level steady-forward
Flash throw. The before/after player network VX is 245 on world ground,
standing, with zero punch and the same active weapon. Held strength 1/pin true,
weapon owner/projectile thrower and the actual first pre-contact
CBaseCSGrenadeProjectile network VX agree. Projectile VX 970.995239 minus its
independent stationary 664.745239 gives 306.25; divided by actual player VX 245
this admits horizontal inheritance 1.25 for Flash grounded PRIMARY only.
Original hashes, exact positions and root's personal review are private under
`native-ground-moving-oct20-r1/flashbang-level-forward/root-review.json`.

Flash's reference now includes angular and horizontal admission. The ordinary
TickInput regression checks the independent moving level target and retained
angular/absent-component behavior. No grounded vertical, airborne or other-mode
response is admitted.
The same pending v18 salt remains unchanged. Tests are authored UNRUN and root
retains runtime/integration ownership.

### Grounded primary Decoy horizontal inheritance (October 20)

Root independently reviewed Decoy's four original level steady-forward images.
The actual player network VX is 245 before/after, with world ground, standing
eye, zero punch and the same active weapon. Held pin true/strength 1, actual
DecoyProjectile identity and owner/thrower match. Player origin moves from
[-342.761261,-875,-319.96875] to [-281.511261,-875,-319.96875]. First pre-contact
projectile VX 970.995239 minus Decoy's independently observed stationary
664.745239, divided by player VX 245, separately admits horizontal 1.25.
Original hashes and root's personal review remain private under
`native-ground-moving-oct20-r1/decoy-level-forward/root-review.json`.

Decoy's exact-gated reference and tests now include angular/horizontal admission,
with its own explicit native moving level target. Incendiary's separate moving
admission follows below. Grounded vertical, airborne and other-mode components
remain unchanged; no extra semantic increment occurs beyond pending v18. All
regressions remain authored UNRUN, with runtime/integration owned by root.

### Grounded primary Incendiary horizontal inheritance (October 20)

Root separately reviewed all four original Incendiary level steady-forward
images. Held Incendiary strength 1/pin true and owner/projectile thrower match;
the actual before/after player network VX is 245 on world ground, standing,
zero punch, with the same active weapon. The player moves from
[-344.675323,-875,-319.96875] to [-283.425323,-875,-319.96875]. First pre-contact
CMolotovProjectile network VX 970.995239 against Incendiary's own stationary
664.745239 gives horizontal inheritance (306.25 / 245) = 1.25. Its held weapon
identity distinguishes Incendiary from Molotov; no projectile-class inference
supplies the kind. Original hashes and personal review remain private under
`native-ground-moving-oct20-r1/incgrenade-level-forward/root-review.json`.

Incendiary's exact-gated reference and existing identity/installed/TickInput
regressions now include this independent inheritance and native moving target.
All four newly measured grounded PRIMARY profiles now independently admit the
bounded angular family 675/bias 10 and horizontal inheritance 1.25. Grounded
vertical remains 0.5; their secondary/combined and airborne components remain
absent with independent prior fallbacks. Smoke/HE, lifecycle and pending v18
salt remain unchanged. Received velocity does not establish initial Vz or exact
spawn age.
Source regressions are authored UNRUN; root owns runtime checks and integration.

### Bot planning follows admitted grounded primary profiles (October 20)

The new angular profiles exposed an integration mismatch: the existing bot
solver admitted only HE/Smoke, so selected Flash/fire utilities still aimed and
cleared a raw-speed-750 arc while authoritative release used their new metadata.
The existing admission gate now accepts any known grenade Kind with grounded
primary velocity metadata. Decoy is included when explicitly supplied to this
solver; normal bot utility selection policy remains unchanged. Unknown names
still fall back even if they carry launch metadata.

The existing bot, grounded, primary-mode, untracked/no-separate-aim and selected
definition/profile guards remain. Air, alternate modes, humans, missing primary
metadata and unknown definitions/kinds retain their prior fallback. Numerical
aim convergence, tolerances, clearance hull/sampling and authoritative release
math are unchanged. No new launch coefficients, AI policy or shared API appear.

The existing end-to-end regression now exercises all six known kinds through
ordinary TickInput at 64/128 Hz, stationary and moving. It checks target-plane
crossing before the shortest existing HE/Flash fuse, rejection of a midpoint
blocker missed by the old arc, and actual matching-owner/kind contact at that
blocker through either bounce or fire contact detonation. Guard/fallback cases
remain covered for each kind. This is authored UNRUN source coverage, not native
bot parity or runtime acceptance; root owns compilation and integration checks.

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
### October 20 lifecycle observation boundary

Original Legacy 1.38.8.1, requested 128 Hz, was observed privately on Train
and Dust II after the grounded-family comparisons. An upward Smoke remained
a moving canister through descent and repeated contacts, then emitted smoke
after its network velocity became zero on the floor. This supports removing
the old unconditional airborne deadline; it does not establish an exact native
settling threshold, dwell time, bounce count or activation polling cadence.

Separate Molotov and Incendiary throws on Dust II visibly burst in the air
and faded without an `inferno` entity. Their current public console settings
reported `molotov_throw_detonate_time = 2.0` and
`weapon_molotov_maxdetonateslope = 30.0`. The capture clock changed from 0.01
to 0.25 timescale and does not independently bind physical projectile birth
to server expiration. These observations flag the retained 3.5-second fire
fallback and unconditional airborne field creation for correction; this
grounded-launch/Smoke change does not claim those fire laws are complete.

The unshipped fire-orientation experiment still shows hard sprite edges.
Scene-depth blending and native omitted renderer defaults remain unresolved;
it is not part of this gameplay candidate.

### October 20 fire expiry correction

The fire-grenade fallback now uses the exposed 2.0-second value with the
existing projectile-age clock and tick polling. Expiry removes the projectile
and emits its existing terminal event; only a qualifying direct floor contact
creates persistent fire. This prevents an airborne expiration from leaving a
floating damaging field. Both Molotov and Incendiary use this path.

The previous contact normal cutoff and early-contact guard are preserved.
Exact native birth/think timing, near-floor projection, slope conversion and
air-burst particles/audio remain separate qualification work. No launch
parameters, other grenade lifecycle laws or network structures change.

### October 20: grounded Flash secondary and combined throws

Separate owned Legacy 1.38.8.1 sessions, requesting 128 Hz, measured Flash's
secondary and combined throws at actual pitch 0, -30 and +45, yaw 0. Each held
weapon was identified as Flash, with pin pulled and strength 0 or 0.5. Player
views before and after release showed a standing World-supported body, zero
velocity and punch, origin Z -319.96875 and eye offset 64.062561. Three moving
pre-contact network projectile samples in each case agreed on horizontal
velocity; the generic absolute-velocity zero was not used.

| Mode | Pitch 0 VX | Pitch -30 VX | Pitch +45 VX | Steady-forward pitch 0 VX |
| --- | ---: | ---: | ---: | ---: |
| Secondary | 199.423584 | 162.429962 | 155.124008 | 505.673584 |
| Combined | 432.084351 | 351.931519 | 336.101959 | 738.334351 |

Each mode's first two stationary samples fit its angular family independently:
202.5 or 438.75 speed, with 10 degrees of pitch-weighted upward adjustment.
The positive-45 holdout agrees without refitting; maximum horizontal residual
against those values is below 0.000051 units/second. The old raw-aim alternate
fallback instead has level horizontal speeds 225 and 487.5 and no upward
adjustment.

The separate moving sessions show player network VX 245 before and after
release, unchanged grounded height/eye/angle and zero punch. Subtracting each
mode's own stationary level VX gives 306.25, independently admitting horizontal
inheritance 1.25 for each. Received VZ samples decline under gravity; none is
claimed as the physical construction-time vertical velocity.

Only the exact installed Flash schema/item/model-header gate gains these two
grounded velocity and horizontal inheritance components. Airborne Flash,
other kinds' alternate modes, spawn traces, throw timing, vertical inheritance
and utility lifecycles retain their existing behavior. The catalog semantic
salt advances to v20; no wire structure changes. Ordinary TickInput regressions
at 64/128 exercise the measured endpoints, preserved release/spawn behavior
and the untouched primary/air/missing-mode fallbacks, alongside the installed
loader's content-bound admission checks.

Private original packet: `overpass-20261019/native-flash-ground-buttons-oct20-r1`.
All eight sessions closed normally and restored all twelve native preference
snapshots exactly. Raw images/logs and their hashes remain private; numeric
review records distinguish source images from unresampled analysis crops.

### October 20: grounded Decoy secondary and combined throws

Eight separate owned Legacy 1.38.8.1 sessions, requesting 128 Hz, independently
qualify Decoy's two grounded alternate modes. Held model names identify Decoy,
with pin pulled and strength 0 or 0.5. Actual stationary pitch is 0, -30 or +45,
yaw 0; before/after player views retain World support, standing height, zero
velocity and punch. Three pre-contact moving network samples per case agree
on VX. These are Decoy observations, independent of the preceding Flash packet.

| Mode | Pitch 0 VX | Pitch -30 VX | Pitch +45 VX | Steady-forward pitch 0 VX |
| --- | ---: | ---: | ---: | ---: |
| Secondary | 199.423584 | 162.429962 | 155.124008 | 505.673584 |
| Combined | 432.084351 | 351.931519 | 336.101959 | 738.334351 |

Fitting each mode's own first two angles yields speeds 202.5 and 438.75 with
the existing pitch-weighted 10-degree upward adjustment. The +45 holdout
agrees within 0.000051 units/second. Each separate running case shows player
network VX 245 before and after release, VZ zero and unchanged grounded
height/eye/angles. Its own stationary subtraction is 306.25, admitting
horizontal inheritance 1.25 independently for each mode.

Only Decoy's exact schema/item/model gate gains the grounded alternate velocity
and horizontal components. Primary, airborne and missing-mode behavior,
physical spawn trace, release timing and lifecycle remain unchanged. Received
VZ samples are not construction-time vertical velocities. Ordinary TickInput
regressions exercise all eight observed endpoints at 64/128 Hz, alongside the
content-bound loader checks. The catalog semantic salt advances to v21 without
changing wire structures.

Private packet: `overpass-20261019/native-decoy-ground-buttons-oct20-r1`.
All eight sessions closed normally with twelve exact preference restorations
each. Original captures/logs, hashes and root numeric reviews remain private.

### October 20: grounded Molotov and Incendiary alternate throws

A further eight owned Legacy 1.38.8.1 sessions per kind independently measured
Molotov and Incendiary secondary/combined throws at 128 Hz. The held model
names distinguish Molotov from Incendiary even though both client projectile
classes are `C_MolotovProjectile`; their dropped-model indices also differ.
Every held view shows pin pulled and strength 0 or 0.5. Each stationary pair
shows standing World support, zero player velocity/punch and actual pitches
0, -30 or +45 at yaw 0. Moving projectile network VX is used rather than the
zero generic absolute-velocity field.

| Kind | Mode | Pitch 0 VX | Pitch -30 VX | Pitch +45 VX | Forward pitch 0 VX |
| --- | --- | ---: | ---: | ---: | ---: |
| Molotov | Secondary | 199.423584 | 162.429962 | 155.124008 | 505.673584 |
| Molotov | Combined | 432.084351 | 351.931519 | 336.101959 | 738.334351 |
| Incendiary | Secondary | 199.423584 | 162.429962 | 155.124008 | 505.673584 |
| Incendiary | Combined | 432.084351 | 351.931519 | 336.101959 | 738.334351 |

Each kind/mode's own 0/-30 fit yields speed 202.5 or 438.75 and the existing
pitch-weighted upward adjustment of 10 degrees. Each independent +45 holdout
agrees within 0.000051 units/second. Separate moving pairs retain player
network VX 245 before and after release, VZ zero and World support. Each
moving-minus-stationary VX is 306.25, admitting horizontal inheritance 1.25.
These observations agree with the independently measured Flash/Decoy values;
no kind inherits qualification from another.

The same v21 batch admits only these exact installed kinds' grounded alternate
velocity/horizontal components. Primary and airborne response, spawn tracing,
release timing, fire contact/expiry and all other utility lifecycle behavior
remain unchanged. Received VZ is not admitted as construction-time velocity.
The actual TickInput endpoint regression covers all three newly admitted kinds
at 64/128 Hz; content identity and absent-air-component checks remain enforced.

Private packets: `overpass-20261019/native-molotov-ground-buttons-oct20-r1` and
`overpass-20261019/native-incgrenade-ground-buttons-oct20-r1`. All sixteen
sessions closed normally and reported twelve exact preference restorations
each. Raw evidence and root numeric reviews remain private.

### October 20: HE airborne primary response

Seven separate owned Legacy 1.38.8.1 sessions, requesting 128 Hz, qualify only
HE's primary airborne angular/horizontal/vertical components. Held views show
`v_eq_fraggrenade`, `w_eq_fraggrenade` and the matching dropped model, pin
pulled and strength 1. Player views before/after retain the actual requested
pitch/yaw, standing eye offset 64.062561, zero punch and NULL ground support in
the jump cases. The separate zero-gravity ground control remains World-supported.

| Primary case | Projectile network VX | Received network VZ, first sample |
| --- | ---: | ---: |
| Air, pitch 0 | 664.745239 | 247.048004 |
| Air, pitch -30 | 541.433167 | 532.917542 |
| Air, pitch +45 | 517.080017 | -296.233643 |
| Air, forward, pitch 0 | 970.995239 | 252.360504 |

The first two horizontal observations independently fit speed 675 and the
existing pitch-weighted 10-degree upward adjustment. The +45 holdout agrees
without refitting; maximum horizontal residual is below 0.000018 units/second.
The moving pair shows player network VX 245 before and after release. Its
projectile-minus-stationary VX is 306.25, admitting horizontal inheritance 1.25.
Normal-gravity player VZ changes during acquisition; received projectile VZ
is not treated as construction-time velocity or used to fit a spawn delay.

Three additional sessions read back `sv_gravity=0` after establishing ground
or two different ascending speeds. In each, before/after player VZ and all
three pre-contact network projectile velocities remain constant:

| Control | Player VZ before/after | Projectile VZ |
| --- | ---: | ---: |
| Grounded zero-gravity baseline | 0 | 117.212517 |
| Air, early zero-gravity plateau | 223.868378 | 397.048004 |
| Air, later zero-gravity plateau | 123.868378 | 272.048004 |

Subtracting this kind's own baseline yields vertical inheritance 1.25 at both
plateaus, within recorded float precision. No physical birth/think clock is
needed for that subtraction. Generic absolute-velocity zeros are excluded.

Only the exact installed HE schema/item/header gate gains these three primary
air components. HE's ground modes and alternate air fallbacks, all other kinds,
spawn tracing, release timing, lifecycles and wire structures retain their
prior behavior. The catalog semantic salt advances to v22. Ordinary TickInput
regressions exercise ascending primary releases at 64/128 Hz, independent
missing-component fallbacks and preservation of the other HE modes, alongside
the content-bound loader and measured constant-VZ checks.

Private packet: `overpass-20261019/native-he-air-primary-oct20-r1`. All seven
sessions closed normally and reported twelve exact preference restorations
each. Original captures/logs, hashes and root numeric review records stay private.


### October 20: Flashbang airborne primary response

Seven separate owned Legacy 1.38.8.1 sessions, requesting 128 Hz, independently
qualify Flashbang's primary airborne angular/horizontal/vertical components.
Root reviewed each session's held weapon, before/after player, and three actual
projectile views. Held models are `v_eq_flashbang`, `w_eq_flashbang`, and
`w_eq_flashbang_dropped`, pin pulled and strength 1. The projectile is the
matching dropped model (index 252 in these sessions), client class
`C_BaseCSGrenadeProjectile`. Player angles match the requested pitch/yaw,
standing eye offset is 64.062561, and view/aim punch remain zero. Jump cases
have NULL ground support before/after release; the separate ground control
remains World-supported. All three projectile samples are pre-contact.

| Primary case | Projectile network VX | Received network VZ, first sample |
| --- | ---: | ---: |
| Air, pitch 0 | 664.745239 | 249.548004 |
| Air, pitch -30 | 541.433167 | 540.730042 |
| Air, pitch +45 | 517.080017 | -296.233643 |
| Air, forward, pitch 0 | 970.995239 | 244.548004 |

The 0/-30 horizontal fit independently gives speed 675 and the existing
pitch-weighted 10-degree upward adjustment. The +45 holdout agrees without
refitting (maximum horizontal residual below 0.000018 units/second).
The moving case has player network VX 245 before and after release, so the
306.25 projectile VX increase independently admits horizontal inheritance 1.25.
Normal-gravity received VZ is not construction-time VZ and does not fit a
physical spawn delay.

Three further sessions read back `sv_gravity=0` after establishing ground or
ascending movement. Before/after player VZ and all three projectile network
velocities stay constant in each control:

| Control | Player VZ before/after | Projectile VZ |
| --- | ---: | ---: |
| Grounded zero-gravity baseline | 0 | 117.212517 |
| Air, early zero-gravity plateau | 223.868378 | 397.048004 |
| Air, later zero-gravity plateau | 130.118378 | 279.860504 |

Subtracting Flashbang's own baseline gives vertical inheritance 1.25 at both
plateaus within recorded float precision. The later plateau differs from HE's
own session; the actual Flashbang value is used, not the requested delay or
another grenade's player velocity. Generic absolute-velocity zeros are excluded.

Only the exact installed Flashbang schema/item/model gate gains the three
primary-air components. Ground modes, alternate-air fallbacks, release timing,
spawn tracing and lifecycles retain prior behavior. Actual TickInput fixtures
cover 64/128 Hz ordinary jump releases, stationary/running controls, independently
missing components, and preservation of the other modes. Constant-VZ fixtures
use this kind's own observations. Native observations do not establish runtime implementation or full historical
parity by themselves; source fixtures exercise the separately measured components.

Private packet: `overpass-20261019/native-flashbang-air-primary-oct20-r1`.
All seven sessions closed normally and reported twelve exact native preference
restorations each. Original captures/logs, hashes and per-case root review records
remain private.


### October 20: Decoy airborne primary response

Seven separate owned Legacy 1.38.8.1 sessions at requested 128 Hz independently
qualify Decoy primary-air components. Root reviewed each held weapon, both
player views and three actual projectile samples. Held `v_eq_decoy`,
`w_eq_decoy` and `w_eq_decoy_dropped` models show pin pulled and strength 1;
the observed projectile is `C_DecoyProjectile`, dropped model index 253 in
these sessions. Actual pitch/yaw, standing eye offset 64.062561 and zero punch
are retained. Jump players are NULL-supported before/after; the separate
zero-gravity ground control remains World-supported. All projectile samples
remain pre-contact.

| Primary case | Projectile network VX | Received network VZ, first sample |
| --- | ---: | ---: |
| Air, pitch 0 | 664.745239 | 254.860504 |
| Air, pitch -30 | 541.433167 | 540.730042 |
| Air, pitch +45 | 517.080017 | -296.233643 |
| Air, forward, pitch 0 | 970.995239 | 244.548004 |

Decoy's own 0/-30 horizontal fit gives speed 675 and the pitch-weighted
10-degree upward adjustment, with the independent +45 holdout agreeing below
0.000018 units/second residual. The forward player network VX is 245 before
and after release; the 306.25 projectile VX increase admits horizontal
inheritance 1.25. These values consistently use the network `m_vecVelocity`:
Decoy's generic absolute-velocity field can differ, so the two fields are not
mixed. Normal-gravity received VZ does not establish construction-time velocity.

The three further sessions read back `sv_gravity=0`. Player network VZ before
and after release and all three projectile velocities are constant per case:

| Control | Player VZ before/after | Projectile VZ |
| --- | ---: | ---: |
| Grounded zero-gravity baseline | 0 | 117.212517 |
| Air, early zero-gravity plateau | 223.868378 | 397.048004 |
| Air, later zero-gravity plateau | 123.868378 | 272.048004 |

Subtracting Decoy's own baseline admits vertical inheritance 1.25 at both
plateaus within recorded precision. No physical spawn delay is inferred.
Only Decoy's exact installed schema/item/model gate gains primary-air metadata;
its ground modes, alternate air fallbacks and existing lifecycle remain intact.
Actual TickInput component/fallback/preservation fixtures cover 64/128 Hz, and
the constant-VZ fixture retains this kind's own measured controls. Source fixtures retain the independent measurement boundaries.

Private packet: `overpass-20261019/native-decoy-air-primary-oct20-r1`.
All seven sessions closed normally with twelve exact native preference
restorations each. Raw images/logs and root numeric/hash reviews remain private;
no full trajectory, later activation law or historical-2015 parity is claimed.


### October 20: Molotov airborne primary response

Seven separate owned Legacy 1.38.8.1 sessions at requested 128 Hz independently
qualify Molotov primary-air components. Root reviewed each held weapon, both
player views and three actual projectile samples. Held `v_eq_molotov`,
`w_eq_molotov` and `w_eq_molotov_dropped` models show the generic pin-pulled
field true and throw strength 1. The actual projectile is `C_MolotovProjectile`,
matching dropped model index 248 in these sessions. Player pitch/yaw and eye
offset 64.062561 match the requested standing controls; punch is zero. Jump
players have NULL support before/after; the zero-gravity ground control remains
World-supported. The projectile samples are pre-contact.

| Primary case | Projectile network VX | Received network VZ, first sample |
| --- | ---: | ---: |
| Air, pitch 0 | 664.745239 | 254.860504 |
| Air, pitch -30 | 541.433167 | 540.730042 |
| Air, pitch +45 | 517.080017 | -296.233643 |
| Air, forward, pitch 0 | 970.995239 | 244.548004 |

Molotov's own 0/-30 horizontal fit gives speed 675 and the pitch-weighted
10-degree upward adjustment. Its independent +45 holdout agrees within
0.000018 units/second. Player network VX is 245 before and after the forward
release, so the projectile VX increase of 306.25 admits horizontal inheritance
1.25. Normal-gravity received VZ is not construction-time VZ; generic absolute
velocity zeros are not substituted for the moving network velocity.

Three separate sessions read back `sv_gravity=0`, with constant player network
VZ before/after and three constant projectile velocities in each case:

| Control | Player VZ before/after | Projectile VZ |
| --- | ---: | ---: |
| Grounded zero-gravity baseline | 0 | 117.212517 |
| Air, early zero-gravity plateau | 223.868378 | 397.048004 |
| Air, later zero-gravity plateau | 123.868378 | 272.048004 |

Subtracting Molotov's own baseline admits vertical inheritance 1.25 at both
plateaus within recorded float precision, without inferring a physical birth
clock. Only the exact installed Molotov schema/item/model gate gains primary-air
metadata. Ground buttons, alternate air fallbacks, raw spawn tracing, release
timing, fire contact/expiry and wire layouts stay unchanged. The existing
TickInput component/fallback/preservation matrix includes this kind at 64/128 Hz;
its constant-VZ fixture uses the own-kind observations above. The source fixtures retain the independent measurement boundaries.

Private packet: `overpass-20261019/native-molotov-air-primary-oct20-r1`.
All seven sessions closed normally with twelve exact native preference
restorations each. Raw assets/captures/logs and root numeric/hash reviews stay
private. These launch observations do not qualify a different contact/expiry
law, full visual presentation or historical-2015 parity.


### October 20: Incendiary airborne primary response

Seven separate owned Legacy 1.38.8.1 sessions at requested 128 Hz independently
qualify Incendiary primary-air components. Root reviewed each held weapon,
before/after player and three actual projectile views. The held models are
`v_eq_incendiarygrenade`, `w_eq_incendiarygrenade` and their matching dropped
model, pin-pulled field true and strength 1. The projectile is
`C_MolotovProjectile`, matching dropped model index 249 in these sessions.
Player angles, standing eye offset 64.062561 and zero punch match the controls;
jump players have NULL support and the separate ground control remains
World-supported. All projectile samples remain pre-contact.

| Primary case | Projectile network VX | Received network VZ, first sample |
| --- | ---: | ---: |
| Air, pitch 0 | 664.745239 | 254.860504 |
| Air, pitch -30 | 541.433167 | 540.730042 |
| Air, pitch +45 | 517.080017 | -304.046143 |
| Air, forward, pitch 0 | 970.995239 | 252.360504 |

The independent 0/-30 horizontal fit gives speed 675 and the pitch-weighted
10-degree upward adjustment. The +45 holdout agrees within 0.000018
units/second. Player network VX is 245 before/after the forward release;
the 306.25 projectile VX increase admits horizontal inheritance 1.25.
Normal-gravity player VZ changes during acquisition, and its received projectile
VZ values are not initial VZ or a physical spawn-clock fit.

Three separate controls read back `sv_gravity=0` and retain constant player
VZ before/after and three constant projectile network velocities per case:

| Control | Player VZ before/after | Projectile VZ |
| --- | ---: | ---: |
| Grounded zero-gravity baseline | 0 | 117.212517 |
| Air, early zero-gravity plateau | 223.868378 | 397.048004 |
| Air, later zero-gravity plateau | 123.868378 | 272.048004 |

Subtracting Incendiary's own baseline admits vertical inheritance 1.25 at both
plateaus within recorded float precision. Generic absolute-velocity zeros are
excluded. These observations only qualify the exact installed Incendiary
schema/item/model's primary-air metadata; ground buttons, alternate-air
fallbacks, release timing, spawn tracing and fire contact/expiry are preserved.
The catalog semantic salt advances to v23 for the four newly qualified
primary-air profiles. Actual TickInput fixtures cover 64/128 Hz with independent
component fallbacks and preservation controls, alongside the own-kind measured
constant-VZ fixtures and exact-identity installed loader checks.

Private packet: `overpass-20261019/native-incgrenade-air-primary-oct20-r1`.
All seven sessions closed normally and restored twelve exact native preference
snapshots each. Original captures/logs, hashes and root numeric reviews remain
private. No new air-expiry/contact law, full-trajectory/visual or historical-2015
parity is inferred from these early-flight observations.


## HE secondary and combined jump-throw response (October 20)

Fourteen separate owned Legacy 1.38.8.1 sessions qualify the HE alternate-air
components: seven for short right-click throws and seven for both-button throws.
Each mode independently supplies level and -30-degree angular measurements, a
+45-degree holdout, steady-forward movement, and ground/early-air/later-air
zero-gravity controls. Original held, player and three projectile dumps were
personally reviewed; every session closed with all twelve native preference
snapshots restored. Raw captures remain private.

The held object is the actual HE with `v_eq_fraggrenade`,
`w_eq_fraggrenade` and dropped model251, with pin pulled. Actual strength is0
for secondary and0.5 for combined. Player angles match the requested samples,
yaw/punch are zero, standing eye offset is64.062561 and the player is airborne
before and after each jump release. The emitted object is the unique returned
`C_BaseCSGrenadeProjectile`, owned/thrown by that player, with model251 and
NULL ground at every sampled precontact vector. Use its network velocity;
the generic absolute-velocity dump is zero and supplies no launch estimate.

| Actual pitch / movement | HE secondary VX | HE combined VX |
|---|---:|---:|
| 0, stationary |199.423584|432.084351|
| -30, stationary |162.429962|351.931519|
| +45, stationary holdout |155.124008|336.101959|
| 0, steady forward |505.673584|738.334351|

Each value remains constant across all three projectile samples. The running
player independently reads245 units/s before and after release in both modes;
each moving-minus-stationary difference is306.25, supporting horizontal
inheritance1.25. Normal-gravity received VZ is deliberately not treated as
construction VZ or used to fit an unknown network, birth or think age.

The level/-30 fits independently recover speed202.500011 and bias9.999996 for
secondary, speed438.749942 and bias9.999995 for combined. Applying those fits
to the untouched +45 holdouts leaves absolute VX residuals below0.000016.
Thus each mode admits its own speed202.5 or438.75 and weighted upward pitch
bias10, rather than borrowing an airborne response from HE primary or Smoke.

Actual gravity0 is read back for the vertical controls. Each airborne player
has equal network VZ before/after release and every projectile has constant
VZ across its three samples:

| Mode | Actual player support/VZ | Actual projectile VZ |
|---|---|---:|
| Secondary |World,0|35.163757|
| Secondary |NULL,223.868378|314.999237|
| Secondary |NULL,123.868378|189.999237|
| Combined |World,0|76.188133|
| Combined |NULL,217.618378|348.211121|
| Combined |NULL,123.868378|231.023621|

Subtracting each mode's own ground baseline and dividing by its own airborne
player VZ independently yields1.25 within float/display precision at both
plateaus. In particular, the early combined player VZ is217.618378, not the
secondary session's223.868378. These controls qualify vertical inheritance;
no normal-gravity clock correction is needed.

Only the existing exact schema/item/MDL gate for HE receives these six optional
alternate-air components. Grounded modes, primary air, other grenade kinds,
release timing, trace placement, collision/gravity and lifecycle stay intact.
The existing content digest advances to v24; serialization remains unchanged.
Actual TickInput coverage at64/128Hz extends the existing alternate-air
component/fallback and grounded/primary preservation cases to HE, alongside
Smoke. Separate HE fixtures retain its own constant-VZ rows and forward pair.
Other kinds' missing alternate-air components still use the prior fallback;
these observations do not admit them, contact/expiry changes or historical
lineup/full-trajectory parity.
