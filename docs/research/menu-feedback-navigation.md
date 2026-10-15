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
The replacement must be
validated with a newly built client before calling this fixed.

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
existing click/key/remaining-cap coverage. Runtime improvement is still pending.

Pinned Ruffle source review also confirms hidden AVM1 clips still receive frame
and global MouseMove work. This is normal Flash behavior, not a reason to skip
hidden helpers globally. Passive call_internal_interface does not call the
generic Player::update; an early hypothesis that it repeated hit tests was
incorrect and is not used by the implementation.

Still open: final slider proof; sustained Pause performance fix; power button
availability while the offline picker owns navigation; all original menu
features/loading artwork fidelity. The current work does not claim these done.


## Cached Home/match player replacement (pending verification)

Cold Home retirement is retained as an unready experiment in PR130 because it
improves Pause but adds about 5.8 seconds of repeat Home initialization. The
replacement keeps the original parsed Home worker parked with SetActive(false),
then uses an isolated cached match worker for Loading/Pause. That worker alone
retires its duplicate Main after actual Main/Pause bootstrap readiness. Returning
Home reuses its existing timeline; no Ruffle visibility semantics/fork change.

Each worker owns separate mutable Data/actions/lobby/readiness and root maps.
Only immutable installation/localization lookup data is shared. Current settings
and actual inventory items refresh before resume. Ordered park/resume replaces
queued input, releases held keys/buttons/focus, revokes old navigation, and gates
in-flight native actions with a worker-owned epoch. Only the selected domain
receives new input and submits new renders; an already submitted old readback
is discarded while inactive and cannot pass the frontend's fresh revision fence.
Viewport is explicitly refreshed even if the global window size did not change.

The first match worker boots its original LoadingScreen/Pause alongside the
required Main globals; subsequent matches reuse it. The existing production
flow optionally exercises native Pause -> Audio -> Back and records actual Exit
to freshly presented Ready Home latency. A separate three-second pending capture
remains if restoration is slow, rather than hiding the delay behind fixture waits.
First-ever Home after a direct developer startup still needs initial Home boot;
it clears black rather than retaining the former world target. First match
bootstrap cost, retained memory and complete runtime roundtrip remain unverified.


## Exact 62e24a2 bounded observations

Five frontend tests and 33 Scaleform tests pass (one ignored) after forced
workspace play builds. The direct-start Pause pressure case exits 0 with four
personally inspected 1920x1080 captures. Pause composites over the world and
Resume clears it. First Pause response is 107 ms; the four-second readback
window measures 22.90 / 36.65 ms mean / p95 versus c1's 52.62 / 60.41 ms.
Reported slow pointer work averages 6.74 ms. This direct fixture has no parked
Home, so it does not establish the cost of both cached workers or physical
input/present latency.

The original Home-start flow is an incomplete failure, not a pass despite its
normal close returning 0. Its --screenshot option suppressed the usual
background startup; the fixed three-second fixture then clicked Help before
Pause boot completed. The preserved flow-pause image contains the world only.
The corrected opt-in flow accepts CSR_TEST_FLOW_OUTPUT without --screenshot,
waits actual fresh Pause presentation, and can apply five seconds of pointer
pressure before Options. Production startup policy is unchanged. That corrected
normal-start full roundtrip remains pending.

Whole-process memory sampling peaks at 10.37 GiB private bytes in the incomplete
two-worker flow and 8.19 GiB in direct pressure, ending at 2.80 / 2.32 GiB.
These include map/load allocations and differing scenarios, not an isolated UI
memory comparison. Both retain the owned-process timeout and free-RAM guard.

## Remaining full menu fidelity

The slider/navigation passes do not establish a complete Audio page. Current
captures show only Master Volume and a large empty microphone panel. The
remaining original controls, microphone services, layout, defaults and
persistence remain open; supported-convar filtering currently excludes them.

The original Scaleform Pause reference in the [Radar centralizado Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=516337371)
([embedded screenshot](https://images.steamusercontent.com/ugc/422566214060763931/1E90D7113BE3543FC527060582B1AF9E6350579E/))
shows the world behind the translucent menu/logo and includes Choose Team,
Call Vote and Community Servers, absent from our five-entry Pause. The guide
was posted September 2015 and updated in 2023; exact image capture date is
unknown. This supports composition/missing-entry evidence, not pixel or timing
parity. Images remain private outside git. Offline power-button availability
and its authored confirmation path also remain open.
