# Equal-group static-prop lighting candidate

This candidate replaces the ambient-cube/Sun approximation only for static
prop meshes with a map-scoped VHV correspondence and a restricted payload.
Matched classic Dust II captures show a substantial correction on crate247,
but decorative arches244/264 remain too bright. This is a partial visible
improvement, not a verified general VHV color decoder or native lighting parity.

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
Matched native admission covers static-prop instances with common flags0.
The October 18 source candidate also accepts lighting-origin/no-cast-shadow modes as
described below; all other combinations retain the existing fallback.
Missing, unsupported, directional, nonzero-control or mismatched data preserves
the previous ambient-cube/Sun calculation. The MDL library retains its opaque
payload API; the restricted interpretation lives in the renderer.

## Authored receiver modes, October 18 (runtime validation pending)

Installed modern Inferno apartment trims use common flags3 (authored lighting
origin plus fading) and carry the same equal-group, zero-control VHV form.
The zero-flag gate discarded these baked inputs before checking correspondence,
leaving their lighting on the hull-center fallback. Mirage's cardboard uses
no-cast-shadow flag0x10 and the same restricted baked form; it previously lost
that input to an approximate spatial ambient/Sun receiver. Lighting-origin and
no-cast-shadow modes (optionally fading) now enter the same
map-scoped identity, model checksum, hardware mesh/count and payload checks.
This consumes the baked result; it does not estimate a lighting origin, broaden
spatial fallback eligibility, decode directional records or alter color units.
Fade-only flags1 and other previously rejected modes remain rejected.

This is a source candidate, not a new native admission. Parent-owned tests and
matched upper/lower trim views in both apartments must verify the change,
including instance tint and preservation of accepted Nuke/old Dust II lighting.
The two new ignored installed-data regressions are also **UNRUN** by the source child.

The proposed interpretation is normalized blue-first8-bit components, with no
additional gamma or scale operation. The existing exposure, material shader,
texture sampling, geometry, indices and batching remain unchanged. This is a
testable candidate, not a conclusion from the byte size or flag alone. Prior
native tire interventions support physical lane0 influencing blue and one
hardware-order cohort; they do not establish a complete decoder or transfer.

## Installed-data scope

The pre-rework archived Dust II BSP SHA1
`8d0bfcabeecc4600208f495308a50681ca7e9eeb` contains 1,523 props and 1,523 VHV
entries with 949,935 opaque records. Decorative arch instances244/264 each
have 1,203 records. Their VHV, MDL, VVD and VTX checksums match, all three
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

## Matched rendered observations

Source `00da221` was included in the frozen root integration build `3f654a33`
(crates tree `6a25fa05fa675ee8b1db3491b258e74b177d1a92`) with the separate
grenade and video-settings changes. Those changes do not alter static-prop
lighting. The immutable PLAY executable SHA256 is
`b46e75d1819ef3c1c6a1d24b364352869fb0ec55ba6e7ab2fb5d100e43353fee`.
The before executable is the previous verified RELEASE union, with crates
identical to merged master `2d4366f`, SHA256
`76eaa58bcbe1fba4f3c26ffc9966533559331937024bf4c6264588fb8adbc477`.
The profile difference permits a visual comparison, not an FPS comparison.

Both native views and all four before/after images were personally inspected.
All use the same classic BSP bytes, a 1600x900 viewport, eye
`-1810 1169 140`, pitch5, and yaw90 (north) or -90 (south). The native mount
was private-first with the read-only installed Legacy directory as fallback;
its acknowledged search path, selected relative map and before/after map hashes
were retained. This establishes map selection through the acknowledged mount
contract, not an absolute native filesystem-open trace. Native preferences,
installed configuration and map bytes were unchanged after cleanup.

The captured `00da221` candidate logs admitted 577 meshes / 224,398 vertices on
this classic map, including both 1,203-vertex arch meshes and the 576-vertex
crate247 mesh. The subsequent flags0 containment gate narrows this scope;
those earlier aggregate counts must not be presented as post-gate counts.
All three measured target instances have flags0 and remain eligible.
Filtering the retained pre-gate per-instance log against actual BSP flags
predicted173 meshes /95,965 vertices after containment. The later actual
flags0 packet024127 against frozen union220b107 confirms exactly those counts
in both owned north/south renders, with both target arches and crate247 still
admitted. These later runtime counts must not be attributed to the earlier
pre-gate executable.
The result is local: previously near-black arch/crate surfaces recover visible
stone/wood details, while the sampled world-wall controls are byte-identical
before and after. The crates are close to the native reference; the arches
are visibly overbright. Missing exterior glow, world lighting and other
material differences remain outside this change.

Raw RGB errors below are in 0..255 levels, with no image registration, color
correction or fitted parameters. Regions were selected from the actual
surfaces before computing metrics; they were not preregistered. Native
automatic exposure was not independently measured. These limits matter when
interpreting absolute brightness.

| Surface region | Before RGB MAE | After RGB MAE | Before / after block8 luma SSIM |
| --- | ---: | ---: | ---: |
| Crate247 upper patch | 65.00 | 5.98 | 0.259 / 0.838 |
| Crate247 middle patch | 85.93 | 7.21 | 0.241 / 0.847 |
| Crate247 lower patch | 103.37 | 11.55 | 0.258 / 0.843 |
| Arch264 north left face | 25.79 | 26.05 | 0.404 / 0.790 |
| Arch264 north crown | 16.01 | 28.78 | 0.559 / 0.713 |
| Arch244 south left face | 19.46 | 25.99 | 0.535 / 0.766 |
| Arch244 south shoulder | 24.95 | 25.31 | 0.487 / 0.812 |
| Arch244 south crown | 24.97 | 23.44 | 0.378 / 0.641 |

The crate's three separated patches support the proposed channel/normalized
record path. Improved arch structural similarity does not cancel its increased
brightness error: several arch regions are worse in raw RGB MAE. The candidate
must not be described as an exact transfer function or complete arch fix.

## Remaining arch defect and bounded audit

The selected classic arch and crate instances use skin0, zero common flags,
zero extended flags and white diffuse modulation. Their model flags are the
same (`0x11`). Both resolved materials are plain `VertexLitGeneric` with a
base texture and no authored color/color2 modulation or normal map. The map
contains no replacement model or material for those paths. Both base textures
are DXT1 with zero texture flags; the current renderer uploads their full mip
chains as sRGB. No differing material modulation has been found that justifies
an arch-only scale or gamma adjustment. Matching silhouettes and material
details support the model selection, but native model/material filesystem
opens have not been traced.

An independent audit compared hardware-output and source-VVD indexing. Both
target meshes have a complete bijection, but fewer than 0.5% of ordinals are
identical. The current hardware correspondence preserves distinct lighting
for differently oriented crate faces; indexing the source VVD vector instead
largely erases that distinction. This supports retaining hardware order. It
does not prove every record's native spatial correspondence or resolve the
arch's brightness error. No fitted per-model correction was added.

Other classic prop flag combinations have not been visually admitted and are
now explicitly rejected before VHV loading. Directional payloads,
nonzero fourth bytes and normal-mapped materials retain the previous fallback.

## Verification and retained negatives

The source CI run `38854784894` passed the complete workflow. The combined
PLAY build passed client/simulation/level/netcode tests, the launcher build,
normal Dust II probe/simulation and classic Dust II simulation. Its classic
probe reported ten failures: nine `EmptyTopology` faces and the missing
`greygrid` dependency on `millwall_01`. The actual immutable baseline reports
the same eleven diagnostic lines byte-for-byte; the candidate build receipt
records `classic_probe_pass=false`. This is a qualified existing negative,
not a successful classic probe.

The nine faces contain only zero-area compiled primitive triangles in both
f32 and f64 arithmetic. An invented polygon fan would create geometry on some
of them and is not an appropriate lighting fix. The missing material belongs
to one tiny nonzero-area triangle on a separate millwall prop. Neither finding
accounts for the visible tunnel arches. No map bytes were changed.

The final flags0 source was admitted in two actual PLAY renders024127. South
is byte-identical to the prior after image; north full-frame RGB MAE is0.00225,
with measured arch/crate patches0..0.0049 and both wall controls unchanged.
Root personally reviewed all six original native/before/after images and both
final flags0 images. PR163 then merged after full CI and combined RELEASE9197101C
(build, workspace, installed-map probe/simulation, classic-map simulation,
1,425 sound decodes and actual frontend/Video/match lifecycle). Classic DustII
probe still reports the10 independently matched baseline faults; it is not a
passing whole-map probe. There is no separate RELEASE classic-pose pair or
complete native/historical parity claim. The earlier UV
transform PR158 was merged after root's separate combined RELEASE verification;
its reworked-map wall correction is distinct from this classic-map candidate.

Private packets retain the original safe failures as well as admitted images:

- `native-classic-vhv-reference-20261016-012621`: prelaunch map-hash pin typo;
  no native launch or preference changes.
- `native-classic-vhv-reference-20261016-013235`: north image admitted; the
  later south angle270 normalized to -90 and failed the strict numeric guard.
- `native-classic-vhv-south-reference-20261016-014302`: corrected south-only
  reference, exit0 and exact preference/config/map restoration.
- `csr-classic-vhv-pairs-20261016-015753`: all four owned captures exit0,
  no capture failure and no owned process remaining.
- `classic-vhv-local-comparison.json`: all six input image hashes and local
  surface metrics; `classic-vhv-hardware-vvd-order-audit.json`: order audit;
  `classic-prop-mode-material-audit-private.json`: mode/material audit and
  the explicitly predicted post-containment count.

The separate reworked-map cascade interventions remain negative observations:
existing entity Enable/Disable acknowledgements did not remove hard shadows,
and the public CVar off command was rejected before an off image was taken.
They do not establish that missing CSM is the cause of the classic arches.
