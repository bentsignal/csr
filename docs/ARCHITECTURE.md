# Architecture

## Data flow

```
CS:GO install (read-only)
  │  gameinfo.txt search paths
  ▼
vfs::Vfs ── map pakfile (zip, LZMA) ─┐
  │        pak01_dir.vpk ─────────────┤ first hit wins
  │        loose csgo/ files ─────────┘
  ▼
level::Level::load(map)
  ├─ bsp::Bsp            geometry, lightmaps, entities, static props
  └─ collision::CollisionWorld   brushes + displacement triangles + prop hulls
        │
        ├──────────────► sim::World (128 Hz)      ◄── TickInput per player
        │                  bots, loadouts, rules      (keyboard, VR, network)
        │                  movement::player_move
        │                  → WorldState / Snapshot
        ▼
client (Bevy)
  world_mesh   faces + displacements → one mesh per material + lightmap atlas
  props        MDL → meshes, ambient-cube + sun lighting per instance
  material     WorldMaterial + shaders/world.wgsl
  player       input → sim → interpolated camera
  hud/hud_art  original HUD imagery, map radar, equipment, round state
  ui_assets    runtime VFONT1 decoder and original PNG art
  interpolation previous/current render poses shared by camera, bodies and gun
```

## Principles

1. **Read, never write, game data.** Everything Valve made is read at runtime
   from the user's install.
2. **Translate at the boundary.** Format crates (`bsp`, `vtf`, `mdl`, ...)
   decode files into plain Rust data in Source units and axes. Only the client
   converts to Bevy space (`client::coords`: `(x, y, z) → (x, z, -y)`).
3. **One simulation path.** `sim::World::step` is deterministic. The server,
   client prediction, replays and headless tests all run the same code.
   Gameplay never runs in render-frame systems.
4. **Game-agnostic identity.** Content gets `ContentId`s (`csgo_weapon_ak47`)
   and normalised definitions (`weapons::WeaponDefinition`). Other games'
   importers produce the same types.
5. **Verify on real data.** `csr probe` and `csr sim` run against the real
   install. Unit tests use synthetic data, so they pass without the game.

## Simulation and server authority

`sim::World::step` is the only place gameplay happens. It runs the bots
(`World::bots`; they think at the start of each step from the previous
tick's state and what they heard), applies each player's `sim::Loadout`
(knife model, finishes, deathmatch practice weapons) to the weapons it
hands out, and runs movement, weapons, grenades, rounds, the economy and
the bomb. The Bevy client turns devices into `TickInput`s and draws; it
never changes the world itself. Offline it calls `step` with the local
input (a listen server of one); online the server does.

- **Tick rate** is `World::tick_rate` (default 128, 16 to 256). Every
  cadence and timer derives from it (`World::dt`, `World::ticks`). Tests
  compare movement, recoil, inaccuracy decay, reloads and round timers at
  64 and 128 tick; `csr sim --tickrate 64` runs the real-map checks at 64.
- **Determinism.** All randomness comes from explicit seeds (the shooter's
  shot count for spread, the round for bomb carriers, the tick and bot id
  for bots), and bot brains are kept in id order. `World` is `Clone`, and
  `World::state_hash` hashes the encoded state. Tests step cloned worlds
  (with bots) and restored worlds side by side and require equal hashes
  every tick.
- **Replicated state.** `sim::state::WorldState` (serde) is everything that
  changes while playing: players (pose, health, armour, money, the
  inventory with ammo and weapon timers, loadout, tracked VR pose), the
  round, dropped items, grenades in flight, smoke and fire, the bomb, and
  the rules and movement settings in force. `Snapshot` adds the tick's
  `GameEvent`s. It is encoded with postcard; weapon definitions travel as
  their `ContentId` and resolve against the receiver's catalog. What both
  ends load from the map (collision, spawns, objectives, the catalog) and
  the server-only bot brains are not part of it. Replaying a match with
  bots requires their authoritative inputs or cloned bot brains; clients
  must not regenerate server AI from a snapshot. Prediction and
  reconciliation remain in Track A2.
- **VR seam.** `TickInput` carries an optional `TrackedPose` (head and both
  hands, as offsets from the player's origin) and an `AimRay`. Shots, knife
  swings and throws use the aim ray when there is one, with its origin
  clamped to 48 units from the eye; players keep the latest pose so others
  can draw it. Flat clients send neither and play exactly as before.

## Animation

* `mdl::anim` decodes both of the game's animation encodings: classic
  per-bone RLE tracks (player animations) and CS:GO's frame animations
  (weapons), whose "smallest three" `Quaternion48S` layout was recovered
  from the data (see the doc comment on `quat48s`).
* Clips are retargeted by bone name (`anim::retarget`), so animation-only
  include models (`animset_t.mdl`, knife anim models) drive any skeleton.
* Viewmodels keep a persistent pose that each sequence layers onto using
  its per-bone weight list; bones an animation does not touch keep the
  previous pose, as the game does.
* `client::skinning` uploads skinned meshes and writes joint transforms;
  `bone_merge` attaches arms to guns and guns to hands by bone name.

## Coordinate systems

| | Source | Bevy |
|---|---|---|
| Up | +Z | +Y |
| Forward (yaw 0) | +X | +X |
| Left | +Y | −Z |
| Units | inches | inches (unchanged) |

## Collision

`CollisionWorld` holds convex solids (plane sets) and triangles in a uniform
128-unit XY grid. A box sweep expands each plane by the box's support
distance and runs the classic enter/leave-fraction clip. Triangles get a
1-unit back thickness and the full separating-axis plane set (face ±normal,
box axes, edge × axis), which makes box-vs-triangle sweeps exact. Prop hulls
get face, axial and edge bevels in the same way.

## Rendering

- World faces are fan-triangulated and wound counter-clockwise around their
  normal. Displacements are tessellated in `bsp::disp` (checkerboard
  diagonals) with smooth normals.
- Lightmaps come from the HDR lump when present. Style-0 luxels go into an
  four-layer `Rgba16Float` atlas (flat plus three radiosity-normal layers), with 1-luxel clamped borders. Unbumped faces repeat the flat layer.
- DXT textures are uploaded to the GPU as BC1/2/3 with every mip level.
- Props use the leaf ambient cube nearest their centre, plus
  `light_environment` sunlight gated by one shadow ray per prop.
- Tonemapping currently uses extended Reinhard in the shader; camera tonemapping is off. Exact Source exposure/colour correction and baked VHV prop lighting remain incomplete.
- Displacement lightmaps use grid UVs over each face rectangle, independent of brush projection vectors.
- Six original sky textures surround the camera behind the 3D sky.
- Input runs before fixed simulation. Position, spectator angles and recoil interpolate between snapshots; local mouse look stays immediate. Teleports, respawns and rounds reset the snapshot pair.
- Original Stratum fonts and HUD PNGs load through VFS in memory. No game assets are copied into the project. Interface layouts are implemented in Bevy; the Panorama runtime is not embedded.

- Normal maps use linear texture sampling. Model tangents come from VVD with
  the same LOD fixup remapping as vertices; missing tangents use MikkTSpace.
  Static props store an ambient cube and sun per instance in a lighting texture,
  indexed by UV1, so batching does not merge their lighting environments.
- Static batches use 768-unit cells while preserving complete geometry bounds.
  PVS membership includes all leaves intersecting a batch's expanded bounds.
  Missing/malformed PVS data fails open, and the 3D sky is excluded from PVS.
- Animation blend-grid names are cached and used clips are predecoded before
  gameplay. Character velocity follows render interpolation. Stair camera
  offsets decay at a bounded rate and reset on respawn/teleport.
