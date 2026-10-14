# Runtime model hitboxes

This is clean-room asset-format work. No engine implementation, Source SDK
function bodies, leaked code or decompiled engine code was used.

## Provenance and supported layouts

- [Valve Developer Community: Half-Life Model Viewer (Source)](https://developer.valvesoftware.com/wiki/Half-Life_Model_Viewer_(Source)?uselang=en)
  documents named hitbox sets and bounds relative to the bone pivot. Its
  authoring examples do not establish CS:GO capsule or local-angle behavior.
- [Crowbar's public SourceMdlHitbox data declarations](https://github.com/ZeqMacaw/Crowbar/blob/0d46f3b6a694b74453db407c72c12a9685d8eb1d/Crowbar/Core/GameModel/SourceCommon/SourceMdlFileData/SourceMdlHitbox.vb)
  identify `boundingBoxPitchYawRoll` and explicitly label that field as required
  by the version-49 CS:GO compiler. Only data declarations/comments were
  consulted, not decompiler/engine algorithm bodies. File blob:
  `ac1235217e370a59ce716fb2c418db12f97bba71`.
- [Valve's public version-48 studio data declaration](https://github.com/ValveSoftware/source-sdk-2013/blob/be57e3d01ac7d7aa6a2cacbfef187c0b46616db0/src/public/studio.h)
  declares `STUDIO_VERSION 48`, bone/group, two vectors, name offset and
  `unused[8]`: a 68-byte box record with eight reserved integers after its
  name offset. Those bytes are not decoded
  as version-49 angles/capsule data. Nonempty hitbox sets in other versions
  fail explicitly instead of inventing an extension layout.

The supported 12-byte set record contains a relative name offset, box count
and relative box-table offset. The 68-byte v49 record contains bone/group at
0/4, endpoints or bounds at 8/20, name offset at 32, pitch/yaw/roll at 36,
and signed radius at 48. Raw group numbers are retained. Signed radius -1
selects the supported box representation; nonnegative values select capsules.
Other negative values are retained by the parser but rejected by posing.

Installed CS:GO Legacy app 4465480 was read without modification. The current
`ctm_sas.mdl` (98 bones) and `tm_phoenix.mdl` (86 bones) are v49 and each
contains one set with 19 records: 15 positive-radius capsules and four
radius-minus-one boxes. Groups are 1 through 8, including one group-8 record;
this module makes no gameplay multiplier decision for that group. All capsule
angle triples are zero. The four box angle triples are nonzero, including
SAS examples `(0, 24.1, 0)` and `(180, 194.6, 0)`. No asset bytes are checked in.

## Parser behavior

All named sets are read. Counts, checked offsets, table bounds, bone indices,
terminated names, finite geometry and ordered box bounds are checked. Capsule
endpoints may run in either direction. Empty set tables are valid. Malformed
present hitbox data returns an informative model load error, rather than being
silently replaced with no hitboxes. `Model.hitboxes` remains a first-set copy
for existing readers; new consumers should select `Model.hitbox_sets`.

## Geometry contract

`HitboxSet::pose` accepts proper rigid bone-to-query-space matrices in Source
axes. Query space may be model space or world space. No inverse bind matrix
is applied: authored hitboxes are already bone local. A box's local QAngle
rotation is composed after its bone transform, around that bone's origin.
Nonzero local angles on capsules are rejected: their interaction has not been
established and is not required by the two observed profiles. Scaled, sheared,
reflected, missing or non-finite transforms are rejected.

Posed hitboxes are owned, cloneable values suitable for bounded server history.
Their AABBs include their complete capsule or all eight oriented-box corners;
movement hulls are not valid narrow-phase bounds. Rays require finite origins,
unit directions and finite nonnegative maximum distance, all in query-space
units. An origin inside a shape returns zero. Nearest equal-distance ties retain
authored order. Raw hit groups and record indices survive the query.

The ray/capsule and ray/box calculations are new geometric implementations,
not ports of engine functions. Capsule intersection solves a finite cylinder
and its spherical caps. Box intersection transforms a ray into box space and
uses intervals along its three axes.

## Limits

These readers and geometric queries do not reproduce native player animation,
select a model/skin or hitbox set for a player, define group-8 damage, implement
bullet penetration, or establish native lag-compensation parity. Simulation
integration must own fixed-tick poses and bounded historical transforms. A
render-frame pose must never decide authoritative gameplay hits. Separate aim
rays and tracked head/hand poses remain independent inputs.

## Validation

Focused synthetic tests cover multi-set retention and malformed later sets,
rotated boxes, capsule side/cap/tangent intersections, nearest raw-group queries,
and rejection of scaled poses. `cargo test -p mdl --locked` passed all 39
crate tests, including a real installed-data check that loads both character
headers and poses all 19 hitboxes from each complete authored set. The bounded
run exited zero without triggering its memory guard. Full workspace and client
integration checks remain with the orchestrator; no native-render parity is claimed.
