# Bounded targeted render trace (source-only R26 profile)

Status: **R26 changes SOURCE ONLY, UNCOMPILED; new fixtures and commands below UNRUN for R26.**
This is an attribution instrument for the next root-controlled diagnostic QA,
not an optimization or an admitted performance result. Cargo.lock and manifests
remain unchanged; no new dependencies or features are needed. The parent must
verify the resolved diagnostic/default feature graph under its exclusive Cargo
slot. Do not ship this feature in the
normal release. No launch, process guard, preferences wrapper or installer is
provided by this module.

## Gates and output

`client/targeted-render-trace` enables `bevy/trace`, `bevy/debug`, and optional
tracing 0.1.44 / tracing-subscriber 0.3.23 dependencies. **Without the feature,
the module, environment reads and instrumentation installation are compiled
out.** No default feature or normal release command changes. With the feature
but without `CSR_RENDER_TRACE_PATH`, our capture adds no clocks, locks,
resources, queries, message readers or systems at runtime. Bevy's compiled
trace/debug features still add engine spans, names, subscriber work and the
trace panic hook even when our runtime capture is off. Such a binary is not a
performance baseline. Do not additionally enable `trace_chrome`, `trace_tracy`
or `detailed_trace`; a bounded file does not remove engine trace overhead.

With PATH set, the bounds and identity variables below are required. SCOPES is
optional: absence preserves the original five systems and v2 bytes. The only
explicit value is exactly `render-phases`; empty/unknown/non-Unicode values fail
startup, even without PATH. Invalid input never selects a fallback profile:

| Variable | Meaning / hard bound |
|---|---|
| `CSR_RENDER_TRACE_PATH` | ASCII absolute local Windows drive path for a **new** TSV; existing parent, outside the independent build-time source workspace, current directory, GameData.game_dir, CSR_DATA_DIR and existing installed userdata directory. Existing files are never overwritten. See namespace policy below. |
| `CSR_RENDER_TRACE_SCOPES` | Optional. Absent: original five/v2. Exactly `render-phases`: three function systems + two existing direct spans/v3, described below. |
| `CSR_RENDER_TRACE_RUN_ID` | 1..64 ASCII letters/digits/`-_.`; join key for the parent's immutable executable/build/context receipt. |
| `CSR_RENDER_TRACE_START_S`, `CSR_RENDER_TRACE_END_S` | Finite sim-time `[start,end)`, start > 0, end > start, width <= 120 seconds. Zero start is rejected up front: a nonnegative `lower_previous` observation below it is impossible. |
| `CSR_RENDER_TRACE_WALL_S` | Finite startup-relative deadline in `(0,300]` seconds, including loading. |
| `CSR_RENDER_TRACE_MAX_RECORDS` | 1..100000, combined boundary + context + sample rows. |
| `CSR_RENDER_TRACE_MAX_BYTES` | 4096..16777216, **whole file** including names/header/footer. |

The diagnostic source-workspace boundary is derived from the client package's
build-time CARGO_MANIFEST_DIR, independently of launcher CWD/CSR_DATA_DIR.
It is used only as a deny boundary, never as a runtime save base. All protected
roots and the existing output parent are canonicalized; an unavailable build
workspace fails closed. Keep that workspace available for a future immutable
capture. No save-path, launcher setup or config behavior changes.

This local diagnostic supports `C:\...` (or `C:/...`) input on Windows only.
UNC/network paths, device paths, **all user-supplied verbatim paths** (even
`\\?\C:\...`), generic verbatim/volume namespaces, drive-relative `C:...` and
root-relative `\...` paths are unsupported and rejected before resolution/file
open. GetDriveTypeW must report a local fixed/removable/RAM drive; mapped network,
unknown and unavailable drives fail closed. The canonical output parent and
every canonical protected root must also satisfy the local-drive policy before
file creation. Canonicalization's `\\?\C:\...` form is accepted at that stage;
canonical UNC/device/generic verbatim results fail closed. Resolved links into
protected local directories are still denied. No directory is created.

All participating canonical paths must be ASCII; unsupported Unicode fails
closed rather than approximating Windows Unicode case rules. Drive and directory
comparisons ignore ASCII case, accept the normal/canonical drive spellings, and
require a component boundary, so `trace-r20-sibling` is outside `trace-r20`.
Case-sensitive directories are conservatively denied on a case-insensitive
match. A protected drive root denies that entire drive. Unavailable or unsupported
protected roots prevent capture, including the independent source workspace even
with private CWD/CSR_DATA_DIR. This is a constrained namespace policy, not a
universal directory-identity or race-resistance guarantee. Root must later verify
real canonicalization, drive classification and new-only I/O under its guards;
the authored policy fixtures use injected results without files/shares/env edits.

Unset RUST_LOG and TRACE_CHROME for this capture. RUST_LOG is rejected because
Bevy's outer EnvFilter can otherwise suppress selected INFO spans. The standard
LogPlugin remains installed, with its supported `custom_layer` hook. The layer
accepts exact `system` metadata at SystemMeta's callsite and String-valued full
`name` fields for the selected profile. Only render-phases also accepts the two
exact INFO direct-span metadata tuples below. No event rows, arbitrary name histories,
Chrome trace, per-joint locks or broad engine interception are added. Other
system callbacks still pay filtering/registry overhead in the diagnostic run.

One read-only ExtractSchedule context system is ordered before extract_skins.
It copies Bevy FrameCount plus SimWorld.tick/time(); desktop_plugins already
disables pipelined rendering. Subsequent selected extraction/render callbacks
use that same identity. This relies on that desktop policy: do not reuse this
collector in another pipelined app. FrameCount is sampled **after main Last**;
it is not a presentation ID and must not be equated to an unrelated logger's
frame counter. Root's receipt should preserve map/mode, camera/pose history,
actual bot/corpse context, executable hash, source SHA, build cwd and wrapper
identity. No queries or gameplay mutation are added to obtain those counts.

## Root-controlled QA commands (future; not executed for R26)

Only after root review and acquisition of the exclusive Cargo slot, verify the
resolved graph. For a shared-target worktree handoff, run the existing
`tools/clean-workspace-profile.ps1 -Profile play -TargetDir <absolute target>`
from the actual build worktree after dependency resolution (the helper uses
locked metadata). Then the diagnostic-only build/fixture commands are:

```powershell
cargo build --profile play -p launcher --features client/targeted-render-trace
cargo test --profile play -p client --features targeted-render-trace targeted_render_trace
```

Parent still owns the repository's required workspace/probe/sim checks and an
independent default-feature build. Do not run these commands during the source
freeze. Never treat an old shared-target executable as this source's artifact:
record actual cwd, copy the newly built exe immutably and record its hash.

Set these **only in the existing private launch wrapper's child environment**,
using a fresh filename/run ID per capture. The private parent directory must
already exist. This table is an environment recipe, not a launch script:

```text
CSR_RENDER_TRACE_PATH=C:\Users\shawn\csr-fidelity\post-playtest-20261016\r20-capture-01.tsv
CSR_RENDER_TRACE_RUN_ID=r20-capture-01
CSR_RENDER_TRACE_START_S=30
CSR_RENDER_TRACE_END_S=60
CSR_RENDER_TRACE_WALL_S=180
CSR_RENDER_TRACE_MAX_RECORDS=100000
CSR_RENDER_TRACE_MAX_BYTES=16777216
```

That recipe deliberately keeps the old v2 behavior. For a new render-phases
capture, root must clone/review a **new** private helper, explicitly pass
`CSR_RENDER_TRACE_SCOPES=render-phases` in its child environment, use a fresh
run/output path and require the exact v3 contract. Old R22 private wrappers
cannot emit this profile/v3. Do not edit old helpers, captures or receipts or
label an old v2 artifact as v3. No private helper is changed by R26.

Use the parent's existing private data/prefs/process guards and finite owned
process lifetime; this code neither launches nor stops a game. Any native
Legacy helper still uses with-native-slot.ps1 and its own guards. No admin,
WPR/xperf, driver, service, Steam edit or owner config change is needed for this
capture. Do not bypass the private wrapper by invoking an exe directly.

The collector buffers capped whole rows and writes and closes its new file
once, at sim end, deadline, a cap/failure, or normal runner return. Wall deadline
checks happen on extraction and selected callbacks; it is not a watchdog that
interrupts a stalled engine call. Collection cannot grow while stalled. Root's
process guard remains responsible for hangs. A hard kill/write failure may
leave an empty/partial file: no valid terminal footer means reject it. Caps
stop collection, never wrap or silently truncate individual rows. A context
rewind/loss, overlapping instance or mismatched invocation also stops it.

Offline, with no game process needed:

```powershell
python -B tools/fidelity/summarize-targeted-render-trace.py --structural-only C:\Users\shawn\csr-fidelity\post-playtest-20261016\r20-capture-01.tsv
```

This bounded standard-library parser has **only structural-only mode**. The
explicit flag is mandatory. Exit 0 means structurally complete supported v2/v3 rows only;
`evidence_complete=False` is unconditional. Missing or mismatched parent
receipts cannot receive evidence-qualified success: this tool has no receipt
join/evidence mode and rejects receipt/expected-run CLI arguments. Root must
separately preserve and validate run ID, exact source SHA, real build cwd,
immutable exe hash/path, map/mode/whole-interval pose/history, wrapper/process/
prefs guard identity and clock/frame joins before using an artifact as evidence.
A run label or this tool's exit code is not such a validation.

Both profiles retain three actual frame/tick/sim_s/start_ns boundary
observations: `lower_previous` (immediately before start), `lower` (first
in-window extraction) and `upper` (first extraction at/after end). `lower`
also acts as the first context. Before crossing, only one predecessor is
retained in memory. First observation already at/after start, even exactly at
start, fails `missed_start`; a jump over the whole interval also fails. Context
loss/rewind, a frame gap (including either crossing), open scopes at a context
transition, or any selected scope missing from the preceding context stops
collection. No per-frame run-condition exception is assumed. Boundary receipts
record the actual discrete extraction times, not interpolated samples exactly
at 30 and 60. The sim tick can advance by multiple ticks between consecutive
render contexts; the parent must assess actual boundary/cadence history.

Header bounds and every context/sample `sim_s` use round-trip f64 Display
formatting of the values used by the collector. This preserves the exact
f32-to-f64 SimWorld time conversion across fractional boundaries. The parser
keeps exact comparisons, with no epsilon. Authored Rust and independent literal
Python fixtures share a 20-tick `[0.0500000009,0.2)` contract: tick 1 is
`0.05000000074505806` and remains below start (the former nine-decimal
`0.050000001` would be above it). These fixtures have not been executed for R23.

A structurally complete interval needs both crossings, contiguous increasing
frame IDs, monotonic tick/sim/clock, `reason=window_end`, zero open scopes, and
at least one invocation of **each** of the five selected scopes in **each** observed in-window
context. The parser prints actual observed range and boundaries, frame gaps,
and per-scope covered/observed-context denominator and missing count beside
invocation mean/median/p95/max wall ms. Missing whole contexts are reported
separately; they have no invented invocations. A retained incoming context on
a failure can have no samples because collection stopped there. Any partial,
capped or invalid artifact exits 2. Preserve raw rows and status even on failure.

Metadata keys must be unique and complete, with exact feature/Bevy/clock values,
the profile's exact ordered map, its exact ten-column schema, and supported
hard bounds/domains. Context-only fields must be empty. Samples require active
frame/tick/sim identity, positive process-local span/thread IDs, consistent
span-to-scope mapping, `context_changed=false`, start at/after context, exit
before the next context/deadline, monotonic completion order and no same-scope
overlap. Different scopes may overlap and rows may be interleaved by
completion order; start timestamps need not be globally sorted. A reused span
has separate invocation rows. None of these checks authenticates a raw file,
proves callback delivery, or supplies the missing external identity receipts.

Authored **UNRUN** Python fixtures are in
`tools/fidelity/test_targeted_render_trace.py`; future parent-only command:

```powershell
python -B -m unittest discover -s tools/fidelity -p test_targeted_render_trace.py
```

## Default v2 scope and interpretation (SCOPES absent)

| Index | Exact function-system scope | Limit |
|---|---|---|
| 0 | extract_skins | Extraction, outside the existing render.prepare scheduler bracket. |
| 1 | prepare_skins | Whole system CPU wall; not GPU transfer execution. |
| 2 | prepare_erased_assets< MeshMaterial3d<WorldMaterial> > | Exactly this monomorphization; no texture/material event counts are inferred. |
| 3 | prepare_mesh_bind_groups | Whole mesh bind-group system, including its other mesh categories. |
| 4 | render_system | Combined graph/encoding/submission/present/other nested work. |

The code stores full engine type names in the header and matches them exactly.
The fixture obtains names through public IntoSystem/System APIs to catch drift
(including the private defining module `bevy_pbr::mesh_material` in the type
name). Missing names are a failed diagnostic, not zero-cost functions.

SystemMeta's persistent spans enter in FunctionSystem::run_unsafe and
ExclusiveFunctionSystem::run_unsafe. Durations cover the engine's function-system
wrapper, parameter work and nested calls; they include waits/descheduling and
subscriber overhead. They are not exclusive active CPU time. Concurrent scopes
may overlap: do not sum them as frame time or subtract them from scheduler
brackets. No extra before/after scheduler timing systems are used.

The default profile deliberately records only these five function-system spans.
Neither wgpu 29.0.4's
public Queue::submit nor SurfaceTexture::present exposes the requested exact
call spans here. Index 4 therefore remains combined; there is no claimed exact
submit/present timing, GPU duration, presentation latency or cause of backend
waiting. No GPU/presentation queries or barriers are introduced.

Use the trace to determine whether the named skin/material/bind-group CPU wall
scopes explain a substantial part of the observed interval. If they are small,
stop treating them as the leading explanation. Large combined render_system
wall with small prepare scopes leaves GPU/back-pressure/driver/presentation
uncertainty open. This candidate does not count joint visits/upload bytes or
material modifications/retries. It cannot by itself establish those mechanisms,
causality, a GPU bottleneck, an FPS gain or performance acceptance. The existing
unadmitted S2/A2 histories remain unadmitted; do not optimize from them.

## render-phases v3 scope and interpretation (explicit selection only)

This is a separate small measurement, with exactly five scopes replacing the
old five for this capture. It uses the same collector, before-extract_skins
context, five-bit mask (31), per-context all-five coverage, crossings, caps,
path guard and finalization. There is no extra ECS/entity census, GPU plugin,
statistics, engine fork, private API call or optimization.

| Index | Kind | Exact name | Exact tracing target |
|---|---|---|---|
| 0 | system | `bevy_render::view::window::prepare_windows` | `bevy_ecs::system::function_system` |
| 1 | system | `bevy_pbr::material::queue_material_meshes` | `bevy_ecs::system::function_system` |
| 2 | system | `bevy_render::view::prepare_view_targets` | `bevy_ecs::system::function_system` |
| 3 | span | `queue_submit` | `bevy_core_pipeline::schedule` |
| 4 | span | `present_frames` | `bevy_render::renderer` |

All new-profile metadata requires INFO. The first three use the `system` span's
String name field and public IntoSystem name fixtures; queue_material_meshes is
**non-generic**, across materials/views. The last two use exact span metadata
name/target/level, independent of fields. No suffix or broad trace matching.

The first line is `# csr_targeted_render_trace_v3` with the existing metadata
plus mandatory `scope_set=render-phases`. Its exact ordered map is:

```text
# scope 0 kind=system name=bevy_render::view::window::prepare_windows target=bevy_ecs::system::function_system level=INFO
# scope 1 kind=system name=bevy_pbr::material::queue_material_meshes target=bevy_ecs::system::function_system level=INFO
# scope 2 kind=system name=bevy_render::view::prepare_view_targets target=bevy_ecs::system::function_system level=INFO
# scope 3 kind=span name=queue_submit target=bevy_core_pipeline::schedule level=INFO
# scope 4 kind=span name=present_frames target=bevy_render::renderer level=INFO
```

The ten columns retain order/types, with column 7 named `scope` instead of
`system`. Old v2 metadata/map/columns remain exact and readable. Unknown
profiles/schemas/maps, wrong kinds/targets/levels, mixed v2/v3 headers/maps or
columns are rejected. No domain/time/coverage/structural-only check is relaxed;
the existing footer reason `missing_function` also covers a missing direct span.

prepare_windows includes window checks/acquisition/reconfiguration/skip paths;
it is not pure acquire wait. queue_material_meshes performs CPU render-phase
queueing, not wgpu submission or specialization/compilation. prepare_view_targets
includes cached target setup and enqueuing Commands, excluding later deferred
application and separate prepare_view_attachments.

Installed public Bevy 0.19.1 source defines queue_submit around the main pending
buffer queue-resource lookup and submit call. PendingCommandBuffers::take finishes
encoders **before** this span. Other direct submits (screenshot/readback and
uncovered swapchain clear included) are outside it. The span is conditional on
nonempty pending buffers: if absent in a player mode/renderer cadence, fail
coverage; never waive it, emit zero, or call a reduced mask complete.
present_frames encloses window iteration, view queries/checks and any presents
as a group. It is not exact per-surface present, display/scanout latency or GPU
execution. Both scopes include body overhead, possible waits and descheduling.

Prior closed old-five observations (3322 contexts, combined render_system mean
2.8311 ms) and the skin-end-to-bind-start wall gap are attribution motivation
only. A wall gap is not CPU/GPU attribution. Existing surface scheduler brackets
and retained 120-measurement GPU histories do not supply an exact frame GPU
timeline. Do not sum concurrent scopes, subtract across captures/brackets or
declare a bottleneck, FPS gain or performance acceptance from this source.

Rust fixtures cover a successful complete interval with actual crossing
receipts and overlapping different functions, missed/late starts, missing
later functions/contexts/upper receipt, bounds/caps/deadline, context/scope
failures, and an independent workspace boundary with private CWD/data inputs.
Existing public engine-name/collision/subscriber fixtures remain. Python
fixtures include every R19 parser counterexample, missing/mismatched receipt
attempts rejected by the structural-only interface, and a valid overlapping
producer-shaped interval.
R23 adds round-trip fractional-boundary rows, zero-start rejection and Windows
namespace/case/drive/unavailable-root policy fixtures. R26 preserves the existing
eleven Rust and eleven Python fixture groups and adds authored **UNRUN**
profile/name/schema, real Layer direct-span rejection, reuse/concurrency,
all-five and omitted-direct-span fixtures. The independent literal v3 contract
does not derive from parser constants. Prior fixture results do not verify
these additions. No compilation, parser import/execution, tests or runtime
verification occurred for R26.
