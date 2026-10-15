# Recoil: dated data and the measurement gap

The owner requested a **2015 gameplay baseline**. This audit identifies dated
weapon configuration and separates it from the still unverified recoil
procedure. It does not change weapon behavior or establish a pixel-perfect
spray pattern. The installed reference executable is CS:GO Legacy 1.38.8.1
(2023), not a runnable 2015 build. Existing classic menu work remains separate.

## Historical source identity

The [2015-12-31 shipped-data capture](https://github.com/SteamTracking/GameTracking/commit/fcb66c41571b716820b1e1c148e0cba9a318f772)
is a third-party SteamTracking archive of game data, not official Valve
hosting. Its `steam.inf` declares patch **1.35.1.6**, client/server version
**254**. Those declarations identify the captured data; they do not supply
the corresponding native executable or prove an engine algorithm.

Files were obtained by exact Git blob identity and independently checked
against Git's blob header and byte hash. Raw files stay outside this repo.
The following are derived configuration facts, not a copied impulse table.

| Definition | Seed | Angle | Angle variance | Magnitude | Magnitude variance | Crouch recovery | Stand recovery |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| AK-47 | 223 | 0 | 70 | 30 | 0 | 0.381571 | 0.460000 |
| M4A4 (`weapon_m4a1`) | 38965 | 0 | 70 | 23 | 0 | 0.302625 | 0.423676 |
| M4A1-S normal fields | 38965 | 0 | 65 | 25 | 3 | 0.302625 | 0.423676 |
| M4A1-S alt fields | 38965 | 0 | 65 | 21 | 0 | same recovery fields | same recovery fields |

AK/M4A4 values come from the standalone shipped `WeaponData` files; M4A1-S
values resolve from the item schema. The normal/alt labels are data field
names: the schema alone does not prove their mapping to suppressor state.
Sources: [AK-47 data](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/scripts/weapon_ak47.txt),
[M4A4 data](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/scripts/weapon_m4a1.txt),
[item schema](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/scripts/items/items_game.txt).

This is one late-2015 snapshot, not proof that every 2015 update behaved
identically. Valve's [December 15 recovery article](https://blog.counter-strike.net/2015/12/13394/)
records a rollback of rifle/pistol changes to their pre-Winter Update state.
Later, the [August 2016 Second Shot article](https://blog.counter-strike.net/2016/08/15304/)
introduced a different recovery method. Valve's [August 5, 2016 notes](https://blog.counter-strike.net/2016/08/15381/)
also explicitly introduced recoil reset on reload. Modern behavior must
not be labeled 2015 merely because it is observed in the installed game.

## Installed data and current code

The installed 2023 schema retains the five listed seed/angle/variance/
magnitude inputs for AK/M4A4. Its recovery values differ:

| Definition | Crouch initial | Crouch final | Stand initial | Stand final |
| --- | ---: | ---: | ---: | ---: |
| AK-47 | 0.305257 | 0.419728 | 0.368000 | 0.506000 |
| M4A4 / M4A1-S | 0.242100 | 0.332888 | 0.338941 | 0.466044 |

The installed transition fields specify bullets 2 through 5. The old
`recovery time stand` / `recovery time crouch` keys are present and nonzero;
they hold the initial values, rather than being replaced by the final keys.
A source-only audit mirrored the repository's repeated-table merge and
prefab traversal and found these values in all three base definitions.
Dust2's embedded pak does not shadow the catalog; the inspected installed
VPK index has no `items_game.txt` entry, so this input is the loose schema.
This audit is not a new executed Rust catalog test.

`weapons::WeaponDefinition.attributes` already preserves the final and
transition fields. Its typed `Accuracy` fields lift the old keys, and
`sim::weapon::think` uses those initial values with a minimum of 0.05.
Thus these rifles do **not** demonstrate a missing-key fallback to 0.05;
the unimplemented distinction is the native recovery procedure.

For weapons without an admitted observed trajectory,
`sim::weapon::recoil_pattern` still uses the project's own RNG seeded
by seed plus definition ID, independent angle/magnitude sampling, a
first-three-shot angle multiplier of 0.25, conversion of 0.02 degrees per
unit, and accumulated offsets. Recovery moves backwards through that
pattern at 12 steps per second. These choices have not been validated by
this historical data. The pattern cache now selects the declared alternate
recoil fields in burst mode; that selects data, not a measured procedure.
Matching seed values and deterministic tests do not
establish the original sampler, filtering, impulse decay or trajectory.

The held-automatic-trigger cooldown correction prevents recovery between
burst shots. It fixes an internal semantic discontinuity without changing
these numeric choices. Current code has no explicit reload recoil reset;
recovery during a reload can naturally bring its index to zero. That must
be measured separately from an explicit reset policy.

## Next measurement boundary

The installed declarative `modevents.res` exposes `bullet_impact` with float
coordinates and `weapon_fire` with weapon and silenced fields. This supplies
a possible observation surface; the current harness does not yet prove it
receives those events. Event coordinates also do not establish complete
ray origin, camera punch, or a historical event schema.

1. Use an owned native session, with the
   [userdata preservation boundary](FIDELITY.md), fixed position/view,
   verified weapon/ammo/mode, acknowledged settings and executable identity.
2. Prove a receiver using public demo/event format documentation or a
   documented runtime observer. Record actual shot ticks and ammo changes;
   console-command receipt is not an exact shot timestamp. Keep the work
   clean-room, with no Source SDK or leaked/decompiled implementation.
3. Measure no-spread bullet rays separately from ordinary spread, and
   camera/viewmodel motion separately from bullet direction. Verify the
   origin/punch observation surface rather than assume one exists.
4. Record tapping, bursts, full magazines, release, empty clip, reload,
   switching, stance and mode transitions. Fit the procedure against
   held-out traces at 64/128 ticks and prediction reconciliation.
5. Replace arbitrary choices only with measurement-constrained behavior
   or an explicitly provenance-bound profile tied to content identity and
   schema/mode. Publish derived observations, not game data or captures.

A 2023 trace can establish an installed-game profile. It cannot establish
2015 pixel-perfect recoil without calibrated historical observations or
matching native execution. Landing remains a separate measurement problem;
the completed 2023 reference session observed real landing slowdown.

## Bounded installed-game spray observation

The next observation first admits one real AK shot. Acknowledged settings
alone are insufficient: before/after captures must show the actual AK ammo
count changing and a new impact marker. `weapon_accuracy_nospread` and
`sv_showimpacts` must report their requested values before this shot.
`cl_pdump` is an attempted public prediction overlay, not an assumed punch
receiver. Earlier `net_showevents` attempts produced no shots or events and
are not calibration evidence.

Only after that admission can full-magazine capture proceed. Any change to
view-only recoil settings is recorded separately from the unchanged bullet
recoil scale. A fixed camera must be established from static scene features,
not inferred from a cvar name. Slow-time capture records image begin/end
intervals and actual ammo transitions; console receipt times are not exact
server shot ticks. Repeated magazines must have fresh, expired prior impact
overlays and a recovered, reloaded weapon.

A usable result needs derived angular ray observations and independent
repeat agreement before replacing the current generator. Native images,
console logs, settings, executable identity and account-bearing session
receipts stay outside the repository. Any resulting profile identifies the
installed 2023 reference and its limitations; unchanged historical recoil
configuration alone does not establish a 2015 trajectory.
## Admitted installed AK held-magazine profile

The first usable trajectory is now an **observed Legacy 1.38.8.1 (2023) AK
profile**, not a reproduction of the engine algorithm or proof of 2015
parity. It replaces the arbitrary generator only for the exact observed
item-schema identity and matching `csgo_weapon_ak47` statistics. Different
schemas, origins, weapon modes and modified statistics keep their existing
fallback. The catalog digest includes the profile policy and derived angles,
so old peers with identical assets cannot silently use a different trajectory.

The native admission positively established an actual AK **30 -> 29** ammo
change, a fresh impact, zero velocity, equal before/after eye positions and
stationary background. The working `cl_pdump 1` overlay exposed origin,
velocity, view offset and punch fields. Requested no-spread, bullet recoil
scale 2, view tracking/extra punch 0, FOV setting 90 and timescale were all
verified from runtime values. Two held, standing, 128-tick, no-spread
magazines at timescale 0.1 each produced 30 independently counted near-shot
sound starts and 30 shot-ordered HUD counts from 29 through 0. Fresh prior
impact overlays expired before each magazine. Sound receipt and capture
intervals locate observations; they are not exact server shot ticks.

Projection was independently calibrated with the same fixed native eye and
actual camera angles at yaw +/-10 degrees and pitch 5/15/25 degrees. Pure
rotation reprojection of 8,749 static masonry samples, without any BSP
geometry, fitted focal lengths **599.825 .. 600.050 pixels**. At 1600x900,
600 pixels corresponds to horizontal **106.260 degrees** and vertical
73.740 degrees. Treating the requested 90 as the actual widescreen horizontal
FOV would have produced the wrong angles. A clipped RGB residual favored
600 over 800 pixels in all four independent rotations.

Pixel centers are measured from the visible blue/red impact boxes with an
approximately **2-pixel uncertainty**, including partly overlapping early
and late shots. The independently repeated slow magazine's center differences
are at most 1.5 pixels (RMS 0.626 pixels). A separate normal-timescale magazine
consumed exactly 30 rounds and reproduced the aggregate marker cloud: derived
centers are within 2.1 pixels of its marker pixels. This last check establishes
aggregate shape at normal speed, not separately resolved normal-speed shot
order. Raw pixel observations remain available privately; decimal degree
rounding is not a claim of hundredth-degree measurement precision.

For each pixel `(x,y)`, the ray was reconstructed as
`normalize(forward + right*(x-800)/600 + up*(450-y)/600)` using the observed
15-degree native pitch. Pitch/yaw were then measured relative to the base
aim. The resulting offsets, in degrees, are:

| Shots | Pitch / yaw, in shot order |
| --- | --- |
| 1-5 | 0/0; -0.19/0; -1.19/0; -2.86/-0.20; -4.57/-0.15 |
| 6-10 | -6.18/0.48; -7.78/0.91; -8.81/1.66; -9.65/0.76; -9.37/-1.61 |
| 11-15 | -9.70/-2.74; -10.30/-2.07; -10.39/-2.92; -9.94/-4.29; -10.13/-4.43 |
| 16-20 | -10.44/-2.50; -10.85/-1.32; -11.13/-0.47; -10.99/0.94; -10.58/2.78 |
| 21-25 | -10.67/1.79; -10.85/2.02; -11.04/1.60; -11.31/1.13; -11.04/2.16 |
| 26-30 | -11.27/2.49; -11.54/1.50; -11.22/-0.42; -10.21/-2.83; -10.12/-3.58 |

The initial climb settles into the observed sideways trajectory instead of
continuing upward. `fire()` samples each bullet offset before incrementing
its index; the cache converts bullet angles to aim punch once, and the
ordinary authoritative ray path applies recoil scale once. The same loaded
profile is available after ContentId-based snapshot decoding and prediction.
The headless `recoilcheck` example emits actual no-spread authoritative rays
from the installed catalog at both supported tick rates.

**Still open:** release/recovery dynamics, tap and burst transitions, crouched
or moving trajectories, camera punch phase/decay, native 64-tick calibration,
M4/other weapons, later magazines without full recovery, and calibrated 2015
observations. Existing backwards-index recovery and camera interpolation are
not established as native behavior. After the last observed point, the cache
clamps rather than inventing extra magazine shots. The observed AK profile is
a concrete spray improvement within these limits, not completion of recoil
fidelity.

## Admitted installed M4A4 held-magazine profile

M4A4 has its own observed Legacy 1.38.8.1 standing profile, independently
admitted after the AK observation. Native pictures show the actual M4A4,
a 30 -> 29 single-shot decrement, a fresh impact and unchanged stationary
player/camera. Both slow magazines contain 30 independently counted near
shot starts and every observed HUD count 29 through 0. A separate normal
speed magazine consumed 30 rounds and reproduced the aggregate shape.

The new camera-rotation calibration fitted 599.825 .. 600.050 pixels over
8,658 static samples, with 599.900 .. 600.150 over 3,696 held-out samples.
The independently repeated magazine's marker-center differences are at
most 1.12 pixels (RMS 0.492); centers are within 2 pixels of the separate
normal-speed marker cloud. The measurement uncertainty remains 2 pixels.

Each pair below is bullet pitch/yaw offset in degrees, relative to fixed
aim. These are derived observations, not copied native implementation.
The exact observed schema, content identity, definition 16, clip 30,
cycle 0.09 and recoil-stat identity gate attachment. Other content and
unobserved modes retain their previous fallback. Catalog policy hashing
includes the reference and angles so peers using the old policy cannot
silently disagree.

| Shots | Pitch/yaw |
| --- | --- |
| 1-5 | -0/0; -0.24/0; -0.72/0.05; -1.67/-0.2; -2.86/0.1 |
| 6-10 | -4.19/-0.29; -5.43/-0.58; -6.32/0.29; -7.13/0.86; -7.22/2.01 |
| 11-15 | -7.69/1.76; -8.16/0.86; -8.02/-0.71; -7.93/-2.1; -7.51/-3.39 |
| 16-20 | -7.84/-3.48; -8.12/-3.04; -8.08/-3.52; -7.85/-4.28; -8.08/-4.23 |
| 21-25 | -8.16/-2.38; -8.39/-1.81; -8.44/-0.38; -8.62/0.05; -8.81/0.66 |
| 26-30 | -8.9/0.14; -9/0.47; -9.04/0.66; -9.18/0.76; -9.18/1.04 |

This still measures 2023 installed behavior, not 2015 parity. Recovery,
camera decay, moving/crouched trajectories and native 64-tick behavior
remain separate uncompleted measurements. M4A1-S attached/detached modes
require their own admitted trajectories.

## Admitted installed M4A1-S suppressor-mode profiles

The M4A1-S now has separate observed attached and detached trajectories.
Both derive solely from fresh full retakes requested after physical mouse
movement contaminated an earlier observation. The entire earlier run is
excluded; no apparently unaffected portion supplies a fitted point or a
held-out reference.

Each fresh mode independently admitted the actual visible weapon and
suppressor state, a 20 -> 19 ammo decrement, a new impact, zero velocity,
equal observed eye position and unchanged background. Two slow magazines
per mode show every actual HUD count 19 through 0 and 20 independently counted
near-shot starts. A separate normal-speed 20-round magazine per mode checks
aggregate shape. Settings verified the minimum sensitivity 0.0001 before
admission and each magazine. This reduces physical mouse influence; it does
not disable input. The original mouse yaw/pitch values were recorded and
all 12 saved preference files restored exactly.

Per-frame audits cover two disjoint regions outside the impact pattern.
Attached frames stay below 0.383 RGB mean absolute difference from the fixed
reference; detached frames below 0.728. Independent rotation calibration
again supports 600-pixel focal length at 1600x900. Attached fitted focal range
is 599.850 .. 600.075 pixels; its held-out region 599.900 .. 600.175. Detached
ranges are 599.850 .. 600.050 and 599.875 .. 600.250 respectively.

The second ordered magazine's independently measured centers differ by at
most 0.71 pixels attached (RMS 0.335) and 1.00 pixel detached (RMS 0.447). Both
normal-speed aggregate clouds are within 2 pixels of fitted centers. The
measurement uncertainty remains 2 pixels, including overlapping early and
late boxes; rounded angles do not imply finer native precision.

| Shot | Attached pitch/yaw | Detached pitch/yaw |
| --- | --- | --- |
| 1 | -0/0 | -0/0 |
| 2 | -0.14/0 | -0.24/0.05 |
| 3 | -0.33/0.05 | -0.86/0.05 |
| 4 | -1/-0.15 | -2.2/-0.24 |
| 5 | -2/0.05 | -3.24/0.1 |
| 6 | -2.96/-0.24 | -4.57/-0.29 |
| 7 | -4.1/-0.39 | -5.81/-0.58 |
| 8 | -4.76/0.29 | -6.75/0.38 |
| 9 | -5.24/0.68 | -7.5/0.91 |
| 10 | -5.34/1.59 | -7.69/2.24 |
| 11 | -5.76/1.25 | -8.02/1.81 |
| 12 | -6.09/0.48 | -8.44/0.76 |
| 13 | -5.9/-0.77 | -8.2/-0.81 |
| 14 | -5.81/-1.73 | -7.88/-2.1 |
| 15 | -5.44/-2.6 | -7.23/-3.2 |
| 16 | -5.81/-2.55 | -7.23/-2.96 |
| 17 | -6.14/-2.11 | -7.56/-2.62 |
| 18 | -6/-2.59 | -7.51/-3.1 |
| 19 | -5.82/-3.08 | -7.14/-3.73 |
| 20 | -5.91/-2.98 | -7.42/-3.63 |

Attachment requires the exact observed schema and `m4a1_silencer` content
identity, definition 60, clip 20, cycle 0.1, full-auto state and both normal
and alternate recoil-stat identities. Both reference identities and angle
tables enter the catalog policy digest. Completed suppressor transitions
select the separate profile through the existing authoritative cache
boundary; no render-frame gameplay or additional wire state is introduced.
The `recoilcheck` diagnostic accepts `weapon_m4a1_silencer` and optionally
`--unsuppressed` to observe actual authoritative rays for each mode.

These are held standing 2023 profiles. Requested 2015 parity, moving or
crouched firing, partial-magazine/recovery dynamics, camera decay and
native 64-tick behavior remain unestablished.

## Private provenance

### M4 observation boundary

The next profiles are measured separately for M4A4, attached M4A1-S and
detached M4A1-S. The installed schema distinguishes the M4A1-S alternate
recoil attributes; sharing the AK trajectory or assuming both suppressor
states have the same spray is not justified. A completed suppressor change
must select the matching observed trajectory and invalidate derived recoil
data at the authoritative simulation boundary.

Each native mode first requires a visible weapon/mode check, an exact
full-clip to full-clip-minus-one decrement, a new impact, stationary eye and
velocity, and an unchanged camera background. Two separately recovered,
shot-ordered no-spread magazines and an independent normal-speed aggregate
follow only after admission. Projection is checked again with known camera
rotations; the earlier AK calibration is not assumed to apply automatically.
Unadmitted captures and startup failures remain negative evidence. Native
data and account-bearing receipts remain private. No M4 trajectory is
claimed from the schema's seed or magnitude alone.

Research output: `csr-fidelity/recoil-reference-2015`, outside the repo.
No assets, raw scripts, native logs or account identifiers are published.

| Input | SHA256 |
| --- | --- |
| Historical item schema | `8F9F82542D61246E8C0447A7E6CBC8D9F5FF6E80E65D9DB7708CEECC09E2C0DB` |
| Historical AK-47 data | `C46608D591A64BD78D40E7E31EB00CBC9D8C77582D27248ACEB4427FBF150C73` |
| Historical M4A4 data | `E1100970423BD3D3C53CBB72BE4D127BACDC3826B59DF89E318C1EE0238FA38C` |
| Historical steam.inf | `793D6F1A98D5ADF9BCE2EEC5D6FBC572D218AFFB91D5B342DFA7371FA4EE7608` |
| Installed loose schema | `510E09B68A01D88EDBA2342972960025FD6484AAAA58365FCF47D8513DE79623` |
| Installed event metadata | `FF9D593FC7FBEA9A5AF2CE5BC0816B7B99FE2B592AA01FC73A8347AAF26B1206` |
| Source-only resolution audit | `A1D6D35EC3D271A779562ED4694B844319E1CD472CFDC7633CF71F4E4695797E` |

Native AK observations: private `csr-fidelity/measured-recoil-20261015`, runs
`admission-20261015-011438` and `projection-20261015-012454`. Both exact
userdata restores passed with no remaining native process. Executable SHA256:
`DE96C3B37E3AAEDCF4D6A24C5F6C30ACD60F3593E8D452DD9857D682EA3FB114`.
Observed item-schema SHA1:
`c1361d351802b7be5f55245c331f5f1d6b36ad35`.

Native M4A4 observations: private `csr-fidelity/measured-m4-20261015`,
run `M4A4-20261015-084848`; exact 12-file userdata restore passed.
The preceding pre-Steam startup failure is retained as negative evidence.

Fresh native M4A1-S observations: private `csr-fidelity/measured-m4-20261015`,
runs `M4SAttached-20261015-100848` and `M4SDetached-20261015-101424`.
Both completed exact userdata restoration. The earlier `092314` run remains
excluded from all profile fitting and comparison.
