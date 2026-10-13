# Authored static-prop lighting foundation

Installed Legacy Dust2 (BSP SHA1
`d905c6f7dd694a0dace9380a8558ca8a177cbd91`) embeds 3,158
`sp_hdr_<ordinal>.vhv` files: one for each static prop, 73,297,408 bytes total.
All model checksums match the selected map-pak or Legacy VPK MDL. All 3,749
mesh spans match the existing reader's LOD0 mesh vertex-count sequences.
Those counts do not establish the order or meaning of individual records.
All 1,410 used model meshes have shuffled VTX-to-VVD source identities;
350 reused model names have different lighting files between instances.

`mdl::Model.checksum` retains the exact unsigned header checksum.
`mdl::Mesh.hardware` retains body-part/submodel/mesh/LOD identity, while
`source_vertex_indices` follows the unchanged emitted vertex sequence. These
indices address the **fixup-resolved LOD0 VVD vector**, not original pre-fixup
file positions. Geometry, indices, materials and renderer behavior are unchanged.

`mdl::vhv::File` checks the observed v2 40-byte header and 28-byte mesh tables,
file/table bounds, totals and disjoint payload spans. It borrows opaque
12-byte records and retains unknown flags/reserved words. The prototype caps
files at 8 MiB and tables at 128 meshes; other versions/record sizes reject.
No RGB, exponent, directional basis or transfer function is decoded.

`ScopedFile` binds an immutable `ContentId`, exact selected BSP digest and
prop ordinal. Callers must obtain bytes through the selected map's **scoped**
mount, never generic fallback for an identically named VHV. The constructor
checks the literal ordinal-derived filename; `candidate` checks expected
scope, checksum, every mesh count and supported default-submodel/LOD0 identity
before returning any matching spans. It rejects unsupported LODs rather than
dropping them. Caller-supplied provenance is not a filesystem attestation,
and a candidate correspondence is not proof of payload-to-vertex order.

Synthetic tests cover malformed/overlapping/truncated payloads, identity and
scope mismatches, actual VFS mount shadowing, multiple hardware strip groups
and fixup-resolved indices. Installed read-only comparison data stays private.
Private provenance is `dust2-vhv-audit-20261013/FINDINGS.md` SHA256
`E404FA55FD07D24D3AC9CE80D029C9554111B3B5B62FF406C245BE3651C3DA4C`.
Official [VHV format declarations](https://github.com/ValveSoftware/source-sdk-2013/blob/be57e3d01ac7d7aa6a2cacbfef187c0b46616db0/src/public/materialsystem/hardwareverts.h)
independently confirm version 2, header/table sizes, checksum and mesh fields.
Only public declarations/comments were consulted, never accessor or engine
implementations. The header explicitly distinguishes record byte size from
its color representation; 12 bytes alone cannot establish the encoding.
The pinned Git blob is `c7a08fe9136aea548ae386ef4520b704a2dbe64e`.

No renderer consumes this API. VHV vertex order and lighting basis still need
independent justification before replacing the ambient-cube approximation.
No color, Sun, exposure or native/historical lighting fidelity claim follows.

The developer example `cargo run -p launcher --example vhv_metadata` uses the
actual Level, MDL and VHV readers against the exact admitted Dust2 digest.
It loads each referenced model once, retaining only that model's geometry at
a time, and requires all 3,158 lighting files from the selected BSP's `map`
mount. It checks model-file checksums and every candidate mesh/count span.
Count, per-file and aggregate byte bounds and a schedule-checked elapsed
bound are explicit. Existing VFS reads are whole-file operations; these
post-read admission limits are not an OS I/O watchdog. The example exports
no assets and neither proves vertex order nor decodes lighting.
