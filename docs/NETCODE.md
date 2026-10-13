# Multiplayer foundation

Track A2 has a Bevy-free UDP session, dedicated server, rendered joining,
prediction/interpolation and an owned local-host backend. Full model hitboxes,
wider discovery and internet identity/admission remain open. See
[REMOTE_JOIN.md](REMOTE_JOIN.md) for rendered lifecycle verification.

## Ownership and messages

`netcode::session::Session` owns `sim::World`. Each transport connection gets
one server-assigned `PlayerId`; client messages never choose a player id or
send gameplay state. Admission checks the protocol version, the map's stable
`ContentId`, map filename, BSP SHA-1 and weapon catalog SHA-1. Both sides must
already have matching content. Required pack downloads are a later layer;
there is no asset transfer in this protocol.

Build manifests from `Level::bsp_sha1()` and `Level::catalog_sha1()`: these
retain the exact BSP input parsed before its pak mounts, and the exact catalog
input successfully parsed after that pak mounts. A later VFS lookup can resolve
a shadowed file or changed loose data, so it must not establish the running
level's identity. Missing or malformed catalog data retains the local default
fallback but has no catalog file digest; the dedicated server rejects that
fallback before advertising a manifest. These SHA-1 fields compare content and
are not a pack authenticity or download-verification mechanism.
The server also uses `Level::overview()` so private and archived map metadata
follows the selected source instead of an unrelated same-basename VFS file.

A content mismatch returns typed `ContentRequired` metadata through
`Client::required_content()`. Content ids, map filenames and hashes are
validated, including rejection of host paths. The front end must resolve
the id against its supported registry and perform a cancellable load before
reconnecting. Receiving the manifest does not mount or download anything.

Control, inputs and events use postcard. Wire v6 retains the bounded lossless
full/patch state envelope introduced in v4, carrying exact postcard WorldState
bytes. Channels are:

| Channel | Delivery | Contents |
|---|---|---|
| Control | Reliable, ordered | Hello, welcome, rejection, leave, snapshot receipt/recovery |
| Input | Unreliable | Up to eight recent sequenced `TickInput`s |
| State | Unreliable, fragmented | Full or lossless baseline delta with last applied input sequence |
| Events | Reliable, ordered | Tick-stamped `GameEvent` batches |

Protocol version 6 rejects v5 and older peers: corrected first-command grounding
and landing semantics would diverge in an old predictor, even though the
replicated movement fields are unchanged. It retains replicated spawn epochs and
server-derived hitscan rewind ticks. See [LAGCOMP.md](LAGCOMP.md) for server timing/lifecycle
bounds and the remaining hitbox fidelity scope.

Kill events retain an immutable authoritative death receipt, including the
victim's life/tick/current pose and original names/teams. Actual firearm/melee
hit rays are optional metadata, never inferred impulses. Independent reliable
events can arrive after a respawn snapshot; consumers use the receipt rather
than current actor lookup. See [DEATH_RECEIPTS.md](DEATH_RECEIPTS.md).

Inputs include the existing optional tracked head/hand poses and separate
aim ray. The server rejects nonfinite values, clamps movement to 450, limits
tracked offsets and normalizes rotations. These initial offset limits protect
the simulation; VR hitboxes and measured tracking limits remain Phase 3 work.

The server admits each sequence once and consumes at most one command per
player per tick. Repeated input batches recover isolated lost packets. Pending
commands are bounded at 32; after a large stall the oldest commands can be
dropped to keep current intent. Movement and held attack stop after 0.25
seconds without a newly applied input. Competitive disconnects drop the
player's normal death gear, including the C4, and cancel an active defuse.

The default simulation is 128 tick. Snapshots are sent at 32 Hz when the
server rate allows it; events are sent each tick. After a host stalls, a
wall-clock accumulator limits catch-up to eight ticks and reports discarded
wall ticks. It never jumps simulation time or steps once per render frame.
State messages omit events so a client cannot replay a shot when the next
snapshot arrives. Clients discard stale state ticks and expose reliable
events separately. Snapshot and channel queues have explicit memory limits;
snapshot compression preserves the exact canonical state bytes without quantization.
Channels are directional: a client may send only control and input messages.
Before exposing state, the client checks the assigned player, unique player
ids, finite movement, entity limits, server tick rate and input acknowledgements.

## Lossless snapshot baselines

The server encodes one shared WorldState body per snapshot cadence. Each
peer's envelope adds its applied-input ACK. A transport frame ID is independent
of the world tick: hosted Preparing can change its roster at a frozen tick.
Only fully reconstructed, catalog/ownership/finite-state validated canonical
bodies enter the client cache or produce receipt feedback. Existing prediction
and authoritative gameplay still consume StateUpdate with newer world ticks.
Reliable events never enter the body and retain their separate delivery.

Both caches hold at most 32 frames and 8 MiB; each body is at most 256 KiB
minus 64 envelope bytes. Peers retain only bounded sent/confirmed IDs, not
world clones. At most eight distinct baseline patches are built per capture,
shared across peers selecting those baselines; other peers receive full bytes.
Each patch has at most 8192 ordered literal spans, parsed without allocating a
peer-controlled span vector. Reconstruction and scanning are linear and bounded.
FNV fingerprints identify exact baseline bytes; they are not authentication.

A sender references only a peer's confirmed receipt. Lost receipts leave full
snapshots available. Missing/evicted baselines request independent full recovery,
without advancing state or trimming pending inputs. Full frames also replace
patches that are larger than full, and every 32nd frame is independently full.
Feedback is coalesced into one pending value and capped at 512 queued reliable
control payload bytes, including other client control messages. Cancellation,
reconnect and policy reprobe discard client history. No input redundancy,
physics, VR pose precision or content identities change.

### Measured payloads and limits

A private 2026-10-12 sizing observer exercised this actual codec with the
installed Dust II catalog: 2/10/16/32/64 synthetic human actors, moving inputs,
flat or changing tracked poses, default deathmatch inventory or competitive
purchases/armor/kit, explicit synthetic eight smoke fields/four projectiles,
and public drop commands. Both 64 and 128 tick were checked at mature ten-minute
counters, with every reconstructed body compared byte-for-byte. The table gives
rounded 128-tick average payload bytes across 32 captured frames. Compressed
averages include the initial full frame and the periodic 32nd full keyframe;
receipts immediately confirm each received frame in this sizing observer.

| Actors / synthetic profile | Always full v4 | Compressed v4 including fulls |
|---|---:|---:|
| 10 flat / default | 3302 | 544 |
| 10 tracked / default | 5462 | 1663 |
| 10 flat / rich | 6108 | 871 |
| 10 tracked / rich | 8268 | 2010 |
| 16 flat / default | 5129 | 826 |
| 32 tracked / default | 16914 | 5224 |
| 64 tracked / default | 33571 | 10444 |
| 64 tracked / rich | 49495 | 12437 |

These are encoded payload sizes, not NIC throughput, measured internet latency
or an actual populated match. Older baselines after inventory/layout changes can
approach full size; full fallback remains essential. No bot thinking, input
redundancy, float precision or gameplay policy changed to obtain these numbers.
Independent actual UDP relay tests at both rates reverse physical encrypted
datagrams, drop a fragment, delay an old frame, lose cached baselines, and verify
recovery, frozen Preparing roster identity, event ownership and disconnect.

Private provenance SHA256 values (no logs, assets or identifiers are published):

- Observer: `7720c1952a517a86a911feafe1e210db8b4fa8937103e44c71d8239ef8b18430`
- Aggregate measurement log: `1e8acd87e35fbc882eeb5850ba3b17106c5668f09da3b5deb8e84b917c751583`
- Measured codec source: `5435655962b072189c0af47294011470a40d38bc9bb9dd398447eb3eecfc300d`

## Transport choice

We use Renet 2.0.0 and renet_netcode 2.0.0 pinned to the owner fork revision
`833a5deea2c22771db665fdf982b37b1ddbe484b`. The
[verified integration patch](https://github.com/bentsignal/renet/pull/1)
validates fragment indices/counts and allocation arithmetic, preserves the
original MIT/Apache-2.0 notices, and adds bounded UDP polling. Each host
update reads at most 1024 datagrams; each client reads at most 256. Pending
hello slots expire after five seconds. Per-peer decoded message work and
input queues are bounded. This is not a complete internet abuse/identity
layer; bandwidth admission/rate limiting remains future host work.
Renet supplies
[reliable ordered and unreliable channels](https://docs.rs/renet/2.0.0/renet/struct.ChannelConfig.html)
and message fragmentation independently of Bevy. Quinn provides
[reliable streams and unreliable datagrams](https://quinn-rs.github.io/quinn/quinn/data-transfer.html),
but its
[TLS certificate trust setup](https://quinn-rs.github.io/quinn/quinn/certificate.html)
adds an identity provisioning dependency to the first direct-IP prototype.
The simulation and session remain independent of either transport.

## Prediction and reconciliation

`netcode::prediction::Predictor` accepts a map-built `sim::World`, a validated
server welcome, and the first authoritative `StateUpdate`. The caller must
obtain that first state before sending player commands. Configure local fixed
ticks from the welcome's `tick_rate`, rather than assuming every server is 128.
For each fixed tick, check `can_predict()`, send a `TickInput` through the
transport, and pass its returned sequence into `Predictor::predict` along with
the same input and the last authoritative/render view tick.

History holds at most 256 inputs. When it is full, wait for authority rather
than dropping replay data or sending an input the predictor cannot retain.
On a newer snapshot, `reconcile` validates acknowledgements and finite state,
restores server truth, discards acknowledged inputs and replays the rest. Its
result includes the final position error for future rendering correction.
Stale state ticks are ignored; backwards acknowledgements, acknowledgements
beyond sent input and incompatible server rates are rejected before mutation.

`sim::World::step_prediction` shares the authoritative simulation path but
omits server bot thinking. It preserves `is_bot`, tracked poses and the
separate aim ray. Remote actors repeat their last authoritative controls
during replay, so their extrapolated positions can differ from new server AI
decisions. Render remote actors from the separate pose history described
below; predicted remote movement is not authoritative targeting truth.

Predicted `GameEvent`s are cleared and are not returned by this API. Consume
the transport's reliable authoritative event batches for effects, and use
`authoritative_state()` for HUD economy/round truth. The predicted world is
available for movement/rendering. Explicit provisional local effects and
their deduplication remain rendered-client integration work.

Dynamic external worlds are rejected because their mutable target/block
state is outside `WorldState`; replay could otherwise apply shared damage
twice. The `DynamicWorld` seam remains available to Track E, which must add
replicated/replay-safe mod state before prediction can support it. Static
content-pack BSPs use the same prediction path as installed CS:GO maps.

## Remote rendering poses

`netcode::remote_pose::PoseHistory` captures render data from newer
authoritative `WorldState`s without retaining inventories, rules, events,
bot decisions or collision worlds. Configure it from the welcome's tick
rate, a snapshot capacity (2 through 128), an extrapolation cap in ticks,
and a teleport distance in Source units. Rendering delay and these limits
are caller policy; their defaults still require CS:GO Legacy measurements.
Each history is bounded to 64 actors per frame. Clear it when changing
maps or sessions.

`sample(player, render_tick)` accepts a fractional server timeline and
returns a separate `RenderPose`, the used timeline and the sampling mode.
It interpolates position, eye, velocity, shortest-path view angles, tracked
head/hand quaternions and the independent aim ray. Quaternions are checked
and normalized before slerp; equivalent antipodal rotations do not spin.
Movement extrapolation stops at the configured cap, holds dead actors,
and does not run collision or gameplay. It leaves tracked offsets and aim
orientation at the latest sample during extrapolation.

Round transitions clear history. Teleports, team changes, death and respawn
epochs snap to the newer pose instead of blending distinct lives, including
a respawn whose death snapshot was lost. Stale snapshots cannot rewind the
history, and invalid new poses are rejected before mutation. Actor admission
and removal follow snapshot boundaries; missing actors are not invented.

Use this output only for remote rendering. Continue using authoritative
state for HUD/round/economy and the local predictor for local movement.
When reporting the viewed tick in a command, obey the protocol's
last-authoritative-tick limit even if rendering extrapolated beyond it.
This module has no Bevy integration yet, and performs no hit rewind.

Direct connect currently uses netcode.io's
[Unsecure admission mode](https://docs.rs/renet_netcode/2.0.0/renet_netcode/enum.ClientAuthentication.html).
It is a prototype connection mode, not authenticated identity. Clients use
random transport ids; Steam is not required. Phase 2 must add generated
player keys and secure connect tokens before treating names or ids as trusted.

## Development verification

The frontend-facing [owned remote session API](REMOTESESSION.md) composes
discovery, exact installed-content admission, prediction, remote pose history,
bounded authoritative events and cancellation. Its library proof is headless;
the [rendered join integration](REMOTE_JOIN.md) additionally exercises two
clients, pause, Home teardown and offline reload at 64 and 128 tick.

The separate `csr-server` executable has no `client` or Bevy dependency.
Source builds use:

```powershell
cargo run --profile play -p server -- de_dust2 --bind 127.0.0.1:27015 --bots 8
cargo run --profile play -p server -- de_dust2 --netcheck --bots 2
cargo run --profile play -p server -- de_dust2 --netcheck --tickrate 64 --bots 2
cargo run --profile play -p server -- de_dust2 --remotecheck --bots 2
cargo run --profile play -p server -- de_dust2 --remotecheck --tickrate 64 --bots 2
```

`--deathmatch`, `--maxplayers`, `--tickrate` and `--ticks` configure the
dedicated host. Bot slots and joining slots total at most 64. Ctrl-C performs
transport shutdown. It reads the player's Legacy install through `level` and
VFS; `CSR_GAME_DIR` overrides detection and `CSR_DATA_DIR` selects runtime
content storage. No installed game files are changed.

`--netcheck` connects two real UDP clients on an ephemeral loopback port,
sends 128 commands each, checks decoded snapshots against authoritative
history at the same tick, resolves installed weapon `ContentId`s, verifies
tracked poses, and confirms disconnect cleanup. It delays transport polling
for four ticks while local controls change, requires pending input replay,
checks all 128 acknowledgements and exact final reconciliation, and verifies
predicted effects remain absent. It exits with `failures: 0`.
It also samples fractional remote poses from decoded snapshots, checks exact
snapshot poses against authority, validates tracked rotations and bounds the
pose history. These are headless checks; rendered lifecycle proof is documented
separately in [REMOTE_JOIN.md](REMOTE_JOIN.md).
The default test data also exercises input ownership, atomic validation,
duplicate/reordered commands, neutral controls after timeout, fixed-tick
accumulation, reliable delivery after packet loss, and fragmented snapshot
recovery after loss and reordering. CI runs the Dust II UDP check with bots.

These commands are development checks. The friend-facing hosting and joining
flow belongs in the menu and installer, without terminal setup.

`--remotecheck` drives two owned `RemoteSession`s through discovery, local map
provision and synchronization against the installed Dust II data. It requires
128 acknowledged inputs each, fractional remote samples, independent tracked
head/hands and aim rays, exact final reconciliation and repeated cancellation
with server-player cleanup. CI runs this proof at both supported test rates.
