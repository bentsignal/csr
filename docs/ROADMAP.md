# Roadmap

How we get to [VISION.md](VISION.md). Work is organised as **phases with
parallel tracks**, not calendar estimates: tracks inside a phase run
concurrently (one agent or worktree per track), and the dependencies between
them are listed. Every item ends in a verified state (`cargo test`,
`csr probe`, `csr sim`, and screenshots for anything visual; see AGENTS.md).

## Where we are (assessed 2026-10-12)

The inherited code is a **real, playable single-player base**. Phase 0 and
the installer/updater release are complete. Gameplay now lives in the fixed
tick sim; dedicated multiplayer, prediction and local content packs have
landed. Rendered joining has passed two-client lifecycle checks at both tick
rates, and the local browser's add/join/reopen/remove flow has passed with
fresh inspected captures. Fidelity, full classic UI coverage and VR remain
unfinished.

| Area | State | Call |
|---|---|---|
| Format readers (kv, vpk, vfs, bsp, vtf, vmt, mdl), collision, movement | Working base; exact movement and surfing parity unverified | **Keep, measure** |
| `sim`: Bevy-free, fixed tick (128 default, runtime setting), per-player `TickInput`, seeded RNG | Right shape for netcode; owns bots and loadouts since Track A | **Keep, extend** |
| Rendering, animation, viewmodels, sounds | Dust II validated; other maps unverified | **Keep** |
| Gameplay glue in `client` (bots, loadouts, skins, overworld rules run in Bevy systems, not `sim`) | Moved into `sim` (Track A) | **Done** |
| Main menu | Installed Scaleform movies, persistent menu/loading/match flow, offline/pause/options bridge | **Extend and measure** |
| Cases / inventory / cosmetics data | Works locally | **Keep data, rewrite UI** |
| Netcode, server, browser | Renet UDP, dedicated server, full snapshots, prediction/interpolation, coarse rewind; verified rendered local favorites/browser joining at 64 and 128 tick | **Menu hosting and wider discovery next** |
| Install detection | Current Legacy app 4465480 and older beta layout supported | **Done** |
| Distribution | CI, portable zip, Velopack installer/updater; v0.3.0 published | **Done; private channel remains** |
| `third_party/minecraftoss` + `minecraft_terrain` (~154k lines) | Ported from decompiled Minecraft, no licence files, hard dependency of `level` and `client`; release zip also bundled Minecraft assets | **Removed** (decision 1) |
| VR | Networked poses/separate aim, calibrated pure input adapter, server wall admission; optional OpenXR smoke under verification | **Prepare and verify hardware before game integration** |

Workspace tests and installed-data `csr probe` / `csr sim` checks run for each
change. Rendered integration also requires freshly saved and inspected images;
successful lifecycle logs alone do not establish visual correctness.

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

Track A1 verified on 2026-10-11 ([PR #9](https://github.com/bentsignal/global-offensive/pull/9)):
workspace tests pass; release-profile Dust II `probe` and `sim` report zero
failures, including `sim --tickrate 64`; the rendered client screenshot was
inspected. Review also fixed firing-snapshot equality after recoil-cache
population and finish ownership on weapons dropped in their spawn, respawn
or purchase tick. A2's transport and simulation foundations have since landed.
Bot brains remain server-only; snapshots alone do not replay
their decisions.

- [x] Move all gameplay into `sim`: bots, loadout on spawn, skin and knife
      definition swaps, round/economy rules. Clients only send input and draw.
- [x] **Tick rate as a server setting** (128 default since Phase 0); test
      that movement, recoil, inaccuracy and timers match at 64 and 128.
- [x] Make `World` cloneable and add a determinism test (same inputs give
      the same state hash).

The A2 foundation now has a Bevy-free Renet UDP session and `csr-server`
dedicated executable, with reliable control/events, redundant inputs,
fragmented full snapshots, content compatibility checks and 128-tick
authority. A two-client Dust II loopback check exercises the installed
catalog and tracked poses. Rendered-client joining now uses the same authority
and prediction; the remaining items are separated below. See [NETCODE.md](NETCODE.md).
A transport-independent
predictor now restores snapshots and replays bounded unacknowledged inputs
through the shared sim without inventing server AI or exposing replay events.
The real two-client check includes delayed transport polling and exact final
reconciliation; the rendered client consumes the same prediction/interpolation.
A bounded, read-only remote pose history now samples fractional snapshot
times, guards teleport/respawn discontinuities and interpolates tracked
head/hands independently of the aim ray. Dust II UDP checks exercise this
at 64 and 128 tick; full hitbox and animation fidelity remain open.
A server-only bounded hull history now supports authoritative hitscan rewind
without moving live actors. The server derives valid view ticks from its own
RTT/caps and requires matching spawn epochs, teams and rounds. Coarse existing
hit groups remain; MDL/VR hitboxes are open.
A renderer-free owned remote session now combines bounded metadata discovery,
verified installed-content admission, prediction, remote poses, authoritative
events and idempotent cancellation. Its two-client Dust II lifecycle proof
runs at 64 and 128 tick. The rendered client has also completed Ready, pause,
Home teardown and offline reload with two actual clients at both rates and
all sixteen fresh captures inspected. See [REMOTESESSION.md](REMOTESESSION.md)
and [REMOTE_JOIN.md](REMOTE_JOIN.md).

- [x] Protocol: `TickInput` up, including optional tracked head/hand poses
      and a separate aim ray; fragmented full snapshots and authoritative
      events down, using serde/postcard and bounded compatibility admission.
- [x] Lossless snapshot deltas with bounded confirmed baselines, full recovery,
      actual UDP loss/reorder tests and measured payload sizes. Further packing
      and network admission budgets remain open; see [NETCODE.md](NETCODE.md).
- [x] Renet UDP with reliable control/events and redundant inputs; the sim
      remains transport-agnostic.
- [x] Prediction/reconciliation through shared `World::step`, remote pose
      interpolation and server-authorized coarse hull rewind.
- [ ] Full character-model and tracked VR hitboxes.
- [x] `csr-server <map>`: headless dedicated executable without Bevy
      rendering, configurable tick rate and server-owned bots.
- [x] Listen server through the original Create Lobby, checked settings and Go
      controls. One retained map worker and render-gated passive owner admission;
      independent 128-tick authority, owned Home/Quit cleanup and cached movie
      cancellation. Authored Competitive/Deathmatch host+guest proofs, including
      two active Deathmatch humans; controller lifecycle also verified at 64 tick.
      See [HOSTING.md](HOSTING.md).
- [ ] Dynamic bot slot filling and original competitive warmup. Existing static
      bots remain; [BOT_POPULATION.md](BOT_POPULATION.md) records measured
      2023 Legacy fill behavior separately from the 2018 target.
- [x] Local address/favorites discovery and IP/hostname join backend.
- [ ] LAN discovery and public server directory.
- [ ] Hosting needs map data (BSP, nav, `items_game.txt`). A home PC host
      already has CS:GO Legacy. For a Linux VPS, verify whether SteamCMD can
      fetch the files (app 740) without a full client install.

### Track B: Distribution and updates

- [x] **Velopack** (Rust crate + `vpk` packer): `Setup.exe` installs per
      user, makes Start-menu shortcuts and applies **delta updates from
      GitHub Releases**. The game checks for updates on launch and applies
      them on restart. Built and published by CI on a version tag.
      (`update.rs`, `tools/package-release.ps1`, `release.yml`; player data
      moved to `%LocalAppData%\CSGO-Rewrite\userdata` so it survives updates.)
- [x] First run: auto-detect CS:GO Legacy and use it without a question
      (VISION §1: "nothing else is asked"); if it is missing, install it
      through Steam and wait for the download, or pick the folder.
- [x] "Add to Steam" option: write a non-Steam-game entry plus library
      artwork to Steam's `shortcuts.vdf`. A real Steam store app is not an
      option: it needs Steamworks approval for a game built on Valve's IP.
      (`steam.rs`; artwork drawn in code by `art.rs`.)
- [x] SmartScreen: installers are unsigned (decision 4). The download page
      and installer readme show the "More info → Run anyway" step.
- [ ] Release channels: `public` (this repo, clean) and an optional
      **`private`** channel for the owner's friends (see Track E and the
      Minecraft decision). Velopack supports channels; the private feed
      must not be on the public repo.

### Track C: The classic (Scaleform) front end

The target is the Scaleform UI in `docs/reference/` (VISION §7). Research:
[research/2018-era.md](research/2018-era.md). The CS:GO Legacy install still
ships all 117 original Scaleform movies (`csgo/resource/flash/*.swf`: main
menu, friends list, store tiles, `inventorypanel.swf` with the case opening,
HUD, scoreboard, buy menu), and the UI sounds, case sounds and menu music are
byte-identical to 2018.

- [x] Persistent menu ↔ loading ↔ match state machine instead of relaunching
      the process. Bounded loading/cancellation, native offline GO, pause
      Resume/Exit and a rendered Dust II → home → Inferno lifecycle proof.
      [PR #10](https://github.com/bentsignal/global-offensive/pull/10).
- [x] Connect the local server browser and rendered network joins to that
      state machine, with render preparation, neutral pause, Home cancellation
      and clean offline reload. Native browser sizing/fidelity remains unmeasured.
- [x] Original lobby and host chooser: trusted installed map catalog, checked
      query staging, one-use Go, epoch-guarded cached lifecycle and restoration
      before offline Practice. Pointer cancellation/reopen and host+guest
      screenshots verified; the added address notice preserves native controls.
- [x] **Spike: play Valve's own SWFs with [Ruffle](https://github.com/ruffle-rs/ruffle)**
      (Rust Flash player, MIT/Apache, wgpu), read from the player's install
      at runtime, with the Scaleform game API stubbed. It works: the home
      screen renders within 6.0/255 mean error of the reference outside web
      content and avatars. Decision and evidence:
      [research/scaleform-spike.md](research/scaleform-spike.md). Needs three
      Ruffle fixes (fork `bentsignal/ruffle`, branch `csgo-scaleform`).
- [x] Integrate Ruffle as the menu layer (`crates/scaleform`), with native
      offline launch, pause/resume/exit, and saved numeric options.
- [x] Remove the moving-cursor input backlog and preload cached Pause during
      normal map loading. Production-input stress and a five-bot movement,
      fire/reload, pause/resume run pass with inspected captures; this does not
      establish physical display latency or full fidelity. [MENU_PERFORMANCE.md](MENU_PERFORMANCE.md).
- [ ] Complete the game API bridge for inventory/cases and the remaining
      classic panels. Original reset defaults still need verified evidence.
- The fallback (a native Bevy layout built from the SWFs with the `swf`
  crate) is not needed.
- [ ] Screenshot-diff harness: render the menu at 1920x1080 and diff it
      against the reference images. The spike tool has it
      (`tools/scaleform-spike diff`); move it into the client's screenshot
      mode with the integration.
- [ ] Case opening as in `menu-2018-case-opening.png`, with its original
      sounds (`UI.CrateOpen`, `UI.CrateItemScroll`, `UI.CrateDisplay`). Reel
      timing comes from `inventorypanel.swf` running, or frame counts from
      footage. Reuse the existing case and cosmetic data.
- [ ] Scaleform scoreboard, buy menu and HUD (the movies are in the install).
- [x] Archive loader: fetch Valve's own "Compatibility Version" Workshop snapshots
      at runtime (map → Workshop ID table in research §1), and also pick up
      any already in `steamapps\workshop\content\730`. Download them
      straight from Valve's CDN, with no Steam step (decision 6). Fourteen
      pinned archives verified; embedded navigation/overview lookup fixed;
      all fourteen simulations pass. [PR #15](https://github.com/bentsignal/global-offensive/pull/15).
- [ ] Offer classic maps in the menu with a visible, cancellable download
      screen. Resolve the four material dependencies and measure fidelity
      against the archived maps before calling them ready. Archive years
      do not establish August 2018 parity. Results: [OLD-MAPS.md](OLD-MAPS.md).
- [ ] Optional: a 2018 build via `download_depot` with the owner's own
      account, if Steam still serves old manifests (unverified). Only adds
      a few changed models and the old HE explosion sound.

### Track D: Fidelity and gameplay completeness

The 2026-10-12 desktop playtest confirms usable menu navigation and bot play,
but identifies major remaining feel/presentation gaps. The owner clarified a
2015 gameplay/audio baseline. The active measured backlog is
[PLAYTEST_FEEDBACK.md](PLAYTEST_FEEDBACK.md); defer new feature polish until
this base is faithful.

The owner's first playtest (2026-10-11): "very far from one-to-one". Many
sounds are wrong, textures look off, the gameplay feels wrong, and the whole
thing is uncanny. Fidelity is measured, never eyeballed:

- [ ] **Reference capture harness.** Drive the real CS:GO Legacy (`csgo.exe`
      on the same machine, `-insecure`, console commands for map, camera
      position and angles, screenshot) and our `csr play --screenshot
      --camera` from the same viewpoints, then diff the image pairs. Do the
      same for sounds (which file and soundscript entry plays for each
      event) and for movement and weapon numbers (`cl_showpos`, recoil
      patterns, timings). Every fidelity fix cites a before/after pair.
- [ ] Fix the gaps the harness finds, worst first: sounds (wrong files,
      missing events, volume and falloff), materials and lighting, then the
      feel of movement, recoil, animation and hit registration.
      Dated recoil data and the unverified procedure:
      [RECOIL_REFERENCE.md](RECOIL_REFERENCE.md).
- [x] Authored static-prop lighting metadata foundation: retained model and
      hardware/source-vertex identities, bounded opaque VHV parsing and
      instance-scoped checksum/count validation against installed Dust II.
      Record order, color interpretation and full VHV lighting remain open.
      [Foundation evidence](research/static-prop-vhv.md).
- [x] Establish a localized native response to an opaque prop-lighting
      payload intervention, with a repeated control and unchanged installed
      data. A second marker supports hardware order for 16 selected records;
      general record-order and color-decoder admission remain open.
- [ ] Match jumping and **surfing** with native traces: supported input,
      ramp/crease/seam contact, speed retention and landing impact at recorded
      server settings and tick rates. Synthetic correctness tests and native
      2023 behavior must remain distinct from demonstrated 2015 parity.
- [x] Bounded developer movement replay through `sim::World::step`, with
      explicit complete state/input, retained map/catalog identities and
      held-weapon checks. Installed Dust II synthetic runs passed at 64/128;
      native comparison remains open. [Replay contract](research/movement-replay.md).
- [ ] Measure and correct the movement candidate's reported excess landing
      slowdown against native Legacy: matched command phase, initial state,
      tick rate, server settings and horizontal-speed traces through impact
      and recovery. Retain version-specific evidence before tuning constants.
- [x] Admit stable native Legacy neutral command/context windows at actual
      64/128 tick, preserving typed scope, missing properties and rejected
      lifecycle transitions. Internal moving-state phase and map replay remain
      open. [Measurement checkpoints](research/native-movement-measurement.md).
- [x] Measure and correct unfatigued standing takeoff velocity, displacement
      and jump stamina at both tick rates. Complete continuous native command
      traces qualify this scoped transition; crouched/fatigued takeoff and
      broader running/surface behavior remain open.
- [x] Correct the scoped standing P2000 ground run/landing at actual 64/128:
      observed partial input, pre-recovery stamina cap, first supported height,
      full held recovery and coast. Numeric targets and synthetic floor remain
      separate from internal command phase, map replay and surfing parity.
- [x] Compare 36 actual standing-hull traces on the selected surf map and
      replay its recorded 64 Hz entry fall through current map collision.
      Sampled geometry and this zero-input fall match; moving ramp contact,
      continuous routes, trigger effects and surfing parity remain open.
- [x] Correct the first measured ramp-contact false zero-fraction collision:
      a separating-plane check removes the rounded ordering error while
      preserving actual blocking contacts. Native hull queries confirm the
      disputed sweep is clear; command 667 now matches, with later response
      differences and full surfing parity still open. Physics protocol 9.
- [ ] Match repeated-crouch fatigue/cooldown after the current jump, landing
      and audio work; measure the original rather than invent a debounce.

- [ ] Grenades: HE, flash, smoke, molotov/incendiary, decoy.
- [ ] Wall penetration (surface properties) and hit groups from the models'
      real hitboxes.
- [x] Footsteps, reload/deploy event sounds, surface impact sounds: soundscript
      mixing and MDL events, fixed-tick foley, matched Legacy/client probes,
      1,425-file decode audit, and spatial-panning/downmix regression checks.
      [PR #11](https://github.com/bentsignal/global-offensive/pull/11).
- [ ] Close remaining audio fidelity gaps: deployment/first-step timing,
      dated weapon mixer inputs and overlapping-shot tone, exact gain curves,
      DSP/occlusion, ricochets/near misses, map soundscapes
      and music. Evidence and limits: [FIDELITY.md](FIDELITY.md).
- [ ] Ragdolls/death animations; burst, suppressor and revolver modes.
      Installed death-pose/PHY and classic kill-feed metadata, authority
      receipt gaps and remaining observation plan:
      [DEATH_PRESENTATION.md](DEATH_PRESENTATION.md). No dynamics or
      authored death-notice integration is complete.
- [ ] Validate the map pool beyond Dust II (Mirage, Inferno, Nuke, Overpass,
      Train, Cache, Vertigo, Office, Italy, ...).
- [ ] The inherited fidelity checklist below (M2–M4).

### Track E: Content packs and modding

The archive-loader groundwork in [PR #15](https://github.com/bentsignal/global-offensive/pull/15)
gives listed maps and loaded levels stable `ContentId`s and exposes
`Level::load_from_vfs` for maps in mounted content packs. A synthetic pack
loads without a game install. [PR #17](https://github.com/bentsignal/global-offensive/pull/17)
landed the bounded, data-only private pack format, atomic installation,
transactional ContentId registry and selected-pack Level/VFS loading. A
manifest-only Legacy recipe and a private Dust II asset pack both passed
release probe/simulation and a rendered private-pack proof; PR #12 was closed
as superseded. Local file opening, owned native selection and the Home content
manager now connect installation to hash-bound map selection; server downloads
and other-game importers remain open. See [PACKUI.md](PACKUI.md).

- [x] Pack format `.csrpack`: a zip with `pack.toml` (id, version, origin,
      provided `ContentId`s, content hash) plus data definitions and
      optional assets. Mounted into `vfs` as another search path.
- [ ] A content registry keyed by `ContentId`, replacing hardcoded names
      (`de_`/`cs_` prefixes, the `"overworld"` special case).
- [x] Local installation: **drag onto the game window**, double-click
      (the installer registers `.csrpack`), or choose a file in the Home
      content manager. Bounded workers, cancellation, progress and immutable
      selected-map ownership; stored under `userdata\packs`. Native picker
      protocol/lifetime checks pass; actual dialog interaction is pending.
- [ ] **Download automatically from the server on join**: advertise required
      pack hashes and serve them, like Source's `sv_downloadurl`.
- [x] Two pack formats (Legacy recipes and private asset maps verified;
      other-game importers remain open):
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

Track A now carries tracked poses and a separate aim ray. Hardware rendering
and the game's VR player are still separate work.

- [x] Verify published Bevy 0.19 support and select exact OpenXR dependency
      versions for an optional, asset-free smoke example.
- [x] Compile the optional smoke and observe fresh focused two-eye tracking
      plus orderly main/render cleanup on the owner's SteamVR/Quest runtime.
- [ ] Verify compositor submission, headset appearance, focus recovery and
      session-loss cleanup before game use; revised camera configuration still
      needs a connected-headset run.
      Implementation and verification are tracked in
      [PR #40](https://github.com/bentsignal/global-offensive/pull/40).
- [x] Pure calibrated input adapter: meters to Source units, recentering,
      head/hands, independent aim and neutral controls on tracking loss.
- [x] Server validates/clamps tracked aim origin against current eye-to-muzzle
      collision; blocked/embedded origins cannot shoot through walls.
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
2. **Old content:** old everything: the classic Scaleform menu (the
   screenshots), sounds, guns, maps (Valve's own Workshop snapshots of old
   map versions), case opening. Research in `docs/research/2018-era.md`.
3. **Hosting:** both your own PC and the cloud.
4. **Code signing:** no. Document the SmartScreen click-through.
5. **Tick rate:** 128 tick.
6. **Old maps:** download Valve's Workshop snapshots straight from the CDN;
   zero friction beats Steam-sanctioned. If that gets the public version
   taken down, so be it.
7. **CI:** runs on the owner's PC (self-hosted runner), not GitHub's
   machines. See [CI.md](CI.md).

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

### Offline grenade practice (owner playtest feedback)

- [x] `csr play de_dust2 --grenade-practice --bots 0` starts directly with
  respawns and HE, flash, smoke, Molotov, incendiary and decoy in inventory.
  Press 4 repeatedly to cycle utility; click to throw again after the sandbox
  cooldown. Grenades remain available after throws and respawns. Bots and
  competitive inventories are unchanged. This exposes existing grenade
  simulation/effects for testing; throw timing, trajectories and particle
  fidelity are still unfinished.
- [x] Retain bounded death bodies using the installed full death animation;
  verified with a lethal simulation shot and rendered pose. Authentic ragdoll
  physics, impulse response and airborne settling remain unfinished.
