# Roadmap

> **October 20 overnight:** [OWNER-PRIORITIES.md](OWNER-PRIORITIES.md) gives
> the current ordered queue and proportional batch checks. Grenade throws and
> knives are accepted for now; preserve them while improving the rest of play.

## October 20 playtest increments

- [x] One desktop entry and an in-game F8 armory for guns, classic knives,
  solo/ten-player sessions and maps, without a restart for each weapon (#270).
- [x] Retained classic gun shots, lower local AK masking, compact HUD/kill feed,
  airborne easing, wall/floor footstep attenuation and trimmed loading work
  (#270). Walking coefficients were preserved; final feel remains owner-tested.
- [x] Publish the combined fire-intersection, bot navigation, impact/decal,
  dropped-weapon placement and prop-loading increment through the same launch
  entry (#271). Fire card borders and broader behavior remain unfinished.
- [x] Correct classic shot pitch and declared mix, make attacker head cues
  audible across the map, and preserve airborne transition velocity (#272).
- [x] Deliver recovered original 2015 sound pairs, prop-loading scratch/output
  reuse, hidden-surface decal exclusions and combat-displaced bot replanning
  (#273). Installed arsenal firing/reloading passed at 64/128 Hz.
- [x] Deliver the field-fire soft-intersection correction (#274). Layer isolation,
  flat/ramp/smoke controls, workspace/Release/probe/sim and CI passed; staged.
- [x] Deliver continuous airborne actor poses across held shot/stance events
  (#275): CI, Release/probe/sim and ordinary ten-player rendering passed.
- [x] Recover continuously followed bot routes after grounded off-route
  displacement (#276), preserving valid gaps/airborne traversal. CI/regressions,
  Release/probe/sim and ordinary match/F8-panel rendering passed.
- [x] Restore original arsenal handling recordings and declarations, including
  shotgun pumps and rifle/pistol/SMG/machine-gun reload cues (#279, #282,
  #286, #287). Historical audio remains a private content pack; unsupported
  bindings retain installed audio.
- [x] Apply current wall/floor geometry to remote footsteps; load authored
  concrete, metal, glass, dirt and sand impact puffs; retire hidden-surface
  decals and keep blood marks attached to moving doors (#277, #278, #279,
  #283, #285).
- [x] Let dropped items fall when their support disappears and update the same
  replicated item's render pose (#285). Full toss/bounce/angular behavior
  remains unfinished.
- [x] Recover bot weapon use after empty magazines or exhausted ammunition;
  clear route doors/breakables and traverse authored ladders, including
  collision-qualified raised-cap mounts (#278, #280, #281, #288).
- [x] Avoid repeated prop material preparation, unused dropped-model imports
  and repeated team-arm decoding (#277, #282, #288). Preserve F8 difficulty
  and bot counts across map changes; add offline Pause/Resume bots (#284,
  #287).
- [x] Use live door/breakable collision for muzzle clipping and runtime model
  lighting, refresh stationary item/body lighting when that geometry changes,
  and keep unsupported drops at their release position (#289, #290).
- [x] Keep local/remote locomotion interpolating between successive shots while
  holding the shot receipt until its frame; restore original Sawed-Off and
  XM1014 shell-insert recordings from the private classic pack (#291).
- [x] Reuse successful cosmetic material preparation, preserve reload foley
  across skipped observations, and let bots clear friendly firing lanes
  (#294, #295, #296).
- [x] Clip bullet-impact artwork at visible opaque receiver edges while
  retaining ordinary wall marks, original composition and door attachments
  (#297).
  Paired GPU controls and normal AK/match captures passed; transparent and
  disconnected coplanar receivers remain limitations.
- [x] Continue explicit F8 weapon/knife choices across F8 map changes through
  the next map's catalog. Clip static blood marks with the same opaque-depth
  receiver test while preserving airborne spray and attached-door behavior.
- [x] Admit reachable ladder-cap mounts beneath low ceilings using the actual
  movement-solver jump preview, retaining blocked-headroom recovery and
  unchanged human physics. Parsed-NAV bot traversal passed at 64/128 Hz.
- [x] Keep Loading active through chunked prop preparation and successive
  scene-import phases; cancel partial scenes without retiring the frontend.
  Geometry/cancellation tests and rendered match/home controls passed. Heavy-map
  preparation remains slower than the old blocking path; indivisible model
  phases and total loading speed still need work.
- [x] Carry authored CROUCH departures through qualified ladder-cap mounts,
  preserving the same human movement solver and blocked-access recovery.
  Increase the soft prop work window while keeping per-update cancellation;
  focused traversal, workspace, Release and rendered cancellation checks passed.
- [x] Preserve authored crouching through ordinary ladder approach, climb and
  dismount. Independent human controls and parsed-NAV bot routes passed in
  both directions at 64/128 Hz; unflagged low openings retain bounded failure.
- [ ] Finish unresolved player outcomes in the approved order; an increment or
  first pass does not finish its backlog category.

The increments after #276 are merged and locally checked but have not replaced
the owner's desktop playtest. Continue implementation; prepare a new owner
testing pass only when requested.

> **October 18 follow-up:** implementation approved: classic maps/playability,
> then movement/viewmodels, hit-audio consistency, then grenades. Grenade feel
> now targets original CS:GO and historical lineups; the prior CS2 throw-feel
> exception is superseded. Bot/mode warning cleared by owner retest.


> **October 18 owner direction:** [Morning feedback](PLAYTEST-2026-10-18.md)
> accepts the major performance improvement as good enough for now. General FPS
> optimization moves low; core 2015-era maps, working traversal/ten-player modes,
> movement speed, airborne viewmodel finish and hit-audio consistency are high.
> [OWNER-PRIORITIES.md](OWNER-PRIORITIES.md) supersedes older priority statements
> below. Daytime reviews should cover coherent batches, not tiny frequent tests.


October 17 rendering progress: restore Bevy's render thread so simulation and
rendering overlap. A private same-binary PLAY comparison at 2560x1440 with ten
participants sampled about 97 FPS serial versus 143 FPS pipelined in one fixed
Dust II view. This is a bounded diagnostic result, not sustained/all-map
acceptance; actor life states varied and the collector reported clock-alignment
warnings. Physical input latency is unmeasured. The 300–500 FPS goal remains
open. Surface latency hint, saved graphics settings and simulation rules stay
unchanged.

> **Current owner direction — 2026-10-17:** Work resumed. Consolidated
> [owner priorities](OWNER-PRIORITIES.md) supersede old work-order statements
> below. Focus on broad playability: performance across supported maps with
> 10 players at a 300–500 FPS target, original feel, lighting and HUD. Current
> bands are rough guidance. Manage priorities through chat; the GUI is retired.

How we get to [VISION.md](VISION.md). Work is organised as **phases with
parallel tracks**, not calendar estimates: tracks inside a phase run
concurrently (one agent or worktree per track), and the dependencies between
them are listed. Every item ends in a verified state (`cargo test`,
`csr probe`, `csr sim`, and screenshots for anything visual; see AGENTS.md).

## October 18 map increment

- [x] Offer eight qualified historical core-map snapshots, including authentic
  October 2015 Cache, using archive-specific identity, navigation and art.
- [x] Correct flush ladder contacts, linked breakable collision retirement at
  Mirage's small CT window, and Cobblestone stationary-axis floor snapping.
- [x] Correct additive/layered unlit map materials, invisible brush rendering,
  bounded archived prop bakes and authored prop lighting origins.
- [ ] Finish current-source rendered ten-player admission and owner playtest;
  October 2015 Overpass now has qualified routes, Release simulation, rendered
  controls and a bounded ten-player result. Final picker/build verification and
  owner acceptance remain open; the original missing-material probe stays visible.

These increments do not complete exact 2015 parity or the full map catalog.
[Version qualification and remaining dependencies](OLD-MAPS.md).

### October 18 evening increments

- [x] Restore admitted Nuke window emission and capture hit protection at
  contact time for consistent flesh/armor cue selection (#243). Full lighting
  and native audio mixing parity remain open.
- [x] Match measured grounded walk/crouch acceleration and retain look motion
  across weapon switches (#244). Vertical jump/landing response remains open.
- [x] Show each admitted classic map's own version-specific photo in the
  authored picker (#245). Warm and fresh-cache picker pages were inspected;
  images remain in private runtime storage.
- [x] Add bounded airborne gun/hand lowering and a continuous return (#246).
  Full native airborne tilt and camera landing response remain open.

## Where we are (assessed 2026-10-17)

The [October17 follow-up](PLAYTEST-2026-10-17.md) confirms bounded menu overlap/Pause fixes but reports transition flicker, restart binding loss, stale HUD binding hints, AWP cooldown/ammo/audio defects and unacceptable sustained FPS. Current ragdolls are accepted for now; custom ragdolls/death-animation work is deferred. Deliver actual verified fixes before requesting another improvement test.

The October16 owner test is complete. [All new feedback and accepted scopes](PLAYTEST-2026-10-16.md)
are retained separately from the October14 ledger. **Priority one is sustained,
smooth gameplay performance**, especially bots versus solo and degradation over
a session. Core input/weapons, menu lifecycle, exact old HUD layout and map
interactions follow; content-pack expansion is deferred. The owner explicitly
authorized CS2 grenade feel on October 16, then revoked that scope on October
18 in favor of original CS:GO throws and historical lineups. Read-only CS2
settings/keybind import remains the narrow exception. No new replacement
test build is requested until fixes have passed their actual verification.

PF01/PF02 offline [diagnostic log analysis](PERFORMANCE-LOG-ANALYZER.md) now
provides conservative helper-bounded early/late summaries and synthetic
regressions. Actual immutable-binary run review, matched camera/local-alive
evidence and performance acceptance remain open.

A local [kill-feed exact-pixel upload candidate](research/kill-feed-exact-pixel-reuse.md)
skips top-image asset mutation only for identical current accepted pixels and
layout. Notification/admission fixtures and actual HUD/performance checks are
pending root verification; this is not an established FPS improvement.

Owner return-home playtest on 2026-10-14: all reported defects, required
features, accepted improvements and priorities are retained in
[PLAYTEST-2026-10-14.md](PLAYTEST-2026-10-14.md). Work continues without another
test request until the owner asks. The owner has now requested a broad test
around 5 p.m. Eastern on October 16; its verified candidate is frozen, with
fifteen numbered desktop tests and carried settings/map caches. The latest
merged source includes grounded HE launch/button corrections,
measured M4 recovery, original menu music and supported Video controls, authored
prop tint, dropped weapon models and the pre-rework Inferno chooser. The next
merged batch adds ordinary world/static-prop detail, authored HE dust placement,
inspection continuity/cosmetic knife cancellation, bounded 2015 weapon-case and
knife availability, and additional bot recovery. Full workspace, combined
RELEASE and ordinary rendered frontend checks passed for the exact combined
source; [evidence and retained limits](research/overnight-fidelity-verification.md)
distinguish those checks from complete native or 2015 parity. Loading-screen
rendering and original Inventory thumbnails now have merged bounded fixes and
inspected actual captures. Persistent weapon bindings, console scrollback,
content-backed bot difficulty, grenade pickup capacity and measured grounded
Smoke/Molotov inheritance have also merged. PR #208 additionally prevents
pre-readiness GPU captures from being cached as settled health/ammo HUD pixels;
its workspace, GPU, release and actual console/Home checks pass. The current
[playtest checkpoint](PLAYTEST-2026-10-14.md#owner-requested-playtest-checkpoint-2026-10-16)
records the exact candidate and limitations. All-map loading proportions and
complete original Inventory/UI fidelity remain open.
Movement/surfing, remaining weapon states, UI coverage/performance and full
lighting fidelity remain open; bounded improvements are not whole-game parity.

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
| Rendering, animation, viewmodels, sounds | Bounded Dust II/Nuke and weapon checks; full lighting, motion and historical sound parity remain open | **Keep, measure and improve** |
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
RTT/caps and requires matching spawn epochs, teams and rounds. Installed
SAS/Phoenix model hitboxes now share tick-owned animation clocks with rendering
and retain posed shapes in that history. Native animation, neck-only damage
and tracked VR hitboxes remain open; see [model hitboxes](research/model-hitboxes.md).
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
- [x] Installed SAS/Phoenix animated capsules/boxes and damage groups, including
      bounded authoritative pose history and sampled render timelines.
- [ ] Full native character-animation/hitbox parity and tracked VR hitboxes.
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
- [x] Nearby LAN server discovery, verified inspection and browser joining.
      A rendered same-PC physical-interface check reached a 128-tick match and
      returned Home. Cross-machine discovery remains unverified; inherited
      loading-screen artwork errors remain. See [SERVER_BROWSER.md](SERVER_BROWSER.md).
- [ ] Public server directory.
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
- [ ] Display the current known local mode through the original Loading movie;
      source candidate retains unknown/remote/sandbox fallback, with actual
      rendered-label admission pending. [Scope](research/loading-selected-mode.md).
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
- [x] Original main-menu music with its runtime track and saved/live gain;
      actual sink lifecycle and combined RELEASE passed. Physical focus,
      complete original mix and2015 historical defaults remain open. PR159.
- [x] Connect original Video display/vsync controls to supported preferences;
      ordinary authored vsync selection changes saved settings and live Window
      policy, with reviewed1920x1080 images. Exclusive/triple modes, physical
      latency and the remaining Video page remain open. PR161.
- [ ] Complete the game API bridge for inventory/cases and the remaining
      classic panels. Original reset defaults still need verified evidence.
- [ ] Verify ordinary action-row Mouse4/Mouse5 capture from Home and Pause,
      immediate persistence across restart, effective HUD hints and opener-click
      suppression. Source fix and public-input regressions authored; root-run
      compilation/tests and rendered hardware checks remain pending.
- [x] Bound recognized Legacy weapon cases and rare knives to the documented
      2015 subset: 17 cases, nine families and 177 supported model/paint pairs,
      preserving raw catalog indices, owned later records and custom identities.
      Installed catalog and transactional opening checks pass. Original Inventory
      labels render; thumbnails (#181), full case UI, other historical cosmetic
      categories and earned progression remain open. [Policy](research/classic-case-knives.md), PR177.
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
      Dust II (2017), the pre-rework archive, now has an experimental native
      card and generation-owned download/progress/cancellation path. Its one
      small prop material fallback remains documented; other archives and
      native visual parity are still open.
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
- [x] Integrate independently observed standing AK, M4A4, both fresh M4A1-S
      attachment modes, Galil and normal-fire FAMAS spray profiles, gated to
      their exact installed Legacy schema. The disturbed M4A1-S set is wholly
      excluded. These are version-specific observations, not complete moving,
      crouching, scoped, other-weapon or 2015 recoil parity.
- [x] Separate measured AK punch angle/velocity recovery from spray phase;
      admit discrete resumed impulses against an independent paused-spray
      observation. Actual World/TickInput rays and rendered recovery passed;
      other weapons' recovery and historical parity remain open.
      [PR #147](https://github.com/bentsignal/global-offensive/pull/147),
      [PR #151](https://github.com/bentsignal/global-offensive/pull/151).
- [x] Separate independently measured M4A4 punch angle/velocity recovery;
      actual128Hz held/restart rays and held/exhausted renders passed, together
      with combined RELEASE/CI. Spray-phase selection stays unchanged;64Hz
      native and historical parity remain open. PR164.
- [x] Restore measured finite-history M4A4 look translation without changing
      the aim ray or weapon orientation. Actual rendered yaw/pitch cases have
      about one pixel of remaining rear-sight discrepancy; walking bob and
      other weapon/view contexts remain open.
      [PR #150](https://github.com/bentsignal/global-offensive/pull/150).
- [x] Omit unused Bevy light clusters from the client's custom-lit cameras.
      Same-binary Nuke comparisons reduced mean frame time about 7-8% in both
      orders; production release/probe/sim and default-path rendering passed.
      Complete pacing, other-map performance and 155 Hz parity remain open.
      [Evidence](research/light-cluster-overhead.md).
- [x] Replace the plain death feed with the installed classic notification movie: original boxes, team colors, local highlights and runtime icons, with independently inspected real kills and multiple rows. Missing-icon fallback and repeated relayout are verified; complete coverage and historical parity remain open. [HUD evidence](research/authored-killfeed.md).
- [x] Authored static-prop lighting metadata foundation: retained model and
      hardware/source-vertex identities, bounded opaque VHV parsing and
      instance-scoped checksum/count validation against installed Dust II.
      Record order, color interpretation and full VHV lighting remain open.
      [Foundation evidence](research/static-prop-vhv.md).
- [x] Establish a localized native response to an opaque prop-lighting
      payload intervention, with a repeated control and unchanged installed
      data. A second marker supports hardware order for 16 selected records;
      general record-order and color-decoder admission remain open.
- [x] Bounded selected-map authored lighting for flags0 unbumped static props
      with equal-group records. Actual classic north/south renders admit173
      meshes/95,965 vertices and preserve unsupported fallbacks; arches remain
      overbright and complete record/color/lighting fidelity remains open. PR163.
- [x] Render ordinary authored detail on the supported world/static-prop subset.
      Actual visible wall grain and selected broad color errors improve; controls
      remain unchanged. Mixed fine-detail metrics, unsupported modes and global
      lighting remain open; the occluded static-only zero-effect packet is retained.
      [Evidence](research/prop-detail-texture.md), PR171.
- [x] Preserve displayed inspection poses through incoming authored fades and
      cosmetic knife stab/inspect/rebound-reload cancellation, while the real
      attack deadline stays unchanged. Installed production checks and actual
      knife/AK images pass; native easing, repeated-input timing, rare selection
      and continuous Karambit loops remain open. [Transitions](research/inspection-transitions.md),
      [input](research/knife-inspect-input.md), PR173/174.
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
- [x] Restore supported authored HE dust boxes and the explicit Source-Z upper
      hemisphere. Four actual before/after images and combined RELEASE checks
      pass; different seeds/profile builds prevent pixel-parity claims. Full
      HE children/operators/materials, other effects and 2015 appearance remain
      open. [Placement](research/utility-particles.md#he-authored-initial-placement), PR172.
- [ ] Wall penetration (surface properties) and hit groups from the models'
      real hitboxes.
      An initial isolated thin-cover path reads mounted material modifiers and
      continues authoritative shots through supported wood/cardboard/plastic/
      glass/grate slabs. A real Inferno wood-panel firing control passes at
      64/128 Hz. Its thickness budget/damage retention remain approximations;
      overlapping cover, other materials and native penetration parity stay open.
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
      [DEATH_PRESENTATION.md](DEATH_PRESENTATION.md). Bounded cosmetic MDL/PHY
      dynamics now retain the victim's pose and velocity; exact VPhysics
      response and authored death-notice integration remain unfinished.
- [ ] Validate the map pool beyond Dust II (Mirage, Inferno, Nuke, Overpass,
      Train, Cache, Vertigo, Office, Italy, ...).
- [ ] The inherited fidelity checklist below (M2–M4).

- [x] Offer pre-rework Inferno (2016) beside the remake in the original map
      chooser; distinct ContentIds, actual authored-card loading and combined
      RELEASE lifecycle passed. The narrow v44 prop importer preserves original
      geometry. MA02's actual chooser/session integration is verified; exact2015
      content/card art and full loading-screen fidelity remain open. PR167/170.

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
- [ ] Complete overlays and decals: non-affine mapping, distance fading,
      dynamic brush receivers and hardware quality filters remain open.
- [x] Render the [affine static-world overlay subset](BSP_OVERLAYS.md):
      87 of Dust II's 90 overlays supported, with actual rendered verification.
      [PR #103](https://github.com/bentsignal/global-offensive/pull/103).
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
- [x] R8 alternate fire uses authored cadence/accuracy and secondary animation,
  with admitted shot mode retained through protocol v32. Instant primary is
  preserved; exact alternate recoil remains unmeasured.
- [ ] Complete burst/suppressor/inspection fidelity and original revolver cocking
  (owner currently requests instant primary)
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
  Press 4 repeatedly to cycle utility. Hold left for overhand, right for
  underhand, or both for the intermediate throw; release both buttons to
  throw. Grenades remain available after throws and respawns. Bots and
  competitive inventories are unchanged. This exposes existing grenade
  simulation/effects for testing; throw timing, trajectories and particle
  fidelity are still unfinished.

- [x] Tick-authoritative grenade pin-pull, hold and release, with installed
  viewmodel events, secondary underhand and combined-button control. Offline
  practice redraws a replenished grenade after the release clip. Numeric
  strengths and inherited velocity remain provisional; trajectories, native
  command timing and particle fidelity are still unfinished.
- [x] Render installed low/mid/high `throwcharge` hand poses while holding
  grenades. Authored grids/masks and repeated throws passed rendered checks.
  A stationary Legacy SAS-arm comparison confirms the three endpoint modes;
  native blend timing and ballistic calibration remain open.
  [Scope](research/grenade-throw-state.md#authored-hold-hand-poses).
- [x] Timed decoy expiry emits a separate positional near/distant cue from
  installed soundscripts, matching observed terminal sound requests. Round
  cleanup is silent. Cadence/lifetime stay provisional; damage and visual
  expiry behaviour remain unmeasured. [Evidence](research/decoy-expiry-audio.md).
- [x] Authoritative grenade contact audio uses installed HE/flash/smoke/incendiary
  entries at the projectile position, with resting chatter and contact-fire
  duplication suppressed. [PR #230](https://github.com/bentsignal/global-offensive/pull/230)
  adds the observed decoy flashbang binding and final soft settling taps;
  workspace and installed sound decoding checks passed.
  Molotov binding and native cadence/mix parity remain unmeasured.
- [x] Retain bounded Rapier death bodies with installed MDL/PHY convex rigs,
  captured live poses, velocity and hit context, with asynchronous nearby
  collision cooking. An evaluated authored pose covers a death before its
  first rendered frame. Actual kill captures show the full body resting on
  the floor. Exact native impulses, friction and solver response remain open.
  [Cosmetic physics and limits](research/cosmetic-ragdolls.md), [PR #89](https://github.com/bentsignal/global-offensive/pull/89).
- [x] Load the installed PCF/VMT/VTF smoke, fire and HE layers into bounded,
  world-anchored particle batches, and render the held Molotov wick at its
  current posed attachment. Actual captures verify the effects and repeated
  throw pose cleanup. Fire/HE brightness, complete SpriteCard behavior and
  original appearance remain unfinished. [Particle subset](research/utility-particles.md).
- [x] Honor authored single-sequence `$BLENDFRAMES` interpolation; landed in
  `34900ea`, [PR #106](https://github.com/bentsignal/global-offensive/pull/106).
  Fire remains too bright and smoke too dark; dual-sequence modes and complete
  SpriteCard/native appearance parity remain open.
- [x] Honor explicit particle sorting flags and order enabled layers by their
  currently drawn centers. Absent flags retain the prior enabled behavior;
  rendered validation and smoke/fire material parity remain pending.
- [x] Preserve the throw-time primary/pistol ContentId for positional decoy
  playback through inventory changes, death and snapshots.
  [PR #230](https://github.com/bentsignal/global-offensive/pull/230) replaces
  AK/AWP 2 Hz timing with deterministic sampling of observed irregular intervals;
  workspace checks passed. Native scheduling law is not established,
  and other weapons retain provisional 2 Hz timing.
  [PR #90](https://github.com/bentsignal/global-offensive/pull/90).
- [x] Remove look-driven viewmodel lag; retain authored weapon animations.
- [ ] Validate the shared accepted M4 look/speed-settling presentation across
  weapons and utility, including airborne continuity and authored absolute
  animation tracks. [PR #230](https://github.com/bentsignal/global-offensive/pull/230)
  reuses accepted coefficients and the held compressed-track sampler. Workspace,
  all installed viewmodel checks (42-model coverage), and Release renders with
  AK/M4A1-S/AWP passed. Full-arsenal native motion/airborne parity remains open.
  [Scope and preserved measured evidence](VIEWMODEL_MOTION.md).
- [x] Tick-authoritative M4A1-S/USP-S suppressor toggling, installed transition
  clips and cues, shot-time sound routing and held/dropped bodypart visibility;
  landed in `aefad89`, [PR #105](https://github.com/bentsignal/global-offensive/pull/105).
  Mode-dependent ballistics, native attack timing and third-person transition
  gestures remain open. [Scope and evidence](SUPPRESSORS.md).
- [x] Present Pause transitions atomically, reject obsolete open-cycle
  rasters, and coalesce stale pointer motion across the passive visibility
  observation. Rendered checks support reduced hover backlog; overall menu
  smoothness and physical input latency remain open. Audio Settings/Back and
  Pause/reopen flow work; Game Settings routing now opens its original scene,
  whose controls are still being connected. [PR #87](https://github.com/bentsignal/global-offensive/pull/87), [PR #93](https://github.com/bentsignal/global-offensive/pull/93).

- [x] Replicate the grenade solver's existing soft-floor rest decision so a
  settled projectile stops cosmetic tumbling through gravity/contact cycles.
  A support query wakes the visual mark if a dynamic world removes its floor;
  translation, bounces, gravity, fuses and authored appearance are unchanged.
  Exact original angular motion and native grenade physics remain unmeasured.
- [x] Keep the same decoy world body across projectile-to-field activation,
  remove it at authoritative expiry/cleanup, and recreate a resting body from
  an active-field snapshot. Native presence and rendered throw/expiry checks
  support this bounded fix; exact lifetime, cadence and replicated orientation
  remain open. [Evidence](research/deployed-decoy-body.md),
  [PR #145](https://github.com/bentsignal/global-offensive/pull/145).

- [x] Implement the October 22 evening weapon-feedback pass: held semi-auto
  cooldown presses, scoped automatic cadence, alternating Dualies, bounded
  Negev recoil, apex-sensitive Scout accuracy, quieter attacker head cues,
  material-selected smaller puffs, restored marks, scope glass/blur, feed icons
  and original scoreboard adapter. Presentation refinements remain tracked in
  [owner priorities](OWNER-PRIORITIES.md); this is not full fidelity acceptance.

- [x] Select impact and step materials from the actual BSP brush contact face,
  retaining face IDs through breakable overlays and rotating brush doors. Keep
  uniform PHY-hull behavior and clip geometry unchanged.
