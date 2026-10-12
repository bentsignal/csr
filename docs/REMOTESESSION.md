# Owned remote sessions

`netcode::remote_session::RemoteSession` owns one UDP connection and its
joining lifecycle. It has no renderer dependency. The caller supplies a
selected server address, local display name and team to `discover`; `update`
polls bounded IO and exposes typed `Status` changes:

`Discovering -> AwaitingMap(ServerInfo) -> Synchronizing -> Ready -> Closed(Error)`.

Discovery allocates no gameplay player and receives no match snapshots or
events. Its temporary connection is released when bounded metadata arrives.
`AwaitingMap` has no networking timeout, allowing asynchronous local loading.
Both networking join stages have a five-second timeout. No server-provided
path, URL or downloadable asset is mounted or fetched by this module.

The frontend must resolve the advertised map `ContentId` to a verified
installed definition, load its exact bytes asynchronously, and build a
`sim::World` with matching collision, spawns, objectives and weapon catalog.
Call `provide_map(world, manifest)` only when the complete map/catalog manifest
matches discovery. Unreplicated dynamic external collision state is rejected.
The actual Hello admission returns fresh bounded `ServerInfo` before Welcome;
version, rate, content and all timing/history policy fields must still agree
with discovery. A change drops the prepared world and reprobes, so the
frontend must observe another `AwaitingMap` and reassess its selection.
Protocol version 3 distinguishes this discovery/admission contract.
Unreliable snapshots can overtake reliable admission after packet loss. The
client discards those raw bytes until validated Info/Welcome arrive; later
full snapshots recover. Valid reliable events stay bounded and hidden until
Ready, then drain once through the authoritative queue.

`Ready` requires the first valid authoritative snapshot. `server_info`,
`player`, `acknowledged_input` and `pending_inputs` expose read-only status.
`authoritative_state` is the HUD/round/economy source. `predicted_world` is
the local movement source and exposes no predicted effect events.
`pose_history` provides separate remote render poses without mutating either
world. The session keeps 32 pose snapshots and caps extrapolation at the
smaller of two observed snapshot intervals and the server interpolation
allowance. The teleport threshold is 128 Source units. These are prototype
rendering policies pending Legacy measurements, not claims of original timing.

Call `update(elapsed_seconds)` once per frontend update and
`advance_inputs(elapsed_seconds, view_tick, callback)` when Ready. The latter
uses the negotiated fixed tick rate, invokes the callback for at most eight
ticks per call, and reports discarded wall ticks and backpressure. The
callback receives the assigned player and predicted/authoritative tick.
Its returned `TickInput` retains optional tracked head/hands and an independent
aim ray. `view_tick` must not exceed the last authoritative tick, including
when a render sample extrapolates ahead. At 256 pending inputs or timeline
exhaustion, the callback is not invoked and input advancement backpressures.

Nonfinite, negative or unrepresentable elapsed values are rejected before
clock mutation. A future view tick is also rejected before clock mutation.
An invalid callback input consumes that call's already computed clock
opportunity, but the rejected command changes neither world nor sender
sequence. Any earlier valid ticks in that call remain committed. Each valid
command is prepared and predicted before transport sequence advancement;
subsequent transport failure or sequence mismatch closes the entire session.
The caller should treat invalid controls as a local input bug rather than
retrying the same elapsed frame.

`take_events` drains authoritative reliable batches only when Ready. Positions,
float payloads, strings and tick bounds are validated before exposure. The
session queue is limited to 128 batches and 4096 events; overflow is a typed
fatal error instead of silently losing authoritative audio/effects. Polling
also bounds decoded messages and UDP packets. The caller must drain events
regularly and avoid replaying already consumed batches.

`cancel` is safe repeatedly in every stage. It disconnects, discards prepared
maps, prediction, timing, poses and queued events, and preserves an existing
closed error. Dropping the session also drops its owned transport. A frontend
map-loading cancellation should discard its pending result and cancel this
session; a late result cannot provide a map to the closed session.

Direct-IP admission remains unauthenticated prototype transport. Discovery is
for one known address, not LAN broadcast or a public directory. Static installed
content packs are supported through the same verified ContentId/manifest
contract; automatic downloads, identity and listen-server UI remain separate
work. The local favorites browser and rendered integration are documented in
[REMOTE_JOIN.md](REMOTE_JOIN.md). `csr-server --remotecheck` verifies this
lifecycle on real Dust II at 64 and 128 ticks without launching graphics.
