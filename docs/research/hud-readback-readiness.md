# HUD readiness and asynchronous readback

The worker advances movie time before polling an earlier GPU readback. The
health/ammo Show observer can therefore announce completion while a capture
submitted during StartShow is still pending. Both captures have the same
presentation revision. Checking only the current success receipt and that
revision can cache partially faded pixels, then park the worker indefinitely
when the gameplay snapshot is unchanged.

This is a code-ordering defect. It does not establish that any particular
retained console-resume screenshot exhibited it; the reviewed 100/100 image
is not used as proof of the race.

`Calls::require_raster` now returns a monotonic capture fence. A successful
component receipt records its fence before publishing acceptance. Rendering
snapshots the current fence at submission, carries it unchanged through the
pending GPU readback, and forwards it with the completed frame. Admission
rejects pixels submitted before that component's receipt. Advancing a fence
also requires one fresh render even if the movie is otherwise idle; an old
readback cannot satisfy this requirement. Multiple receipts can share that
render. Existing viewport, player-life, rejection and text-fallback behavior
remain in force. No sleep or animation-duration estimate is introduced.

The deterministic client regression delivers success, then a pre-Show frame
with the same presentation revision and distinct pixels. It must retain the
ordinary fallback and keep the worker active until post-fence pixels arrive.
With the admission check omitted, this regression fails because the old frame
enters the snapshot cache. The installed-data GPU readback regression also
advances a fence while mapping is pending, checks the old capture retains its
old fence, and requires a fresh capture without a visual/time change.

This change does not tune HUD placement, typography, native animation timing,
physical input latency or game-frame performance.
