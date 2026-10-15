# Installed viewmodel geometry diagnostic

`vmprobe2` already reports installed model animation metadata. Its optional
`--sight-rois` report projects bounded vertex candidates for the observed M4A4
idle framing: 1600?900, viewmodel FOV60, offsets1,1,-1. It reuses the existing
MDL reader and skeletal skinning, with no new parser or asset files.

```powershell
cargo run -p launcher --example vmprobe2 -- models/weapons/v_rif_m4a1.mdl ACT_VM_IDLE --sight-rois
```

The fixed regions cover the front post, rear aperture, carry-handle tip and
receiver side. Each reports at most128 exact-position-unique vertices, sorted
by depth, with model/checksum and mesh/vertex provenance. This is a developer
measurement aid; it does not establish that a vertex is visible or corresponds
to a particular image edge. A hole center has no surface vertex. Raw geometry
and image overlays belong in private observation output, never in the repo.

At source81395c6, all23 workspace packages were cleaned for the dev profile,
then the example built and ran against installed Legacy M4A4 checksum1954873999
with exit0. The immutable binary was checked for its new body-region marker;
actual output included110 front,84 aperture,40 carry-handle and104 receiver
candidates. The projected body regions were visually checked against the
qualified native baseline. Full CI38810149963 passed both workspace/real-data
and no-assets jobs on that exact source. Later integration with master preserves
this example unchanged; final-head CI verifies the combined tree.

These observations are not a bob implementation. The public base viewmodel
origin/rotation fields stayed unchanged while visible walking bob occurred,
so they were rejected as a rendered-pose receiver. Thin three-point pose fits
and a dense temporal edge tracker were also rejected: low image residual alone
did not preserve anatomical identity on held-out frames. The finite body
landmarks improve pose constraints but do not prove periodic phase, historical
2015 fidelity or behavior for other weapons. Look motion is separate; see
[VIEWMODEL_MOTION.md](VIEWMODEL_MOTION.md).
