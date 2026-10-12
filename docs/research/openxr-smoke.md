# PC OpenXR smoke preparation

This is an asset-free hardware preparation example, not VR gameplay or a
cross-play implementation. It loads no maps, textures, fonts or player data and
does not edit the OpenXR runtime registration, Steam, headset settings or game
configuration. The desktop camera is an independent preview, not a headset
capture or stereo evidence.

## Optional build

The client declares the optional dependencies and a feature-gated example:

```toml
[features]
xr-smoke = ["dep:bevy_mod_openxr", "dep:bevy_mod_xr", "dep:openxr"]

[dependencies]
bevy_mod_openxr = { version = "=0.6.0", optional = true, default-features = false, features = ["vulkan", "window_support"] }
bevy_mod_xr = { version = "=0.6.0", optional = true, default-features = false }
openxr = { version = "=0.21.1", optional = true }

[[example]]
name = "xr_smoke"
required-features = ["xr-smoke"]
```

The feature must remain optional for normal player builds. The example uses
`add_xr_plugins`, `OxrInitPlugin`, `OxrSessionConfig`,
`XrSessionPlugin { auto_handle: false }`, and the published lifecycle messages.
Vulkan is explicitly selected with `GraphicsBackend::Vulkan(())`. OPAQUE is
the only blend preference; runtime-supported swapchain formats are selected
without assuming a specific format. LOCAL reference space avoids requiring a
configured room boundary. No hand-tracking or passthrough extension is requested.

After integration, the intended developer invocation is:

```text
cargo run --profile play -p client --features xr-smoke --example xr_smoke
```

## What the example observes

Initialization reports the actual runtime, backend and recommended eye image
sizes. Before creating a session, the runtime must expose exactly two
PRIMARY_STEREO views and OPAQUE composition. Missing runtime/HMD/Vulkan reports
a nonzero graceful initialization failure before opening the preview window.

The example requests one session, logs generic lifecycle and raw runtime session
states, and never automatically restarts after loss. Raw states distinguish
SYNCHRONIZED, VISIBLE and FOCUSED, which the backend maps to the same generic
Running state. Escape/window close requests orderly termination.

Thirty strictly advancing predicted display times are counted only while the
runtime is FOCUSED, `should_render` is true, both camera indices exist, OPAQUE
is active, and a fresh `locate_views` query reports valid finite position,
orientation and FOV for both eyes. Rechecking validity avoids treating the
backend's retained last-known pose buffer as fresh tracking.

Completion requests runtime exit, responds to STOPPING with end, and responds
to EXITING/LOSS_PENDING with destruction. Success requires the backend's
both-world destruction acknowledgment, no remaining main-world session and no
XR cameras. Runtime/instance/event loss invalidates an otherwise completed
observation. Thirty engine observations do **not** certify successful compositor
submission or headset visual fidelity: published `end_frame` logs its errors
without exposing a success result. Any such error invalidates a hardware run.

The generated stationary scene has a red cube on the left, green cube on the
right, a farther white pillar and a floor, in metre-scaled LOCAL space. An actual
headset check must separately record whether both eyes see it, view changes
follow head movement, focus loss/recovery behaves correctly, and no compositor
errors occurred. No hardware evidence has been produced by this leaf.

## Bounds and failure limits

The observation deadline is 45 seconds including plugin initialization, followed
by a 10-second orderly cleanup budget. A cancellable owned watchdog covers
synchronous native initialization/frame waits that cannot be interrupted via
the published API. At the combined hard deadline it exits **only this smoke
process**, with nonzero status and explicit `cleanup unverified` output. This
forced path is not graceful-cleanup evidence. Normal return wakes and joins the
watchdog immediately. Native dependency panics are reported as failures; no
claim of graceful cleanup is made for a panic or process teardown.

The published backend has additional limitations: session-creation failure can
still enter session-created schedules with missing resources; begin/end/poll
and frame-wait paths contain panic-on-error calls; instance-loss pending has no
default handler; and render submission errors are logged only. The example
preflights stereo/blend availability, handles instance loss, disables automatic
restart, quiesces the retained session-started flag before loss destruction,
catches unwinding panics, and provides a hard process bound. The flag is needed
because published destruction removes the frame waiter without resetting that
flag; a loss path without STOPPING could otherwise schedule another frame wait
against removed resources. It cannot
repair blocking native drivers, process aborts or prove visual output without
hardware observation.

## Source provenance and verification

Reviewed exact published source archives extracted under
`%TEMP%\csr-openxr-src`, plus Bevy 0.19.1 registry sources. Package sources:
[bevy_mod_openxr 0.6.0](https://docs.rs/crate/bevy_mod_openxr/0.6.0/source/),
[bevy_mod_xr 0.6.0](https://docs.rs/crate/bevy_mod_xr/0.6.0/source/).
These are public library APIs, not game function bodies.

SHA256 of reviewed archive bytes:

- `bevy_mod_openxr-0.6.0.crate`: `af0560ba6db564db1e3b60dfc9abf359443a33ba9295acfd18aff8996b8f6288`
- `bevy_mod_xr-0.6.0.crate`: `f2469b8e28ee37de403db2e6b415ffe91da27e27c99034df257658d76392fe80`

The feature and exact published dependencies are wired. Compilation and actual
example tests remain pending, followed by workspace/real-data/audio checks.
No-runtime, connected-headset, focus transitions and loss cleanup require owned
runtime executions before this work can be called verified.
