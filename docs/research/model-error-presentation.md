# Absent model VMT presentation

Source-only candidate based on `8328ad0d7aa9fa2a3900ab26a7fcbcee492db819`.
This is an empirical rendering policy for an unresolved model material. It
does not recover an authored material or establish complete fallback parity.

## Evidence and decision

Root's original CS:GO Legacy observation identifies the exact fence-model draw
as runtime `___error` and shows black geometry in normal and reverse views.
Root inspected our same-view geometry with the diagnostic purple checker.
The subsequent complete native `___error` parameter block reports
`UnlitGeneric`, a runtime `error` base texture of 32 by 32 pixels, white color
and color2, alpha one, and an identity base-texture transform. The private
receipts are `native-fence-identity-20261019-084812` and
`native-error-parameters-20261019-090345` under
`C:/Users/shawn/csr-fidelity/overpass-20261019/`. The prior investigation is
`error-material-r2.md` there.

The authentic requested fence VMT remains absent from the checked original
data. No VFS defect or justified alternate-material alias was found. The r2
candidate corrected UNLIT shading but retained the checker appearance. Root
now explicitly selects the existing neutral black texture for absent model
VMTs at the shared translator seam. This changes presentation beyond r2;
missing content remains missing.

The neutral texture is the existing one-pixel opaque black resource. Its
bytes are not the native runtime 32 by 32 texture. Native runtime texels,
complete model UV behavior and universal error-material behavior have not
been recovered. This policy is supported by the measured reference, not a
claim that every unresolved native model must render this way.

## Caller classes and preserved behavior

`world_mesh::material_from_vmt` changes only its early absent-VMT branch:

| Input | Presentation |
| --- | --- |
| Absent VMT, model (`lightmapped = false`) | Existing black texture, UNLIT |
| Absent VMT, BSP surface (`lightmapped = true`) | Existing diagnostic checker, UNLIT |
| Parsed VMT with absent/unreadable declared base texture | Existing checker and authored material processing |
| Parsed VMT without a declared base texture | Existing white texture and authored material processing |
| Parsed VMT with an available base texture | Existing authored texture and processing |

All production callers were inspected at the specified base commit:

- Static props in `props.rs` and studio models in `skinning.rs` pass `false`.
  Skinned dynamic-light flags added afterwards retain UNLIT; the world shader
  overrides illumination for UNLIT and skips its exposure/tonemapping branch.
- Main/3D-skybox/breakable BSP surfaces in `lib.rs`, door brush surfaces in
  `doors.rs`, and window brush surfaces in `window_fade.rs` pass `true`.
- `world_mesh::world_material` and the overlay translator pass `true`.
  The overlay translator supplies a parsed VMT.
- The 2D-sky caller in `sky.rs` passes `false`, but first requires a parsed
  sky VMT, then passes `Some`. Its missing sky material exits before the
  translator, and its missing sky texture has a separate checker path.

The existing boolean therefore distinguishes both reachable absent-VMT
caller classes without a new API or path/name exception. Future non-model
callers using `false` with `None` must revisit this contract.

`launcher/src/probe.rs` still increments failures when no model material
candidate loads. The renderer neither changes the VFS nor introduces an alias,
suppresses a probe failure, reclassifies missing content, or waives admission.
Meshes, UVs, collision and unknown historical VHV flags4/stride4 are untouched.

## Verification boundary and risk

The synthetic fixture
`authored_transform_tests::missing_material_uses_unlit_caller_fallback_without_changing_valid_materials`
checks both absent-VMT caller modes, retained opaque/culling/default tint and
UV parameters, the real BSP material-loading wrapper, and continued failure to
load the absent VMT. For both caller modes it also checks lit and unlit parsed
materials with an available texture, a missing texture and no declared texture.
The missing-texture counter must still record its cached failure once.

The risk is the deliberate generic effect on every unresolved static or
skinned model VMT, beyond the measured fence reference: such geometry loses
its visible purple diagnostic and appears black. Correctly loaded materials,
including valid textureless materials and materials missing only a texture,
retain their existing paths. The strict dependency failure remains visible to
the probe. A different measured native unresolved-model presentation would
require refining this empirical policy.

Only source inspection and `git diff --check` were performed by this child.
The fixture, workspace tests, installed-data probe/simulation checks and all
rendered checks are unrun by instruction. Root owns serialized Cargo/native
work, the matched-camera Overpass comparison and accepted Dust II/Nuke
controls before admission. No parity, historical-map admission, staging,
owner acceptance, merge or release is claimed.
