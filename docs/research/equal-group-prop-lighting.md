# Equal-group static-prop lighting candidate

This candidate replaces the ambient-cube/Sun approximation only for static
prop meshes with an admitted map-scoped VHV correspondence and a restricted
payload. It is not yet a verified VHV color decoder; native and candidate
captures must admit or reject the proposed channel and transfer interpretation
before this branch is merged.

The existing VHV parser checks version, bounded spans, counts and non-overlap.
The selected map's content identity, BSP SHA1 and static-prop ordinal select
`sp_hdr_<ordinal>.vhv` from that map's embedded archive only. The candidate
also requires the model checksum, complete mesh counts, LOD0/default-submodel
hardware identities and per-mesh hardware vertex counts. The proposed record
correspondence indexes the existing hardware-output vertex traversal, never the
fixup-resolved VVD vector. Those count/checksum gates do not themselves prove
individual ordering; multiple spatially separated native prop/vertex regions
must admit or reject the correspondence along with the color interpretation.

Only raw flags2, three identical four-byte groups per record, and zero fourth
bytes qualify. One unsupported record rejects that complete mesh. Only the
renderer's non-normal-mapped `VERTEX_LIT` material path uses these colors.
Missing, unsupported, directional, nonzero-control or mismatched data preserves
the previous ambient-cube/Sun calculation. The MDL library retains its opaque
payload API; the restricted interpretation lives in the renderer.

The proposed interpretation is normalized blue-first8-bit components, with no
additional gamma or scale operation. The existing exposure, material shader,
texture sampling, geometry, indices and batching remain unchanged. This is a
testable candidate, not a conclusion from the byte size or flag alone. Prior
native tire interventions support physical lane0 influencing blue and one
hardware-order cohort; they do not establish a complete decoder or transfer.

## Installed-data scope

The pre-rework archived Dust II BSP SHA1
`8d0bfcabeecc4600208f495308a50681ca7e9eeb` contains1,523 props and1,523 VHV
entries with949,935 opaque records. Decorative arch instances244/264 each
have1,203 records. Their VHV, MDL, VVD and VTX checksums match, all three
groups are identical and all fourth bytes are zero. Nearby crate247 satisfies
the same byte restrictions. This identifies a concrete classic-map target;
it does not prove native consumption of those exact entries.

Current reworked Dust II has its own distinct BSP/content scope. Its tunnel
arch2813 has3,690 authored VHV records but a normal-mapped material, so this
restricted candidate leaves that material on its previous lighting path.

Private admission reports are retained under
`C:\Users\shawn\csr-fidelity\lighting-overnight-20261015`, including
`classic-arch-vhv-admission-private.json`,
`classic-vhv-record-statistics-private.json` and
`tunnel-arch-vhv-admission-private.json`. No game bytes or images are committed.

## Verification pending

The source has not yet received a Cargo build lease. Focused decoder checks,
workspace checks, release probe/simulation and matched original-map
native/before/after renders remain required. Capture the same resolved BSP,
viewport, eye/angles and video settings, then personally inspect the actual
arch pixels. Do not call an unmeasured correction native or historical parity.
