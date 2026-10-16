# Loading presentation before scene imports

Candidate stacked on archived loading-art PR182. This is production staging,
separate from the same-fit diagnostic callback correction in a6b7741.

## Problem and behavior

A completed asynchronous Level previously requested Match immediately.
OnEnter(Match) synchronously imports world/props and character/viewmodel/item
assets before the next frontend Update. A loading movie could initialize its
image on its worker during those imports, then disappear at the first main
Update without its artwork ever reaching the frontend image.

The loader now retains its completed result, owned by the current serial.
An actual current overview fit's Init supplies production readiness, checked
against the active callback epoch, selected map/serial and exact Loading root.
The frontend sends an existing Present transaction with a new revision after
that Init. Only an assigned raster carrying that submission revision passes;
an older readback finishing later cannot pass it. Admission occurs in a later
app frame, allowing the assigned image a completed app/render iteration before
synchronous imports. This is a presentation opportunity, not monitor-scanout
or physical-latency measurement.

The same check precedes hosted-server staging, and remote map entry retains its
server-info ownership/status validation. Cancellation/reselection discards the
completed result; changed serials and stale callback/frame revisions cannot
admit a replacement map. Headless clients without a frontend retain direct
admission.

## Bounded fallback

Unsupported art, missing context, a reported movie failure, and screenshot-only
runs keep the existing progress/cancel surface for one app iteration. The
currently supported production image acknowledgment is the installed/authored
radar-fit path; a separate loading JPEG is not treated as an acknowledged radar.
If a supported movie never initializes or produces its raster, a maximum 5s
wait after CPU-result readiness escapes to the existing fallback. Successful
art has no minimum timer. There is no test-only loading delay or synthetic
completion callback; the strict archive capture flow remains unchanged.

## API and verification scope

Read locally: Bevy 0.19.1 FrameCount increments during Last and wraps; the state
schedule follows PreUpdate; nonpipelined SubApps updates render after the main
schedule. The client already disables pipelined rendering. Existing scaleform
Frame.presentation_revision is captured at raster submission, rather than
copied from a later input at readback completion.

Focused pure gate cases cover late old readbacks, same-frame rejection,
matching-generation admission, cancellation/reselection, unsupported fallback,
frame-counter wrap and the finite never-ready deadline. Source checks only so
far: Cargo/installed/graphical validation is pending the orchestrator's lease.
The archive-loading-flow-20261016-071953 negative remains preserved: decoded
archive bytes were not an admitted displayed image. No rendered UI11 delivery
claim is made until actual loading captures pass.
