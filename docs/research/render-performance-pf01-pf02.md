# PF01/PF02: render attribution and sustained-play measurement

Source inspection starts at `af8f046` (October 16 feedback). This track adds
diagnostics, not a measured FPS fix. No Cargo/rustc/build, game, native helper,
soundcheck, shared-target mutation or owner-desktop operation was run here.
The frozen `feedback-20261016-9cabef2-staged` candidate was not changed.
Root owns build/runtime validation, publication and integration.

## Existing evidence and its limits

[Unused light clusters](light-cluster-overhead.md) records the previous
same-binary Nuke comparison: 1920x1080, five actual bots, CT spawn, deathmatch,
VSync off. Omitting unused camera clusters reduced mean frame time in both
orders, but did not establish uniformly better tails or native-class FPS.
That policy is already present; it is not a new fix in this track.

Private evidence inspected read-only:

- `C:/Users/shawn/csr-fidelity/nuke-frame-pacing-20261015/pr-body.md` and
  `render-pass-pr-body.md`: an older bounded normal-input Nuke run reported
  main 2.607 ms, render 7.331 ms, character presentation 0.552 ms and simulation
  0.482 ms per app frame. Different-binary/shared-load deltas were explicitly
  not accepted as causal speedups.
- `C:/Users/shawn/csr-fidelity/light-clustering-20261015/nuke-none-cc99bc7-3/stderr.log`:
  the last two complete interval reports had render wall means 6.451/6.524 ms,
  prepare 3.147/3.204 ms, submit 3.272/3.291 ms, surface 0.054/0.055 ms,
  main 2.448/2.625 ms and characters 0.490/0.448 ms.
- `C:/Users/shawn/csr-fidelity/bot-performance-final-bf03ac8/result.json`:
  owned normal-input fixture exited successfully; its short play/Pause exercise
  is not sustained-play attribution. The October 16 overnight proof directory
  mostly establishes feature correctness, not paired solo/bot frame distributions.

The sampled GPU pass means in the older Nuke log were much smaller than the
render schedule wall time. This does **not** establish that the whole scene
or all GPU queues were cheap: diagnostic paths can receive repeated samples
from multiple views, retained histories differ from interval distributions,
and the Scaleform worker renders outside Bevy's render diagnostic scopes.
Bevy 0.19.1 `diagnostic/internal.rs::sync_diagnostics` appends measurements by
path; do not sum path means as a per-frame GPU total. None of this old evidence
isolates the owner's current old-Dust-II bot case or late Nuke degradation.

## Ordered source-backed candidates

1. **Bot-associated render preparation and animated geometry.**
   `characters.rs::sync` evaluates a prepared character pose, copies the death
   pose, merges the held weapon skeleton, and writes joint transforms for every
   alive actor except the first-person target/explicitly hidden local actor.
   It does not use static-map PVS or camera frustum visibility to skip these
   evaluations. `skinning.rs::spawn` gives every skinned mesh
   `NoFrustumCulling`, including world weapons and corpses. Occluded/offscreen
   actors therefore still incur presentation and render preparation costs.
   `bone_merge` also searches parent bone names for every child bone per call;
   this is a cacheable static relation if profiling identifies a meaningful cost.
   Do not simply remove `NoFrustumCulling`: bind-pose bounds can cut off moving
   models. A future culling change needs conservative posed bounds, all camera
   layers and death/muzzle/lighting continuity verified. No pose rate or visual
   detail is reduced here.
2. **Corpses accumulating to a bounded steady state.**
   `death_bodies.rs` retains at most 16 bodies for 20 simulation seconds and
   advances physical bodies/evaluates fallback poses and writes every retained
   skeleton each render frame. It is nested inside `characters`. Async body
   cooking also consumes background CPU. This can explain a startup-to-combat
   cost increase that plateaus; the source does not prove an unbounded leak.
   New reports expose body/pending/physical counts and nested body wall time.
3. **Classic HUD work becoming active during combat.**
   `kill_feed.rs::update` keeps the Scaleform worker active while drawable
   notices, pending combat receipts or commands exist. Active kill-feed frames
   crop/upload the top half of the viewport; combat health/ammo components
   already avoid uploads for unchanged accepted component snapshots.
   `scaleform::run` steps/renders/readbacks separately from the main render
   scopes. Continuous bot kills can keep that worker active when a quiet solo
   range does not. Fixed image counts alone cannot rule out repeated upload,
   GPU readback or worker contention. The new `classic_hud` scope measures the
   main-thread pump, not the worker's entire CPU/GPU cost.
4. **Transient effects, item geometry and cosmetics.**
   `feedback.rs` budgets 512 impact/blood entities and expires them; authored
   bullet marks last 30 seconds. `utility_fx.rs` bounds field/explosion emitters
   and `Emitter::despawn` explicitly removes dynamic mesh assets. Emitters
   allocate fresh vertex attribute vectors and update meshes each frame;
   fire initialization also traces floor geometry per emitted particle.
   `items.rs` spawns a full skinned instance for each authoritative item;
   its removal search is items-by-items, while skin/decorations paths look up
   ground items per part. These costs scale with retained drops. However,
   `sim::death_drops` explicitly skips deathmatch, and competitive round resets
   clear items: ordinary deathmatch kills alone do not establish drop growth.
   Do not change sim item lifetimes or bot behavior in this render track.
5. **Scheduling and presentation, rather than scene complexity.**
   `input_latency::desktop_plugins` disables CPU frame-ahead pipelining and
   requests surface latency 1. These latency choices can serialize waits;
   the existing nested wall scopes include waits and are not additive CPU work.
   Preserve this input policy until a paired latency/throughput experiment
   supports a different choice. Requested present mode is not proof of the
   backend's actual swapchain/compositor behavior.

Static `visibility::update` only recalculates map-batch PVS when the camera
cluster changes. `MapClusters` conservatively includes every leaf intersecting
batch bounds. Large material batches can thus remain visible through a wide
area, but this does not by itself grow during a stationary session.

## Minimal new instrumentation

`CSR_PERF_REPORT=1` enables existing CPU wall scopes and Bevy render diagnostics
without individual slow-frame lines. `CSR_FRAME_LOG=<nonnegative ms>` retains
its old threshold behavior and also enables these summaries. With neither
switch set, the reporting systems/GPU diagnostic plugin are not installed;
new named scopes only load the existing relaxed enabled flag.

Every interval of at least one wall second reports:

- App cadence: valid count, mean, exact nearest-rank p50/p95/p99, maximum,
  rejected values and overflow. At most 8,192 `f64` samples are retained, storage
  is reused, and overflow suppresses quantiles rather than hiding later spikes.
- Context: map, app/menu state, physical size, focus, requested present mode,
  latency hint, screenshot/input/noclip flags and local/spectator target IDs;
  local/view-target life epoch, alive/team and active inventory slot/weapon index;
  simulation tick/rate/time,
  round/mode and actual players/bots/alive/items/projectiles/fields.
- Main-world entities, mesh entities, locally hidden meshes, meshes visible to
  any view, no-frustum meshes, skinned meshes, UI nodes, audio players, impact
  effects, cameras, body/pending/physical counts and CPU asset-store counts.
- Existing wall scopes plus audio update/prune, feedback, utility FX, ground
  items, HUD/art, classic HUD pump, cosmetics/decorations and nested bodies.

The census scans mesh visibility once per reporting interval. Pure entity
counts use Bevy 0.19.1 `Query::count` (matched archetype/table counts), avoiding
`Entities::count_spawned`'s scan over every allocated index. Asset counts use
`Assets::len`. `diagnostic.report` exposes quantile sorting, census and logging
inside `frame_report::report` after its deadline check. It excludes per-frame
context observation/sample collection, the later `report_stages` drain and its
stage/GPU-history log lines, timing-scope clock/lock costs elsewhere, GPU query
overhead and asynchronous log/worker work. It is **not total diagnostic
overhead**. Reporting and GPU queries perturb timing; compare the same external
presentation trace with both `CSR_PERF_REPORT` and `CSR_FRAME_LOG` unset too.
A periodic report spike is not automatically a gameplay regression.
`main` is the existing First-to-Last scope, not a sum of every CPU thread.

`report_wall_s` is the Instant-to-Instant reporting deadline interval, at least
one wall second. `sampled_real_s` is the sum of accepted `Time<Real>` frame
deltas; their First-schedule boundaries differ from Last-schedule report calls.
`elapsed_real_s` is Bevy real elapsed time at the endpoint. Cadence mean uses
the accepted delta total divided by accepted frame count; quantiles/max also
use those deltas. Do not divide the count by `report_wall_s` as if these were
identical windows, or use simulation time as a cadence denominator. Rejected
deltas contribute to neither count nor sampled duration. Sample overflow still
retains the complete accepted count/total/max.

Every frame compares a small Copy-only context (no formatted strings, map-name
clones, hashes or extra locks). App state, menu page/in-match flag, window
size/focus/requested present/latency hint, screenshot/input flags, and local
player/noclip/spectator target changes increment `context_changes`. Bevy resource
change detection also marks map replacements, including the same map name.
Local and view-target authoritative life epoch, alive/team, active inventory slot
and weapon definition index also participate, including a missing actor. The key
does not include continuous coordinates, angles, recoil or interpolated camera
pose; moving frames are not automatically rejected as mixed.
Any observed change sets `mixed_context=true` for that whole report. The context
line is explicitly `endpoint_only=true`, even for mixed windows; it never labels
all earlier Loading/paused/unfocused/other-target samples as Match. Returning
to the same endpoint does not erase intervening changes. The endpoint survives
report reset so the first delta of the next report can also mark a boundary.
Use only unmixed windows for a fixed-context comparison; retain mixed windows
for transition analysis. Observations happen once per report-system invocation:
unobserved transitions within a frame and every gameplay-state mutation are
not tracked. Census values remain endpoint snapshots. This does not alter
simulation, rendering/input policy or default-off diagnostic behavior.

### Endpoint camera evidence

Once per summary, `perf player endpoint` reports both the requested local actor
and `LocalPlayer::view_target()` from `SimWorld`: life epoch, alive/team, active
slot/definition index, weapon class/content ID, Source origin (inches), and Source
view angles (pitch/yaw/roll degrees). A missing actor is explicit `None`; the local
resource being absent is also explicit. In a network session these are the local
simulation's current values, not an independent server receipt. `perf view intent
endpoint` separately records interpolated render eye/angles and is not camera
component proof, particularly in noclip/spectator cases.

`WorldPlayerCamera` marks only the main playable-world camera at spawn. Sky and
viewmodel cameras also have `PlayerCamera`, so neither that component, iteration
order nor the current FOV identifies the world view alone. `perf world camera
endpoint` requires a unique marked candidate, reports candidate count and role
flags, and emits `selection=unavailable` for zero/multiple candidates. Admit its
endpoint only with `candidates=1`, `role_valid=true`, `is_active=true` and the
expected target/layers/viewport. The record includes actual `Transform` (Bevy
axes/inches, quaternion rotation), propagated global matrix (column-major), full
projection parameters, perspective vertical/horizontal FOV degrees, computed
clip-from-view matrix, order, target, layers, viewport and sub-camera view. Absent
`RenderLayers` means Bevy's default layer 0. Perspective FOV conversion assumes
the standard full perspective view; sub-camera/custom/orthographic projection
comparisons must use their explicit parameters/matrices.

These are `endpoint_only=true`, `source=main_world_last` observations, after
camera update/transform propagation. They are inputs to render extraction, not
readback from the render sub-app, proof of a presented frame or a camera trace.
`interval_camera_stability=unproven` is explicit. Matching endpoints and
`mixed_context=false` cannot establish paired same-view intervals: motion,
camera activation/FOV changes, same-index content replacement and transitions
between invocations can still occur within the interval. Root's admission tools
must require independent full-interval camera/presentation proof before admitting
that comparison. No per-frame precise-camera logging, allocation, mutex or
gameplay/presentation timing change is added by this metadata follow-up.

Counts are snapshots, not peaks; `hidden_local` is local `Visibility`, and
`view_visible_any` can include sky/viewmodel/other cameras. They are not occlusion,
draw-call, texture-upload-byte, process-memory, VRAM or live render-world asset
measurements. Fixed asset counts cannot rule out a backend/worker allocation
leak. One-second quantiles cannot be averaged into a whole-run quantile.

## Root measurement sequence (not executed here)

1. **Admit one immutable binary.** Under the exclusive Cargo slot, clean all
   workspace packages for the explicit profile/absolute shared target before
   building in this worktree. Record actual build cwd, source/tree SHA and copied
   exe hash/marker. Root runs workspace tests, release Dust II `probe`/`sim`, asset
   guard and normal-input render inspection. Preserve the old candidate and
   owner preferences. Use a new private `CSR_DATA_DIR` for each run seeded with
   identical selected settings. Stop other builds/CI/native/game/render work
   for accepted paired measurements and record machine/adapter/driver/backend.
2. **Reproduce at one resolution and mode.** Use ordinary input, no screenshot
   option and no `--camera`; matching camera transform is an observer requirement,
   not a reason to enter noclip. Start with 1920x1080, then repeat the meaningful
   comparison at the owner's recorded play resolution. In the private profile
   match video settings, VSync, exposure, weapon/loadout, bot difficulty,
   map content hash and tick rate. For example the diagnostic command shapes are
   `csr play de_dust2_2017 --direct --deathmatch --bots 0 --size 1920 1080`
   and the same with `--bots 5`; repeat on the exact Nuke variant from feedback
   (record it; do not silently substitute `de_nuke_2016` or `de_nuke_2019`).
   These `--size` examples explicitly use a windowed session. For the owner's
   saved fullscreen mode, seed the private video preferences and omit `--size`;
   verify the reported physical dimensions rather than assuming the defaults.
   `--bots 5` creates nine bots plus the human, not five total: `N` enemies and
   `N-1` teammates. `--bots 3` produces five actual bots plus the human.
   Use the reported actual census when naming results.
3. **Pair solo and bots, early and late.** Retain `CSR_PERF_REPORT=1` throughout
   each run, with `CSR_FRAME_LOG` unset. Record fixed-view windows at simulation
   seconds 30-60, 300-330 and 600-630; between windows use the same normal-input
   move/fire/reload/death/respawn route. Compare the same position, pitch/yaw,
   alive/spectator state and resolution in each window. Repeat in reverse run
   order. Record native presentation intervals/dropped presents with an external
   observer, actual swapchain/compositor mode, process working/private bytes,
   GPU utilization/VRAM and CPU-thread activity. Preserve raw intervals to compute
   true per-window distributions; the new exact quantiles are per log interval.
   Distinguish steady corpse/effect plateaus from growth past their lifetimes.
4. **Isolate the measured expensive stage, one switch at a time.**
   `CSR_NO_CLASSIC_KILL_FEED=1` removes notices while retaining combat HUD;
   adding `CSR_NO_CLASSIC_COMBAT_HUD=1` disables both classic HUD components.
   `CSR_NO_HUD=1` suppresses HUD setup/update paths; it is not proof that every
   existing UI entity is absent. `CSR_NO_PROPS=1` skips static props at load.
   These change visuals and are diagnostic comparisons only, never acceptance
   settings. `CSR_NO_PVS=1` forces map PVS fail-open. `CSR_NO_CULL=world|props|all`
   disables **material backface culling**, not character frustum/PVS culling.
   `CSR_PRESENT=immediate|mailbox|fifo` overrides saved VSync for the session
   without rewriting it; start with the saved policy and verify actual presents.
   `CSR_TEST_LIGHT_CLUSTERS=1` restores the already-rejected extra clustered-light
   path for attribution only. Preserve default fidelity in accepted runs.
5. **Only then optimize the identified owner.** If bodies correlate, measure
   `characters.bodies` and cooking CPU separately; if classic HUD correlates,
   root can use bounded `CSR_UI_WORKER_TRACE=1` / `CSR_COMBAT_HUD_TRACE=1` on a
   separate attribution run. If meshes/assets keep rising after effect expiry,
   compare item/body/effect counts and then inspect the specific retained owner.
   If CPU render prepare dominates with stable counts, profile its systems and
   changing assets before shader tuning. If genuine GPU pass/presentation time
   dominates, inspect per-view draws/overdraw/queue contention. Do not infer a
   speedup from hiding content or guess coefficients/FPS gains.

`CSR_TEST_BOT_PERF=<output folder>` already exercises normal Bevy input messages
with `play de_dust2 --direct --deathmatch --bots 3` and checks five actual bots,
fire/reload/Pause/Resume. It can sanity-check this instrumentation's integration,
but its short scripted path is not the sustained old-map measurement matrix or
physical mouse-to-display proof. `CSR_WATCH_BOT`, `--camera` and `CSR_NO_SELF`
change viewpoint/body visibility; screenshot mode disables normal input, enables
an unfocused continuous loop and may run screenshot-only event fixtures.
`CSR_AUTOFIRE=1` applies only with normal input disabled. Keep all those modes
out of the accepted real-input solo/bot comparison.

Any real Legacy comparison remains root-coordinated through the native-slot
wrapper with its process/preferences guards; no native harness was launched by
this track. Retain raw captures/account data/runtime assets privately.

## Validation status

New `FrameWindow` regressions cover quantile ordering, invalid samples, reset,
the singleton case and bounded overflow retaining later maximum/total while
suppressing incomplete quantiles. Added context regressions cover Loading to
Match, Pause, focus loss, spectator-target switches, returning to the same
endpoint, report-boundary retention, same-name map changes and a missing window.
All are **not run** under this track's resource
restriction. Bevy APIs were read in installed 0.19.1 source before use. The
original candidate module was parsed/formatted by standalone Rustfmt; the R1
context follow-up used only `git diff --check` and an independent small context/
denominator model. It has not been parsed or compiled. Root must run focused
`cargo test -p client input_latency::frame_report::tests` as well as the existing
`cargo test -p client input_latency::tests` presentation-policy regressions.
Compilation, the frame-report regressions, full workspace/release/installed-data checks,
instrumentation overhead, actual screenshots and the paired sustained runs
remain required. PF01/PF02 stay open; no FPS or fidelity acceptance is claimed.

Parent reports the prerequisite four `FrameWindow` tests and workspace checks
passed, with its frozen-source release build in progress. That result does not
validate this new endpoint commit.

The endpoint follow-up adds two unrun fixtures: authoritative actor death/life,
team and weapon/slot transitions mark either local/view-target context mixed,
while movement/angles do not; a numeric 90-degree vertical, 16:9 projection
reports 121.28449-degree horizontal FOV. Root must rerun
`cargo test -p client input_latency::frame_report::tests` after integration (six
tests including these two), plus `cargo test -p client input_latency::tests`,
`cargo test --workspace`, release Dust II `probe`/`sim` and the asset guard.
Runtime verification must inspect an ordinary-input summary for unique active
world-camera selection, exact metadata and discrete transitions. The endpoint
follow-up was only source-reviewed/formatted and checked with `git diff --check`;
no Cargo/rustc, game, native, GUI or shared-target operation was run here.
