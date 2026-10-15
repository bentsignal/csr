# Observed viewmodel motion

CT09 currently has a bounded presentation candidate for the installed M4A4.
Its model origin uses the time-interpolated forward history 0.1 seconds
earlier, scaled by 1.6. In the current camera frame, the delayed transverse
components remain unchanged and the forward component is
`1 - dot(past_forward, current_forward)`. This preserves the native inward
rear-sight movement in both pitch directions and yaw-step asymmetry.
Model/camera orientation and gameplay aim stay current. This differs
from the removed guessed angular low-pass filter. Authored clips, muzzle and
held attachments continue using the same resulting model transform.

Admission requires the raw observed item-schema SHA1
`c1361d351802b7be5f55245c331f5f1d6b36ad35`, canonical M4A4 ContentId/class
and model path. Other weapons/schemas remain neutral. Raw schema identity is
retained separately from the accumulated gameplay catalog digest. This
presentation change introduces no wire/gameplay state or protocol change.

History retains the samples bracketing the delayed time, with bounded storage.
Respawn/view-target/weapon changes, noclip and clock discontinuities reset it.
Scoped/hidden/spectated views stay neutral. Only local unpunched flat look
feeds this admitted history; existing rendered recoil punch stays current. The effect does not change authoritative
movement, aim, input sampling or tracked-pose interfaces.

## Native reference boundary

Two owned Legacy1.38.8.1 sessions used the installed Train map and M4A4,
Desktop framing, viewmodel FOV60 and offsets1/1/-1. Actual selected HUD was
30/90; no shots were fired. Public metadata positively reported
`cl_wpn_sway_interp=0.1` and `cl_wpn_sway_scale=1.6`. Native captures show
pixel-stable front/rear sights throughout idle baselines. The installed MDL
has no pose parameters, and its two-frame idle clip leaves decoded muzzle
and hand positions constant; no authored running sequence was found.

Known10-degree yaw/pitch steps and returns produce transient front-sight
shifts of about9 pixels and rear-sight shifts of about17-19 pixels at
1600 by900, then return to baseline. A5-degree yaw return produces about4
and9 pixels respectively. The depth-dependent displacement is consistent
with origin translation; a common angular rotation is not justified.

The first packet sampled verified0.25 timescale, with actual image-capture
intervals retained. It shows an approximately constant short displacement,
then return around0.1 scaled seconds, not an exponential decay. A separate
normal-timescale packet positively reproduces yaw/pitch/half-angle transients.
Some initial normal frames still show the old world angle or miss the brief
peak; those cases remain sampling limits. No exact native server/frame
instant is claimed from command receipt or scaled wall time.

## Walking bob remains open

The same reference has actual running speed225 and walking speed117.
Motion is visibly cyclic. Its current bob amplitudes are customized:
`cl_bobamt_lat=0.33` and `cl_bobamt_vert=0.14`; runtime help separately declares
defaults0.4 and0.25. Other reported values are lower amount21, cycle0.98,
bobup0.5, bob version0 and new headbob1. These are observed2023 parameters,
not proof of historical2015 settings. They have not been used to invent a
walking-bob formula. Automatic moving-gun template matches that followed
background edges were rejected; CT08 still needs admitted motion extraction.

Both native packets restored all12 preference entries exactly and left no
native process. A preceding transient existing-process preflight failure is
preserved separately. Raw game files, logs, screenshots and account data
remain private.

## Rewrite verification boundary

The first world-space-difference candidate (`af7e26b`) passed focused client
and level tests and five actual1600 by900 M4 render checks. All retained
intact models/hands/HUD30/90, unmodified owner preferences, and zero shots.
Its front/rear shifts were yaw10: +9/+19 pixels; yaw5: +4/+9; pitch10:
-8/-18 vertically, with rear x+1; settled yaw: zero. Native positive yaw
was +9/+17 and both pitch directions moved rear x-1. This small but repeated
longitudinal discrepancy remains recorded; the first candidate was not exact.

The corrected camera-frame longitudinal sign is derived from those independent
native step/return and normal-time observations. It retains the original
0.1/1.6, transverse response, finite window, resets, and unchanged aim.

Frozen corrected `631a83d` passed focused client/level checks (298 client,
6 private-map integration and13 level tests), full workspace CI, and an
all-workspace-package-clean release build with required DustII probe/sim
both reporting `failures: 0`. Five actual1600 by900 renders all exited0,
retained untouched M4/HUD30/90 and owner preferences, and passed raw-log,
image-dimension and screenshot-acknowledgment guards. The author and a
second agent personally viewed all five complete images.

Corrected front/rear relative shifts were yaw10: +9/+18 pixels; yaw5:
+4/+9; pitch10: -9/-19 vertically, with rear x-1; settled yaw: zero.
The native reference was approximately +9/+17, +4/+9 and -9/-18. The
inward sign now matches; the roughly1-pixel rear residual remains explicit.
Neither scale nor delay was tuned to conceal it. These finite image checks
do not establish all-angle, recovery, recoil, other-weapon or2015 parity.

Latest master was merged normally after that immutable verification, with
no change to the look-motion source; final integration CI remains required.
CT08 bob is still open. A separate finite native public-parameter packet
positively isolates running lowering from lateral/vertical oscillation,
with actual225 speed and exact preference restoration. Installed idle
geometry diagnostics provide bounded front/rear vertex candidates; image
region membership alone does not prove point visibility or a bob formula.


## Standing forward speed-settling candidate (CT08, periodic bob still open)

The M4A4's mean held pose also lowers and moves back while moving forward.
A separate presentation transform now uses the cycle mean from12 ordinary
normal-time installed Legacy1.38.8.1 frames, each with actual225u/s readout.
The current camera and aim stay unchanged. The model and its authored offset
rotate together in the eye-model frame; delayed-look translation remains a
separate world-space offset. Authored weapon clips and simulated movement,
shot rays, recoil and network state are unchanged.

The reference framing is FOV60, offsets1,1,-1 and the same exact raw schema/M4
identity as the look response. Its actual bob settings were lower21,
lateral0.33 and vertical0.14; these were customized values, not a historical
default claim. Five manually identified visible landmarks per frame were
reprojected using the player's installed model. The pose fit carries2px
manual-pick uncertainty. Across the12 frames the forward-axis roll balances
zero; mean pitch is about1.924degrees, yaw about-0.536degrees and translation
about(-1.803,-0.246,-0.201)eye-model units. Pitch spans1.845..2.009degrees;
forward translation spans-1.891..-1.594units. First/second-half averages and
unweighted/time-weighted means agree within the stated uncertainty.

Capture intervals are retained privately. Their midpoint span is1.595seconds;
one screenshot has a0.424second acquisition window. The public cycle setting
was0.98seconds, but that does not establish exact animation phase. No periodic
curve or phase was inferred. The dense temporal tracker was rejected when
its low residual lost actual weapon-edge identity on held-out images.

The bounded candidate scales the observed mean with actual forward speed up
to225u/s. Independent117u/s walk frames support its lowering/backtranslation
within the manual pose uncertainty. Instantaneous held-out visible landmark
errors reach about9.4px; one walk handle error is8.2px. Periodic motion is
omitted, so this is a mean speed-settling feature, not per-frame bob parity.
Some run handle landmarks were hidden by the public dump legend and excluded.

Only local, flat, standing-ground forward M4 movement is admitted. Air,
crouch, strafing/backward motion, spectator/tracked poses, other weapons and
content packs remain outside this measured slice. No new timing filter or
invented return rate is added: actual speed drives the pose and a stopped
player has a neutral transform. The screenshot fixture uses ordinary
TickInput movement; rendered checks must admit actual speed and ground state.
Frozen source `a0c7aff` passed focused DEV tests, all client PLAY tests, the
launcher build and required RELEASE build/probe/sim with zero failures.
Actual 1600x900 baseline/run/walk/coast images were independently inspected:
the M4, hands, world and 30/90 HUD remained intact, running lowered the model
more than walking, and stopping returned the neutral pose. All sessions
preserved the owner's configuration and exited successfully.

The first wrapper rejected walk's printed `116.99999` against a literal
`117.0` pattern; the child's zero exit, image and raw logs were retained.
A separate finite numeric audit admitted all twelve actual walk states within
0.001 units/s and 0.00001 weight. Only the missing coast case was captured
afterward. Exact-source full CI passed before
[PR #157](https://github.com/bentsignal/global-offensive/pull/157) merged.
