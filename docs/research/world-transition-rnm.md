# World transition normal projection

Ordinary two-normal lightmapped materials now use the same blended tangent
normal for directional lightmaps that the shader selects for surface shading.
Previously, the shader interpolated the two normals but independently
interpolated their RNM weights. Squared, clamped projections are nonlinear:
opposite tangential slopes can cancel to a flat surface while their averaged
weights still favor one incoming lighting direction.

The clean-room mathematical basis is Valve's published
[Efficient Self-Shadowed Radiosity Normal Mapping](https://cdn.steamstatic.com/apps/valve/2007/SIGGRAPH2007_EfficientSelfShadowedRadiosityNormalMapping.pdf),
section 3. An ordinary RGB texel encodes a normal whose squared nonnegative
basis projections are normalized into lighting weights. SSBump instead stores
the weights themselves, including visibility loss. The latter must remain
linearly interpolated without normalization. The paper establishes those two
representations; choosing the shader's already-blended surface normal before
ordinary RNM projection is our mathematical inference, pending native admission
of the two-layer result.

The new branch requires LIGHTMAPPED and BLEND and excludes SSBUMP, inside the
existing NORMAL/NORMAL2 path. Single-normal materials, static/dynamic prop
lighting, SSBump/mathfix, texture transforms, exposure, the atlas and the
material/uniform layout retain their existing paths. No brightness factor is
introduced. Depending on incoming light direction, the correction can either
brighten or darken a texel.

A private read-only audit of installed Legacy app4465480 found both color
layers and both normal layers on 54 Dust II material references (5,707 HDR face
records) and 16 Inferno references (1,794 records). These records are all
displacements; 5,312 and 1,753 respectively contain interior vertex alpha.
Examples include Dust II's `hr_dust_plaster_18` / `hr_dust_stone_01` blend and
Inferno's `flagstone_d_blend` and `cobblestone_b_blend`. No installed Nuke world
material in this audit declared all four inputs. Counts are source coverage,
not visible pixels or proven native improvement. The audit uses raw key/include
resolution, not the production VMT parser.

Two source regressions cover an opposed-slope directional-light oracle, layer
endpoints, actual material flags and WGSL branch wiring, plus preservation of
SSBump visibility loss and its existing mathfix. **Tests and runtime/native
verification are UNRUN** by this implementation agent and belong to root.
Retained before/native images were inspected, but there is no after render.
This is a bounded shared candidate, not map-wide lighting completion.
