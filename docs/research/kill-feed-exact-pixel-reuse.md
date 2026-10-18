# Kill-feed exact-pixel upload candidate

Originally prepared on `6d79cae4b800ef377e2ba23a664e18571f6c52fc`, this work
was still absent from master after #230. It is now integrated with bounded
collision scratch reuse and contiguous capture copying in #231. It addresses
redundant top-image mutation in
`crates/client/src/kill_feed.rs`; it establishes no FPS improvement or dominant
HUD bottleneck. Root owns A/B attribution with immutable executables.

After the existing exact presentation-revision and viewport acceptance, combat
HUD admission still runs on every accepted frame. Notice readiness and keyed
row rejection/fallback remain unchanged. The top crop validates the complete
RGBA readback and borrows its contiguous full-width prefix. An immutable asset
lookup compares every byte plus the actual extent and expected RGBA8 sRGB,
single-mip, single-sample 2D layout before any mutable asset access. Only literal
byte equivalence permits reuse: the raster is preserved without hashes,
tolerances, animation throttling, or stale-frame acceptance. A reused current
frame still completes `presented` and the existing first-admission trace.
Changed bytes/dimensions, missing CPU data, or an unexpected layout retain the
old size/data mutation path; a missing asset still cannot finish presentation.

Bevy 0.19.1 source inspection: `bevy_asset/src/assets.rs` queues `Modified` when
`AssetMut` is mutably dereferenced; `bevy_render/src/render_asset.rs` clones
changed main-world assets for extraction; `texture/gpu_image.rs` writes the
image extent. Avoiding notice mutation can therefore avoid redundant cloning
and upload. Full-viewport Ruffle rendering/readback and alpha-update costs are
unaffected, as are combat HUD snapshot reuse and #208 readiness receipt/fence
rules. Worker creation, activity, and update rate are unchanged.

The two new `kill_feed::tests` fixtures invoke the production admission method
with real `Assets<Image>` and Bevy's asset notification systems, without a
renderer/window/worker. They count `AssetEvent<Image>::Modified` for identical
frames, one changed alpha bit, changed physical shape with identical crop bytes,
first CPU data, and changed layer extent; they also exercise stale/future
revision, viewport mismatch, unready, malformed readback, missing asset, and
no-drawable-row completion. Existing combat HUD fresh-readback fixtures remain
unchanged.

## Pending root verification

The original source implementation and independent review ran no builds.
Integrated #231 workspace, Release and runtime checks are pending; the earlier
collision-only workspace pass does not validate the new HUD changes.

- `cargo test -p client kill_feed::tests` (new notification/admission fixtures).
- `cargo test -p client combat_hud::tests` (existing #208 receipt/fence fixtures).
- `cargo test --workspace`.
- Release `launcher -- probe de_dust2` and `launcher -- sim de_dust2`, each with
  `failures: 0`, under root's exclusive Cargo slot and shared-target hygiene.
- Actual HUD changed pixels, fade, resize (including odd sizes), clear, reopen,
  per-row fallback and local/view-life transitions from an immutable candidate.
- Matched HUD A/B attribution and performance checks; dominant-cost and FPS
  claims remain unestablished.
