# Observed viewmodel motion

CT09 currently has a bounded presentation candidate for the installed M4A4.
Its model origin translates by the difference between the current forward
vector and the time-interpolated forward history 0.1 seconds earlier, scaled
by 1.6. Model/camera orientation and gameplay aim stay current. This differs
from the removed guessed angular low-pass filter. Authored clips, muzzle and
held attachments continue using the same resulting model transform.

Admission requires the raw observed item-schema SHA1
`c1361d351802b7be5f55245c331f5f1d6b36ad35`, canonical M4A4 ContentId/class
and model path. Other weapons/schemas remain neutral. Raw schema identity is
retained separately from the accumulated gameplay catalog digest. This
presentation change introduces no wire/gameplay state or protocol change.

History retains the samples bracketing the delayed time, with bounded storage.
Respawn/view-target/weapon changes, noclip and clock discontinuities reset it.
Scoped/hidden views stay neutral. The effect does not change authoritative
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
remain private. Candidate rewrite projection/render verification is pending;
this document does not claim complete CT09, other-weapon or2015 parity.
