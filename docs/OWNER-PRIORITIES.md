# Owner priorities

**Status: DRAFT — GAME DEVELOPMENT PAUSED**

The owner controls priority and order. This document supersedes older agent-selected work orders in ROADMAP.md and feedback ledgers. Historical documents remain evidence, not scheduling authority.

Until the owner locks this list, do not implement game features, optimize, build, run native/render tests, merge feature PRs, or independently choose the next task. Only this planning inventory and board may change.

When locked, work strictly from Critical to High to Medium to Low, top to bottom. Do not jump to a lower item, add speculative scope, or run parallel feature work out of order. Surface a dependency to the owner before changing this order.

Updated: 2026-10-17T19:08:38.213200+00:00 · revision 4

Source line numbers refer to the inventory snapshot at merged master `8b85197`; feedback IDs remain stable as planning notes are added.

Latest playtest: Mouse4/5 binding works; AWP cycling, firing cooldown and tested bullet marks passed. Side-button HUD labels were not checked by the owner. Performance is still unacceptable.

Owner preference authority remains `C:/Users/shawn/csr-playtest/feedback-20261017-34ea6f7-staged/userdata`. Preserve the tested build and these settings.

## Owner notes

(None yet.)

## Strict execution order

### Critical

(Not assigned.)

### High

(Not assigned.)

### Medium

(Not assigned.)

### Low

(Not assigned.)

## Unranked — blocks locking

- Old Dust II: high, stable FPS with bots / full matches (`perf-dust-bots`)
- Stop FPS degrading during longer sessions (`perf-sustained`)
- Old Dust II solo: remove tunnels / B-site frame dips (`perf-dust-solo`)
- Nuke: eliminate 80–120 FPS bot-match performance (`perf-nuke`)
- Return from a match to visible Home without a long black delay (`home-return-delay`)
- Make the console look exactly like the Source console (`console-appearance`)
- Match CS sensitivity / raw mouse response exactly; default 1 (`mouse-sensitivity`)
- Automatically translate CS2 sensitivity and keybinds at setup (`cs2-config-import`)
- Apply the good M4A4 walk / look / run feel to every weapon (`viewmodel-all-weapons`)
- Stop viewmodel teleporting during jumps and landings (`viewmodel-air`)
- Fix Galil firing glitches in the hands (`galil-animation`)
- Hold right-click to cycle Glock / FAMAS firing modes (`burst-held-switch`)
- Fix Glock semi-auto first-to-second-shot delay (`glock-tap-cadence`)
- Restore old gun sounds across all weapons (M4A4, Galil, FAMAS, etc.) (`classic-gun-audio`)
- Replace crude circular bullet-impact dust with original effects (`impact-particles`)
- Bullet decals on wood, boxes, pillars and all relevant surfaces (`impact-surfaces`)
- HUD: correct minimap size and layout (`hud-radar`)
- HUD: correct health / armor scale (`hud-health`)
- HUD: correct ammo scale (`hud-ammo`)
- HUD: correct right-side weapon size / spacing (`hud-weapons`)
- HUD: reproduce the original top player display (`hud-players`)
- Kill feed: original scale, name spacing, alignment and local-kill row (`hud-killfeed`)
- Correct weapon and headshot icons / fitting for every gun (`hud-icons`)
- Make noclip behave as expected and be discoverable (`noclip`)
- Fix strange hands after death, reload, spectating or respawn (`spectator-hands`)
- Stop bots sliding / moonwalking (`bot-moonwalk`)
- Optional original situation-specific death animations (`death-animations`)
- Correct old Inferno lighting (`inferno-lighting`)
- Correct Nuke interior / exterior materials and lighting (`nuke-lighting`)
- Working ladders on Nuke and all supported maps (`ladders`)
- Breakable Nuke vents and relevant breakable map objects (`vents`)
- Open / close map doors with correct sounds (Nuke squeaky, Cache, etc.) (`doors`)
- Exact grenade throw feel / trajectories, including jump and alternate throws (`grenade-trajectories`)
- Remove the flat smoke top; correct expansion / moving-view appearance (`smoke-shape`)
- Molotov / incendiary: correct flame height, brightness and floor flicker (`molly-visuals`)
- Decoys: original weapon-dependent bursts / pauses instead of a metronome (`decoy-cadence`)
- Restore decoy bouncing / contact sounds (`decoy-bounce`)
- Make volume 1 a reasonable level while preserving saved volume (`master-volume`)
- Optional physics for the Molotov rag (`molly-cloth`)
- Make Offline with Bots picker open promptly (`picker-delay`)
- Put Keyboard / Mouse settings in the original game UI (`authored-controls`)
- Verify / correct poor contrast of the black Pause logo (`pause-logo`)
- Old Dust II: fix overbright tunnel boxes / arches and underlying lighting (`dust-lighting`)
- AWP: correct audible bolt-pull cues (`awp-bolt-audio`)
- AWP: historical 10-round magazine (`awp-magazine`)
- AWP: moving scope crosshair blur / inaccuracy cue (`awp-blur`)
- Verify AWP numerical accuracy and movement inaccuracy against native (`awp-accuracy`)
- Confirm side-button bindings display correctly in the HUD (`mouse-hud-check`)
- Keep volume slider thumb aligned while dragging (`OLD-UI02`)
- Make the FPS counter easy to discover (`OLD-UI07`)
- Match bot difficulty dialog opacity and layout (`OLD-UI09`)
- Make starting a match self-explanatory (`OLD-UI10`)
- Finish original loading screens for every map (`OLD-UI11`)
- Play the original Valve startup movie and audio (`OLD-UI12`)
- Expand the crosshair editor with sliders and preview (`OLD-UI13`)
- Complete the original classic menu panels and settings (`OLD-CLASSIC-PANELS`)
- Finish the classic scoreboard and buy-menu presentation (`OLD-SCOREBOARD`)
- Complete the supported console and command surface (`OLD-CONSOLE-COMMANDS`)
- Match original movement, jumping and landing (`OLD-MOVEMENT`)
- Match CS:GO surfing and full bunnyhop behavior (`OLD-SURF`)
- Restore repeated-crouch fatigue and cooldown (`OLD-CROUCH-FATIGUE`)
- Match bullet-hit movement slowdown (`OLD-TAGGING`)
- Verify original body/view response when hit (`OLD-HIT-RESPONSE`)
- Restore competitive-start desaturation and fade (`OLD-COUNTDOWN`)
- Finish authentic recoil and spray across weapons and states (`OLD-SPRAY-COVERAGE`)
- Finish original burst switch and recovery timing (`OLD-BURST-LIFECYCLE`)
- Remove the extra center fire-mode notice; keep the original HUD indicator (`OLD-FIREMODE-NOTICE`)
- Verify empty-magazine automatic reload lifecycle (`OLD-EMPTY-RELOAD`)
- Complete original out-of-ammo dry-fire cues (`OLD-DRYFIRE`)
- Finish knife inspect interruptions and Karambit loops (`OLD-KNIFE-ANIM`)
- Give dropped weapons original toss and settling physics (`OLD-WEAPON-DROP`)
- Replace flat buy-menu character with original-quality presentation (`OLD-BUY-CHARACTER`)
- Original audio mixing, DSP, occlusion, footsteps and event coverage (`OLD-AUDIO-COVERAGE`)
- Finish original HE explosion dust, art and lighting (`OLD-HE-EFFECT`)
- Verify smoke detonation and emission audio lifecycle (`OLD-SMOKE-AUDIO`)
- Implement original Molotov fire spread as gameplay (`OLD-FIRE-SPREAD`)
- Finish the visible decoy's deployed lifecycle (`OLD-DECOY-BODY`)
- Stop bots repeatedly jumping against corners and boxes (`OLD-BOT-STUCK`)
- Match all actors' jump pose and airborne interpolation (`OLD-ACTOR-AIRBORNE`)
- Complete the old-map library and exact dated content (`OLD-CLASSIC-MAPS`)
- Remaining renderer fidelity: shadows, reflections, water, materials and detail (`OLD-RENDER-COVERAGE`)
- Finish all knives and finishes in the 2015 target (`OLD-KNIFE-COLLECTION`)
- Complete the dated skin and cosmetic collection (`OLD-COSMETIC-COLLECTION`)
- Match the classic Inventory and case-opening experience (`OLD-CASE-OPENING`)
- Earn and save cases, skins and knives by playing (`OLD-PROGRESSION`)
- Browse public community servers (`VISION-SERVER-DIRECTORY`)
- Host a server in the cloud (`VISION-CLOUD-HOST`)
- Use a Steam-free identity and original friends panel (`VISION-FRIENDS-ONLINE`)
- Fill multiplayer bot slots and match original warmup (`VISION-BOT-FILL-WARMUP`)
- Verify playing together across separate PCs (`VISION-FRIEND-MULTIPLAYER`)
- Finish frictionless local pack installation acceptance (`VISION-PACK-INSTALL`)
- Automatically download required content packs on join (`VISION-PACK-DOWNLOAD`)
- Provide the stable mechanics connection layer for other rewrites (`VISION-MOD-CONNECTION`)
- Bring in the MW2 Intervention with its original behavior (`VISION-INTERVENTION`)
- Return Overworld as an external mod (`VISION-OVERWORLD`)
- Offer an optional private release/content channel for friends (`VISION-PRIVATE-CHANNEL`)
- Play on Quest VR in the same matches as flat-screen friends (`VISION-VR-CROSSPLAY`)
- Complete remaining original combat and round rules (`VISION-CORE-MECHANICS`)
- Support original hostage rescue gameplay (`VISION-HOSTAGE-RULES`)
- Match original rifle optics and scope transitions (`VISION-RIFLE-OPTICS`)
- Make in-match Pause / Help and Options hover animations smooth (`owner-inventory-pause-hover`)

## Complete request inventory and evidence

### perf-dust-bots: Old Dust II: high, stable FPS with bots / full matches

Area: Performance · Priority: Unranked · Status: open

Old Dust II with bots about80 FPS/jitter; solo about200–300. Establish and remove actual bottlenecks, target native-class smooth high FPS.

Old Dust II bots about150, barely200, Long about120, A site about80 and often below100. Owner reports six bots; actual active counts must be verified independently. Less than a normal10-player workload is already unacceptable.

- Source: `docs/PLAYTEST-2026-10-16.md:52` — PF01
- Source: `docs/PLAYTEST-2026-10-17.md:27` — PF17-02

### perf-sustained: Stop FPS degrading during longer sessions

Area: Performance · Priority: Unranked · Status: open

Nuke frame rate falls as play continues. Find accumulating work/resources and sustained frame spikes, not only initial average.

- Source: `docs/PLAYTEST-2026-10-16.md:53` — PF02

### perf-dust-solo: Old Dust II solo: remove tunnels / B-site frame dips

Area: Performance · Priority: Unranked · Status: open

Old Dust II solo200-300 initially, occasional100s; tunnels toward B fall to about100. Preserve smooth feel and remove location-dependent dips; support lower-end PCs too.

- Source: `docs/PLAYTEST-2026-10-17.md:26` — PF17-01

### perf-nuke: Nuke: eliminate 80–120 FPS bot-match performance

Area: Performance · Priority: Unranked · Status: open

Nuke bots about80-120, repeatedly below100. Owner expects native-class performance on high-end PC, around500 desired. These are reported targets, not achieved results.

- Source: `docs/PLAYTEST-2026-10-17.md:28` — PF17-03

### menu-overlap: Settings / Home pages no longer overlap

Area: Menus · Priority: Confirmed pass · Status: owner pass

Audio Settings -> Play -> Offline with Bots looks fine, but Audio Settings -> Home still overlays pages. Fix transition ownership/lifecycle so pages cannot overlap; no route-specific concealment.

- Source: `docs/PLAYTEST-2026-10-16.md:54` — UI16-01

### pause-lifecycle: Escape / Pause / Options no longer hides the HUD incorrectly

Area: Menus · Priority: Confirmed pass · Status: owner pass

In-match Audio Settings -> Escape returns to game; next Escape hides HUD, next returns, next finally shows Pause. Correct single coherent Escape/Pause/Options lifecycle.

- Source: `docs/PLAYTEST-2026-10-16.md:55` — UI16-02

### menu-flashes: Remove black flashes between menu pages

Area: Menus · Priority: Confirmed pass · Status: owner pass

Black frame or two/flicker between Audio/Video/Game/Home states; also Offline with Bots transitions. Transition complete presentations atomically without old-page overlap or blank frames.

- Source: `docs/PLAYTEST-2026-10-17.md:20` — UI17-01

### home-return-delay: Return from a match to visible Home without a long black delay

Area: Menus · Priority: Unranked · Status: open

Match -> Home had about5 seconds black screen while music already played after AK test. Make restored Home visible promptly; measure transition stages.

- Source: `docs/PLAYTEST-2026-10-16.md:56` — UI16-03

### power-confirmation: Power-button cancel / confirmation flow

Area: Menus · Priority: Confirmed pass · Status: owner pass

Test2 power-button/cancel/confirmation passed.

- Source: `docs/PLAYTEST-2026-10-16.md:57` — UI16-04

### console-appearance: Make the console look exactly like the Source console

Area: Controls / console · Priority: Unranked · Status: open

Console/bindlist, wheel-up/down jump and bunnyhop work. Source-style console appearance requested but low priority.

Console commands and wheel-jump behavior were accepted; this card is the remaining appearance request.

- Source: `docs/PLAYTEST-2026-10-16.md:58` — CT16-01

### mouse-sensitivity: Match CS sensitivity / raw mouse response exactly; default 1

Area: Controls / console · Priority: Unranked · Status: open

Sensitivity1 feels off; establish counts-to-angle/raw input and acceleration behavior against CS. Default1 for new users.

- Source: `docs/PLAYTEST-2026-10-16.md:59` — CT16-02

### cs2-config-import: Automatically translate CS2 sensitivity and keybinds at setup

Area: Controls / console · Priority: Unranked · Status: implemented; acceptance pending

Auto-import CS2 sensitivity/keybinds with best supported translation, read-only source and existing-setting preservation. Keyboard/mouse settings must be usable without console.

A read-only import path has landed. Verify the complete setup experience and translation; preserve existing settings.

- Source: `docs/PLAYTEST-2026-10-16.md:60` — CT16-03

### viewmodel-all-weapons: Apply the good M4A4 walk / look / run feel to every weapon

Area: Viewmodels / animation · Priority: Unranked · Status: open

Grounded M4A4 look/run/walk viewmodel motion feels very good; use this feel across all weapons including grenades. Existing measured paths are evidence, not permission to invent coefficients.

- Source: `docs/PLAYTEST-2026-10-16.md:61` — VM16-01

### viewmodel-air: Stop viewmodel teleporting during jumps and landings

Area: Viewmodels / animation · Priority: Unranked · Status: open

M4A4 running jump/air/landing teleports the viewmodel front/center and glitches. Preserve continuity through movement domains.

- Source: `docs/PLAYTEST-2026-10-16.md:62` — VM16-02

### galil-animation: Fix Galil firing glitches in the hands

Area: Viewmodels / animation · Priority: Unranked · Status: open

Galil glitches around in hand when shooting; reload animation/sound seems good.

- Source: `docs/PLAYTEST-2026-10-16.md:63` — VM16-03

### ak-spray-pass: AK spray / recovery test

Area: Weapons · Priority: Confirmed pass · Status: bounded owner pass

AK spray/recovery test seemed good. No claim that every spray is exact.

- Source: `docs/PLAYTEST-2026-10-16.md:64` — WP16-01

### m4s-pass: M4A1-S spray / reload / inspect and distant sound

Area: Weapons · Priority: Confirmed pass · Status: bounded owner pass

M4A1-S spray/reload/inspect seems good; attached/detached and distant sound good enough for now.

- Source: `docs/PLAYTEST-2026-10-16.md:65` — WP16-02

### m4-inspect-pass: M4A4 inspect / reload spam improved

Area: Weapons · Priority: Confirmed pass · Status: bounded owner pass

M4A4 inspect/reload spamming improved.

- Source: `docs/PLAYTEST-2026-10-16.md:66` — WP16-03

### burst-held-switch: Hold right-click to cycle Glock / FAMAS firing modes

Area: Weapons · Priority: Unranked · Status: open

Holding secondary should repeat Glock/FAMAS mode changes; owner estimates about0.5s intervals. Measure native held behavior and actual deadline; no arbitrary estimate admission.

- Source: `docs/PLAYTEST-2026-10-16.md:68` — WP16-05

### glock-tap-cadence: Fix Glock semi-auto first-to-second-shot delay

Area: Weapons · Priority: Unranked · Status: open

Glock semi-auto spam has a much longer first-to-second-shot gap than subsequent shots. Compare native maximum tap cadence and exact first-shot/lifecycle state.

- Source: `docs/PLAYTEST-2026-10-16.md:69` — WP16-06

### classic-gun-audio: Restore old gun sounds across all weapons (M4A4, Galil, FAMAS, etc.)

Area: Audio · Priority: Unranked · Status: open

M4A4 is audibly the newer sound; restore classic original. Galil likely newer too; check FAMAS. Audit every gun against the old target instead of repeatedly needing reminders.

- Source: `docs/PLAYTEST-2026-10-16.md:70` — AU16-01

### impact-particles: Replace crude circular bullet-impact dust with original effects

Area: Impacts / effects · Priority: Unranked · Status: open

AK and M4A1-S impact dust looks like crude circles on many surfaces; one rocky surface looked plausible but is not general acceptance. Match original material-dependent authored effects.

- Source: `docs/PLAYTEST-2026-10-16.md:71` — FX16-01

### impact-surfaces: Bullet decals on wood, boxes, pillars and all relevant surfaces

Area: Impacts / effects · Priority: Unranked · Status: open

Wooden pillars/boxes show dust but no bullet decals, while walls show marks. Correct all relevant surface/prop impact paths and transforms.

Latest AWP marks passed in the tested situation. All-surface coverage is still open.

- Source: `docs/PLAYTEST-2026-10-16.md:72` — FX16-02

### hud-radar: HUD: correct minimap size and layout

Area: HUD · Priority: Unranked · Status: open

Minimap too small; health/armor and ammo too large; right-side weapons too small and cluttered; top player display wrong. Compare old CS:GO references at matching resolution and match layout/scale exactly.

- Source: `docs/PLAYTEST-2026-10-16.md:73` — HUD16-01

### hud-health: HUD: correct health / armor scale

Area: HUD · Priority: Unranked · Status: open

Minimap too small; health/armor and ammo too large; right-side weapons too small and cluttered; top player display wrong. Compare old CS:GO references at matching resolution and match layout/scale exactly.

- Source: `docs/PLAYTEST-2026-10-16.md:73` — HUD16-01

### hud-ammo: HUD: correct ammo scale

Area: HUD · Priority: Unranked · Status: open

Minimap too small; health/armor and ammo too large; right-side weapons too small and cluttered; top player display wrong. Compare old CS:GO references at matching resolution and match layout/scale exactly.

- Source: `docs/PLAYTEST-2026-10-16.md:73` — HUD16-01

### hud-weapons: HUD: correct right-side weapon size / spacing

Area: HUD · Priority: Unranked · Status: open

Minimap too small; health/armor and ammo too large; right-side weapons too small and cluttered; top player display wrong. Compare old CS:GO references at matching resolution and match layout/scale exactly.

- Source: `docs/PLAYTEST-2026-10-16.md:73` — HUD16-01

### hud-players: HUD: reproduce the original top player display

Area: HUD · Priority: Unranked · Status: open

Minimap too small; health/armor and ammo too large; right-side weapons too small and cluttered; top player display wrong. Compare old CS:GO references at matching resolution and match layout/scale exactly.

- Source: `docs/PLAYTEST-2026-10-16.md:73` — HUD16-01

### hud-killfeed: Kill feed: original scale, name spacing, alignment and local-kill row

Area: HUD · Priority: Unranked · Status: open

Kill feed too big, poorly aligned and excessive space left of local name; local-kill row especially large. Match original spacing/scale for all guns and headshots.

- Source: `docs/PLAYTEST-2026-10-16.md:74` — HUD16-02

### hud-icons: Correct weapon and headshot icons / fitting for every gun

Area: HUD · Priority: Unranked · Status: partial; full coverage pending

AK/M4A1-S icons and headshot icon appear; ensure complete weapon coverage and correct fitting. Components working does not accept layout.

- Source: `docs/PLAYTEST-2026-10-16.md:75` — HUD16-03

### noclip: Make noclip behave as expected and be discoverable

Area: Controls / console · Priority: Unranked · Status: needs verification

Owner found noclip but it did not seem to work as expected. Verify discoverability and actual behavior before claiming it works.

- Source: `docs/PLAYTEST-2026-10-16.md:76` — CT16-04

### spectator-hands: Fix strange hands after death, reload, spectating or respawn

Area: Viewmodels / animation · Priority: Unranked · Status: open

Odd spectator/respawn hands recur. Death during own/bot reload may be related, but is only a hunch. Test interrupted animation, death, spectator target changes and respawn ownership without tunnel vision.

- Source: `docs/PLAYTEST-2026-10-16.md:77` — AN16-01

### bot-moonwalk: Stop bots sliding / moonwalking

Area: Bots · Priority: Unranked · Status: open

Bots sometimes moonwalk/slide; movement direction/animation interpolation need inspection. Otherwise behavior seems okay.

- Source: `docs/PLAYTEST-2026-10-16.md:78` — BO16-01

### harmless-pass: Harmless bots / tested difficulty selection

Area: Bots · Priority: Confirmed pass · Status: bounded owner pass

Harmless bots do not shoot; difficulty selection works in observed test.

- Source: `docs/PLAYTEST-2026-10-16.md:79` — BO16-02

### death-animations: Optional original situation-specific death animations

Area: Viewmodels / animation · Priority: Unranked · Status: owner deferred

Ragdoll deaths look acceptable. Authored situation-appropriate dying animation transitions would be ideal later.

Current ragdolls are accepted. Custom ragdoll work is unnecessary for now.

- Source: `docs/PLAYTEST-2026-10-16.md:80` — BO16-03

### inferno-lighting: Correct old Inferno lighting

Area: Maps / lighting · Priority: Unranked · Status: open

Old Inferno looks good as a map and owner appreciates access; lighting remains wrong.

- Source: `docs/PLAYTEST-2026-10-16.md:81` — MA16-01

### nuke-lighting: Correct Nuke interior / exterior materials and lighting

Area: Maps / lighting · Priority: Unranked · Status: open

Nuke exterior sometimes good, but interior/material lighting broadly wrong (yellow beams an example, not sole target); exterior also has issues. Diagnose renderer/material/lightmap causes.

- Source: `docs/PLAYTEST-2026-10-16.md:82` — MA16-02

### ladders: Working ladders on Nuke and all supported maps

Area: Map interactions · Priority: Unranked · Status: open

Nuke ladders do not work. All ladders on supported maps need functional climbing.

- Source: `docs/PLAYTEST-2026-10-16.md:83` — MA16-03

### vents: Breakable Nuke vents and relevant breakable map objects

Area: Map interactions · Priority: Unranked · Status: open

Nuke vents must be breakable. Preserve map entity interaction/state through authoritative simulation.

- Source: `docs/PLAYTEST-2026-10-16.md:84` — MA16-04

### doors: Open / close map doors with correct sounds (Nuke squeaky, Cache, etc.)

Area: Map interactions · Priority: Unranked · Status: open

Nuke squeaky door absent/nonfunctional; doors must open/close with correct sounds on Nuke, Cache and all supported maps.

- Source: `docs/PLAYTEST-2026-10-16.md:85` — MA16-05

### grenade-trajectories: Exact grenade throw feel / trajectories, including jump and alternate throws

Area: Grenades · Priority: Unranked · Status: open

Standing overhand trajectory arches incorrectly; jump throws too low/short; underhand/running feel more reasonable but not accepted as exact. Different grenade types appear to diverge under the same throw. Establish full measured launch/release/path law, with owner's CS2-feel exception above.

Explicit owner exception: use observed CS2 launch / throw feel. Assets and UI remain old CS:GO.

- Source: `docs/PLAYTEST-2026-10-16.md:86` — GR16-01

### smoke-shape: Remove the flat smoke top; correct expansion / moving-view appearance

Area: Grenades · Priority: Unranked · Status: open

Smoke looks good and fade-out looks very good, but has a conspicuously flat top.

- Source: `docs/PLAYTEST-2026-10-16.md:87` — GR16-02

### molly-visuals: Molotov / incendiary: correct flame height, brightness and floor flicker

Area: Grenades · Priority: Unranked · Status: open

Molotov/incendiary fire too bright/high, floor/light changes glitch as camera moves. Initial ignition/extinguishing look fine.

- Source: `docs/PLAYTEST-2026-10-16.md:88` — GR16-03

### decoy-cadence: Decoys: original weapon-dependent bursts / pauses instead of a metronome

Area: Grenades · Priority: Unranked · Status: open

Decoy repeats primary sound at perfectly uniform pace, clearly artificial; bouncing sound has disappeared. Previously reported twice. Need observed burst/random timing and collision audio, not another guessed fixed interval.

- Source: `docs/PLAYTEST-2026-10-16.md:89` — GR16-04

### decoy-bounce: Restore decoy bouncing / contact sounds

Area: Grenades · Priority: Unranked · Status: open

Decoy repeats primary sound at perfectly uniform pace, clearly artificial; bouncing sound has disappeared. Previously reported twice. Need observed burst/random timing and collision audio, not another guessed fixed interval.

- Source: `docs/PLAYTEST-2026-10-16.md:89` — GR16-04

### master-volume: Make volume 1 a reasonable level while preserving saved volume

Area: Audio · Priority: Unranked · Status: open

Master1 painfully loud, about0.15 comfortable. Implement requested reasonable nominal gain while preserving saved volume. Menu-music slider works.

- Source: `docs/PLAYTEST-2026-10-16.md:90` — AU16-02

### molly-cloth: Optional physics for the Molotov rag

Area: Grenades · Priority: Unranked · Status: owner deferred

Optional moving Molotov rag physics after correct shared viewmodel motion and core playability.

- Source: `docs/PLAYTEST-2026-10-16.md:92` — FX16-03

### picker-delay: Make Offline with Bots picker open promptly

Area: Menus · Priority: Unranked · Status: open

Play -> Offline with Bots picker takes multiple seconds to appear and flickers. Remove avoidable transition delay.

Black transition flashes passed. Picker latency is a separate remaining request.

- Source: `docs/PLAYTEST-2026-10-17.md:21` — UI17-02

### authored-controls: Put Keyboard / Mouse settings in the original game UI

Area: Menus · Priority: Unranked · Status: open

Keyboard/Mouse uses an obviously custom screen. Integrate controls into normal original game settings look and navigation. Functional engine controls panel is not fidelity acceptance.

- Source: `docs/PLAYTEST-2026-10-17.md:22` — UI17-03

### pause-logo: Verify / correct poor contrast of the black Pause logo

Area: Menus · Priority: Unranked · Status: needs verification

Pause Counter-Strike logo is black and has poor contrast on some map backgrounds. Compare actual historical handling before changing; owner accepts it if genuinely original.

- Source: `docs/PLAYTEST-2026-10-17.md:23` — UI17-04

### binding-persistence: Bindings survive closing and restarting

Area: Controls / console · Priority: Confirmed pass · Status: owner pass

Changed bindings work during the session but do not survive closing/restarting. Persist accepted changes and verify with a second real process, not same-process disk parsing/reopen alone.

- Source: `docs/PLAYTEST-2026-10-17.md:24` — CT17-01

### hud-key-labels: HUD updates after keyboard rebinding

Area: HUD · Priority: Confirmed pass · Status: owner pass

Bottom-right pistol/knife/etc. hints continue showing old keys after rebinding. Derive displayed keys from actual effective bindings and refresh when bindings change.

- Source: `docs/PLAYTEST-2026-10-17.md:25` — HUD17-01

### dust-lighting: Old Dust II: fix overbright tunnel boxes / arches and underlying lighting

Area: Maps / lighting · Priority: Unranked · Status: open

Old Dust II generally looks excellent, but lower-tunnel boxes and arches are too bright. Keep broad interior lighting diagnosis; do not mistake a single surface observation for the full issue.

- Source: `docs/PLAYTEST-2026-10-17.md:29` — MA17-01

### awp-bolt-audio: AWP: correct audible bolt-pull cues

Area: Audio · Priority: Unranked · Status: implemented; specific acceptance pending

AWP missing bolt-pull audio after firing. Use correct authored legacy animation/audio timing.

Runtime sound requests and decoding verified after #220; latest owner did not specifically confirm bolt audio.

- Source: `docs/PLAYTEST-2026-10-17.md:30` — WP17-01

### awp-magazine: AWP: historical 10-round magazine

Area: Weapons · Priority: Unranked · Status: implemented; specific acceptance pending

AWP magazine is5; historical CS:GO target is10. Apply explicit historical content policy rather than inheriting newer install balance.

10-round magazine rendered and tested after #220; latest owner did not specifically discuss ammunition count.

- Source: `docs/PLAYTEST-2026-10-17.md:31` — WP17-02

### awp-cooldown: AWP: safe bolt / firing cooldown and rescope lifecycle

Area: Weapons · Priority: Confirmed pass · Status: owner pass in latest test

AWP post-shot unscope immediately re-scopes; spamming secondary allows rapid primary firing through magazine. Zoom transitions must not shorten primary cooldown; correct bolt/rescope lifecycle.

- Source: `docs/PLAYTEST-2026-10-17.md:32` — WP17-03

### awp-held-zoom: AWP: hold right-click to cycle zoom levels

Area: Weapons · Priority: Confirmed pass · Status: owner pass in latest test

AWP scope zoom levels cycle more slowly than the original; holding secondary should repeat at its legitimate maximum rate. Preserve the primary bolt cooldown fix while correcting held-secondary behavior and verifying native timing.

- Source: `docs/PLAYTEST-2026-10-17.md:44` — WP17-04

### awp-blur: AWP: moving scope crosshair blur / inaccuracy cue

Area: Weapons · Priority: Unranked · Status: open

Missing blurred sniper crosshair/inaccuracy cue while moving. Match the original cue; movement appears to reduce shot accuracy, but the owner has not accepted the numerical accuracy model.

- Source: `docs/PLAYTEST-2026-10-17.md:45` — WP17-05

### awp-accuracy: Verify AWP numerical accuracy and movement inaccuracy against native

Area: Weapons · Priority: Unranked · Status: needs verification

Missing blurred sniper crosshair/inaccuracy cue while moving. Match the original cue; movement appears to reduce shot accuracy, but the owner has not accepted the numerical accuracy model.

- Source: `docs/PLAYTEST-2026-10-17.md:45` — WP17-05

### awp-mark-pass: AWP bullet marks in the latest tested situation

Area: Impacts / effects · Priority: Confirmed pass · Status: bounded owner pass in latest test

AWP bullet marks appear inconsistently, making it difficult to judge impacts or accuracy. Check impact/decal delivery and surface coverage before attributing the symptom to weapon spread.

- Source: `docs/PLAYTEST-2026-10-17.md:46` — FX17-01

### mouse-capture-pass: Bind Mouse 4 / Mouse 5 through ordinary settings

Area: Controls / console · Priority: Confirmed pass · Status: owner pass in latest test

Cannot assign a side mouse button (apparently Mouse 4) to Knife through ordinary binding capture. Mouse side buttons must be valid bindings and persist/display correctly.

- Source: `docs/PLAYTEST-2026-10-17.md:47` — CT17-02

### mouse-hud-check: Confirm side-button bindings display correctly in the HUD

Area: HUD · Priority: Unranked · Status: implemented; owner check pending

Cannot assign a side mouse button (apparently Mouse 4) to Knife through ordinary binding capture. Mouse side buttons must be valid bindings and persist/display correctly.

Latest owner explicitly confirmed side buttons work, but did not check their HUD labels. Rendered Mouse4/5 labels passed developer checks.

- Source: `docs/PLAYTEST-2026-10-17.md:47` — CT17-02

### OLD-UI02: Keep volume slider thumb aligned while dragging

Area: Menus · Priority: Unranked · Status: implemented, owner acceptance pending

Owner: master-volume slider thumb must stay centered on its authored bar. Oct14 records developer verification in #122, but no owner acceptance of this specific fix.

- Source: `docs/PLAYTEST-2026-10-14.md:94` — UI02

### OLD-UI07: Make the FPS counter easy to discover

Area: Menus · Priority: Unranked · Status: implemented, owner acceptance pending

Owner requested a usable FPS counter in console/settings. cl_showfps 1 and a console readout were implemented in #125; discoverability and owner acceptance of this specific workflow were not established by these older documents.

- Source: `docs/PLAYTEST-2026-10-14.md:99` — UI07

### OLD-UI09: Match bot difficulty dialog opacity and layout

Area: Menus · Priority: Unranked · Status: partial; needs verification

Owner requested the original difficulty dialog opacity/layout. All six choices now reach installed difficulty profiles; the Oct16 difficulty pass covers functionality, leaving presentation and exact original AI/persona behavior unaccepted.

- Source: `docs/PLAYTEST-2026-10-14.md:101` — UI09

### OLD-UI10: Make starting a match self-explanatory

Area: Menus · Priority: Unranked · Status: needs verification

Owner initially could not find how to start a match and requested a clear original flow without an external checklist. Distinct from merely speeding up the Offline with Bots picker.

- Source: `docs/PLAYTEST-2026-10-14.md:102` — UI10

### OLD-UI11: Finish original loading screens for every map

Area: Menus · Priority: Unranked · Status: partial; needs verification

Owner requested original map loading artwork, scale and radar proportions: generic Nuke, oversized Mirage and janky Dust II were examples. Bounded archive/radar/label fixes landed; all-map proportions, Cancel interaction and remaining label fidelity were still open.

- Source: `docs/PLAYTEST-2026-10-14.md:103` — UI11
- Source: `docs/PLAYTEST-2026-10-14.md:168` — MA03
- Source: `docs/ROADMAP.md:226` — selected-mode label
- Source: `docs/ROADMAP.md:285` — classic map download presentation

### OLD-UI12: Play the original Valve startup movie and audio

Area: Menus · Priority: Unranked · Status: needs verification

Owner explicitly requested the head-valve startup movie/audio, read at runtime from legitimate installed or private historical content when available; public releases must not distribute the media.

- Source: `docs/PLAYTEST-2026-10-14.md:104` — UI12

### OLD-UI13: Expand the crosshair editor with sliders and preview

Area: Menus · Priority: Unranked · Status: needs verification

Owner requested sliders and a live preview as a quality-of-life exception to the historical baseline. Existing working color changes do not satisfy the expanded editor.

- Source: `docs/PLAYTEST-2026-10-14.md:105` — UI13
- Source: `docs/PLAYTEST-2026-10-14.md:175` — accepted crosshair color changes

### OLD-CLASSIC-PANELS: Complete the original classic menu panels and settings

Area: Menus · Priority: Unranked · Status: partial; needs verification

Original vision calls for the supplied Scaleform home/menu layout and full panel behavior: profile/rank, medals/commendations, local news, local cosmetic store and remaining original settings. Oct12 singled out clipped placeholder home news. Recreate online-only elements locally; do not infer live Valve services or Steam inventory integration.

- Source: `docs/VISION.md:117` — reference-screen checklist
- Source: `docs/VISION.md:145` — local substitutes for online-only panels
- Source: `docs/PLAYTEST_FEEDBACK.md:32` — full menu content/layout, including news
- Source: `docs/ROADMAP.md:256` — remaining classic panels and reset defaults

### OLD-SCOREBOARD: Finish the classic scoreboard and buy-menu presentation

Area: Menus · Priority: Unranked · Status: partial; needs verification

Original vision includes full classic Scaleform UI. The roadmap retains scoreboard and buy-menu integration; a basic functional scoreboard/buying flow is not original visual parity. The live buy-menu character is tracked separately.

- Source: `docs/ROADMAP.md:278` — Scaleform scoreboard, buy menu and HUD
- Source: `docs/PLAYTEST_FEEDBACK.md:36` — classic settings/gameplay/HUD coverage

### OLD-CONSOLE-COMMANDS: Complete the supported console and command surface

Area: Controls · Priority: Unranked · Status: partial; needs verification

Owner asked for editable input/history, useful errors, extensible commands and core controls for volume, crosshair, connection, game/mode settings and binds. Many commands and scrollback landed; complete command coverage, arbitrary command binding and completion remain open. Source-style console appearance is retained by CT16-01.

- Source: `docs/PLAYTEST-2026-10-14.md:111` — CT01
- Source: `docs/PLAYTEST-2026-10-14.md:112` — CT02
- Source: `docs/PLAYTEST-2026-10-14.md:113` — CT03

### OLD-MOVEMENT: Match original movement, jumping and landing

Area: Movement · Priority: Unranked · Status: partial; needs verification

Owner repeatedly said playable movement still feels unlike CS:GO and requested exact measured jump/landing response, including excess landing slowdown. Bounded standing takeoff/run/landing changes landed, but crouched/fatigued, broader running and surface behavior remain unaccepted. Keep the native reference version/settings explicit.

- Source: `docs/PLAYTEST-2026-10-14.md:115` — CT05
- Source: `docs/PLAYTEST_FEEDBACK.md:25` — jump/landing speed
- Source: `docs/VISION.md:168` — movement candidate still slows excessively
- Source: `docs/ROADMAP.md:379` — remaining landing response

### OLD-SURF: Match CS:GO surfing and full bunnyhop behavior

Area: Movement · Priority: Unranked · Status: partial; needs verification

Owner explicitly requires ramp contact, air control, sliding, corners, creases, seams and speed retention at recorded tick rates/server settings. The Oct16 bounded wheel-jump/bunnyhop pass does not establish full surf parity; selected trace/collision corrections are partial.

- Source: `docs/PLAYTEST-2026-10-14.md:114` — CT04
- Source: `docs/VISION.md:160` — surf requirements
- Source: `docs/PLAYTEST_FEEDBACK.md:106` — surf ramps/intersections/seams/speed
- Source: `docs/ROADMAP.md:371` — native surfing backlog

### OLD-CROUCH-FATIGUE: Restore repeated-crouch fatigue and cooldown

Area: Movement · Priority: Unranked · Status: needs verification

Owner requires the original resistance to rapid repeated crouching eventually, after current movement/audio work. Measure the native response rather than invent a debounce.

- Source: `docs/VISION.md:174` — repeated-crouch fatigue
- Source: `docs/ROADMAP.md:404` — repeated-crouch fatigue/cooldown

### OLD-TAGGING: Match bullet-hit movement slowdown

Area: Combat · Priority: Unranked · Status: needs verification

Owner requested verification and exact implementation of native tagging/movement slowdown when hit. Older ledger explicitly marks behavior for verification rather than assuming it is absent.

- Source: `docs/PLAYTEST-2026-10-14.md:116` — CT06

### OLD-HIT-RESPONSE: Verify original body/view response when hit

Area: Combat · Priority: Unranked · Status: needs verification

Owner wondered about body/view reaction on damage and explicitly said not to add it if the original lacks it. Native evidence determines whether any change is required.

- Source: `docs/PLAYTEST-2026-10-14.md:117` — CT07

### OLD-COUNTDOWN: Restore competitive-start desaturation and fade

Area: Combat · Priority: Unranked · Status: needs verification

Owner requested original competitive start/countdown desaturation, with fade-in near the end.

- Source: `docs/PLAYTEST-2026-10-14.md:120` — CT10

### OLD-SPRAY-COVERAGE: Finish authentic recoil and spray across weapons and states

Area: Weapons · Priority: Unranked · Status: partial; needs verification

Owner's exact spray/recoil requirement extends beyond bounded accepted AK/M4A1-S behavior: moving/crouched/scoped states, remaining weapons, punch recovery and historical 2015 parity. Standing version-gated profiles are partial progress, not a complete pattern set.

- Source: `docs/PLAYTEST-2026-10-14.md:143` — WP01
- Source: `docs/PLAYTEST_FEEDBACK.md:17` — bullet directions, camera punch and weapon-specific spray
- Source: `docs/ROADMAP.md:321` — standing-profile limits

### OLD-BURST-LIFECYCLE: Finish original burst switch and recovery timing

Area: Weapons · Priority: Unranked · Status: partial; needs verification

Owner requested Glock/FAMAS switch cue/animation, attack lockout and burst-repeat/recovery lifecycle. Native evidence contradicted recalled held-primary automatic repeat and confirms one three-round held cycle; preserve evidence rather than invent repeat. Exact switch/recovery deadlines remain open; later held-secondary changes are linked.

- Source: `docs/PLAYTEST-2026-10-14.md:144` — WP02
- Source: `docs/PLAYTEST-2026-10-14.md:146` — WP04

### OLD-FIREMODE-NOTICE: Remove the extra center fire-mode notice; keep the original HUD indicator

Area: HUD · Priority: Unranked · Status: partial; needs verification

The October16 owner explicitly said the bottom-right single/burst indicator is enough and the extra center notice is unnecessary. This newer owner instruction supersedes the older request to match native center-notice placement.

- Source: `docs/PLAYTEST-2026-10-14.md:145` — WP03
- Source: `docs/PLAYTEST-2026-10-16.md:44` — Latest owner presentation instruction

### OLD-EMPTY-RELOAD: Verify empty-magazine automatic reload lifecycle

Area: Weapons · Priority: Unranked · Status: implemented, owner acceptance pending

Owner reported reload requiring another fire attempt and requested original automatic reload timing across all guns. Released-empty burst and generic reload/sustained-fire cadence fixes landed in #136/#138; older evidence does not record owner acceptance across all guns.

- Source: `docs/PLAYTEST-2026-10-14.md:147` — WP05

### OLD-DRYFIRE: Complete original out-of-ammo dry-fire cues

Area: Weapons · Priority: Unranked · Status: partial; needs verification

Owner requested the original 0/0 click on a shoot attempt, without invented extra clicks before normal empty-magazine reloads. AK and M4A4/M4A1-S paths are partial; other guns, detached taps and historical parity remain.

- Source: `docs/PLAYTEST-2026-10-14.md:148` — WP06

### OLD-KNIFE-ANIM: Finish knife inspect interruptions and Karambit loops

Area: Animation · Priority: Unranked · Status: partial; needs verification

Owner requested soft animation interruption/restart for attack/inspect/reload, repeated R/F, rare inspect selection and continuous Karambit spins. Pose-fade/cancel fixes landed; exact original easing, priority, repeated-input timing and rare loops remain open.

- Source: `docs/PLAYTEST-2026-10-14.md:149` — WP07
- Source: `docs/ROADMAP.md:365` — inspection continuity limits

### OLD-WEAPON-DROP: Give dropped weapons original toss and settling physics

Area: Weapons · Priority: Unranked · Status: partial; needs verification

Owner requested toss, flight, contact and settling instead of teleporting to the floor. Original dropped models landed; motion/contact/settling is still separate unfinished work.

- Source: `docs/PLAYTEST-2026-10-14.md:150` — WP08

### OLD-BUY-CHARACTER: Replace flat buy-menu character with original-quality presentation

Area: Menus · Priority: Unranked · Status: needs verification

Owner rejected the flat PNG character and requested a live/posed character with the correct original authored scene composition.

- Source: `docs/PLAYTEST-2026-10-14.md:153` — WP11

### OLD-AUDIO-COVERAGE: Original audio mixing, DSP, occlusion, footsteps and event coverage

Area: Audio · Priority: Unranked · Status: partial; needs verification

Owner requires the original 2015 experience beyond the correct underlying firing WAV: shot overlap/tone, EQ/DSP, speaker routing, attenuation/occlusion, reload/deploy/first-step timing and missing events. Broader roadmap also retains ricochets/near misses, map soundscapes and music. Latest gun-version audit and master-volume requests remain separately linked; M4 attenuation is accepted good enough for now by Oct16.

- Source: `docs/PLAYTEST_FEEDBACK.md:100` — 2015 mix/operator inputs, lifetime, pitch, routing and DSP
- Source: `docs/VISION.md:164` — waveforms alone do not establish sound fidelity
- Source: `docs/ROADMAP.md:419` — remaining audio fidelity gaps

### OLD-HE-EFFECT: Finish original HE explosion dust, art and lighting

Area: Grenades · Priority: Unranked · Status: partial; needs verification

Owner rejected the substitute HE explosion look. Authored dust placement improved in #172, but complete children/operators/materials and historical appearance remain unaccepted.

- Source: `docs/PLAYTEST-2026-10-14.md:130` — GR04

### OLD-SMOKE-AUDIO: Verify smoke detonation and emission audio lifecycle

Area: Grenades · Priority: Unranked · Status: implemented, owner acceptance pending

Owner reported missing smoke detonation/emission audio. Observed positional cues were implemented in #124 and rendered/decoded checks passed; full mix and owner acceptance of this specific fix remain unrecorded.

- Source: `docs/PLAYTEST-2026-10-14.md:131` — GR05

### OLD-FIRE-SPREAD: Implement original Molotov fire spread as gameplay

Area: Grenades · Priority: Unranked · Status: needs verification

Owner requested measured ground/obstacle fire growth as authoritative gameplay, not a fixed visual stamp. Bot hazard consistency fixes do not implement spread cells, spacing, cadence or obstacle-growth rules.

- Source: `docs/PLAYTEST-2026-10-14.md:134` — GR08

### OLD-DECOY-BODY: Finish the visible decoy's deployed lifecycle

Area: Grenades · Priority: Unranked · Status: partial; needs verification

Owner requested the decoy remain visible while firing. Body persistence/expiry and snapshot support landed in #145; exact duration and orientation remain unaccepted, separately from the latest cadence/collision-audio report.

- Source: `docs/PLAYTEST-2026-10-14.md:135` — GR09

### OLD-BOT-STUCK: Stop bots repeatedly jumping against corners and boxes

Area: Bots · Priority: Unranked · Status: partial; needs verification

Owner reported repeated jumping/rubber-banding under Dust II B-window. Recovery improvements landed in #126/#175, but that exact jam was not reproduced in final proof and corner/airborne interpolation acceptance remains open.

- Source: `docs/PLAYTEST-2026-10-14.md:164` — BO01

### OLD-ACTOR-AIRBORNE: Match all actors' jump pose and airborne interpolation

Area: Animation · Priority: Unranked · Status: needs verification

Owner requested checking jump/airborne visual glitches for every actor, without attributing all symptoms to bot navigation. Later local viewmodel and bot moonwalk issues cover narrower paths.

- Source: `docs/PLAYTEST-2026-10-14.md:165` — BO02
- Source: `docs/ROADMAP.md:169` — full character animation/hitbox parity

### OLD-CLASSIC-MAPS: Complete the old-map library and exact dated content

Area: Maps · Priority: Unranked · Status: partial; needs verification

Original vision requests old Dust II, Inferno, Nuke and other classic maps. Old Dust II and old Inferno access is accepted/bounded progress; remaining archives, exact historical identity, artwork and zero-friction visible cancellable downloads remain unfinished. Do not equate a dated archive with exact 2015 or August 2018 parity.

- Source: `docs/VISION.md:106` — old everything and old maps
- Source: `docs/PLAYTEST-2026-10-14.md:167` — MA02
- Source: `docs/ROADMAP.md:285` — remaining classic archives and download UI
- Source: `docs/ROADMAP.md:429` — broader map pool

### OLD-RENDER-COVERAGE: Remaining renderer fidelity: shadows, reflections, water, materials and detail

Area: Rendering · Priority: Unranked · Status: partial; needs verification

Original full-fidelity goal retains broader renderer work beyond latest Nuke/Inferno/old-Dust lighting reports: hard/dynamic shadows, baked prop-lighting coverage, reflections, water/translucency, detail vegetation, overlays/decal coverage, exposure/color correction and model LOD/fade. These are inherited fidelity gaps needing present-state verification, not newly confirmed owner defects.

- Source: `docs/PLAYTEST-2026-10-14.md:166` — MA01
- Source: `docs/ROADMAP.md:553` — inherited rendering coverage
- Source: `docs/PLAYTEST.md:183` — older rendering gaps
- Source: `docs/PLAYTEST_FEEDBACK.md:27` — Dust II arch renderer diagnosis

### OLD-KNIFE-COLLECTION: Finish all knives and finishes in the 2015 target

Area: Cosmetics · Priority: Unranked · Status: partial; needs verification

Owner requested all period-correct knives, excluding later additions, with original animations/finishes. A documented 2015-12-31 policy and nine-family/177-pair catalog landed; exact historic assets/finish rendering and rare animation selectors remain open.

- Source: `docs/PLAYTEST-2026-10-14.md:169` — CO01
- Source: `docs/ROADMAP.md:262` — bounded case/knife policy

### OLD-COSMETIC-COLLECTION: Complete the dated skin and cosmetic collection

Area: Cosmetics · Priority: Unranked · Status: partial; needs verification

Owner requested skins/finishes/cases from the chosen 2015 cutoff, with no accidental later items. Existing bounded cases/knives are partial; ordinary finish coverage, sticker/souvenir/music/glove history and accurate wear/seed/odds/rendering remain unaccepted. No real-money trading/payment requirement is inferred.

- Source: `docs/PLAYTEST-2026-10-14.md:170` — CO02
- Source: `docs/ROADMAP.md:265` — other historic cosmetic categories

### OLD-CASE-OPENING: Match the classic Inventory and case-opening experience

Area: Cosmetics · Priority: Unranked · Status: partial; needs verification

Owner's supplied Chroma screenshot requires the original horizontal reel, gold center marker, rarity/item grid, status bar, classic Inventory and original sound/timing. Original thumbnails are bounded progress; complete authored case-opening UI and reward presentation remain unfinished.

- Source: `docs/VISION.md:140` — case-opening reference checklist
- Source: `docs/PLAYTEST-2026-10-14.md:170` — CO02
- Source: `docs/ROADMAP.md:274` — classic case opening and sound/timing

### OLD-PROGRESSION: Earn and save cases, skins and knives by playing

Area: Cosmetics · Priority: Unranked · Status: needs verification

Owner explicitly requested cases/openings earned through gameplay faster than original drop pace, with unlocked skins/knives and persistent progression. Do not infer real-money trading or payments.

- Source: `docs/PLAYTEST-2026-10-14.md:171` — CO03

### VISION-SERVER-DIRECTORY: Browse public community servers

Area: Multiplayer · Priority: Unranked · Status: partial; needs verification

Original vision requests a community browser plus direct connect without Steam. Favorites/local/LAN joining is implemented; public directory, wider discovery and browser ping/map/player/pack/history coverage remain unfinished or unverified.

- Source: `docs/VISION.md:69` — servers and server browser
- Source: `docs/ROADMAP.md:181` — local and LAN discovery
- Source: `docs/ROADMAP.md:186` — public server directory
- Source: `docs/ROADMAP.md:483` — directory and browser fields

### VISION-CLOUD-HOST: Host a server in the cloud

Area: Multiplayer · Priority: Unranked · Status: partial; needs verification

Owner wants both home-PC and cloud servers. Home listen/dedicated hosting landed; practical Linux/VPS runtime map/nav/item data acquisition remains unresolved. Keep installation friction low; SteamCMD app740 is an unverified possible route, not an accepted dependency.

- Source: `docs/VISION.md:71` — own PC or cloud
- Source: `docs/ROADMAP.md:187` — Linux VPS data availability

### VISION-FRIENDS-ONLINE: Use a Steam-free identity and original friends panel

Area: Multiplayer · Priority: Unranked · Status: needs verification

Original vision makes the friends panel our own server/player list. Roadmap calls for local player identity/keypair, recent players and online presence on known servers. Do not infer access to Steam social services.

- Source: `docs/VISION.md:145` — local friends/player list
- Source: `docs/ROADMAP.md:487` — Steam-free identity and friends panel

### VISION-BOT-FILL-WARMUP: Fill multiplayer bot slots and match original warmup

Area: Multiplayer · Priority: Unranked · Status: needs verification

Broader CS:GO multiplayer goal retains dynamic bot-slot filling as humans join/leave and original competitive warmup. Static bots are implemented; the roadmap explicitly leaves these behaviors open and separates measured 2023 quota behavior from the historical target.

- Source: `docs/ROADMAP.md:178` — dynamic bot fill and competitive warmup

### VISION-FRIEND-MULTIPLAYER: Verify playing together across separate PCs

Area: Multiplayer · Priority: Unranked · Status: implemented, broader owner acceptance pending

Original goal is the owner and friends in the same match, without Steam. Local two-client hosting/joining and same-PC LAN checks are implemented; cross-machine LAN discovery and an ordinary friends session remain unverified. Owner previously deferred multiplayer testing until friends are available; this is acceptance coverage, not a claim that networking is absent.

- Source: `docs/VISION.md:8` — owner and friends play together
- Source: `docs/ROADMAP.md:172` — listen server implemented
- Source: `docs/ROADMAP.md:182` — cross-machine LAN unverified
- Source: `docs/PLAYTEST-2026-10-14.md:182` — friends-session test deferred

### VISION-PACK-INSTALL: Finish frictionless local pack installation acceptance

Area: Modding · Priority: Unranked · Status: implemented, owner acceptance pending

Owner wants drag/drop or double-click installation, one file and done. Bounded local installation, file association and Home content manager landed; actual native picker interaction remains unverified. Keep private copyrighted packs outside public repo/releases.

 Owner deferred content-pack expansion until the core game works.

- Source: `docs/VISION.md:57` — private sideloading
- Source: `docs/ROADMAP.md:458` — local pack install and native dialog limitation
- Source: `docs/PLAYTEST-2026-10-16.md:91` — CO16-01

### VISION-PACK-DOWNLOAD: Automatically download required content packs on join

Area: Modding · Priority: Unranked · Status: needs verification

Original vision explicitly prefers server join automatically downloading required packs with near-zero setup. Local pack installation exists; advertising required hashes and server delivery remain open. Packs must stay outside public releases when private.

- Source: `docs/VISION.md:64` — automatic server pack downloads
- Source: `docs/ROADMAP.md:463` — required hashes and server downloads

### VISION-MOD-CONNECTION: Provide the stable mechanics connection layer for other rewrites

Area: Modding · Priority: Unranked · Status: partial; needs verification

Owner wants cross-game mechanics as well as assets via a stable mod API, with one-file delivery and server sharing. Data packs/seams exist; mechanics plugin API/runtime and complete ContentId registry coverage remain unfinished. Roadmap WASM/native choice is a design question, not a requested user feature by itself.

- Source: `docs/VISION.md:79` — first-class cross-game mechanics
- Source: `docs/ROADMAP.md:456` — ContentId registry completeness
- Source: `docs/ROADMAP.md:471` — mechanics connection layer
- Source: `docs/STITCHING.md:20` — shared catalog and behavior bridge

### VISION-INTERVENTION: Bring in the MW2 Intervention with its original behavior

Area: Modding · Priority: Unranked · Status: needs verification

Owner names the Intervention as the first crossed-in item. Deliver an IW4L/own-install recipe importer and private-pack route while retaining the weapon's home-game mechanics; assets never enter this repo/public releases.

- Source: `docs/VISION.md:59` — Intervention example
- Source: `docs/ROADMAP.md:490` — first crossed-in Intervention
- Source: `docs/STITCHING.md:43` — first crossed-in target

### VISION-OVERWORLD: Return Overworld as an external mod

Area: Modding · Priority: Unranked · Status: needs verification

Owner's broader crossing-games vision includes CS:GO in Minecraft. Roadmap returns Overworld through the connection layer as an external mod, never by restoring removed decompiled/vendored Minecraft code to this repository.

- Source: `docs/VISION.md:10` — crossing games
- Source: `docs/ROADMAP.md:494` — external Overworld mod
- Source: `docs/ROADMAP.md:525` — Minecraft code decision

### VISION-PRIVATE-CHANNEL: Offer an optional private release/content channel for friends

Area: Distribution · Priority: Unranked · Status: optional vision goal; needs verification

Roadmap retains an optional private channel for the owner's friends, separate from the public clean repository/feed. Public installer/updater distribution is already complete; this is only the unfinished private option.

- Source: `docs/ROADMAP.md:208` — public/private release channels
- Source: `docs/VISION.md:57` — private sideloading

### VISION-VR-CROSSPLAY: Play on Quest VR in the same matches as flat-screen friends

Area: VR · Priority: Unranked · Status: partial; needs verification

Original vision requires tracked head/hands and a held gun with the same mechanics, separate aim, smooth thumbstick locomotion and world-space menus. Flat clients must see tracked poses through third-person IK; tracked hitboxes/clamping must preserve fairness. Networked poses/input foundations and an OpenXR smoke exist, but headset appearance/recovery/session cleanup and actual game integration remain unfinished.

- Source: `docs/VISION.md:177` — Quest/OpenXR VR cross-play
- Source: `docs/ROADMAP.md:506` — headset rendering and lifecycle verification
- Source: `docs/ROADMAP.md:515` — VR player, third-person IK and tracked hitboxes

### VISION-CORE-MECHANICS: Complete remaining original combat and round rules

Area: Gameplay · Priority: Unranked · Status: partial; needs verification

The original full CS:GO rewrite retains wall penetration by surface properties, exact knife sweep/contact/animation events, revolver alternate-fire/cocking and remaining suppressor-mode ballistics/timing/third-person gestures. Basic shooting/bursts/suppressor toggling are implemented; older inherited boxes need current verification rather than treating all mechanics as absent.

- Source: `docs/VISION.md:8` — full original CS:GO rewrite
- Source: `docs/ROADMAP.md:413` — penetration and model hit groups
- Source: `docs/ROADMAP.md:575` — knife, rifle-optic and remaining weapon mechanics
- Source: `docs/ROADMAP.md:709` — suppressor implementation limits

### VISION-HOSTAGE-RULES: Support original hostage rescue gameplay

Area: Gameplay · Priority: Unranked · Status: needs verification

A full original CS:GO game includes supported cs_ maps and hostage rules/tactics. Older playtest and inherited roadmap explicitly leave hostage gameplay unfinished; verify present state before planning because these older limits are not a fresh owner report.

- Source: `docs/VISION.md:8` — full original CS:GO rewrite
- Source: `docs/PLAYTEST.md:218` — hostage-map mode restriction
- Source: `docs/ROADMAP.md:639` — hostage rules remain open

### VISION-RIFLE-OPTICS: Match original rifle optics and scope transitions

Area: Weapons · Priority: Unranked · Status: needs verification

Inherited fidelity roadmap retains original rifle-optic refraction, lens effects and transition parity. Later AWP lifecycle and moving-scope cue reports are separate; verify current rifle optics before calling old limitations current defects.

- Source: `docs/ROADMAP.md:576` — rifle-optic refraction/lens/transition parity

### owner-inventory-pause-hover: Make in-match Pause / Help and Options hover animations smooth

Area: Menus · Priority: Unranked · Status: partial; owner acceptance pending

In-match Pause and Help/Options hover animation is responsive; owner estimated about 5 fps despite acceptable Home hover. Diagnose in-match menu responsiveness separately from game FPS.

Navigation and lifecycle fixes passed; those do not establish animation responsiveness parity.

- Source: `docs/PLAYTEST-2026-10-14.md:96` — UI04

