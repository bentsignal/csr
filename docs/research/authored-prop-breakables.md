# Authored studio breakables: source candidate, all executable checks UNRUN

This October 18 source candidate extends the combined ladder/door/brush-health
base. The previous brush slice handles windows on installed Nuke; actual Nuke
and Cache covers are `prop_dynamic` studio models with embedded `prop_data`.
No installed asset, capture, SDK/leaked/decompiled code or CS2 data is included.

`mdl::Model::keyvalues` reads only the bounded declared v48/v49 MDL keyvalue
range. Mounted MDL `prop_data` local fields override `scripts/propdata.txt`
bases (16-level bound). Admission requires positive finite declared health and
finite declared bullet AND club damage scales. Entity health, a suggestive
name, and a solid prop alone never establish destructibility. Missing/invalid
health/scales/models/PHY keep the prior path. Indestructible models do not load
PHY through this new path. Definitions use the map ContentId and BSP ordinal;
there are no model-name, target-name, map-name or coordinate special cases.

The bounded supported slice is unparented, enabled, VPhysics-solid
`prop_dynamic` in its static rest pose, with ordinary opaque rendering and no
named damage filter/default animation/random-animation mode. BSP origin,
all three angles and model scale transform authored PHY hulls and actual MDL
meshes identically. Body uses the MDL bodypart base encoding; skin and RGB tint
use the existing studio material path. No guessed health/damage coefficients
or fabricated cover mesh is supplied. The 1024 combined brush/prop limit is
retained. Nonzero/nonfinite/unsupported metadata is not a fidelity claim.

Props join the existing breakable definition/state vector and exact winning
collision-owner trace. Both ordinary gun and knife TickInput damage use the
existing health logic. Current posed doors and intact brush/studio hulls share
one collision overlay; R4 swept-bounds rejection and strict hit-owner/mask/tie
rules are retained. Health-only damage does not rebuild. Destruction, restore,
prediction and round reset derive collision and real-model visibility from the
same state. Individual death leaves the opening intact. Retained models enter
remote asset preparation and leave with the match; per-instance lighting/tint
reuses the door studio material initialization. Studio PVS/frustum and lighting
parity still need measured work; this path shares the door's uncullable behavior.

Literal immediate argument-free `OnBreak -> Kill` outputs remove bounded
unparented/unrotated static brush dependencies. Supported classes are
`func_brush`, `func_wall`, `func_detail_blocker` and `func_button` with the
no-movement bit. ESC and comma output separators are accepted. Delayed,
wildcard, exhausted, argument-bearing and general scripted outputs are not
executed. Up to 64 outputs per source and 128 distinct dependencies are retained.
A dependency with several sources disappears when any source breaks and
returns when correction/reset restores them. Dependency hulls have no damage
owner; their strict shot-occlusion behavior is retained. Their ordinary brush
meshes are isolated too; `rendermode 10` dependencies remain invisible. The
Nuke button's shot-facing thickness lies within the cover's declared MDL
depth bounds. This does not prove actual PHY hit order; model alignment and
shots from both sides require parent runtime checks.

No wire field or event variant changed: protocol remains 27 / ID
`0x4353_5257_0000_001b`, retaining ladder, door and breakable state. A new digest
marker binds definition count/identity, resolved health/damage, transforms,
skin/body/tint, convex geometry and linked removal edges into the existing
catalog identity. Missing catalog identity remains missing. An older parser
of the same BSP/items cannot silently agree with the new definition digest.

The installed Cache `OnBreak -> PlaySound` targets are `ambient_generic` raw
wave declarations, not supported soundscript entries. Existing raw ambience
playback has no faithful translation for those entities' radius/modulation
fields. No guessed soundlevel, timing, volume mix, break-wave substitution or
new wire event was added. Those authored sounds, Nuke particle/animation
outputs, debris/gibs, animated machinery and general entity I/O remain gaps.
Bullet penetration/shot continuation and dynamic corpse/lighting parity retain
the previous limitations.

Focused synthetic fixture source (all UNRUN) covers bounded MDL keyvalues and
body encoding, required health and inherited damage, unsupported admissions,
full-angle PHY transforms/exact ownership, bounded immediate/shared Kill
edges, real shot/knife/passage/prediction/restore/reset at 64/128 ticks, digest
binding, retained render assets and match cleanup. Parent must compile and run
these plus the required workspace/probe/sim gates under its exclusive slots,
then inspect the immutable executable on actual Nuke/Cache covers from both
sides. Verify model/PHY alignment, disappearance, passage, correction and reset;
retain door, ladder, apartment, PHY, viewmodel and grenade regressions. No
roadmap or owner acceptance item is marked complete by this source candidate.
