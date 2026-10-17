# Bounded targeted render trace (source-only R20 candidate)

Status: **SOURCE ONLY, UNCOMPILED; all fixtures and commands below UNRUN.**
This is an attribution instrument for the next root-controlled diagnostic QA,
not an optimization or an admitted performance result. Cargo.lock deliberately
remains unchanged; the parent must resolve the optional dependency/feature graph
when its exclusive Cargo slot is available. Do not ship this feature in the
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

With PATH set, every variable below is required. Invalid input refuses startup
rather than silently running an unbounded collector:

| Variable | Meaning / hard bound |
|---|---|
| `CSR_RENDER_TRACE_PATH` | Absolute **new** TSV; existing parent, outside the independent build-time source workspace, current directory, GameData.game_dir, CSR_DATA_DIR and existing installed userdata directory. Existing files are never overwritten. |
| `CSR_RENDER_TRACE_RUN_ID` | 1..64 ASCII letters/digits/`-_.`; join key for the parent's immutable executable/build/context receipt. |
| `CSR_RENDER_TRACE_START_S`, `CSR_RENDER_TRACE_END_S` | Finite sim-time `[start,end)`, start >= 0, end > start, width <= 120 seconds. |
| `CSR_RENDER_TRACE_WALL_S` | Finite startup-relative deadline in `(0,300]` seconds, including loading. |
| `CSR_RENDER_TRACE_MAX_RECORDS` | 1..100000, combined boundary + context + sample rows. |
| `CSR_RENDER_TRACE_MAX_BYTES` | 4096..16777216, **whole file** including names/header/footer. |

The diagnostic source-workspace boundary is derived from the client package's
build-time CARGO_MANIFEST_DIR, independently of launcher CWD/CSR_DATA_DIR.
It is used only as a deny boundary, never as a runtime save base. All protected
roots and the existing output parent are canonicalized; an unavailable build
workspace fails closed. Keep that workspace available for a future immutable
capture. No save-path, launcher setup or config behavior changes.

Unset RUST_LOG and TRACE_CHROME for this capture. RUST_LOG is rejected because
Bevy's outer EnvFilter can otherwise suppress selected INFO spans. The standard
LogPlugin remains installed, with its supported `custom_layer` hook. The layer
only accepts `system` span metadata at SystemMeta's callsite; exact full `name`
fields select the five systems below. No event rows, arbitrary name histories,
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

## Root-controlled QA commands (future; not executed for R20)

Only after the active controls checks close, resolve Cargo.lock and acquire the
exclusive Cargo slot. For a shared-target worktree handoff, run the existing
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
explicit flag is mandatory. Exit 0 means structurally complete v2 rows only;
`evidence_complete=False` is unconditional. Missing or mismatched parent
receipts cannot receive evidence-qualified success: this tool has no receipt
join/evidence mode and rejects receipt/expected-run CLI arguments. Root must
separately preserve and validate run ID, exact source SHA, real build cwd,
immutable exe hash/path, map/mode/whole-interval pose/history, wrapper/process/
prefs guard identity and clock/frame joins before using an artifact as evidence.
A run label or this tool's exit code is not such a validation.

The v2 artifact retains three actual frame/tick/sim_s/start_ns boundary
observations: `lower_previous` (immediately before start), `lower` (first
in-window extraction) and `upper` (first extraction at/after end). `lower`
also acts as the first context. Before crossing, only one predecessor is
retained in memory. First observation already at/after start, even exactly at
start, fails `missed_start`; a jump over the whole interval also fails. Context
loss/rewind, a frame gap (including either crossing), open scopes at a context
transition, or any selected function missing from the preceding context stops
collection. No per-frame run-condition exception is assumed. Boundary receipts
record the actual discrete extraction times, not interpolated samples exactly
at 30 and 60. The sim tick can advance by multiple ticks between consecutive
render contexts; the parent must assess actual boundary/cadence history.

A structurally complete interval needs both crossings, contiguous increasing
frame IDs, monotonic tick/sim/clock, `reason=window_end`, zero open scopes, and
at least one invocation of **each** function in **each** observed in-window
context. The parser prints actual observed range and boundaries, frame gaps,
and per-function covered/observed-context denominator and missing count beside
invocation mean/median/p95/max wall ms. Missing whole contexts are reported
separately; they have no invented invocations. A retained incoming context on
a failure can have no samples because collection stopped there. Any partial,
capped or invalid artifact exits 2. Preserve raw rows and status even on failure.

Metadata keys must be unique and complete, with exact feature/Bevy/clock values,
all five exact ordered system names, the exact ten-column schema, and supported
hard bounds/domains. Context-only fields must be empty. Samples require active
frame/tick/sim identity, positive process-local span/thread IDs, consistent
span-to-system mapping, `context_changed=false`, start at/after context, exit
before the next context/deadline, monotonic completion order and no same-system
overlap. Different functions may overlap and rows may be interleaved by
completion order; start timestamps need not be globally sorted. A reused span
has separate invocation rows. None of these checks authenticates a raw file,
proves callback delivery, or supplies the missing external identity receipts.

Authored **UNRUN** Python fixtures are in
`tools/fidelity/test_targeted_render_trace.py`; future parent-only command:

```powershell
python -B -m unittest discover -s tools/fidelity -p test_targeted_render_trace.py
```

## Scope and interpretation

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

Bevy has a coarse present_frames span, but this bounded collector deliberately
records only the requested five function-system spans. Neither wgpu 29.0.4's
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

Rust fixtures cover a successful complete interval with actual crossing
receipts and overlapping different functions, missed/late starts, missing
later functions/contexts/upper receipt, bounds/caps/deadline, context/scope
failures, and an independent workspace boundary with private CWD/data inputs.
Existing public engine-name/collision/subscriber fixtures remain. Python
fixtures include every R19 parser counterexample, missing/mismatched receipt
attempts rejected by the structural-only interface, and a valid overlapping
producer-shaped interval. All fixtures and the helper remain **UNRUN**; no
compilation, feature resolution or runtime verification has occurred in R20.
