# Bounded input guard after closing UI

Pause resume and console close set `InputBlockFrames(2)` after relocking the
cursor. Previously `player::look` kept that value unchanged while either mouse
button stayed held. It returned before mouse look and key-edge collection,
and the shared sampler also blocked movement. Holding a UI click could
therefore freeze look and movement indefinitely rather than just suppressing
the click's gameplay action. Bevy0.19.1 resets accumulated motion each frame,
so those discarded deltas were never replayed later.

The transition now transfers currently held buttons into the existing
per-button release-to-rearm capture guard and clears their pending pointer
edges. Its finite block expires even while those buttons stay held; the
existing immediate clear when neither primary mouse button is held remains.
Normal pointer capture uses the same helper. Look and movement resume while
the original held click remains unable to fire until release and a new press.
No sensitivity, interpolation, presentation, worker timing or gameplay rules
change. UI/readiness/capture/alive guards and short-tap sampling remain in place.

One existing production-input regression now starts with a locked cursor,
block2, held primary/secondary buttons, W and mouse motion. It requires finite
expiry, resumed look/movement, cleaned pending attacks and independent
release/repress rearming of both buttons. Source syntax/diff checks pass;
Cargo and actual owner validation remain pending integration. This is an input
correctness fix, not evidence of improved Nuke FPS or measured physical latency.
