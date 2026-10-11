# Roadmap

How we get to [VISION.md](VISION.md). Work is organised as **phases with
parallel tracks**, not calendar estimates: tracks inside a phase run
concurrently (one agent or worktree per track), and the dependencies between
them are listed. Every item ends in a verified state (`cargo test`,
`csr probe`, `csr sim`, and screenshots for anything visual; see AGENTS.md).

## Where we are (assessed 2026-10-11)

The inherited code is a **real, playable single-player base**. The engine
layers are worth keeping. The UI, the gameplay/sim boundary and the bundled
Minecraft code need work before multiplayer and public releases.

| Area | State | Call |
|---|---|---|
| Format readers (kv, vpk, vfs, bsp, vtf, vmt, mdl), collision, movement | Solid; cite public docs; tested | **Keep** |
| `sim`: Bevy-free, fixed 64 Hz, per-player `TickInput`, seeded RNG | Right shape for netcode | **Keep, extend** |
| Rendering, animation, viewmodels, sounds | Dust II validated; other maps unverified | **Keep** |
| Gameplay glue in `client` (bots, loadouts, skins, overworld rules run in Bevy systems, not `sim`) | Blocks server authority | **Refactor into sim** |
| Main menu (late-2023-style, rebuilt every change, ~950-line `draw`, relaunches the process to start a match) | Not the 2018 look | **Rewrite** |
| Cases / inventory / cosmetics data | Works locally | **Keep data, rewrite UI** |
| Netcode, server, browser | None. `Snapshot`/`TickInput` not serialisable; `Snapshot` lacks weapons, money, round and bomb state; `World` not `Clone` | **Build** |
| Install detection | Misses the current CS:GO Legacy app (4465480, `common\csgo legacy`) | **Fix now** |
| Distribution | Manual zip; no CI, installer or updater | **Build** |
| `third_party/minecraftoss` + `minecraft_terrain` (~154k lines) | Ported from decompiled Minecraft, no licence files, hard dependency of `level` and `client`; release zip also bundled Minecraft assets | **Removed** (decision 1) |
| VR | Single view-angle model from input to hitscan to cameras | **Design seam now, build later** |

Tests: 180 `#[test]`s, mostly sim (55) and client (51), on synthetic data.
Real-data checks are the `csr probe` / `csr sim` commands.

## Phase 0: take ownership

Small and sequential. Everything else builds on it.

- [x] Install the Rust toolchain on the dev machine; build; run the full
      verification list (baseline: 178 tests, probe and 10-round sim all
      pass on the inherited code).
- [x] Install detection: search `common\csgo legacy\csgo` (app 4465480)
      as well as the old 730 beta layout. If neither exists, offer to open
      Steam's installer (`steam://install/4465480`). Setup messages, README
      and PLAYERS.md updated.
- [x] Remove the vendored Minecraft code, the Overworld mode and the bundled
      Minecraft assets (decision 1). Left behind on purpose: the
      game-agnostic `collision::DynamicWorld` trait, voxel/climbable hooks in
      `movement` and `GameMode::Special` in `sim`. They are inert on CS maps
      and are input to the Track E connection layer (keep, generalise or
      delete there).
- [x] GitHub Actions: build + `cargo test --workspace` on Windows for every
      push and PR; tagged pushes build and publish a release zip.
- [x] CI guard (`tools/check-no-assets.sh`): fails if the repo tracks game
      asset file types.
- [x] README/attribution and an Apache-2.0 `LICENSE` file.
- [x] 128 tick (decision 5): `sim::TICK_RATE` is 128 and tick-counted
      cadences derive from it. Making it a per-server setting stays in
      Track A.

## Phase 1: parallel tracks

### Track A: Multiplayer (critical path)

Phase 2 and VR depend on this track.

- [ ] Move all gameplay into `sim`: bots, loadout on spawn, skin and knife
      definition swaps, round/economy rules. Clients only send input and draw.
- [ ] **Tick rate as a server setting** (128 default since Phase 0); test
      that movement, recoil, inaccuracy and timers match at 64 and 128.
- [ ] Make `World` cloneable and add a determinism test (same inputs give
      the same state hash).
- [ ] Protocol: `TickInput` up (with **optional tracked head/hand poses and a
      separate aim ray**, reserved now for VR), full-state snapshots then
      deltas down. Serialisation via serde + a compact binary codec.
- [ ] Transport: UDP with reliable channels (evaluate `renet`/`quinn`
      against the sim's needs; the sim stays transport-agnostic).
- [ ] Client prediction and reconciliation (re-run `World::step` from the
      last acknowledged tick), entity interpolation, lag-compensated hits
      (rewind hitboxes to the shooter's view tick).
- [ ] `csr server <map>`: headless dedicated server binary without Bevy
      rendering. Listen server from the menu ("host a game"). Bots fill
      empty slots.
- [ ] Join by IP and LAN discovery.
- [ ] Hosting needs map data (BSP, nav, `items_game.txt`). A home PC host
      already has CS:GO Legacy. For a Linux VPS, verify whether SteamCMD can
      fetch the files (app 740) without a full client install.

### Track B: Distribution and updates

- [ ] **Velopack** (Rust crate + `vpk` packer): `Setup.exe` installs per
      user, makes Start-menu shortcuts and applies **delta updates from
      GitHub Releases**. The game checks for updates on launch and applies
      them on restart. Built and published by CI on a version tag.
- [ ] First run: auto-detect CS:GO Legacy and confirm; folder picker only
      as a fallback (reuse `setup.rs`).
- [ ] "Add to Steam" option: write a non-Steam-game entry plus library
      artwork to Steam's `shortcuts.vdf`. A real Steam store app is not an
      option: it needs Steamworks approval for a game built on Valve's IP.
- [ ] SmartScreen: installers are unsigned (decision 4). The download page
      and installer readme show the "More info → Run anyway" step.
- [ ] Release channels: `public` (this repo, clean) and an optional
      **`private`** channel for the owner's friends (see Track E and the
      Minecraft decision). Velopack supports channels; the private feed
      must not be on the public repo.

### Track C: The 2018 front end

- [ ] New UI layer with an app state machine (menu ↔ loading ↔ match ↔
      server browser) instead of relaunching the process, and retained
      widgets instead of rebuilding the whole tree every change.
- [ ] Rebuild the home screen to match `docs/reference/menu-2018-home.png`
      at 1920x1080, then scale. Use art and fonts from the install's
      `panorama/code.pbin` and VPKs at runtime; recreate only what is
      missing, as original work.
- [ ] Screenshot-diff harness: render the menu headless at the reference
      resolution and overlay/diff it against the reference images.
- [ ] 2018 case opening (`menu-2018-case-opening.png`): reel, centre line,
      "Unlocking Container...", contents grid with rarity strips, original
      sounds and timing. Reuse the existing case and cosmetic data.
- [ ] 2018-style play menu, inventory, options, scoreboard and buy menu.
- [ ] Research: can Steam's `download_depot` still fetch 2018-era CS:GO
      depot manifests for an owned copy? If so, the real 2018 Panorama
      layouts, art, sounds and map versions can be **read at runtime** from
      a second, older install. Verify before relying on it. Fallback:
      reconstruct from screenshots and 2018 video footage.

### Track D: Gameplay completeness

- [ ] Grenades: HE, flash, smoke, molotov/incendiary, decoy.
- [ ] Wall penetration (surface properties) and hit groups from the models'
      real hitboxes.
- [ ] Footsteps, reload/deploy event sounds, impact sounds.
- [ ] Ragdolls/death animations; burst, suppressor and revolver modes.
- [ ] Validate the map pool beyond Dust II (Mirage, Inferno, Nuke, Overpass,
      Train, Cache, Vertigo, Office, Italy, ...).
- [ ] The inherited fidelity checklist below (M2–M4).

### Track E: Content packs and modding

- [ ] Pack format `.csrpack`: a zip with `pack.toml` (id, version, origin,
      provided `ContentId`s, content hash) plus data definitions and
      optional assets. Mounted into `vfs` as another search path.
- [ ] A content registry keyed by `ContentId`, replacing hardcoded names
      (`de_`/`cs_` prefixes, the `"overworld"` special case).
- [ ] Install with zero steps: **drag onto the game window**, double-click
      (the installer registers `.csrpack`), or **downloaded automatically
      from the server on join** (the server advertises required pack hashes
      and serves them, like Source's `sv_downloadurl`). Stored under
      `userdata\packs`.
- [ ] Two pack kinds:
      - *recipe packs* hold no assets; they tell the game how to read content
        from the player's own install (e.g. MW2 from Steam). Public-safe.
      - *asset packs* contain extracted assets. Private only: never
        committed, never on public releases.
- [ ] **Connection layer for mechanics.** Mods bring mechanics (weapon
      behaviours, movement profiles, game modes, world types), not just
      assets. Rust has no stable plugin ABI, so choose between: (a) WASM
      modules (sandboxed, portable, one file, safe to auto-download from
      servers) against a versioned mod API; (b) native DLLs built with the
      exact same compiler and API crate (fast, fragile, unsafe to
      auto-download). Leaning (a), with the API shaped from `sim`'s seams
      (`DynamicWorld`, `MoveConfig`, `WeaponDefinition`, `GameMode`).
      Prototype it by re-adding Overworld as an external mod.

## Phase 2: after multiplayer works

- [ ] **Server browser**: a tiny master server (heartbeat in, list out)
      hosted on a free tier or next to the game server. The in-game browser
      shows ping, map, players and required packs, plus favourites and
      history.
- [ ] Identity without Steam: a player name and key pair generated on first
      run. The 2018 friends panel shows recent players and who is online on
      known servers.
- [ ] **The Intervention** as the first crossed-in item: an IW4L importer
      producing `WeaponDefinition` with IW4 behaviour (STITCHING steps 1–2).
      A recipe pack reads it from an MW2 install; an asset pack covers friends
      without MW2.
- [ ] Overworld returns as an external mod through the connection layer,
      maintained outside this repo.

## Phase 3: VR cross-play

Depends on Track A's protocol carrying tracked poses.

- [ ] OpenXR through `bevy_mod_openxr` (successor of `bevy_oxr`). First
      check its Bevy 0.19 support; pin, port or contribute as needed.
      Quest connects to the PC through Steam Link / SteamVR (or Virtual
      Desktop / Meta Link), so it is ordinary PC VR.
- [ ] VR player: tracked head and hands, gun held in hand, aim from the
      weapon's ray, smooth thumbstick locomotion through CS:GO movement,
      world-space menus.
- [ ] Flat players see VR players' head and hands: IK on the third-person
      model driven by the networked poses.
- [ ] Fairness: hitboxes follow the tracked head; the server clamps head
      and hand offsets relative to the player hull.

## Decisions (owner, 2026-10-11)

1. **Minecraft code:** removed from the repo (it was ported from decompiled
   Mojang code). Other games come in as third-party mods through the
   connection layer (Track E), never in this repo.
2. **Old content:** old everything: 2018-era menu, sounds, guns, maps
   (Workshop has many old map versions), case opening. Research in
   `docs/research/2018-era.md`.
3. **Hosting:** both your own PC and the cloud.
4. **Code signing:** no. Document the SmartScreen click-through.
5. **Tick rate:** 128 tick.

---

# Inherited milestones (from the original author)

### M1 — Walk a real map ✅

Format readers, collision, CS:GO movement, 64-tick sim, a renderer with
lightmaps, props and sun, HUD, and the weapon catalog.

### M2 — Look like CS:GO

- [x] 3D skybox (`sky_camera`, drawn by a second camera at 16x)
- [x] 2D skybox cube faces loaded from the original materials
- [ ] Overlays and decals (`info_overlay`)
- [ ] Detail props (grass and bushes from `detail.vbsp`)
- [ ] Static prop vertex lighting from the pakfile's `sp_hdr_*.vhv` (replaces the ambient-cube approximation)
- [x] Bump-mapped directional lightmaps, `$bumpmap`/`$bumpmap2`, SSBump and model normal maps
- [ ] Env-map reflections (`env_cubemap` from the pakfile)
- [ ] Water, `Lightmapped_4WayBlend`, and translucent sorting
- [x] Conservative PVS cluster visibility and spatial batches with frustum culling
- [ ] Prop fade distances and model LOD selection

### M3 — Shoot

- [x] Viewmodels: MDL skeletal animation (bones, sequences, anim blocks, frame anims)
- [x] Weapon state machine from `WeaponDefinition` (fire rate, ammo, reload, deploy)
- [x] CS:GO inaccuracy model and a seeded per-weapon recoil pattern (tuning pending)
- [x] Range falloff, armor ratio, hit groups (hull-height approximation)
- [x] Separate knife slash/stab events, sounds and animations; no firearm effects
- [x] Right-click zoom for all six scoped weapons, original item FOV/speed/accuracy data
- [x] Weapon deployment consumes old attack events instead of replaying them
- [ ] Exact knife sweep volume, impact timing, and model animation-event dispatch
- [ ] Original rifle-optic refraction, lens effects and transition parity
- [ ] Burst modes, suppressor toggling, revolver alternate fire/cocking, weapon inspection
- [ ] Wall penetration (`penetration`, surface props)
- [ ] Hit groups from the character models' hitboxes
- [x] Sounds: soundscripts and WAV/MP3 playback (weapons)
- [ ] Footsteps, impacts, reload sounds

### M4 — Play a round

- [x] Player models (T/CT) with a simplified third-person animation state
- [ ] Ragdoll/death animations, lean/strafe layers, aim yaw, foot IK
- [x] Rounds, freeze time, buy zones and buy menu, money
- [x] Bomb plant/defuse (`func_bomb_target`), win conditions
- [x] Halftime, match results, spectator camera and Tab scoreboard
- [x] Dropped weapons/C4/defuse kits; blocked pickups and defuses respect world collision
- [ ] Grenades: HE, flash, smoke, molotov/incendiary, decoy

### M5 — Play together

- [ ] Server-authoritative netcode over UDP: input commands up, snapshot deltas down
- [ ] Client prediction and reconciliation (reuses `sim::World::step`)
- [ ] Lag compensation for hit registration
- [x] First-pass bots (wander, react, aim, burst fire)
- [x] Bots using the map's `.nav` meshes
- [x] Bot buying, bomb carrying/recovery, planting, guarding and defusing

### M6 — Stitch

See [STITCHING.md](STITCHING.md).

#### Movement and visual fidelity pass (2026-10-01)

Implemented shared render-pose interpolation, smooth bot aim error, corrected
spectator viewmodels, displacement grid lightmap UVs, six-face sky rendering,
runtime original Stratum fonts/HUD art, radial buying and an overview radar.
Remaining fidelity work: Panorama layout parity/main menu, rotating radar,
VHV prop lighting, reflections/specular material paths, Source colour correction
and distant sky geometry. See PLAYTEST.md for current checks and limitations.

#### Rendering and bot tactics pass (2026-10-01)

World lighting now reads the flat and three directional lightmap layers. The
shader samples normal maps in linear colour space; transition materials blend
both normal maps and SSBump uses its stored directional weights. Static props
retain their own ambient cubes/sun visibility in a compact texture; authored
VVD tangents feed normal shading on models. VHV lighting remains unfinished.

Static meshes are partitioned into spatial batches, rejected conservatively by
the BSP PVS and then by the camera frustum. Player animation clips are decoded
during loading. Bot sightlines gate target updates and firing; remembered
positions expire. Movement follows the path independently of head turning,
uses bounded angular acceleration, persistent stuck recovery and teammate
separation. Defusers get covering teammates; guards choose reachable hiding
spots and visible approaches. Reloading bots can move toward nearby cover.
Camera stair transitions are eased while local mouse look stays immediate.


#### Menus and combat feedback pass (2026-10-01)

Implemented original-asset main/pause/settings pages, installed map selection,
Competitive/Deathmatch selection, saved video/audio/control/crosshair settings,
bounded impact/blood effects and original damage sounds. Fixed orphan held
weapons at team changes and resting pivots for ground models. Exact Panorama
layout, complete settings parity, hostage rules and original particle-system
fidelity are still open. See PLAYTEST.md for verification and limitations.

#### Tracers, inspection and inventory pass (2026-10-01)

Tracers use animated MDL muzzle attachments, including parent-bone offsets,
and account for the first-person camera's separate FOV. Trails are short,
muted and fading; the simulation's hit rays are unchanged. F plays installed
lookat inspection clips, interrupted by fire/reload/scope/switch actions.

The front end uses a late-CS:GO Panorama arrangement with a narrow navigation
rail, content panels, local profile panel and animated 3D character. Installed
map thumbnails are direct cards with hover lift/tilt/shadow and a persistent
selection border. Inventory provides 36 base weapon cards, filters, rotating
3D previews and saved team-specific primary/pistol choices for Deathmatch.
Steam inventories, skins, online friends/store services, exact Panorama option
parity and the gameplay gaps listed above remain unfinished.
