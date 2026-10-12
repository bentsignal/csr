# Authoritative death receipts

`GameEvent::Kill` carries one owned `sim::death::DeathReceipt`. There is no
second death event, physics impulse, corpse solver or authored HUD integration.
The existing plain-text feed retains the structured receipt inside its
six-entry/six-second bound and uses the original receipt names.

The key is `(victim PlayerId, life_epoch, producer tick)`. Capture happens in
`World::kill` before dropped gear changes, using current authoritative feet,
view angles, velocity, duck state/fraction, ground state, and optional admitted
head/hands. Names and teams belong to the participants at the kill. WORLD
damage has no attacker participant. A subsequent rename, team change,
respawn or removal cannot rewrite that context.

The producer stamps `self.tick`: a death during step N to N+1 has tick N,
while the enclosing post-step `EventBatch` has tick N+1. Generic environment
damage outside a step uses its current world tick. Validation permits the
receipt tick to precede the batch tick, with the existing protocol ceiling.
Client scene/session generation remains local ownership, not a server-supplied
permission to retain effects across teardown.

Firearm and melee damage call sites pass their actual ray direction and hit
point/group. Firearm direction includes the actual recoil/spread sample.
Lag compensation can place the hit on an older hull; the death pose is still
the current victim state. Bomb, HE, fire-field and generic environmental
damage supply no hit ray. No direction is reconstructed from attacker
location, blast center or unrelated impact events, and direction does not
define a force magnitude or solver response.

Wire version 5 changes the Kill payload and the netcode.io protocol ID, so
v4 peers cannot join. Snapshot compression, input acknowledgement,
ContentIds, reliable ordered event delivery and hosted admission remain the
same seams. The whole event batch is validated before queue publication:
identity/key consistency, nonzero life, bounded ticks/names, finite pose,
existing admitted tracked limits, normalized rotations/rays, duck fraction
and headshot/hit-group consistency. No current actor lookup is required;
receipts for removed actors or older lives remain meaningful.

The direct transport client and RemoteSession share an aggregate 4,096-event
cap; the existing per-batch, message-byte and batch-count bounds remain.
Validation and capacity checks precede whole-batch queue mutation. Consuming
a batch releases its event budget. Transport failure/overflow and explicit
disconnect clear pending events so a terminal client cannot expose a valid
prefix from an update that failed later.

Prediction clears all its events after replay. Only authoritative events
reach presentation. Cancellation clears existing queues and match teardown
releases the feed. A future corpse adapter must separately define bounded
scene/life ownership and observed persistence/round behavior; it must not
attach an old receipt to a new player mesh or duplicate authoritative drops.
Original death-notice calls, 2015 behavior, impulses and falling motion remain
unmeasured by this foundation.
