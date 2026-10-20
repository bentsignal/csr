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
Final source `01535ad8e8eeecb4b9fffcbf73b5891bb425c3b6` passed both
CI jobs in run `38880083763`. The orchestrator's frozen union `52ff900` passed
full DEV/PLAY checks and both installed production viewmodel tests. The
[final combined RELEASE checks](overnight-fidelity-verification.md) also
passed; this is not a standalone feature-branch RELEASE claim. The subsequent
normal-merge head `80dbb7f` passed CI run `38891064045` with the exact final
tested crates tree.

The exact immutable union executable (SHA-256
`58E91BC08E180AE2F1FD55A1293911506F6405F8AEBD3EA05B55B40D869E4705`)
ran two ordinary screenshot-only cases. Both exited zero. The CT knife case
logged actual `INSPECT` then `INSPECT_CANCEL`, zero shots and intact personally
reviewed knife/hand/sleeve geometry. The AK case logged actual `INSPECT`, zero
shots, 30/90 ammunition and intact personally reviewed weapon/both hands.
Private packets are `knife-interruption-20261016/render-52ff900-ct-knife-inspect-cancel`
and `render-52ff900-ak47-inspect` under the owner's fidelity folder. Raw
receipts are unchanged; separate agent image reviews record these findings.
This admits requested activity and model integrity only. A single cancel
still image does not establish native easing, repeated-input timing or spin
loop parity.

## October 20: distinguish reload from the following inspect

Bounded original Legacy controls now supersede the earlier assumption that
reload itself should return a knife to idle. Untouched CT stock controls show
that standalone inspect and reload presses preserve the active inspection.
A post-completion inspect starts a fresh animation. A rapid R/F trial delays
the closing cue to follow the final inspect, so absence of another opening
sound alone cannot prove that a restart was rejected.

A matched Butterfly control isolates that distinction. Inspect alone at about
0.43 seconds after the initial press keeps the original closing cue at 3.494
seconds after its opening cue. Adding one reload press before the same inspect
produces a second opening cue and a closing cue 3.497 seconds after that second
opening. Original images show the resumed handle motion. Both cases selected
`lookat03`; this does not establish whether a restart always retains its variant.
The private native selection used an ordinary local give plus a documented
classname pickup workaround, with actual Butterfly model/HUD verified. The
untouched stock controls provide a separate check on isolated input behavior.
All twelve native preference snapshots were restored exactly after each run.

Local presentation therefore keeps the current knife inspect when reload is
pressed, and permits the next inspect to restart it once. Repeated inspect
without reload is consumed without changing pose, variant, accepted edge or
pending sounds. A fresh inspect after the selected authored clip completes is
accepted even when no intervening render processed that completion. Accepted
attacks, view/life/weapon changes and clock resets clear restart permission.
Weapon readiness and TickInput remain authoritative and unchanged.

This is a shared knife presentation rule inferred from the stock and Butterfly
controls, not independent parity verification for every knife. Exact native
restart variant selection, easing, simultaneous-key ordering, Karambit spin
trajectories and historical-version differences remain open. Installed checks
exercise the shared dispatch across all eleven early families; they do not
replace those original-game observations.
