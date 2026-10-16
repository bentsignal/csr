# Authored static-prop lighting origin

Status: source candidate; build, installed import and visible native/before/after
validation are pending. This does not resolve the modern Dust II hard-shadow gap.

The renderer previously discarded a static prop's authored lighting point and
sampled ambient light only near its transformed hull center or placement origin.
That loses the mapper's explicit sample location, including a point shared by
several adjacent pieces of geometry.

## Format and implementation boundary

The public [ValveBSP format documentation](https://pysourcesdk.github.io/ValveBSP/datastructures/gamelumps/lump_prps.html)
lists the record fields: two three-float vectors, three unsigned shorts, two
bytes, a four-byte skin index and two fade floats precede `lightingOrigin`.
Their widths place the next three-float vector at byte44. Independent
[srctools documentation](https://srctools.readthedocs.io/en/latest/modules/bsp.html)
describes a world-space point used for non-vertex lighting, the explicit
`HAS_LIGHTING_ORIGIN` flag0x02, and CS:GO record variants v10/76 and v11/80.
Only documentation was consulted; no Source SDK or leaked implementation was
read or copied.

The parser admits the point only for those exact version/stride pairs, exact
record divisibility, flag0x02 and finite coordinates. Zero coordinates are
valid. Other layouts and absent/invalid authored points retain `None`.
The point is already in Source world coordinates; no prop transform or fallback
height offset is applied to it.

Only the existing ambient-cube fallback tries this point first. If its leaf has
no ambient samples, the prior hull-center/origin/default sequence remains.
The geometric Sun visibility ray and its start point stay unchanged. Authored
VHV checksums, flags0 admission, vertex mapping and decoding stay unchanged,
as do world lightmaps, exposure and shadow composition.

## Installed observations and pending admission

A private read-only metadata audit of the selected map bytes found two flag0x02
props in original Dust II, 50 in original Inferno and none in the installed
reworked Dust II. The original Dust II pair uses
`models/props/de_dust/palacemeddome166.mdl` at ordinals 1077/1078 and shares the
same authored point, substantially displaced from either placement origin.
Inferno includes trees, lamps, pillars and a CT-spawn railing. These counts
describe authored candidates, not visible affected triangles or native parity.

One synthetic parser regression covers preserved world-space coordinates on a
translated/rotated/scaled record, both admitted layouts, unrelated flags, absent
flag, nonfinite coordinates, unsupported layouts and ambiguous trailing bytes.
There are no bundled game assets.

Before acceptance, an actual affected prop must be proved visible in matched
native/before/after cameras, with an unaffected control. The prior occluded
Palace-pillar detail experiment is not evidence for this feature. Builds and
rendered checks have deliberately not been run while the shared resource lease
belongs to the root integration job.
