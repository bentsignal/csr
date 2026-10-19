# Grounded Smoke launch observations

## Initial horizontal-inheritance admission

Two independently admitted grounded primary Smoke throws in installed CS:GO
Legacy 1.38.8.1 at 128 ticks measured the horizontal contribution separately
from HE and Molotov:

| Case | Player network velocity | Projectile network velocity X |
| --- | --- | --- |
| Stationary | `[0,0,0]` | `664.745239` |
| Forward | `[245,0,0]` | `970.995239` |

All five player velocity brackets per case retain those vectors, real world
ground, standing eye and zero view/punch. Five projectile images contain four
distinct pre-contact states with the same printed horizontal velocity, a NULL
projectile ground handle and coherent airborne positions. Empty pre-release
lookups are followed by one fresh `C_SmokeGrenadeProjectile`, index150; server
metadata independently identifies `smokegrenade_projectile`. Current held
weapon138, viewmodel130, active weapon, pin/primary strength1 and matching
weapon/viewmodel owner plus projectile owner/thrower were personally reviewed
by both the observing agent and the orchestrator.

The moving-minus-stationary difference is `306.25`. The preregistered 0.5
candidate predicts `787.245239`, missing by `183.75`; the 1.25 candidate
predicts the observed `970.995239` exactly within the declared 0.002 tolerance.
This difference does not infer gravity age or physical launch timestamp.

Private packet:
`measured-recoil-20261015/public-smokegrenade-ground-primary-selected-inheritance-20261016-110448`.
The two-case process receipt records35 images, no failure, exact restoration
of12 preference files and no remaining native process. Agent transcription
`posthoc-agent-smoke-horizontal-inheritance.json` records all numeric rows and
original image hashes; `posthoc-root-smoke-horizontal-inheritance.json` records
the independent root review of26 original images. Actual 32-bit module paths,
public runtime version and freshly read schema/model identity were separately
admitted before continuing.

The earlier packet ending105024 is preserved as a negative: a retained Smoke
weapon entity existed, but the second case actually had the P2000 active and
created no Smoke projectile. The corrected bounded packet requests selection
after settling and requires a fresh root-owned player/viewmodel matching-handle
admission before attack or movement in each case. No partial stationary value
from the failed packet substitutes for the new baseline. Raw receipt admission
flags remain unchanged; admission is a separate posthoc record.

The initial content-gated profile changed only grounded primary horizontal inheritance
to1.25, under exact items schema SHA1
`c1361d351802b7be5f55245c331f5f1d6b36ad35` and Smoke MDL-header SHA1
`43e9d797e8c9e219b1917a3a608ba1a091c0c4cb`. It uses the existing release
calculation without modifying its code. Base speed/pitch, vertical inheritance,
airborne throws, underhand/combined modes, raw aim origin, throw timing and the
wire format retain their prior behavior. At that stage no stationary velocity profile was assigned. Crouch, other movement directions/speeds, complete trajectories and
historical2015 parity remain unmeasured. Changed content cannot borrow this
measurement. The semantic catalog salt advances once to v11.

The existing Molotov `World`/`TickInput` differential witness now also throws
Smoke through ordinary hold/release at64 and128 ticks, checks the independently
observed horizontal delta and compares complete unsupported mode/air releases
against their prior behavior. Identity-gate and installed-content coverage are
extended in place. Source parsing and diff checks pass; Cargo and installed
verification await the orchestrator's combined resource lease.
## October 19 primary launch velocity correction

New independent standing Smoke observations at -30 and +45 degrees supplement
the original level case. Both angular cases use fresh held Smoke, primary-only
input, literal pin-pulled/strength-1 fields and matching held-owner/projectile
thrower. Actual `C_SmokeGrenadeProjectile::m_vecVelocity` is used; the generic
zero absolute-velocity field is excluded. The +45 case admits its first
pre-contact row only.

| Pitch | Original pre-contact horizontal velocity X |
| --- | --- |
| 0 | 664.745239 |
| -30 | 541.433167 |
| +45, independent check | 517.080017 |

The existing weighted-pitch family uses
`adjusted_pitch = pitch - bias * (1 - abs(pitch) / 90)`.
Fitting the level and -30 horizontal observations gives speed 675 and bias 10
degrees to printed precision; the separate +45 observation agrees without
refitting. This supports using that family for the admitted grounded primary
Smoke definition, independently of the HE profile. A finite sample does not
uniquely determine every possible throw law.

The exact schema/model gates above remain. Only grounded primary Smoke gains
this velocity profile; other buttons, airborne throws, other grenade kinds and
unmatched content retain their prior definitions. Horizontal movement
inheritance remains 1.25 and vertical movement inheritance remains 0.5. The raw
aim spawn trace, release timing, gravity, contact and lifecycle are unchanged.
The catalog identity advances to v13 to bind the corrected response.

Initial vertical velocity and physical spawn time were not directly measured.
The predicted vertical component is checked only for compatibility with the
received values and an independent command-release-to-capture gravity bound;
no received value is treated as a construction-time value. Historical 2015
identity, full trajectories, bounces and smoke lineup parity remain open.

The existing ordinary TickInput hold/release fixture now checks level Smoke
stationary/moving velocities at 64/128 Hz, preserving Molotov and unsupported
releases. A separate regression checks the three Smoke angles, unchanged spawn
trace, movement inheritance, absent calibration and unsupported modes. The
64 Hz cases are regression consistency checks rather than new native evidence.

## October 19 stationary secondary/combined velocity extension

Six fresh original Legacy 1.38.8.1 sessions on Train at requested 128 Hz and
confirmed `host_timescale .01` independently qualify the secondary and combined
Smoke base vectors. In every case the player is standing on world ground with
zero movement, standing eye offset `64.062561`, and actual pitch 0, -30 or +45
(yaw/roll zero). Root personally reviewed the full player-before-hold,
held-weapon and projectile-0 originals and nearest-neighbor held-field crops.
The active Smoke weapon and matching held-owner/projectile-thrower handles,
literal pin-pulled `true`, and held strength `0` for secondary or `0.5` for
combined establish the named modes. Secondary used only `+attack2`; combined
held `+attack` and `+attack2` together, then released both.

Only each case's first pre-contact projectile frame is admitted. The measured
field is `C_SmokeGrenadeProjectile` index 150 `m_vecVelocity`; the generic zero
absolute-velocity field is excluded. Later projectile frames are not admitted
for this extension.

| Mode | Actual pitch | First pre-contact velocity X | Received Vz, compatibility only | Release-to-later-dump upper bound, seconds |
| --- | --- | --- | --- | --- |
| Secondary | 0 | 199.423584 | 10.163757 | 0.251998098 |
| Secondary | -30 | 162.429962 | 95.924629 | 0.245490531 |
| Secondary | +45 | 155.124008 | -152.664490 | 0.244251862 |
| Combined | 0 | 432.084351 | 51.188133 | 0.245796353 |
| Combined | -30 | 351.931519 | 237.003326 | 0.247952960 |
| Combined | +45 | 336.101959 | -304.523041 | 0.242968991 |

Fitting 0/-30 independently for each mode within the existing weighted-pitch
family gives secondary speed `202.50001144765758`, bias
`9.999995533351424` degrees, and combined speed `438.749942113388`, bias
`9.99999520725116` degrees. Their separate +45 checks without refitting have
horizontal residuals about `-0.000004573` and `-0.000015721` units/second.
These parameters were **not prospectively preregistered**. The bounded profile
uses speeds `202.5` and `438.75` with bias `10` degrees, supported by these
Smoke observations independently of HE. The finite samples do not uniquely
identify every possible throw law.

Neither initial Vz nor physical spawn time was measured. Received Vz is only
a gravity compatibility bracket for the vertical component constructed by the
existing family. Each first screenshot precedes its later server classname/
gravity dump; logged receive time minus command release, scaled by the
confirmed .01 clock, gives the upper bounds above without subtracting an
unknown physical spawn delay. Use `320 * per_case_bound` for the gravity-loss
ceiling; every bound is below .3 seconds, so 96 units/second is conservative.
The old blanket 80 units/second ceiling must not be reused: secondary level's
upper bound exceeds .25 seconds. None of these bounds is an actual frame age.

Private evidence remains in
`overpass-20261019/native-smoke-buttons-r1/root-admission.json`, its six case
subfolders' `root-review.json`, original captures and console logs. The job
closed with exit 0, all six cases closed and all 12 preference files restored
per case; no owner files changed. Original images and logs remain private.

The exact schema and Smoke model-header gates above are unchanged. The two
grounded velocity fields are added through the existing definition seam, and
the catalog identity advances to v14. Primary Smoke remains unchanged, as do
HE, Molotov and other grenade definitions. These stationary observations alone
do not admit a moving response; the independent follow-up below supplies that
measurement. Vertical movement inheritance stays 0.5. Airborne/no-mode throws, unmatched packs, the
raw spawn trace, timings, gravity, contact and lifecycle are preserved.

The existing ordinary TickInput fixture is extended to stationary secondary/
combined Smoke at all three angles and 64/128 Hz, with separate movement
checks. Numeric release regressions use only
the admitted first rows and their individual compatibility bounds, preserving
raw spawn positions and the air/no-mode/absent-profile fallbacks. The older
unsupported-mode full-vector comparisons now distinguish the newly admitted
base vectors from movement response. 64 Hz remains regression coverage, not new
native evidence. Historical 2015 identity, full lineups, jump and bounce parity
remain unestablished.

## Independent steady-forward secondary/combined response

Two further fresh Legacy sessions use the same standing, level, grounded setup
and literal held strengths. Before and after each throw, the actual player's
velocity is `[245,0,0]`, with world ground, standing eye and the same active
Smoke. Forward input remains held across the interval in an open corridor.
Root personally reviewed both player brackets, the held weapon and the first
pre-contact projectile frame; weapon owner and projectile thrower match.

| Mode | Independent stationary VX | Steady-forward projectile VX | Difference |
| --- | --- | --- | --- |
| Secondary | 199.423584 | 505.673584 | 306.25 |
| Combined | 432.084351 | 738.334351 | 306.25 |

For each named mode, `306.25 / 245 = 1.25`. The previous 0.5 response would
predict 321.923584 and 554.584351, missing both by 183.75 units/second. These
pairs independently support the two optional grounded horizontal-inheritance
fields, which now use 1.25. The source's existing horizontal scalar response is
retained; other directions/speeds are not newly observed here. Vertical movement
inheritance is unchanged and uncalibrated by these horizontal differences.

The earlier moving attempt is retained as a negative: its before velocity was
225.665039 and after velocity 245, so it cannot establish the exact release-tick
coefficient. It is excluded from the point calibration. The repeat waits for the
equal before/after plateau rather than estimating a release velocity from that
acceleration interval.

Private evidence is `overpass-20261019/native-smoke-moving-r3`, with per-case
root reviews and the aggregate admission; the accelerating negative is `r2`.
Both repeat cases closed successfully, restored all 12 preference files and
removed only their 224 new session byproducts. Raw captures remain private.
The existing TickInput regression checks the observed moving VX as well as the
stationary values at 64/128 Hz and preserves airborne and other-definition
behavior. The single unshipped v14 identity is named for the complete grounded
button response. No initial vertical velocity, physical spawn timestamp,
historical lineup, arbitrary movement or bounce parity is inferred.


## Airborne primary response (October 19 follow-up)

Independent Legacy 1.38.8.1 sessions on Train requested 128 ticks/second,
confirmed host_timescale 0.01, and used ordinary primary input. Root personally
reviewed the held pin/strength (true/1), actual player ground/angle/velocity
brackets and first pre-contact C_SmokeGrenadeProjectile in every case. Held
weapon owner and projectile thrower match; generic zero absolute-velocity
metadata is excluded. Normal-gravity cases have NULL ground before and after,
standing eye offset 64.062561 and the same active Smoke throughout.

| Pitch | Stationary airborne projectile VX |
| --- | --- |
| 0 | 664.745239 |
| -30 | 541.433167 |
| +45 | 517.080017 |

Fitting the level and -30 observations within the existing weighted-pitch
family independently gives speed 675.000001 and bias 9.999998 degrees. The
separate +45 case differs from that prediction by less than 0.00003. This is
finite evidence for speed 675 and bias 10 in that family, not a unique general
law or a prospectively preregistered experiment. A separate forward-jump case
has player VX 245 before and after and projectile VX 970.995239. Its difference
from stationary is 306.25, independently supporting horizontal inheritance
1.25. Normal-gravity player VZ changes across the bracket; it does not identify
exact release velocity, initial projectile VZ or physical spawn time.

To separate vertical inheritance from those unknowns, three fresh sessions
read back sv_gravity=0: a grounded stationary control and two jumps where
gravity is disabled after jump onset. Before and after each airborne throw,
the player's VZ remains on the same measured plateau. These controls remove
flight-age gravity loss without fitting an unknown timestamp.

| Control | Player VZ before and after | Projectile VZ |
| --- | --- | --- |
| Grounded | 0 | 117.212517 |
| Earlier ascending plateau | 217.618378 | 389.235504 |
| Later ascending plateau | 123.868378 | 272.048004 |

The two airborne rows give (389.235504-272.048004)/(217.618378-123.868378)
= 1.25. Their independently derived intercept is 117.2125315, within 0.000015
of the grounded control. This admits vertical inheritance 1.25 for primary
Smoke in the existing scalar response. Normal-gravity observations are
compatibility checks, not exact-age or initial-VZ fits. Production gravity
remains unchanged; the zero-gravity experiment is a calibration control only.

The exact schema/model gate now supplies these three independently qualified
air components for Smoke only. Catalog semantics advance once to v15.
Grounded button modes, other grenade definitions, airborne secondary/combined,
no-mode calls, spawn tracing, release timing, integration and contact remain
unchanged. Optional components preserve the prior behavior independently when
absent, including content-pack definitions. Existing bot planning does not
assume this is a grounded path.

Ordinary TickInput regressions cover standing/running jump release at 64/128 Hz,
three angles and independent absent-component fallbacks; separate numeric
regressions anchor the three constant-VZ native controls. 64 Hz is regression
coverage, not a new native observation. Arbitrary movement/directions,
descending or crouched throws, VR motion, physical spawn position/time,
complete trajectories/contact and historical 2015 lineups remain unqualified.

Private evidence: native-smoke-jump-pilot-r1 and
native-smoke-jump-cases-r1/root-admission.json under overpass-20261019. The pilot
and all six follow-ups closed successfully and restored all 12 exact native
preference snapshots per session. Original captures/logs stay private. A pilot
player-VZ transcription typo was explicitly corrected in its root receipt;
original evidence was not modified.
