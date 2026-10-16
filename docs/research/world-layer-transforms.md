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

## Matched verification

Source `ae5733546bd589db85a507a4ecaa5e2ee6ae32be`, crates tree
`649eec5427aa487f380d2e3a8b4c3133bbdbefe5`, passed the full workspace CI
in [run 38848645310](https://github.com/bentsignal/global-offensive/actions/runs/38848645310).
Local focused VMT tests passed 7/7; local PLAY client tests passed 315 with
three ignored. Coverage checks identity, a repeated tile crossing UV one,
negative incoming UVs, centered nonuniform scaling with translation, reordered
fields, rejection/fallback and the actual forward/shadow uniform prefix.
The local PLAY launcher build, required Dust II probe and simulation exited
zero, with zero reported probe/simulation failures.

All 23 local workspace packages were cleaned for PLAY before building in the
isolated worktree. The immutable copied executable SHA-256 was
`1257DC6B7F7552786B37A9F62804C6C596AA4671A7C550D0148FDF831BCEC0EB`;
its compiled unsupported-transform diagnostic was checked in the copied binary.
The existing read-only `vhv_metadata` example loaded the actual selected BSP
through the shared level loader, verified the map hash above and exited zero.
This identifies the installed reworked Dust II, not the pinned pre-rework
`de_dust2_2017` archive or a 2015 map.

Native Legacy and before/after CSR captures used 1600x900 and native `getpos`
readback eyes `(250,450,350)` and `(250,650,400)`, both at pitch/yaw/roll zero.
The baseline executable was built from `aaa2e72fd1bd7d33b3112c054dec81c495322a43`.
Both native images and all four CSR images were personally inspected: the
previous doubled stone-course size is corrected in both views. Each owned CSR
process exited zero without ERROR/panic; the native session restored all twelve
preference snapshots exactly and left no native process. The orchestrator
independently inspected the native/before/after trio at the low camera; that is
not a claim of independent review of all six images.

Fixed unoccluded wall rectangles, selected from the viewed images before
computing metrics, exclude sky, foreground dishes/rails and the dark roof
shadow. RGB MAE uses the 0-255 range; SSIM uses non-overlapping 8x8 luma blocks.

| Wall reference | Pixel rectangle x0,y0,x1,y1 | RGB MAE before / after | SSIM before / after |
| --- | --- | --- | --- |
| Low | 920,250,1160,600 | 19.22 / 15.32 | 0.344 / 0.465 |
| High | 650,380,1000,520 | 15.56 / 12.31 | 0.442 / 0.553 |

Whole-frame RGB MAE improved 22.67 to 22.00 and 23.79 to 23.36; SSIM improved
0.696 to 0.720 and 0.725 to 0.746. Remaining lighting, prop and texture-blend
differences are visible; these local structural improvements do not establish
whole-scene parity. The original global process-present flag is retained: a CI
headless `csr sim de_dust2` was observed after the last screenshot child exited.
It was not an owned screenshot leak and was not stopped. Private posthoc
admission keeps that distinction separate from the original receipt.

Required RELEASE build/probe/simulation validation is pending the orchestrator's
frozen combined integration build. No separate RELEASE pass for `ae57335` is
claimed here. Captures establish material mapping, not an FPS improvement.

This does not fix missing cascaded sun shadows, the unknown original owner
screenshot pose, new-layer blend/border composition, general world lighting,
or historical 2015 parity. Screenshots, material files and other game assets
remain private and are not committed.
