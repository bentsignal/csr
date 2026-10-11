# Multiplayer foundation

Track A2 has a Bevy-free UDP session and a dedicated server. The rendered
client still plays offline: joining from the classic menu, prediction,
interpolation, lag compensation, snapshot deltas, listen hosting and discovery
remain open. This is an integration foundation, not a multiplayer release.

## Ownership and messages

`netcode::session::Session` owns `sim::World`. Each transport connection gets
one server-assigned `PlayerId`; client messages never choose a player id or
send gameplay state. Admission checks the protocol version, the map's stable
`ContentId`, map filename, BSP SHA-1 and weapon catalog SHA-1. Both sides must
already have matching content. Required pack downloads are a later layer;
there is no asset transfer in this protocol.

A content mismatch returns typed `ContentRequired` metadata through
`Client::required_content()`. Content ids, map filenames and hashes are
validated, including rejection of host paths. The front end must resolve
the id against its supported registry and perform a cancellable load before
reconnecting. Receiving the manifest does not mount or download anything.

The wire format is postcard. Channels are:

| Channel | Delivery | Contents |
|---|---|---|
| Control | Reliable, ordered | Hello, welcome, rejection, leave |
| Input | Unreliable | Up to eight recent sequenced `TickInput`s |
| State | Unreliable, fragmented | Full authoritative state with last applied input sequence |
| Events | Reliable, ordered | Tick-stamped `GameEvent` batches |

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
the full-state foundation is intentionally bandwidth-heavy until deltas land.
Channels are directional: a client may send only control and input messages.
Before exposing state, the client checks the assigned player, unique player
ids, finite movement, entity limits, server tick rate and input acknowledgements.

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

Direct connect currently uses netcode.io's
[Unsecure admission mode](https://docs.rs/renet_netcode/2.0.0/renet_netcode/enum.ClientAuthentication.html).
It is a prototype connection mode, not authenticated identity. Clients use
random transport ids; Steam is not required. Phase 2 must add generated
player keys and secure connect tokens before treating names or ids as trusted.

## Development verification

The separate `csr-server` executable has no `client` or Bevy dependency.
Source builds use:

```powershell
cargo run --profile play -p server -- de_dust2 --bind 127.0.0.1:27015 --bots 8
cargo run --profile play -p server -- de_dust2 --netcheck --bots 2
cargo run --profile play -p server -- de_dust2 --netcheck --tickrate 64 --bots 2
```

`--deathmatch`, `--maxplayers`, `--tickrate` and `--ticks` configure the
dedicated host. Bot slots and joining slots total at most 64. Ctrl-C performs
transport shutdown. It reads the player's Legacy install through `level` and
VFS; `CSR_GAME_DIR` overrides detection and `CSR_DATA_DIR` selects runtime
content storage. No installed game files are changed.

`--netcheck` connects two real UDP clients on an ephemeral loopback port,
sends 128 commands each, checks decoded snapshots against authoritative
history at the same tick, resolves installed weapon `ContentId`s, verifies
tracked poses, and confirms disconnect cleanup. It exits with `failures: 0`.
The default test data also exercises input ownership, atomic validation,
duplicate/reordered commands, neutral controls after timeout, fixed-tick
accumulation, reliable delivery after packet loss, and fragmented snapshot
recovery after loss and reordering. CI runs the Dust II UDP check with bots.

These commands are development checks. The friend-facing hosting and joining
flow belongs in the menu and installer, without terminal setup.
