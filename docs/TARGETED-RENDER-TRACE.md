# Bounded targeted render trace (source-only R18 candidate)

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
| `CSR_RENDER_TRACE_PATH` | Absolute **new** TSV; existing parent, outside current data/workspace directory, GameData.game_dir, CSR_DATA_DIR and existing installed userdata directory. Existing files are never overwritten. |
| `CSR_RENDER_TRACE_RUN_ID` | 1..64 ASCII letters/digits/`-_.`; join key for the parent's immutable executable/build/context receipt. |
| `CSR_RENDER_TRACE_START_S`, `CSR_RENDER_TRACE_END_S` | Finite sim-time `[start,end)`, start >= 0, end > start, width <= 120 seconds. |
| `CSR_RENDER_TRACE_WALL_S` | Finite startup-relative deadline in `(0,300]` seconds, including loading. |
| `CSR_RENDER_TRACE_MAX_RECORDS` | 1..100000, combined context + sample rows. |
| `CSR_RENDER_TRACE_MAX_BYTES` | 4096..16777216, **whole file** including names/header/footer. |

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

## Root-controlled QA commands (future; not executed for R18)

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
CSR_RENDER_TRACE_PATH=C:\Users\shawn\csr-fidelity\post-playtest-20261016\r18-capture-01.tsv
CSR_RENDER_TRACE_RUN_ID=r18-capture-01
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
python tools/fidelity/summarize-targeted-render-trace.py C:\Users\shawn\csr-fidelity\post-playtest-20261016\r18-capture-01.tsv
```

This bounded standard-library parser prints per-system count/mean/median/p95/max
CPU wall ms. Exit 2 means invalid/incomplete coverage. A structurally complete
capture requires `reason=window_end`, zero open scopes and samples for all five
systems; `app_exit`, deadline and cap footers are explicitly incomplete. Keep
raw rows: each has extraction frame/tick/sim time, startup-relative monotonic
start_ns, duration_ns, selected system index, span ID and thread ID. A reused
span produces separate invocation rows. IDs are process-local; RUN_ID is the
cross-artifact join. Context rows record each sampled in-window frame, including
frames with missing selected invocations. The summary is interval-level only;
inspect/group raw rows by frame to investigate gaps and correlations.

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

The new Rust fixtures cover exact engine name drift/collisions, supported
subscriber routing, reused spans, finite half-open window/deadline, complete
capped rows/footer, and context/scope failures without files or game execution.
They and the offline summary helper are authored **UNRUN** pending parent QA.
