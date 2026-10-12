# Rendered-client adapter seam

`client::remote::RemoteClient` owns a `netcode::RemoteSession` as a Bevy
resource. The frontend creates it with `discover` or `from_session`, observes
typed `Status`, and calls `provide_map` after its asynchronous installed-content
validation/loading. `server_info` retains bounded metadata. `cancel` is
idempotent and preserves the original closed error; `remote::clear` removes
the resource and clears old render overrides and queued effects.
Every non-ready transition also discards published local/tracked/remote poses,
camera-life/unsent-control caches and the last-snapshot marker. The next
baseline is restored even if a replacement session repeats the same actor ids,
life epochs or authoritative tick.

The root plugin must schedule `remote::drive` once per `Update`, before
`player::update_camera`, characters, viewmodels, effects and HUD. It may run
before a match exists: discovery/synchronization continue while optional
`SimWorld` and `LocalPlayer` are absent. Once those resources exist, the
adapter adopts the server-assigned player id and publishes authority.
The actual frontend/module scheduling and map ownership remain root wiring
work; declaring this module alone does not enable rendered networking.

`drive` uses `Time<Real>` so a paused offline virtual clock does not stall
network IO or input acknowledgement. An open front-end/pause menu sends
neutral controls while retaining tracked poses and aim, and discards unsent
latched attacks, slots, drops and purchases. The buy menu separately suppresses
movement/attack while allowing its queued purchases. The frontend should keep
remote visual timing running while a pause overlay blocks interaction.

Only validated authoritative snapshots restore `SimWorld`. HUD, economy,
inventory and round rules therefore never read prediction. Only validated
reliable events enter `PendingEvents` and `SimWorld.events`; prediction emits
no renderer effects. The adapter bounds the downstream queue to 4096 events
and closes with `EventOverflow` if a consumer falls behind. Root must disable
offline fixed-tick simulation and select appropriate authoritative foley
scheduling in remote mode. `player::tick` also defensively returns whenever
`RemoteClient` exists, preventing accidental offline gameplay or bot AI.

`player::sample_tick_input` is shared by the offline tick and remote callback.
It preserves movement, held/latched buttons, weapon slots, purchases and drop
requests. Optional `TrackedControls` overlays head/hands and a separate aim
ray. Remote desktop look suppresses the offline noclip toggle. Camera
life/spectating changes use authoritative life epochs, including a missed
death snapshot; mouse look survives ordinary snapshot corrections.

`RenderFrame` exposes a local predicted pose with separate tracked/aim data,
and at most 63 remote read-only `RenderSample`s. `RenderPoses` has an opt-in
override map for these already sampled poses, consumed by existing camera,
character and weapon rendering. It never writes poses into gameplay state.
Local prediction retains the existing 140 units/s stair-eye easing and clears
that offset on new lives, teleports and corrections at the same timeline tick.
Cancellation/closure clears overrides. Ordinary offline sampling remains the
existing fixed-tick interpolation when no override map is populated.

Remote timing uses the observed authoritative tick, real elapsed time since
the latest snapshot, and the negotiated interpolation allowance. Its timeline
is bounded by history and the core extrapolation policy; the transmitted view
tick never exceeds authority. These remain explicit prototype policies pending
Legacy measurements. Rendered UI joining, weapon/model timing fidelity and
VR cameras still require integration and visual verification.

Headless Bevy tests use actual encrypted UDP sessions and prove assigned-id
adoption, silent local prediction separate from unchanged authoritative HUD
state, remote render samples, absence of offline bot steps, pause-neutral
network progress with independent tracking, reconciliation, exactly-once
reliable jump effects, bounded downstream effects and cancellation cleanup.
