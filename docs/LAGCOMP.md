# Authoritative hit rewind foundation

This is a bounded server-side hitscan seam, using the simulation's existing
axis-aligned player hulls and height-based hit groups. It does not implement
animated MDL hitboxes, historical dynamic-world damage, melee rewind, VR
head/hand hitboxes or rendered multiplayer. Those fidelity steps remain open.

## Behavioral basis

Valve's public [Source multiplayer networking overview](https://developer.valvesoftware.com/wiki/Source_Multiplayer_Networking)
describes recent player-position history and server command timing that
accounts for packet latency and entity interpolation. Its generic Source
history window is one second. Its interpolation explanation describes
rendering between delayed snapshots. These are behavioral references, not
proof of exact CS:GO Legacy defaults. The implementation here is original
Rust; no SDK function code was used.

## Timing and ownership

Protocol version 2 adds a server-derived `TickInput::rewind_tick` and
replicated `Player::life_epoch`. Version 1 peers cannot join. The client still
reports its viewed server tick through `InputCommand::view_tick`; it never
reports a hit or supplies a trusted rewind tick. Session overwrites the inner
field. At receipt **and** command consumption, a requested tick must:

- Be between the oldest retained history tick and the current server tick.
- Be no older than `min(max_history_ticks, ceil(RTT_seconds * tick_rate / 2)
  + interpolation_allowance_ticks)`.
- Have passed receipt validation; time spent queued cannot upgrade an
  originally invalid request into an accepted rewind.

RTT is observed by the server transport. Invalid/negative measurements add
zero allowance, and large measurements cannot exceed the server cap. Future,
expired and otherwise invalid view requests execute against current hulls;
their valid gameplay controls continue. Repeated controls clear rewind data
so held automatic fire cannot keep targeting a stale historical frame. Bots
and client prediction always use current hulls.

`LagPolicy` is server configuration in **server ticks** and Source units.
The prototype defaults to one server second of history and two snapshot
intervals of interpolation allowance: eight ticks at 128 tick, four at 64.
These interpolation defaults are explicit prototype policy, pending Legacy
measurement. Both allowances can be reduced by the server; history never
exceeds one second. Dedicated development hosts may use
`--max-unlag-ticks N --interpolation-ticks N`. Clients cannot change policy.

## Historical queries and lifecycle

Only the authoritative World records the optional history. Frames retain
minimal hull/yaw/lifecycle data for at most 64 actors and at most
`max_history_ticks + 1` ticks. They retain no inventory, bot brains, map or
collision clones and never enter `WorldState`. Restoring replicated state or
changing server tick rate invalidates history.

Hitscan queries historical target hulls without changing any live origin.
Static map occlusion is evaluated against the current collision world.
Damage is applied only to a currently alive enemy with the same player id,
spawn epoch, team, round and continuous historical life. Every spawn or
placement advances the replicated epoch, including round placement of a
survivor. Death/team/round/placement transitions and teleports beyond the
configured per-frame threshold break continuity. A missing or incompatible
historical hull falls back to the current hull.

The optional head/hands and separate aim ray continue to travel independently
through TickInput and snapshots. Current shooter aim still determines the
shot; client poses cannot choose a historical target box. Future VR target
geometry needs measured hitboxes rather than extending the current hull
approximation by guesswork.

## Verification

Tests compare an authoritative historical hit with a current-state miss,
verify unchanged live origins and current wall occlusion, and cover expired
and future ticks, life/team/round/teleport transitions, repeated inputs, bots,
prediction isolation and spawn epoch replication. Session tests cover
receive/consume timing, untrusted inner metadata, RTT caps and old-protocol
admission. The real Dust II UDP check exercises bounded view requests while
transport polling is delayed and verifies final prediction reconciliation.
