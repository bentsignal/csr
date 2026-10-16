# Per-instance static-prop diffuse modulation

The renderer currently shares one material among instances of the same prop
material and omits the BSP's authored instance tint. A placed blue barrel can
therefore retain the untinted texture even when its selected map specifies a
strong blue modulation. This change exposes the selected map's raw RGBA bytes
and carries supported opaque RGB into each instance's prop-data texture.

## Public format basis and admitted layouts

The independent [ValveBSP data-structure documentation](https://pysourcesdk.github.io/ValveBSP/datastructures/gamelumps/lump_prps.html)
lists diffuse modulation after CPU/GPU level bytes in v10/v11 prop records.
The [srctools StaticProp documentation](https://srctools.readthedocs.io/en/latest/modules/bsp.html#srctools.bsp.StaticProp)
identifies per-instance model tinting and distinguishes version-dependent
features. Only public field/layout documentation was used; no Source SDK,
leaked source or parser implementation was consulted or copied.

The two concrete CS:GO map layouts inspected here are version10 / stride76
(classic Dust II) and version11 / stride80 (current reworked Dust II). Their
four modulation bytes begin at offset64. The parser accepts this field only
for those exact pairs and an exactly divisible record payload. Unsupported
versions, differing strides or ambiguous trailing bytes expose `None`, while
the existing leading-field parser behavior remains unchanged. Raw RGBA values
are preserved without inferring a transfer function in the BSP library.

The first renderer candidate accepts alpha255 only, normalizes RGB by255 and
multiplies the prop's albedo before the existing tone mapping. Nonopaque alpha
and unsupported layouts retain identity modulation. Opacity/transparency
behavior is not implemented by this change. No global brightness coefficient,
per-model correction, new game content or lighting scale is introduced.

## Renderer seam

The existing prop texture holds six ambient faces, Sun direction and Sun
color. A ninth texel now holds the instance tint. Both normal-mapped and
unbumped static-prop paths apply it to albedo, independently of their lighting
source. Each mesh already carries its prop ordinal, so this needs no new mesh
attribute or per-instance material clone and preserves shared batching.

Materials with the previous one-texel default do not sample a nonexistent
tint: the shader explicitly retains identity unless the texture is wide enough
to carry the ninth texel. White modulation is the exact identity value.

## Prospective visible target and verification

Classic BSP SHA1 `8d0bfcabeecc4600208f495308a50681ca7e9eeb` contains rusty
barrel instance1083 at `828.271 797.292 0.883`, with authored RGBA
`67 95 118 255`. Sixteen eligible rock meshes have a warm `179 170 162 255`
modulation. The previously measured tunnel arches244/264 and crate247 have
white modulation, so this feature does not explain their remaining lighting
differences.

The blue barrel is a prospective native target, not yet a viewed/admitted
reference. Matched same-BSP native/before/after captures must establish visible
behavior and judge the normalized RGB interpretation. No native tint parity
or visible correction is claimed before those captures. All game data and
reference images remain private and read-only.

Focused synthetic parser tests cover independently colored adjacent records,
preserving alpha, exact v10/v11 layouts and unsupported/ambiguous fallback.
Cargo, complete workspace checks, RELEASE probe/simulation and graphical
verification remain queued behind the shared machine's build/native leases.
