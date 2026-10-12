# Compiled BSP face tessellation

The planar world renderer previously ignored a face's primitive metadata and
always triangulated from its first surfedge vertex. It now uses declared
triangle lists or strips from lumps 37/39, retaining the compiler's boundary
splits. Faces without primitives retain the existing fan; zero-area triangles
are omitted. Displacements, stored face normals, texture/lightmap coordinate
mapping and lighting parameters are unchanged.

## Clean-room format sources

- [Rof's public BSP format documentation](https://www.bagthorpe.org/bob/cofrdrbob/bspformat.html)
  describes 56-byte faces, `numPrims`/`firstPrimID`, and PrimIndices tessellation
  of face vertices for T-junction seams. The
  [VDC-format translation](https://github.com/ZenkakuHiragana/SourceBSPFormatJP)
  also preserves this description; no SDK implementation was used.
- [srctools public BSP API documentation](https://srctools.readthedocs.io/en/latest/modules/bsp.html#srctools.bsp.Primitive)
  identifies these primitives as T-junction stitching and distinguishes lists
  from strips. The face count's high bit is a separate shadow flag, so only
  its low 15 bits select primitive records.
- [Microsoft's triangle-strip topology documentation](https://learn.microsoft.com/en-us/windows/win32/direct3d9/triangle-strips)
  establishes alternating strip winding and degenerate connector handling.

The supported form has `vertex_count == 0`: indices refer to the selected
face's surfedge-order positions, not global BSP vertex IDs. Dedicated
PrimVerts are deliberately unsupported until their indexing is independently
established. A declared unsupported or malformed primitive rejects that face
with a diagnostic; it never silently draws a substitute fan.

## Installed-data measurement

A read-only audit of locally installed CS:GO Legacy DustII (BSP 21, revision 10535)
found 9852 HDR faces. 81 world faces reference primitives; all 365 primitive
records are type 0 lists with zero dedicated vertices. The PrimVerts lump is
empty and PrimIndices contains 6438 u16 values. Every referenced index is within
its face's vertex count; the largest index is 20.

Across those 81 faces, the previous fan emits 158 zero-area triangles. The
declared topology contains 492 triangles and no zero-area triangles. Four
referenced faces have mixed signed polygon turns around the T-spawn-side
plaster near(-2688,-1200). These measurements establish a format gap, not the
cause of the user's chipped arch screenshot. Near the upper-tunnel reference,
face 536 is a wooden ceiling spanning (-1656,952,416)..(-1160,1304,416); its old
fan has six zero-area triangles. The nearest referenced face to the CT-garage
reference centre is 815 units away. Fresh native and CSR captures at identical
poses remain required before attributing any visible arch defect to this fix.

All 9852 nondegenerate polygon windings oppose the stored plane normal for
both `side` values. That installed-map result supports retaining our current
normal convention; it does not establish all BSP versions' normal semantics.
Assets, measured indices and private screenshots are not committed.

## Bounds and validation

Face surfedge/edge/vertex ranges and finite positions/normals are checked.
Primitive and index subranges, topology kinds, list divisibility, strip
minimum length and face-local indices are validated. A face may consume at
most 65536 primitive indices, including repeated ranges, bounding expansion
even when hostile metadata repeats the same list. Partial primitive/index
records reject the BSP. High-bit-only counts do not request primitives.

Asset-free tests cover a concave notch, a collinear boundary split, strip
alternation/connectors, masked counts and multiple lists, malformed metadata,
unsupported dedicated vertices, repeated-range expansion, binary layouts,
and renderer integration preserving UVs/normals. Compilation, full mandatory
checks and matched rendered comparisons are pending coordinated build/GPU
ownership; this checkpoint makes no visual-parity claim.
