# Authored ordinary prop detail layer candidate

This branch depends on PR166's authored tint path. The classic Dust II
Palace_Pillars material explicitly requests `detail/dt_stone1`, UV scale
`[12 10]`, and blend factor `0.4`. The renderer previously ignored them.
Its normal map is commented out; this is an albedo detail feature, not
normal mapping or a correction to authored VHV lighting.

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
lists VertexLitGeneric defaults: scale 4, factor 1 and mode 0. Its UnlitTwoTexture
discussion describes mode 0 with half color as equivalent to multiplication.
Applying that relationship to this ordinary VertexLitGeneric candidate is
an inference requiring an actual Legacy comparison, not an SDK-code port.
Microsoft's [MODULATE2X operator documentation](https://learn.microsoft.com/en-us/windows/win32/direct3d9/d3dtextureop)
defines the conventional doubled product.

The candidate multiplies base RGB by a linear interpolation from 1 to twice
the detail RGB, weighted by the authored factor. It samples the detail as
data so its midpoint stays neutral, and scales the existing model UVs by
the authored two-axis scale. Opacity, tint masks, lighting and exposure are
unchanged. The sampling transfer and operation are pending native pixel
admission; no brightness fit or per-model coefficient is introduced.

## Supported scope and fallback

Only static VertexLitGeneric ordinary/default mode is rendered initially.
The shader requires VERTEX_LIT; skinned characters and weapons replace that
flag with DYNAMIC_LIT and retain their old path. Explicit other modes,
transforms, alternative UV mappings, tints, animation declarations and
proxies are rejected with a warning and retain the base material.
Scalar and two-axis positive finite scales and nonnegative finite factors
are parsed completely. Malformed declarations never become accepted defaults.
Missing/unloadable detail textures keep the base material.

One added texture/sampler binding uses the existing cached mip/sampler path.
WorldParams appends one vector, preserving the shadow shader's 80-byte prefix.
The existing scene-asset ownership gather now includes the new binding;
its retained-owner test covers all 11 images. Parser tests cover independent
scalar/vector/default declarations and malformed or unsupported cases.

Compilation, actual affected-mesh counts, native/before/after images and
neutral controls are pending scheduled resources. No native or historical
visual parity is claimed by this source candidate.
