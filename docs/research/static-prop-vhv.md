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
Dedicated public Valve format pages returned access errors during this audit;
no SDK, leaked or decompiled implementation was consulted. This foundation
uses the repository's existing public-format readers and installed metadata.

No renderer consumes this API. VHV vertex order and lighting basis still need
independent justification before replacing the ambient-cube approximation.
No color, Sun, exposure or native/historical lighting fidelity claim follows.
