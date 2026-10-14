# Menu responsiveness

Status: the final menu and normal bot fixtures passed with owned exit 0 on `bf03ac82056af3e982e466c237dd39a892e9c851`, using the same copied executable. Independent QA approved all four bot captures. Final mandatory and CI results are tracked in [PR53](https://github.com/bentsignal/global-offensive/pull/53). The preceding `9737` stress run narrowly failed the unchanged input p95 limit: 101.997 ms against 100 ms; it remains recorded as a failure.

## Problem and correction

Real cursor movement previously appended a new MouseMove for every Bevy frame to an unbounded channel. Each delivered movement runs Flash event propagation, actions and hover picking. Obsolete movements accumulated ahead of clicks while the Bevy application itself continued updating normally.

The normal moving-pointer baseline reproduced six presented menu frames with a 3.226 s mean gap, a 4.617 s input-receipt maximum and an outstanding receipt when the proof finished. The baseline failed. Its earlier two-sample p95 calculation was subsequently corrected; the mean, maximum and pending state above do not depend on that calculation.

The producer queue now replaces only consecutive pending MouseMove events with the newest coordinates. Button down/up, keys, text editing, focus, resize, scroll, property calls and lifecycle requests remain ordered barriers. The latest preceding position is delivered before each barrier. A worker turn yields a movie-progress opportunity after 256 inputs or 8 ms measured between calls. This cannot interrupt a single slow movie action or synchronous GPU readback. When work remains pending, the worker no longer adds its 4 ms idle sleep.

Hidden menu prewarming loads Pause during normal map loading. Worker activity is separate from UI visibility and input ownership, and remains enabled until both MainMenu and Pause receive their actual OnReady callbacks. One deferred request shows the registered cached Pause root after readiness, even when the callbacks arrive in reverse order. Close, Home and Options revoke pending work. Native Options Back retains its existing restoration path; a closed late Pause callback stays hidden. Cached reopening does not repeat RequestElement or its stage sizing.

## Evidence and limits

Nine input-worker regressions cover generated high-rate motion across every barrier kind, 50,000 concurrent movements with 1,000 balanced clicks, elapsed/count progress under continuous arrivals, final motion before disconnect, and receiver shutdown. The isolated input leaf passed all 16 scaleform library tests; the final sleep-only update also passed the 16-test Scaleform check and explicit shim compilation check. Four causal readiness regressions cover reversed completion order, Loaded-before-Ready, missing root, close/reopen/Home and Options/Back without duplicate showing. The combined client library passed 216 tests with 2 ignored before the final sleep-only update.

The menu fixture uses normal startup/settings and the production Bevy input bridge, continuous owned cursor movement, asynchronous input receipts, presented-frame assignments and actual Play → Friends navigation. Its thresholds remain fixed. A separate normal direct-play fixture exercises bot movement, firing/reload, Pause, resumed simulation and acknowledged screenshots. The final bot run used five bots and natural focused state: movement advanced 187.52827 units, one shot consumed ammunition, reload restored 20 rounds, and all four screenshots acknowledged. Independent review approved the initial gameplay, played, Pause and resumed images.

| Final verification | Result |
| --- | --- |
| Frozen source / copied EXE SHA256 | `bf03ac82056af3e982e466c237dd39a892e9c851` / `6E7FEA7D726CC4F06054C6B8360DC88E2A1FA7B856E13F43102D3773A7395DF3` |
| Moving menu presentation | 496 image intervals over 20 s (~24.8 UI FPS); mean / p95 / max 40.314 / 56.396 / 96.059 ms |
| Input receipts; p95 / max; pending | 40; 87.023 / 93.036 ms; none |
| Play → Friends navigation | 120.865 ms; MainMenu ready in 5.207 s |
| Normal bot pacing / cached Pause | 992 samples over 5.118 s; mean / p95 / max 5.159 / 7.123 / 15.005 ms; no overflow; first Pause 90.7 ms |
| Fixture exit status | Menu and bot owned exit 0; source clean |

Both final fixtures used normal 1600×900 physical pixels at DPI scale 1. Frame assignments and engine input receipts do not measure OS input, compositor presentation or physical input-to-photon latency. The moving-menu fixture owns programmatic focus; the bot fixture uses its documented normal-input scope. Runtime captures use installed Legacy assets kept private. These measurements establish responsiveness within the stated fixtures, not complete menu/gameplay parity or 1:1 visual fidelity. Inherited News and unsupported UI surfaces remain separate gaps. No authored movie instructions, clock rate or VM timeout were changed.

## Live presentation diagnostics

The pending nonblocking-readback change replaces the worker's synchronous GPU
wait with one outstanding capture and a bounded completion callback. Input turns
and movie ticks continue while mapping is pending; rendering waits until that
same transfer buffer has been unmapped. The frontend retains its last completed
image. Completed captures retain their submission-time Pause revision, and a
viewport generation discards captures submitted before resize. This preserves
the existing readiness fence without republishing old pixels as a new frame.
Movie clock and input handlers are unchanged; raster opportunities can coalesce
while the GPU capture is outstanding.

This change has no measured FPS or latency result yet. Its ignored Host GPU
lifecycle test exercises pending-submit rejection, revision retention, resize
discard and remapping after pending-host teardown. Compilation, that GPU test
and the existing production Pause fixture still require central validation.
The render trace's `completion_age_ms` includes intervening input and movie
work; it is not a measurement of GPU wait time.

`CSR_PRESENT=immediate|mailbox|fifo` overrides the requested presentation mode
both at startup and when normal display preferences are reapplied. Without a
valid override, the saved VSync choice controls the request. The override does
not change that saved choice. Previously live preference application silently
replaced this startup-only override; screenshot runs skipped that application.
The `display presentation` log reports the requested mode, saved VSync choice
and surface queue hint. Backend fallback or compositor behavior can differ.

`CSR_FRAME_LOG=<milliseconds>` now logs app-frame intervals above a finite,
nonnegative threshold in ordinary menu/gameplay runs as well as screenshots.
It previously lived only in the screenshot system. The opt-in logger does not
control capture timing or process exit. These intervals and existing menu input
receipts can reveal frame/worker stalls; they do not measure physical mouse
latency, GPU presentation or scanout.
