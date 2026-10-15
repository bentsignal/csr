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

A per-widget compatibility adapter preserves the existing pip's measured
resting y for its zero-height drag constraint, while delegating its original
startDrag method, x constraints, callbacks, value conversion and sounds. It
does not replace the slider or globally patch Flash dragging. The adapter must
be validated with a newly built client before calling this fixed.

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

The next trace splits the existing opt-in `CSR_UI_WORKER_TRACE=1` into bounded
slow MouseMove, movie tick, callback and passive visibility sampling phases.
It records timing labels only, never authored movie instructions. Do not rewrite
or disable the backdrop blur on the strength of this A/B: it did not explain
the measured slowdown.

Still open: final slider proof; sustained Pause performance fix; power button
availability while the offline picker owns navigation; all original menu
features/loading artwork fidelity. The current work does not claim these done.
