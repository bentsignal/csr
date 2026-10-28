# Full Skate runtime

GPL-3.0-only, isolated executable for the CS crossover. The public upstream
revision is pinned in Cargo.toml/Cargo.lock and credited in NOTICE. Original
game files are supplied only by the player's private pack.

`upstream/` contains the public rewrite's Rust host coordinator from
SK8-ENGINE/skate-3-rust-engine at
`2e16697797c8358d16f747a8259354ff2921041a`: physical player states, full board
and skeleton solve, action/motion/animation graphs, gesture recognition,
camera and scoring. It is deliberately separate from the Apache CS host.

Adaptation points:

- Root module files were moved into their matching directories as `mod.rs`;
  relative explicit paths in scoring were adjusted accordingly.
- The physics root disables only its standalone Bevy plugin/scheduling and
  presentation entry points. `csr_advance` calls the original `frame::advance`
  coordinator. Its original state transitions, joint drives, contacts, output
  and camera still run in their original order.
- The camera/input/modding root files expose pure owners without the original
  renderer, SDL/window polling, Lua execution or application schedule.
- The optional Lua debugger snapshot in player_physics is excluded. Vanilla
  solver passes and override filtering remain the upstream implementation.
- `src/skate_world.rs` retains the original portable collision compiler,
  adjacency welding, edge flags and query metadata, without map rendering.
- `src/lib.rs` supplies scheduling names and plain HUD/diagnostic transport
  types referenced by those owners. They contain no gameplay calculations.
- `src/bridge.rs` converts Source inches/Z-up geometry to native metres/Y-up,
  removes host bevel triangles that collapse at the compiler's existing 1 mm
  weld boundary, converts Xbox reports with the original native Pad/map, and
  exposes completed physical bones/camera. The original GLB inverse binds
  provide reference axes for CS retargeting and imported board skinning.
- Completed world bones include the original animation-to-world root, using
  the upstream matrix conversion to normalize native affine padding. The host
  maps modern CS names and Y-up bind anatomy rather than assuming ValveBiped.
  Its CS leg solve preserves segment lengths while reaching native ankle
  targets, accounting for the original skater's different proportions.
- Prepared collision, graph and animation metadata are cached across restores;
  a restore still reconstructs physical state from its authoritative journal.
- `src/dynamic.rs` cooks CS convex parts once, updates door kinematic poses and
  intact breakables, and feeds original solid contacts and moving queries.
  `upstream/modding/moving_queries.rs` retains the upstream non-Lua query adapter.
  CS top/side edges supply derived grind splines rather than Skate course data.
- `src/asset_cache.rs` retains immutable stock collections and animation banks
  from the frozen admitted bundle. Custom difficulty overlays are cached
  separately, preserving the original distinction from camera/input data.
  Actor reconstruction lends the same prepared collision world back to the
  next runtime; native queries reset their transient contacts before use and
  dynamic external queries are cleared/replaced. Physical actors and their
  state are always rebuilt/replayed; no previous player's state is reused.

The normal executable accepts only `--stdio-provider-v1`. It receives framed,
bounded binary messages from `crates/mechanics-link`; it has no UI or game
window. The app chooses its executable. A content pack cannot choose or supply
a native executable. Its data bundle has a specific allowlist excluding game
executables, Lua, native libraries and external paths.

CS `World::step` is the only caller. A host timestep accumulates into the
upstream simulation clock; the physical integration timestep is unchanged.
The host retains a persistent journal of original spawn/momentum, Xbox reports
and authoritative CS door/breakable states, plus executable/archive hashes.
The helper is a derived cache:
restoring another journal reconstructs and replays it. A SHA-256 prefix receipt
distinguishes branches of equal length. Snapshots do not depend on unrecorded
process state. Long historical replay can cost time; this provider remains
offline and existing network admission rejects active mechanic state.

Build without game data:

```
cargo build --manifest-path mods/skate-runtime/Cargo.toml --release --locked
```

The upstream standalone application's embedded tests refer to its renderer,
full app and private fixtures; this isolated library does not compile that
application test target. The host's contract checks and private `skatecheck`
exercise this actual executable, including controller gestures, both host tick
rates, snapshot replay, native ledge acquisition, walking/remounting, live
door/breakable contacts and mounted CS geometry. No private fixture is in Git.

The adapter also runs the upstream session-marker validation, UI hold timing
and actor-reset path. Explicit Start/Pose checkpoints preserve placed markers
in host snapshots and across mode changes; no hidden process-global state is
required. Marker reports are sampled on every host tick so a D-pad press
between native physics periods is retained. The walking camera alone reverses
its vertical publication axis; raw pad/trick controls remain unchanged.
