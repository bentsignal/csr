# Per-instance static-prop diffuse modulation

The renderer currently shares one material among instances of the same prop
material and omits the BSP's authored instance tint. A placed blue barrel can
therefore retain the untinted texture even when its selected map specifies a
strong blue modulation. This change exposes the selected map's raw RGBA bytes
and carries supported opaque RGB into each instance's prop-data texture.

Installed barrel material `dust_rusty_barrel` also enables
`$blendtintbybasealpha`. The renderer now exposes that authored
`VertexLitGeneric` parameter explicitly: combine material and instance diffuse
tint, then interpolate from identity to that tint using the original base
alpha. Alpha0 preserves base RGB; alpha1 applies the complete tint. Materials
without the parameter retain uniform tint. This mask does not change opacity.
Public [firsthand material-authoring guidance](https://steamcommunity.com/app/1840/discussions/0/3055111535922308396/)
describes tint applying to opaque base-texture regions; a separate
[firsthand Valve issue report](https://github.com/ValveSoftware/Source-1-Games/issues/5841)
shows per-entity color masking in another Source1 title. These observations
and the installed VMT motivate the candidate; the exact CS:GO result still
requires the prospective native comparison.

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
`67 95 118 255`. The complete BSP has 104 nonwhite instances across rocks,
rusty barrels and plaster edges; thirty rock instances have a warm
`179 170 162 255` modulation. Their resolved materials have no active normal
map, so this map can establish colored unbumped behavior and unchanged white
normal-mapped controls, not colored normal-mapped parity. The previously
measured tunnel arches244/264 and crate247 have
white modulation, so this feature does not explain their remaining lighting
differences.

Matched same-BSP native/before/after captures now establish a bounded visible
correction. In two barrel views, the authored blue body replaces the erroneous
white body while the rusty cap and nearby world control stay byte-identical.
Preselected upper/lower body RGB error against native improves from55.16/58.98
to27.44/31.74 in the west view and45.47/50.99 to16.02/20.67 in the north view.
Classic crate and actual reworked normal-mapped white controls are unchanged;
the classic arch mean delta is0.00174/255. Residual barrel brightness and the
older arch-lighting errors remain. No exposure/gamma/per-model fit was used,
and complete tint transfer or native lighting parity is not claimed.

All seven actual PLAY images and both native barrel references were independently
viewed. Frozen d71d5f6 contains the same static-render/parser source as final166;
its full workspace and real-data checks passed. Combined ec95b6a then passed
RELEASE probe/simulation/audio and ordinary frontend lifecycle verification.
The original broad preflight rejection of CI's headless sim is preserved;
no graphical child had launched, and the amended guard admits only exact known
probe/sim command lines. All game assets and reference images remain private.
