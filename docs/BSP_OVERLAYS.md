# BSP overlays

`info_overlay` is compiled into BSP lump 45, not retained as a normal map
entity. The renderer now reads that authored geometry and its material through
the map's mounted VFS. A map `ContentId` loaded from a private pack follows the
same path as an installed map; the material entry already exists as a BSP
texinfo/texdata reference. No installed content is copied into this repository.

The supported subset is a static world-model overlay with affine (parallelogram)
handles and a `LightmappedGeneric` or `UnlitGeneric` material. Projection clips
each explicitly listed receiver triangle against the handle quad. Displacements
retain their actual triangles, smooth normals and interpolated grid lightmap
coordinates. Planar receivers retain compiled face tessellation. Authored UV
bounds, including atlas subrectangles and reversed axes, are preserved. Materials
use the existing texture/VMT loader, including texture alpha and normal maps.

Overlay meshes use world PVS/frustum visibility or the existing 3D-sky layer.
Their materials are separate from opaque world material handles, disable depth
writes, and use Bevy's reversed-Z raster bias to avoid coplanar depth fighting.
The two authored render-order bits are retained in material specialization and
transparent sort bias. This does not replace the renderer's broader unresolved
translucent-surface sorting behavior.

Limits are 512 records (the documented BSP overlay limit), 64 receiver faces per
record, 65,536 receiver triangles and output vertices per overlay, and 524,288
output vertices per world/sky build. Partial records, malformed face counts,
non-finite metadata and unsupported lump versions fail parsing. Invalid receiver
indices, malformed topology, non-affine handles and exceeded mesh budgets skip
the affected overlay. Unsupported/missing materials are skipped, without drawing
replacement art over a valid surface. Dynamic brush overlays, non-affine handle
mapping, hardware quality filters and authored distance fading remain unsupported;
fade limits are preserved in the parsed data for a measured follow-up.

## Installed Legacy Dust II metadata

The locally installed CS:GO Legacy Dust II has 90 overlays and 11 materials.
All 11 are translucent `LightmappedGeneric` decals: road striping, wall signs,
wear, panel paint, manholes and the two bombsite X sprays. Its 276 receiver face
references all resolve to displacements. This is not a face-table mismatch:
LDR lump 7 and HDR lump 58 contain the same receiver indices/texinfo/dispinfo;
the map has 8,324 displacement records and 1,528 planar face sentinels among
9,852 faces. For example, bombsite X overlay 41 references faces 8259/8260,
which select displacement records 6868/6869.

87 handles are affine. Records 36, 37 and 38 use edited non-affine sign quads
and are intentionally skipped. 49 records have distance fade ranges, whose
native interpolation is not yet reproduced. Neither this metadata audit nor a
synthetic geometry check establishes visual parity; installed rendered captures
are still required before shipping the feature.

## Clean-room references

- [Source BSP File Format](https://developer.valvesoftware.com/wiki/Source_BSP_File_Format#Overlays):
  lump 45 record layout, face indices, face-count/render-order bit packing.
- [Original independent BSP format documentation](https://www.bagthorpe.org/bob/cofrdrbob/bspformat.html):
  overlay layout and material/receiver references.
- [srctools BSP format API](https://srctools.readthedocs.io/en/latest/modules/bsp.html#srctools.bsp.Overlay):
  corner handles, packed basis, V-axis cross product/flip and squared fade fields.
  The independent reader's field unpacking was checked only to disambiguate the
  packed Z slots, not to obtain engine rendering code.
- Cached Bevy 0.19.1 `Material`, `StandardMaterial`, mesh pipeline and transparent
  phase documentation/source: depth/sort bias API and reversed-Z depth state.

No Source SDK, leaked or decompiled engine implementation is used.
