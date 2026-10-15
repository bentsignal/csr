# Menu navigation follow-up (2026-10-14 playtest)

The owner's Home hover response is acceptable. Two reproducible problems are
separate: leaving Audio for Offline with Bots retained the Options movie, and
Pause hover remained substantially slower than the main game update.

## Navigation ownership

An engine navigation command now revokes the pending Options show and hides its
registered root before opening another destination. Cold Options and offline
roots remain hidden until authored Ready; a late callback cannot reactivate a
superseded destination. The offline picker has its own requested flag rather
than treating every Menu/Browser state as permission to show.

Scene root changes are one worker Present transaction. Returning Home starts a
new presentation revision so the previous Pause raster is not exposed while
commands/readback are pending. Static screenshots cannot establish that every
transient flash is gone.

On `da0bfa14c431c89de30707f290fb633a3b5cf77c`, the normal production pointer
bridge at 1920x1080 reached the offline picker from Audio without overlapping
Options, and the existing Dust II -> Pause -> Resume -> Home -> Inferno flow
completed with six screenshots and failures: 0. Those screenshots were reviewed.
Private captures are under `csr-fidelity/overnight-feedback-20261014/` in
`menu-audio-offline-da0bfa1` and `menu-navigation-flow-da0bfa1`.

## Slider geometry

Public runtime display inspections of the installed horizontal slider show its
`ScrollPip` resting at local y=5, then moving to y=0 during a drag and staying
there after release. The reproduction moved the pointer off the horizontal
track; the value still changed correctly. No movie instructions or function
bodies were read.

An initial instance startDrag wrapper did not intercept the installed path;
the 386966a production drag screenshot still shows the bug. The replacement
wraps only that pip's observed onPress/onMouseMove callbacks, delegates the
original and arms a one-event correction when the widget reports native mouse
dragging. The host uses Ruffle's public UpdateContext drag constraint and
Player::update_drag to preserve the measured y, retaining x bounds, offset,
value conversion and sounds. A fresh Options load clears any pending request.
The replacement was built after cleaning all 23 workspace play packages at
c1e7b04. Its production 1920x1080 off-axis pointer drag (from volume 0.5)
exited 0; the inspected final screenshot shows the pip on the track, and the
authored callback plus private saved config record volume 0.79. The same exact
client's Audio -> Play -> Offline screenshot shows the picker without Audio
overlaid. These captures establish those cases, not all menu fidelity.

## Pause response investigation

Using the existing bounded bot/Pause hover fixture at 1920x1080 on the same
`da0bfa1` binary, turning menu_blur off did not remove the bottleneck:

| Existing setting | Worker readback mean / p95 | Input-turn mean | Movie-step mean |
| --- | --- | --- | --- |
| blur on | 74.39 / 95.50 ms | 45.19 ms | 25.29 ms |
| blur off | 74.12 / 92.27 ms | 44.53 ms | 25.61 ms |

Render submission averaged about 1.7 ms in both cases. The first Escape response
was 139.4 / 119.4 ms respectively; both old fixture checks passed because those
checks cover first response and gameplay cadence, not sustained Pause hover.
The release linker was active during this shared-PC comparison. This is worker
CPU/readback cadence, not physical input or compositor latency, and is not a
claim that gameplay matches a native frame rate.

The 386966a opt-in subphase trace measured 94 MouseMove handlers averaging
27.75 ms and 70 movie ticks averaging 26.89 ms during the fixed Pause hover
window; passive visibility sampling never reached the 5 ms reporting threshold.
It records timing labels only, never authored movie instructions. Do not rewrite
or disable the backdrop blur on the strength of this A/B: it did not explain
the measured slowdown.

A concrete queue path permits unnecessary pointer work after the elapsed input
budget: without a real click/key/lifecycle boundary, [MouseMove, passive sample]
was still treated as a completion pair. The pending correction drains only a
leading passive prefix in that case, allowing presentation before the next
pointer handler. Actual boundaries retain their original FIFO completion rule.
A focused regression exercises both passive/motion orders and preserves the
existing click/key/remaining-cap coverage. A fixed four-second window after the existing Pause capture measured worker
readback cadence improving from 71.78 / 89.90 ms mean / p95 on 386966a to
52.62 / 60.41 ms on c1e7b04. The candidate processes one MouseMove per input
turn in that window (76 turns), rather than the baseline's additional move in
24 of 56 turns. Both runs exit 0. This shared-PC observation supports the
queue fix, but sustained Pause remains slow and physical input/present latency
is not measured.

Pinned Ruffle source review also confirms hidden AVM1 clips still receive frame
and global MouseMove work. This is normal Flash behavior, not a reason to skip
hidden helpers globally. Passive call_internal_interface does not call the
generic Player::update; an early hypothesis that it repeated hit tests was
incorrect and is not used by the implementation.

Still open: sustained Pause performance fix; power button
availability while the offline picker owns navigation; all original menu
features/loading artwork fidelity. The current work does not claim these done.


## Explicit inactive Home retirement experiment

One bounded existing menu_probe run on the all-workspace-clean 386966a build
compared the same four Pause pointer moves before and after standard Flash
`_level31.unloadMovie()`. This retired the engine-owned inactive Home root; it
did not alter Ruffle's visibility behavior or hidden helper callbacks.

| Worker operation | Before Home unload | After Home unload |
| --- | --- | --- |
| Four MouseMove handlers, mean / maximum | 24.69 / 29.00 ms | 6.14 / 7.23 ms |
| Reported slow movie ticks, mean / maximum | 20.17 / 61.95 ms | 5.85 / 8.66 ms |

Tick reporting has a 5 ms threshold, so these tick means describe only reported
slow samples, not all ticks. The Pause controls and hover remained visible in
the inspected captures, but Home's blue background disappeared and the player
cleared opaque gray. A production fix therefore needs transparent readback and
an explicit Home restore lifecycle with stale callback/presentation fences.
The unload experiment alone is not a validated shipping lifecycle policy.

A separate existing-probe observation on c1e7b04 confirms the original
`RequestElement("MainMenu", gameAPI)` loads Home again after standard unload
(a second Loaded event and a clean final Home capture). Its uninitialized
Pause request exposes a green rectangle before Home is unloaded; after the
normal Pause show/hide, final Home is clean. This is not attributed to reload.

The pending production policy retires only Home after actual MainMenu and
Pause readiness in Match. It reloads through that same authored loader on
return to Menu/Browser using an engine-owned immutable receiver generation.
Stale load/Ready/navigation callbacks and Loaded events cannot claim the new
root at the reused level. Bootstrap readiness survives retirement, while live
movie readiness does not. Restored Home stays behind a fresh presentation
fence until Ready; input is withheld during restoration. Transparent Flash
window mode preserves the world behind Pause using the existing straight-alpha
readback. Required runtime roundtrips and transparent composition are pending.


## Cold-retirement production result: do not ship

Exact cf83d08 was built after cleaning all 23 workspace play packages; five
frontend regressions and 32 Scaleform tests passed (one ignored). The existing
production flow with optional native Pause -> Audio -> Back then Resume and a
second Inferno launch exits 0. Transparent Pause is visibly over the world.
The matching four-second pressure window measures 20.85 / 34.44 ms worker
readback mean / p95, compared with c1e7b04's 52.62 / 60.41 ms. Reported slow
pointer handlers average 6.17 ms; first Escape is 76.9 ms. Slow subphase samples
have a 5 ms threshold and none of this measures physical input/present latency.

However, Home reload after Exit takes about 5.8 seconds: roughly 1.8 seconds
of reported load-executor work plus one 3827 ms initialization tick. The old
fixed-three-second returned-Home capture still shows the prior world target;
that capture is preserved as a failed/late Home image, not a fidelity pass.
A correct subsequent native second launch does not excuse this delay. The
flow correction waits actual Ready plus a fresh Home presentation, retaining
a separate pending-state capture/log if Home has not returned after three
seconds. That correction is not yet built. The cold-unload policy remains a
draft and is being replaced by separately cached Home/match workers.

One bounded public onUIHide/onUIShow probe confirms authored hide alone does
not remove the dominant cost: the same four pointer moves average 27.37 ms
before and 22.51 ms after hide, far above explicit-unload cost. Its final Home
capture is backdrop-only because the setup explicitly set Home _visible=false
and the final commands did not restore that property; no complete visual Home
roundtrip is claimed from that observation. No generic hidden-clip behavior or
Ruffle fork has been changed.

## Remaining full menu fidelity

The P0 slider and navigation checks do not establish a native-complete Audio
page. Current production captures show only the Master Volume row and a large
empty microphone panel. All remaining original audio controls, microphone
services, layout, defaults and persistence remain open.

The original Scaleform Pause reference in the [Radar centralizado Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=516337371)
([embedded screenshot](https://images.steamusercontent.com/ugc/422566214060763931/1E90D7113BE3543FC527060582B1AF9E6350579E/))
shows the world behind the translucent menu/logo. It also includes Choose Team,
Call Vote and Community Servers, which our current five-entry Pause lacks.
The guide was first posted September 2015 and updated in 2023; exact screenshot
capture date is unknown. This supports composition and missing-entry evidence,
not pixel/timing parity. The screenshot is retained privately, outside git.
