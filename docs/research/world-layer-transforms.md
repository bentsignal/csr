# Authored world-layer texture scale and offset

The world renderer previously sampled both color layers, both normal layers
and the blend mask at one raw brush UV. This ignored a material's separate
`$basetexturetransform2`, `$bumptransform2` and `$blendmodulatetransform`.
The secondary color/normal and blend-mask samples now each apply their own
authored UV scale and offset. The first layer, lightmap UVs, light intensities,
exposure and tonemapping are unchanged. Diagnostic shadow-depth alpha sampling
uses the same secondary-color and blend-mask coordinates as the forward pass.

## Clean-room basis and scope

The installed Legacy materials provide the parameter names and complete
`center ... scale ... rotate ... translate ...` declarations. The original
material tutorial [Dynamic Materials with Proxies](https://nodraw.net/2010/01/dynamic-materials-with-proxies/)
describes these texture-coordinate parameters, including the center used for
scaling and rotation and translation by a fraction of the texture. No engine
implementation, Source SDK shader, leaked code or CS2 content was consulted.

This slice accepts only complete axis-aligned declarations with finite values,
strictly positive X/Y scales and zero rotation. Fields may appear in any order
and their names compare without ASCII case. Scaling is around the declared
center, followed by translation; the folded GPU representation is
`uv * scale + center * (1 - scale) + translation`.

Absent parameters retain identity UVs. Partial or malformed declarations,
nonfinite values, duplicate/unknown fields, rotation, reflected/collapsed scales
and matrix syntax log an unsupported-transform warning and retain identity.
That fallback is not acceptance of those forms or complete VMT transform
support. Proxies and animated transforms are outside this change.

## Installed data

A read-only audit of installed Legacy 1.38.8.1 Dust II, BSP revision 10535,
identified 116 faces whose plaster material explicitly requests 2x secondary
color, secondary normal and blend-mask tiling about the origin. A further 27
faces in the similarly named plaster family have no such declaration and
retain identity coordinates. These counts are BSP material references, not
a claim that all those faces are visible from one camera. The map SHA-1 was
`d905c6f7dd694a0dace9380a8558ca8a177cbd91`.

The 2x declaration halves the width/height of the sampled stone pattern
relative to the old renderer. All these observed declarations specify a zero
center, zero rotation and zero translation, so their intended scale does not
depend on unobserved transform defaults or operation-order differences.

## Verification status

Focused asset-free coverage checks identity, a repeated tile crossing UV one,
negative incoming UVs, centered nonuniform scaling with translation, reordered
fields, rejection/fallback and the forward/shadow uniform prefix. The installed
scene comparison, full workspace checks and required release probe/simulation
are pending coordinated build/render admission. No rendered improvement is
claimed at this checkpoint.

This does not fix missing cascaded sun shadows, the unknown original owner
screenshot pose, new-layer blend/border composition, general world lighting,
or historical 2015 parity. Screenshots, material files and other game assets
remain private and are not committed.
