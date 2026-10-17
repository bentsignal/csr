# Menu transition ownership: October 16 implementation handoff

This patch addresses the source lifecycle gaps behind UI16-01/02 and adds
UI16-03 stage measurements. It is **not visually accepted or built yet**.
The implementation child was prohibited from Cargo/rustc, shared targets,
native/game launches, soundcheck, publishing and owner desktop interaction.
Only this isolated worktree was edited. The frozen owner candidate was not
opened or changed.

## Source and retained runtime evidence

Reviewed #122 (`1193a93`), #132 (`5735057`), #208 (`8718ed4`) and their research
notes. The cached Home/match workers remain separate; no cold retirement or
renderer/performance changes are introduced.

The retained private `overnight-feedback-20261014/menu-audio-offline-da0bfa1/`
`stderr.log` records OptionsMenu.OnUnload with `mc:_level21` before the next
native navigation command (lines 962-963). The bridge handled Options OnCancel
but omitted OnUnload. Home can leave a cached movie without going through the
BasePanelRunCommand cleanup. This establishes an unhandled lifecycle callback;
it does not independently prove which callback the owner's Home click emitted.

The private `csr-fidelity/audio-settings-20261015/options-deadzone-8d3fb1d/`
`stdout.log:771` records the public root `_level21`, elementName OptionsMenu,
gameAPI, HidePanel and ShowPanel. Only public runtime names/properties and
existing bridge code were inspected; no movie instructions were read.

The host leaves Menu.page at Pause while native Options is open. Before this
patch, menu::input closed Pause on Escape and frontend::input could deliver
that same physical Escape to a newly opened movie. That gives two independent
handlers authority over one press, with asynchronous native actions following
later. No gameplay/HUD geometry needs to change to correct that ownership.

## Resulting invariants

- Each cached Options opening replaces OnCancel and OnUnload properties on our
  shared cached API with functions carrying its OwnedOptions generation. A
  retained old function keeps the old receiver; a retained API object doing a
  new property lookup sees the replacement receiver. Only the current receiver
  may close; unload also requires the exact registered root. This does not
  establish which reference pattern the installed movie uses asynchronously.
- Closing revokes pending Ready/show, performs the original public HidePanel
  cleanup, hides the cached root and restores the current Home/Pause base.
  This applies to every supported settings page, rather than a Home-only hide.
- Match Escape is consumed by menu::input. With an owned Options child it
  requests that child's cancellation and retains Pause; otherwise it toggles
  Pause once. The original press/release/repeat is not forwarded to the movie.
- Native opening/closing callbacks request a fresh capture fence using #208's
  existing worker facility. Menu assignment requires both the current Present
  revision and the post-callback fence, including after earlier fresh pixels
  have been assigned. The Loading surface keeps its existing assignment gate.
  This separates actual movie visibility from a stale captured presentation.

## Root validation

The child ran standalone rustfmt parsing (`--edition 2024 --emit stdout`, no
files rewritten) on the three edited Rust modules and `git diff --check`.
Both succeeded. No Rust typecheck, executable tests or rendered proof was run.

Under the exclusive root build slot, clean every workspace package in the
selected shared profile with tools/clean-workspace-profile.ps1, record this
build cwd and copy the finished binary immutably before target reuse. Required:

```text
cargo test -p client frontend_pause_surface::tests
cargo test -p client cached_options_unload_revokes_the_opening_and_stale_closes_cannot_resume_match
cargo test -p client options_escape_returns_to_pause_then_each_press_toggles_once
cargo test -p client menu::tests
cargo test -p client frontend::tests
cargo test -p scaleform bridge_shim_compiles_with_the_key_table_and_callbacks
cargo test --workspace
cargo run --release -p launcher -- probe de_dust2
cargo run --release -p launcher -- sim de_dust2
```

Run the ordinary launch at **1920x1080**, with private copied userdata and the
owned native-slot/process/preferences guards. Do not use CSR_TEST_MENU or a
--screenshot startup to substitute for normal Home initialization. Enable
CSR_UI_TRACE=1 and, when diagnosing UI16-03, CSR_UI_RENDER_TRACE=1. Capture the
intermediate transition and settled page, not only the final screenshot:

1. Home -> Options -> Audio Settings -> Home icon. Require only Home, then
   reopen Audio and repeat. Repeat with Video/Game/Keyboard-Mouse settings.
2. Home -> Audio -> Play -> Offline with Bots. Require no Options overlay;
   verify the retained power/cancel/confirmation path still works.
3. Start Dust II through that picker -> Escape -> Pause -> Audio -> Escape.
   Require Pause over the world. Next Escape returns to gameplay with the HUD;
   next opens Pause; next resumes. Repeat four cycles with full key release,
   then hold Escape to ensure OS repeat cannot perform additional transitions.
4. In-match Audio -> native Back. Require the same Pause base; Resume by click,
   reopen Pause with Escape, then Audio again. Test immediate Back during cold
   Options loading and immediate Audio reopen after closing it.
5. Fire/reload AK during ordinary gameplay, then Pause -> Exit to Home. Repeat
   after a second map and after Audio roundtrips. Require no stale world,
   Options or Pause pixels; Home must be interactive once the fresh image is
   displayed. Compare returned Home with docs/reference at 1920x1080.

Use log timestamps from `Home transition requested; awaiting scene teardown`
to `Home restoration resume queued`, then `Ready transaction queued`, then
`fresh raster assigned`. The first interval includes main-thread scene teardown;
later elapsed_ms values start at worker selection. `cached=false` identifies a
first-ever Home after direct development startup and must not be presented as
a cached ordinary-launch measurement. The assignment log is not monitor scanout
or physical input latency. Retain the pending black frame/music observation
and use the existing frontend flow's real Exit-to-Home measurement as a control.

UI16-03 remains open until these stages are measured in the owner's route.
No new latency coefficient, timeout, fade duration or speculative optimization
was introduced. Runtime validation must also check that original HidePanel and
the owned proxy binding preserve navigation across immediate settings reopen;
standalone shim parsing/typecheck cannot establish this.

## Review r1 follow-up (unbuilt; root validation pending)

The original candidate's match Escape decision checked a boolean under one
lock, then cancellation acquired a second lock and reread the current receiver.
If the worker closed opening A and reopened B between those locks, Escape for A
cancelled B. The decision now captures `Option<String>` under one lock, and
`cancel_options(receiver)` passes that captured identity without rereading it.
The Bridge regression captures through `Frontend::options_receiver()`, replaces
A with B before dispatch, then requires B to survive the captured cancellation.
It also rejects malformed current-generation unload/cancel argument shapes.
The shim rejects a non-string/foreign path or malformed receiver before resolving
it, then requires the actual registered level MovieClip, element and API identity.

Added executable fixtures (authored here, **not executed by this child**):

- `crates/scaleform/tests/options_lifecycle.rs` compiles the actual `shim.as`
  plus a small independently authored driver and Options movie. It loads the
  generated movie at the real `_level21` through a memory-only navigator and
  ordinary `loadMovieNum`; it does not replace `csrResolve`, `csrMakeProxy` or
  the bindings. Null render/audio backends are used. Exact ExternalInterface
  receipts cover current cancel/unload, retained old function calls after
  rebind, new lookups on a retained API object, host cancellation for A/B,
  untouched ordinary getters and failed bindings (not-yet-loaded root, bad
  string/non-string paths, names, element
  and API identity). Failed bindings must preserve the current functions.
- `fresh_and_adopted_workers_reject_delayed_readbacks_below_their_callback_floor`
  exercises real `Frontend::select_worker` fresh/adopted swaps, worker-local
  callback floors and the presentation admission gate. It rejects outgoing
  worker revisions, pre-callback pixels and stale completion after admission.
  It does not execute GPU readback or image assignment.
- `actual_escape_edges_keep_cold_options_in_pause_and_ignore_repeat` feeds real
  `KeyboardInput` press/repeat/release messages through Bevy `InputPlugin` and
  `menu::input` with Options requested but not Ready. It checks one child-cancel
  decision and subsequent Pause toggles only after release/repress. There is no
  movie worker in this fixture; the child acknowledgement is simulated. It
  does not prove the frontend movie input suppression or installed key delivery.

Root's focused checks, in addition to the original required checks above:

```text
cargo test -p scaleform --test options_lifecycle -- --nocapture
cargo test -p client cached_options_unload_revokes_the_opening_and_stale_closes_cannot_resume_match
cargo test -p client fresh_and_adopted_workers_reject_delayed_readbacks_below_their_callback_floor
cargo test -p client actual_escape_edges_keep_cold_options_in_pause_and_ignore_repeat
```

This child ran only `git diff --check` for the follow-up. No Cargo, rustc,
runtime fixture, native/game/GUI, shared-target operation, owner data or Steam
write, push, PR or delegated child was performed. Compilation of the new test
and its generated ActionScript is pending root. Public Ruffle interfaces were
read locally; the fixture is independent of game code/assets and uses no
bytecode/profiler/introspection interface.

Remaining root checks: ordinary 1920x1080 Home/Audio/Home and the already-correct
Audio/Play/Offline Bots route; current cancel/unload and immediate reopen on the
installed movie, including a retained API object's callback lookup behavior;
real keyboard press/release/OS repeat before OptionsReady, subsequent HUD/Pause
cycles; actual delayed GPU readback admission in fresh and adopted workers;
and the existing return-Home stage timings plus inspected intermediate/settled
images. UI16-01/02 are not accepted by this source patch. UI16-03 remains open;
no route-specific concealment or latency tuning was added. The frozen owner
candidate/settings are unchanged. The independent input follow-up is not
included and must be integrated/checked separately by root.

## Combined integration handoff (2026-10-17)

The completed controls and lifecycle source stacks are reconciled in
[the integration handoff](controls-options-integration-20261017.md), including
replacement commit mapping, scoped Restore Defaults and pending root checks.
Use that combined apply order; the original lifecycle hashes are superseded
there. Runtime and visual acceptance remain pending.
