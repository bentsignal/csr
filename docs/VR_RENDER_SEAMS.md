# Tracked rendering preparation

The render timeline retains feet-relative tracked head/hands and the separate
admitted aim ray. Remote actors use the already-interpolated `RenderPose`
sample, not raw authoritative tracking at snapshot frequency. An actor absent
from the delayed timeline has no tracking/aim override until a valid sample
exists. Local prediction retains its tracked data separately from stair-eye
easing. Tracking availability and new-life transitions snap instead of blending;
cancellation discards cached tracking while preserving flat body/camera behavior.

This prepares data for rendering; it does not implement head/arm IK, tracked
hitboxes, a VR gun, headset cameras or compositor submission. The ordinary
character animator and desktop viewmodel consume the same fields as before.

## Installed model metadata and calibration

`mdl::Model::eye_position` retains the authored header vector at offset 80 in
Source model axes/units. `mdl::Attachment::rotation` retains all three local
basis columns from its row-major 3x4, alongside the existing local translation.
Both header-only and geometry loading validate finite eye metadata; attachment
loading validates finite basis/translation and rejects truncated records. The
basis is not projected onto a quaternion or assumed to be orthonormal by parsing.

Read-only inspection of the installed Legacy SAS/Phoenix models found 98/86
bones and named neck/head, clavicle/upper/lower-arm/hand chains, with twist and
finger descendants. Bone indices differ between the models. Runtime rig binding
must resolve names, validate parents and derive segment lengths from each model.
AK/M4-S world models provide `weapon_hand_R`, muzzle and offhand attachment data.
No model geometry, skeleton dump or other game asset is included in this repo.

The two player headers advertise an eye vector at height 73 Source units; that
does not establish the gameplay eye height, a physical headset pivot or a
head-bone offset. The current sim camera uses its own eye height. No eyes
attachment exists in those two models. Head-to-eye and controller-grip-to-hand
bindings require explicit measured calibration before IK/gun use; this change
does not substitute the header vector for any existing camera.

The input adapter's converted rotations have local +Y forward/+X right/+Z up,
whereas Source body yaw uses +X forward. A model/bone binding must account for
that distinction. Tracked offsets are already in Source world axes relative to
the feet; do not rotate them by body yaw a second time. Current network bounds
are 96 units for the head, 128 for each hand and 48 from the current hull eye for
the admitted aim origin. These bounds are not anatomical IK reach limits. The
server still owns wall admission/damage, and cosmetic retargeting must not alter
world state, collision or reliable effects.

Synthetic asset-free tests verify metadata layout, nonfinite/truncated rejection,
sampled network tracking at 64/128 tick, independent aim, quaternion guards,
missing samples, cancellation and life resets. Future pure IK can overlay the
existing source-space animation pose before `skinning::apply_pose`; hierarchy
and axis conversion should remain in the established skinning path.
