# Render preparation before admission

`client/src/remote_render_gate.rs` provides a small render-world acknowledgment
for the frontend loader. Call `install(app)` during plugin build after Bevy's
RenderPlugin has created RenderApp, before PipelinedRenderingPlugin cleanup
moves that app to its render thread. Missing RenderApp returns a typed error;
headless or recovering renderers never produce fake readiness.

After the expensive scene spawn and all deferred resources/entities have been
applied, call the persistent `RenderGate` resource's `begin(mesh_ids,
image_ids)`. Supply every currently owned scene Mesh and Image AssetId. The
combined list is bounded to 65,536 IDs. Keep map cameras and geometry active
behind an opaque loading overlay: hiding them prevents their view/material
pipelines from being queued and defers their first real render until after
admission. Continue to gate gameplay/input/HUD/effects separately.

The returned cloneable `GateTicket` exposes `generation()`, `status()` and
`cancel()`. Poll it without blocking. Only `Ready` permits the loader to hand
its already prepared map/world/manifest to RemoteClient::provide_map. Pending
preparation remains in AwaitingMap, outside the five-second Synchronizing
deadline. Failed/cancelled tickets trigger caller-owned cleanup. A replaced
generation cancels the old ticket; cancellation also revokes old readiness.
Each ticket has a distinct atomic state, so late render observations cannot
approve a later scene even if a removed/recreated main resource repeats a
generation number. Invalid begin requests leave the existing request intact.

Extraction carries only the active ticket and shared immutable bounded ID
arrays. At RenderSystems::PostCleanup, after the render CPU preparation,
render graph submission/presentation calls and cleanup have run, the gate
checks all supplied IDs against RenderAssets<RenderMesh>/RenderAssets<GpuImage>
and checks PipelineCache's queued work. It requires two consecutive complete
cycles with assets present and no waiting pipelines. Missing assets, renderer
resources or renewed queued work reset progress. Retryable shader errors stay
pending; errors remaining after the pipeline queue settles produce Failed.
This conservatively includes persistent unrelated pipeline errors.

This is a CPU preparation/submission guarantee. It is not a GPU-idle fence,
a promise that everything was drawn, or prewarming of future character,
viewmodel, camera or material variants not queued by the preparation scene.
No GPU polling, waits, network traffic or simulation mutations occur here.
Cancellation removes the main request and the next extraction clears the old
render request. One in-flight request and one progress counter are retained.
