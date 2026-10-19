# Grounded primary Smoke launch observations

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
