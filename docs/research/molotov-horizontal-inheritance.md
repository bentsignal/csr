# Grounded primary Molotov movement inheritance

The previous primary Molotov throw inherited half the player's horizontal
velocity. Two ordinary, grounded primary throws in installed CS:GO Legacy
1.38.8.1 at 128 ticks independently observed a different contribution:

| Case | Player network velocity | Projectile network velocity X |
| --- | --- | --- |
| Stationary | `[0,0,0]` | `664.745239` |
| Forward | `[245,0,0]` | `970.995239` |

The observed difference is `306.25`, or `1.25` times the independently
bracketed player velocity. The preregistered half-speed candidate predicts
`787.245239`, missing by `183.75`; the 1.25 candidate matches the printed
moving value exactly. This comparison does not depend on projectile gravity
age or a presumed physical spawn timestamp.

Five numeric images per throw contain four distinct pre-contact states.
Each fresh public lookup identified one `C_MolotovProjectile`, index150,
following an empty pre-release projectile lookup. Actual server metadata
identified `molotov_projectile`. Held weapon138 and viewmodel130 ownership,
the player's active weapon, primary button/pin/strength1, the projectile's
thrower, world ground, zero view/punch and standing eye were reviewed.
All five player release brackets retained stationary zero velocity or forward
`[245,0,0]`; the sampled projectile remained airborne with coherent positions.
Generic/interpolated velocity fields are not substituted for network vectors.

Private packet:
`measured-recoil-20261015/public-molotov-ground-primary-inheritance-20261016-084332`.
Its two-case process receipt reports33 images, no failure, exact restoration
of12 preference files and no remaining native process. Separate agent
transcription records the original image hashes and numeric/ownership fields;
raw receipt admission flags remain unchanged. Root review is independent.

Only the grounded primary horizontal inheritance is imported under the
existing exact selected-items-schema and Molotov model-header gates. No
stationary velocity profile is assigned: the existing base speed/pitch still
differs from the native stationary endpoint above. Vertical inheritance,
airborne throws, underhand/combined modes, raw aim origin and physical release
timing retain their previous behavior. Other grenades and changed content
identities cannot borrow this measurement. The catalog policy salt advances;
the wire format does not change. Historical2015 and complete trajectory
parity remain unmeasured.

The focused regression drives actual hold/release `TickInput` through a
grounded `World`, checks the independent native moving-minus-stationary delta,
and compares unsupported button/air scopes against their complete prior
release. Cargo and installed-content checks await the orchestrator's combined
verification; source parse and diff checks passed before opening the PR.
