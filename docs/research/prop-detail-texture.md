# Authored ordinary world and prop detail candidate

This branch builds on the merged authored tint path. The classic Dust II
Palace_Pillars material explicitly requests `detail/dt_stone1`, UV scale
`[12 10]`, and blend factor `0.4`. The renderer previously ignored them.
Its normal map is commented out; this is an albedo detail feature, not
normal mapping or a correction to authored VHV lighting. The foreground
tunnel columns are instead BSP world surfaces using `de_dust/dusttile03`.
That LightmappedGeneric material also requests `detail/dt_stone1`, with no
explicit scale, factor or mode. Its installed VMT SHA256 is
`7D859C4EBA546E61B9580A2BCCAE79F8F31D3083E57962BA206E6A4A2EE50FCC`.

## Clean-room evidence and bounded operator

The installed material and texture are read at runtime. The selected
classic-map material has SHA256
`531227A959AFDEF1248A6BFD1A90369AB011B1E91D67F4BAD50D47919B44A94A`;
its detail VTF is a 128×128, one-frame DXT1 texture with 8 mips and flags 0,
SHA256 `7EA3A3EEA41306F1C7182A6BDFD9D071BA675CDF340F62789301EA93497D54C9`.
Neither is embedded in the private classic BSP. No assets are included here.

Firsthand [material-authoring guidance](https://github.com/yogensia/vmt-parameters/blob/master/vmt-shader-reference.md)
describes detail scale, strength and the ordinary grey-neutral operation.
The independently authored [shader-parameter guide](https://steamcommunity.com/sharedfiles/filedetails/?id=2162570606)
lists VertexLitGeneric and LightmappedGeneric defaults: scale 4, factor 1
and mode 0 (the latter table labels its mode key `detail1blendmode`; that
alternate declaration remains explicitly unsupported here). Its UnlitTwoTexture
discussion describes mode 0 with half color as equivalent to multiplication.
Applying that relationship to this ordinary VertexLitGeneric candidate is
an inference requiring an actual Legacy comparison, not an SDK-code port.
Firsthand [world-material authoring guidance](https://valvedev.info/guides/getting-around-in-the-material-system/)
and a [mapper's worked example](https://steamcommunity.com/sharedfiles/filedetails/?id=1372652108)
also demonstrate ordinary LightmappedGeneric detail with explicit mode 0.
Microsoft's [MODULATE2X operator documentation](https://learn.microsoft.com/en-us/windows/win32/direct3d9/d3dtextureop)
defines the conventional doubled product.

The candidate multiplies base RGB by a linear interpolation from 1 to twice
the detail RGB, weighted by the authored factor. It samples the detail as
data so its midpoint stays neutral, and scales the existing model UVs by
the authored scale. The two-axis prop scale interpretation remains a candidate;
the observed world material uses the documented scalar default. Opacity, tint masks, lighting and exposure are
unchanged. The sampling transfer and operation are pending native pixel
admission; no brightness fit or per-model coefficient is introduced.

## Supported scope and fallback

Only ordinary/default LightmappedGeneric world and static VertexLitGeneric
detail are admitted. The shader requires LIGHTMAPPED or VERTEX_LIT and rejects
DYNAMIC_LIT; skinned characters and weapons retain their old path. Explicit other modes,
transforms, alternative UV mappings, tints, animation declarations and
proxies are rejected with a warning and retain the base material. Alternate
detail-mode, second-detail and detail-alpha-mask declarations also reject.
Scalar and two-axis positive finite scales and nonnegative finite factors
are parsed completely. Malformed declarations never become accepted defaults.
Missing/unloadable detail textures keep the base material.

One added texture/sampler binding uses the existing cached mip/sampler path.
WorldParams appends one vector, preserving the shadow shader's 80-byte prefix.
The existing scene-asset ownership gather now includes the new binding;
its retained-owner test covers all 11 images. Parser tests cover independent
scalar/vector/default declarations and malformed or unsupported cases.

## Initial actual comparison and corrected target

The private four-capture packet `045847` ran successfully against the initial
static-only candidate. It admitted 52 detail meshes, but both complete tunnel
images were byte-identical to their baselines. This is a real zero-effect
comparison, not evidence that the shader or sampling transfer is correct.

A subsequent read-only geometry audit identified the target mistake: the
foreground north panel rays intersect world faces 4984/4965 (DUSTTILE03,
no primitive records) before the Palace_Pillars prop 261. South panel rays
likewise intersect world faces 5062/5071. The jointly logged DETAIL and
VERTEX_LIT/VHV admission for prop 261 was real, but it was occluded in this view.
The classic BSP contains 172 DUSTTILE03 face records; this is a source count,
not an actual visible or rendered-face count.

The correction admits the same authored ordinary operator for the visible
LightmappedGeneric surfaces, preserving all unsupported fallbacks. The
initial negative is retained and is not claimed as a fidelity pass.

## Actual verification after the world-surface correction

Code `46ea73d` passed CI run `38879847913`. The clean combined PLAY artifact
`52ff900` contains byte-identical detail renderer/parser files. Parent's full
workspace/DEV and focused PLAY checks, installed-model checks, launcher build
and map probe/simulation checks passed. Classic Dust II's ten known probe
faults remain byte-identical to the admitted baseline; its simulation passes.

The corrected private packet `055001` rendered three fresh after images:
classic tunnel north, tunnel south and the blue-barrel control. All three
owned children exited successfully; no graphical process remained. Both the
implementing agent and parent independently reviewed all actual images at
1600×900. The retained native references use the same private classic BSP and
acknowledged eye/angle poses; retained PLAY baselines use the same camera,
resolution and exposure settings.

The foreground DUSTTILE03 surfaces now visibly receive their authored detail
grain. Selected north-panel RGB mean absolute errors against Legacy changed
from 34.38 to 32.71 and 33.10 to 31.53; south panels changed from 13.35 to 12.29
and 11.17 to 10.04. North first-difference errors improved; both south
first-difference errors slightly worsened. These are raw, fixed-region
comparisons with no registration, exposure, gamma or material-coefficient fit.

The wall controls resolve to authored normalmapped PalaceWall01 materials
without detail. The barrel/cap and its background wall likewise have no
detail declaration. Those controls, crate 247 and the arch controls are
pixel-identical before and after. Existing authored VHV admission remains
173 meshes/95,965 vertices, nonwhite tint instances remain 104, and ordinary
static-prop detail admission remains 52 meshes. The 172 DUSTTILE03 faces are
a source-data count, not a measured visible/drawn count.

This admits a bounded visible world-detail improvement. Exact detail transfer,
fine-grain parity, the two-axis static-prop scale and the remaining classic
lighting/arch differences stay open. The [final combined RELEASE checks](overnight-fidelity-verification.md)
passed on `a9997ea`; there is no separate `46ea73d` RELEASE claim. The earlier four-image
static-only zero-effect packet is preserved with its occlusion/target-identity
explanation.
