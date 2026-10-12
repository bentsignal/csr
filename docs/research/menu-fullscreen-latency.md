# Full-resolution menu capture and input latency

Normal-startup tests on the same combined source/artifact showed a resolution
dependency. Windowed 1600x900 passed with input-receipt p95 84.489 ms and max
93.064 ms. Fullscreen 2560x1440 failed: 397 fresh image assignments over 20
seconds, mean/p95/max intervals 50.296/66.928/118.394 ms, and 40 FIFO receipts
with p95/max 120.281/172.522 ms. No marker remained pending. Compilation and
the native reference game were idle during these measurements.

Those figures measure the existing production input path and assignment of
fresh worker frames, not GPU presentation or physical OS input latency. The
same artifact's fullscreen gameplay test passed, including a 116.7 ms first
Pause response. The menu failure does not establish a gameplay-frame defect.

The Scaleform worker services inputs, ticks the movie and performs a
synchronous full-resolution capture on one thread. Ruffle's public
`capture_image` maps its transfer buffer, waits for the GPU with
`Device::poll(PollType::Wait)`, copies padded rows, and converts premultiplied
bytes to straight alpha before returning. At 2560x1440 the RGBA image is
14,745,600 bytes, versus 5,760,000 at 1600x900. That entire operation delays
the next input turn; producer motion coalescing cannot preempt it.

`CSR_UI_RENDER_TRACE=1` adds up to 1,024 diagnostic frame records. Submission,
GPU wait, row copy and alpha conversion are measured separately. The existing
`CSR_UI_WORKER_TRACE=1` identifies input/tick turns. Keep the final normal
performance gate free of these optional traces after using them to locate
the causal bottleneck.

The first bounded correction preserves exact raster and alpha semantics.
Opaque pixels need no conversion: division by their unit alpha changes no
byte. Partial-alpha pixels retain the existing float formula, including
saturation; zero-alpha RGB remains unchanged. The local capture helper also
checks padded-row dimensions before constructing a complete image. Asset-free
tests cover all 65,536 alpha/channel combinations, padded row order and
malformed layouts. These tests and a same-resolution rendered comparison
still need the coordinated build/proof slot before a passing claim.

No movie clock, FIFO barrier, resolution, sampling mode, quality setting or
responsiveness threshold has changed. A nonblocking readback would be a
separate correction if measured GPU wait remains dominant.
