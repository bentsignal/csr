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
