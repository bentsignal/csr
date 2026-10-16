# Knife inspection input

Owner feedback WP07 describes interrupting a visible knife stab with inspect,
and using repeated reload/inspect inputs to interrupt and restart inspection.
This firsthand behavior is the reference for this bounded correction. It does
not establish exact native animation selection, easing or continuous spin
behavior.

The existing viewmodel admitted inspect only after the weapon's next attack
deadline. Knives also reject simulation reloads, so their rebound reload
input never affected the purely local inspection state.

Knife inspection now bypasses that presentation-only attack-readiness check.
It does not change the weapon's simulated deadline, permit another stab or
alter firearm inspect restrictions. A fresh attack still owns the pose before
inspection. The existing rebound reload action edge also issues a local knife
inspection cancellation; no physical key is hardcoded. It returns to the
authored idle clip through its incoming fade-in using the actually displayed
pose and the fixed masked sample base introduced by #173. A subsequent inspect
starts from the partially transitioned pose.

Start and cancel share one local serial so consumed input does not replay
across frames or weapon lives. Normal shot/reload/scope/plant/suppressor
restrictions remain. The viewmodel's transition state is cosmetic and is not
serialized gameplay state. Native simultaneous-edge priority, rare inspect
selection, outgoing fade policy and spin-loop fidelity remain open.

One installed-data regression drives the actual simulation's secondary knife
attack, presents its authored stab pose through the production viewmodel,
starts inspection before the real attack deadline, and cancels inspection.
Both new transitions must preserve the exact displayed pose at entry, and a
second actual simulation attack remains rejected until the unchanged deadline.
The test reuses the existing installed viewmodel fixture and needs no GPU.
Runtime verification remains pending.
