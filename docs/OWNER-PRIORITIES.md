# Current owner priorities

## October 24: active implementation after owner playtest

1. Wallbang tests in F8: stationary targets behind real map cover, starting
   with both Dust II mid-door leaves; report actual hits/damage and exits.
2. Sniper blur: blur the complete crosshair without a sharp central core;
   preserve the scope circumference and world image.
3. AUG/SG close-focus blur must soften the outside silhouette as well as the
   weapon interior. Preserve accepted SG firing motion.
4. Saved headshot-feedback volume slider with a visible numeric value.
5. Prepare surfing, player-contact, boosting and run-boosting tests in the same
   launcher. Core mechanics precede buy-wheel/menu polish and multiplayer.

The owner explicitly resumed work and requested these tests. Preserve accepted
F8 lists, Dualies motion, R8 balance, impact dust/marks, fire, grenade, knife,
HUD size and airborne weapon motion. Older completion/staging notes below are
historical and do not close this queue.

Implementation for the requested next test: F8 includes stationary wallbang
targets admitted by the shared live penetration policy, an open headshot-tuning
target beside the saved volume slider, target-side inspection, and F7 reset.
Sniper defocus affects the entire reticle; rifle foreground blur crosses its
silhouette. Player hull contact, flat moving support and same-session partner/
surf tests are implemented. Native run-boost calibration, crouch-to-stand lifting,
uneven-base behavior and whole-course surf parity remain open; the new mechanics
test does not declare those solved. See [player-contact evidence and limits](research/player-contact.md).

## October 23 playtest follow-up: current ordered backlog

The owner resumed implementation after testing PR #317. Complete these player
outcomes in order; continue through commits and PRs until the approved queue is
finished or the owner interrupts. Do not stage another test unless requested.

1. F8: replace weapon and map Next/Previous pages with tall scrollable lists;
   group weapons by type and keep the whole catalog accessible.
2. Wall penetration: cover wallbangable spots across the maps, including Dust II
   mid doors. Check actual passage and damage separately from exit marks.
3. Scopes: convincing sniper blur; correct unscoped AUG/SG lens artifacts;
   close-focus weapon blur and restrained scoped firing motion, especially SG.
4. Dualies: preserve ongoing firing motion instead of snapping the other pistol
   back to its starting pose on every shot.
5. Headshot feedback: increase slightly from the latest quieter build.
6. R8: slow instant primary slightly and tune torso damage toward Deagle-like
   two-hit kills, preserving lethal headshots and the faster right-click.

Accepted by the owner: wall-impact presentation, Glock/Tec-9 cadence, AUG
automatic firing, improved Negev recovery, visible muzzle flashes and normal R8
kill-feed placement. Preserve previously accepted grenade, knife, fire, audio,
HUD sizing and airborne weapon-motion scopes. Multiplayer stays deferred.

### October 23 six-outcome implementation

- F8 uses tall scrollable lists with named weapon groups and all map choices;
  the old twelve-item pages and Next/Previous actions are removed.
- Cover resolves numeric Legacy material IDs, loaded VMT defaults and header-only
  MDL surface properties. Exact tied cover retains every crossed material;
  admitted shots leave exit marks. Actual current/classic Dust II door damage
  passes both directions at 64/128 Hz; coverage checks pass on 36 available maps.
- Sniper lines use the mounted soft coverage sprite with a dark core. AUG/SG
  lose invalid grey lens contributions and gain depth-aware close weapon blur;
  SG scoped firing motion is restrained. Optical pixels outside the sniper
  opening pass through unchanged, preserving the HUD.
- Dualies preserve independent hand clocks and blend same-hand restarts instead
  of resetting the other hand; firing presentation clears at life boundaries.
- Attacker headshot feedback gain rises from .35 to .42; other feedback is kept.
- Instant R8 primary uses a .65-second interval and mounted Deagle damage for
  two close torso hits and lethal heads; the .4-second secondary is preserved.

Local checks: 1,345 workspace tests, 1,425 sound decodes, installed map/door and
rapid-Dualies controls, normal PLAY build, Dust II probe/sim and GPU comparisons.
The penetration loss curve remains approximate; unknown definitions, sealed BSP
space and unsupported exit geometry stay blocked. This does not close M3 native
formula/geometry parity. Owner staging remains the last tested #317 build; do
not replace it or prepare another owner playtest without a request.

## October 22 evening playtest: active owner-approved follow-up

This latest feedback supersedes the older queue below. Continue useful combined
batches until interrupted; multiplayer is explicitly deferred.

1. Fix firing across the arsenal: skipped semi-auto clicks (Glock, P250, Tec-9
   and peers), AUG automatic fire, alternating Dual Berettas. Check Negev spray
   rise and Scout airborne accuracy against available original data. Preserve
   instant R8 firing; owner requests a modest cadence reduction.
2. Restore missing bullet marks (M4A4/Deagle examples) and correct oversized,
   fast impact dust with material-appropriate selection, especially wood.
3. Lower excessively loud attacker headshot feedback without losing range.
   Owner retracts earlier gun-recording complaints: retain accepted recordings.
4. Scope presentation: smooth inaccuracy blur for movement/jumping/firing;
   correct AUG/Scout white or grey optic backs and AUG scoped motion. Fact-check
   original muzzle flashes before adding effects.
5. HUD: repair R8 center-left white fallback kills and normal kill-feed flicker;
   replace placeholder bot portraits, soften incorrect damage indicators, restore
   classic scoreboard. Preserve accepted HUD size and kill-feed background (owner
   retracted opacity request). F8 orders all guns before grenades; catalog is
   present, owner found subsequent pages.

October 23 resumption: the owner explicitly rejects treating the first pass as
backlog completion. Continue unresolved gun handling/accuracy, scope integration,
material impacts and classic scoreboard work. Track specific remaining behavior;
a merged increment or provisional implementation does not close that outcome.
Preserve all owner-accepted scopes above and below. Do not make a new playtest.

October 23 implementation status: the combined pass delivered initial firing,
impact, head-cue, scope and HUD changes above. Workspace tests (1288), sound
decoding (1425 entries), Release build and Dust II probe/sim pass. Final Release
captures show wall marks, corrected AUG eye relief, dark Scout glass and the
original scoreboard. Follow-up captures confirm visible M4 muzzle flashes and compact, softened
directional damage arcs. The CT heading moves above its roster; keep scoreboard
scaling consistent between small and large rosters. Optional scoreboard panels
and exact decorative layout remain outside this first pass. Scout/Negev curves and visual balance remain practical first
passes. A further input correction preserves release/repress sequences seen
between simulation ticks; the ordinary sampler emits an up tick before the
retained fresh press. Held automatic fire and sim cadence stay unchanged.
Follow-through also preserves a complete release/repress delivered in one render
frame, stretches the sniper reticle strips across the scope, and retains the
original scoreboard during same-view stat updates instead of flashing fallback.
The arsenal follow-through restores previously missing R8 secondary fire using
the installed alternate cycle/spread/inaccuracy and shared ammo/readiness. The
instant 0.55s primary is preserved; no new cocking behavior is added. Alternate
recoil parity remains outside this bounded firing correction. Shot events now retain
the admitted mode through protocol v32, selecting the installed R8 secondary
activity and its three alternate clips. A normal PLAY capture confirms secondary
animation with four shots/four rounds remaining at 1.5s; primary cadence and
shared ammo are unchanged. Exact blend/recoil parity is not claimed.
The next combined increment fixes a concrete contact-material bug: BSP brushes
previously reported their first face material for every hit. Contacts now retain
the actual hit-face material, including breakable overlays and rotating brush
doors, without changing clip geometry. Attached marks validate collision material
rather than the visual door model. Negev recoil now recovers its live angle on
release; accuracy recovery uses authored final times and transition bullets.
Optic bloom stays inside the posed lens and no longer resets on zoom cycling;
the scoreboard labels both rosters and aligns visible score text to headings.
The combined 1307-test workspace, installed arsenal at 64/128 Hz, PLAY build,
Dust II probe/sim and seven rendered controls pass. PR #309 merged these changes.
The next increment aligns the original background emblems with their team
headings. Thin-cover penetration now continues ordinary authoritative shots
through up to four isolated wood, cardboard, plastic, glass or grate slabs.
Mounted material modifiers feed a bounded approximate thickness/damage policy;
unknown materials and overlapping geometry stay opaque. Workspace checks and an
actual installed Inferno wood-panel firing/opaque control pass at 64/128 Hz.
The final PLAY build and Dust II probe/sim pass; small/full scoreboard captures
confirm aligned original emblems and all twelve rows per team. PR #310 merged
after both CI checks passed.
Penetrating pellets emit one completed tracer path with separate entry impacts.
The original M3 task stays unchecked:
native damage/thickness parity and broader cover geometry are unfinished.
The active follow-through joins exact touching/overlapping convex intervals,
counts their physical thickness once and checks every crossed component face.
Hidden opaque/unknown backing must still block the shot. Positive mounted
concrete/metal declarations now use that same bounded policy rather than an
unconditional material-code rejection. The loss formula is unchanged and
approximate. The combined 1,328-test workspace, normal PLAY build, Dust II
probe/sim and ordinary ten-player Nuke render pass. Both CI checks passed;
PR #311 merged the connected-cover increment.
Early brush-only controls passed a 16-unit Inferno frame and 8-unit Nuke
concrete/metal panels at 64/128 Hz. The later native check below invalidates
the old wood target and replaces that case. Removing ballistic definitions
from otherwise identical valid controls still blocks the shot; these controls
establish functionality rather than measured native damage parity.

Dualies follow-through now reuses the accepted alternating fire receipt for
third-person `muzzle_flash` / `muzzle_flash2` attachments. First-person numbering
and single-barrel fallbacks stay intact. The 1,329-test workspace, normal PLAY,
Dust II probe/sim and two left/right client captures passed before the exit fix.
The Dualies first-person particle is still unsupported: its main layer omits a
radius and the core selects atlas sequences absent from the installed texture.
No replacement radius, sequence or generic flash was invented.

After the owner opened Steam, a real Inferno AK comparison exposed a false
positive in the old wood control: the target stood inside sealed BSP world
space. Native `setpos` warned "into world"; the original front shot stopped.
The installed BSP confirms the exit and target occupy SOLID leaf 387.
Ballistic exits now consult an immutable, validated copy of the world BSP
partition. The old lane becomes a blocking regression; positive controls must
put both players in nonsolid leaves. Brush/movement geometry and the approximate
damage formula remain unchanged. All 1,332 workspace tests and installed
positive/opaque and sealed-void controls pass at 64/128 Hz. A replacement
4-unit Inferno wooden post has nonsolid space on both sides; two ordinary native
AK shots confirm passage from both sides. The native reverse eye settled lower
than the planned height, so this establishes passable geometry, not matched
damage. Native damage/thickness parity remains unfinished. The final normal Release build,
immutable Dust II probe/sim and inspected ten-player Nuke client capture pass.
The strict repeat capture retains protected userdata and binary hashes; the
first capture caught only Steam changing its AppInfoChangeNumber metadata.
Both CI checks passed and PR #312 merged.

A focused native scope comparison at 1600x900 and 1920x1080 confirms a
stationary opening about 84% of viewport height, unchanged by HUD scaling
0.85 versus its clamped 0.95 maximum. The client quarter-arc layout exposed
about 98%. A measured 0.86 mask-layout factor corrects that excess while
preserving the centered aim ray, zoom/FOV and existing blur. This is a bounded
presentation fit; native movement-dependent vignette deformation and exact
blur/refraction parity remain unmeasured. The resize-in-session 1080p capture
was unusable and excluded; the admitted 1080p control launched at that size.
All native sessions restored twelve exact preference snapshots. The final
client still measures 758px at 900p, with black outer edges and a centered reticle.
All 1,332 workspace tests and the normal PLAY build pass; immutable Dust II
probe/sim passed before the outer-bar-only seam correction. The final strict
client capture preserves protected userdata and executable hashes. A preceding
capture flagged two live Steam-file changes; that failed receipt is retained,
without restoring Steam files or attributing the changes to this UI correction.
Both CI checks passed and PR #313 merged.

Penetrated pellets now recheck their existing damage budget at the player
contact: range falloff after an admitted exit can no longer revive an exhausted
pellet through the shared one-health minimum. Normal direct hits and the
approximate cover coefficients are preserved. The compatibility policy salt is
v5. All 1,333 workspace tests, the normal Release build and immutable Dust II
probe/sim pass. The inspected ordinary ten-player Nuke capture retains protected
userdata and binary hashes. Both CI checks passed and PR #314 merged.
One ordinary original AK shot through
the valid four-unit post now has readable side-on labels: thickness 10.3cm and
lost damage 7.8, with entry damage 36 at 3.2m. The observer camera moved only
after the shot. This is a concrete native sample, not a general loss law.

The same post has genuinely selected M4A4 and AWP references: 33/7.3 and
115/20.1 entry/lost damage. PR #315 merged after green checks, but its AWP
synthetic fixture incorrectly used power 3 instead of the mounted 2.5, so the
reported 0.5-damage fit was overstated. The follow-up corrects that parameter
and fits the same practical curve across all three guns: prefactor 0.925 gives
loss errors below 0.6. A separate 30-degree AK shot through 11.9cm wood reports
7.9 loss; the corrected curve predicts 8.04. Every crossed face still must
match W/0.9/0.6. Other material and geometry guards remain intact; thicker
cover remains an extrapolation. Policy salt is v7. The installed-data fixture
now checks weapon powers as well as actual firing/opaque controls. Correction
checks pass: all 1,334 workspace tests, installed Inferno firing/opaque controls
at 64/128, normal PLAY and immutable Dust II probe/sim. Both CI checks passed;
PR #316 merged. See
[calibration and boundaries](research/thin-wood-loss.md).

The Dualies presentation now retains each shot's accepted barrel through a
multi-shot render batch. Both posed attachments remain available; each queued
tracer and live muzzle card selects its own side instead of following the
final shot. A real capture/effects/draw regression covers three shots in one
batch, first/third person and an empty following frame; the tracer draw also
checks distinct origins with current-collision clipping. All 1,335 workspace
tests, normal PLAY, immutable Dust II probe/sim and the inspected installed
left/right firing captures pass. Protected userdata/binary hashes remain
unchanged. CI is pending. The exact recovered June 2015 particle file retains
the same missing radius, incompatible atlas sequences and unsupported trails
as Legacy, so no first-person particle substitute was added. Dynamic scope
vignette/edge blur and exact native thick-cover losses remain unqualified.

The owner launcher/profile has not been replaced.

October 23 recovery instruction: an hourly T3 scheduled check resumes approved
unfinished work after an accidental early stop. Newer owner pauses, planning
discussions and active playtests take precedence. Those no-op checks must stay
silent; never bury the owner's testing instructions with repeated status posts.
Do not duplicate active implementation, builds, CI or delegated work.

Accepted for now: airborne weapon presentation (explicitly stop polishing),
Molotov visuals/damage, positional audio, HUD scale, visible loading progress,
and improved bot movement. Do not reopen these as general research projects.
Preserve one launcher/F8 workflow and latest owner profile in
playtest-progress-20261022-f83cfba/userdata. No unsolicited new testing handoff.


Status: **resumed October 21 for continuous work until every approved item is
finished or the owner explicitly stops it**. Root owns integration/builds/CI/staging; parallel source
work is authorized. VISION defines the final target. The owner explicitly
requires faster playable progress and proportional verification.

A partial pass across categories is not completion. Keep the ordered queue
active after builds, merges and staging. Do not stop because every category
received some work; track unresolved player outcomes and finish them.

October 21: #272 is merged and staged through the same desktop launcher. It
corrects shot pitch/mix and listener-local attacker head cues, plus interrupted
airborne easing. Quiet-walk input and weapon speed scaling match existing
native measurements; saved Shift=crouch and Ctrl=walk are preserved. Original
2015 waveform recovery is now staged with #273: 67 private original files,
35 selected weapon modes and revolver fallback. The same delivery includes
prop-loading allocation/conversion cleanup, hidden-receiver decal exclusions
and bot replanning after combat displacement. Installed arsenal firing/reload
checks passed at 64/128 Hz, and bounded ten-player Nuke remained playable.
Fire layer isolation traced the remaining hard floor contacts to the blended
center/fancy layers. A provisional wider field-fire depth band now improves
the flat/ramp controls without changing smoke, disabled-depth layers or sim
coverage. That field-fire correction is merged and staged with #274; workspace,
Release, probe/sim and CI passed. The earlier padding/feather/underlay-omission
experiments were reverted. The next bot pose candidate keeps the airborne
clock moving across held shot/stance events, preserving discrete event timing
and movement physics. It is merged/staged with #275 after CI, Release and
probe/sim; the ordinary ten-player bot render remained playable. Continuous
off-route navigation recovery also passed its 64/128 Hz regressions, Release,
probe/sim and ordinary match rendering (#276), preserving legitimate airborne
and uncovered-gap traversal. The combined F8 panel is rendered and readable.
**October 21 evening correction:** the owner did not request a playtest or a
pause. Continue the remaining ordered player outcomes overnight. Do not create
an owner-testing dependency, prepare another playtest handoff or stop after a
build, PR, merge or first pass. Improve the testing tools as authorized; arrange
a testing pass only when the owner requests one. Existing shipped increments
do not establish completion of weapon presentation/audio, positional audio,
fire in motion, loading responsiveness, combat effects or bot behavior. Keep
working through those outcomes with useful implementation and proportional
checks. The owner will interrupt when they want to test.

## Active batch and accepted order

1. **Easy playtesting first:** one double-click entry point and an in-game
   playtest panel. Switch weapons/early knives, solo/combat and maps inside
   the same process; reuse the current match when possible. No numbered folder
   of separate shortcuts, repeated restarts or separate knife launches.
2. **Quick movement/presentation pass:** fix airborne weapon jolts/pauses and
   quiet-walk speed where wrong. Jump/landing gameplay itself feels good;
   preserve it. Do not spend hours researching small finishing touches.
3. **Classic weapon sounds and hit feedback:** use the owner's old-gun video
   as a private reference for the whole arsenal, suppressed/unsuppressed modes,
   and fix AK head-feedback audibility while preserving nearby body/M4 cues.
4. **HUD:** reduce the oversized overall HUD and kill feed; correctly anchor
   the visible original kill feed at the top right.
5. **Positional audio:** improve wall/floor footstep attenuation and vertical
   separation, especially Nuke.
6. **Molotov visuals:** remove obvious surface triangles and flicker.
7. **Loading/menu responsiveness:** speed slow match loading and transitions.
8. **Weapons:** a practical pass across every gun's firing/handling; preserve
   accepted AWP and existing recoil/zoom results.
9. **Combat effects:** material impacts/decals and dropped-weapon behavior.
10. **Bots:** improve awkward movement/airborne animation/navigation recovery.

The owner accepts grenade throws and knives as **good enough for now**. Move
on; do not keep polishing them. Nuke windows and Train version pictures are
believed fixed; reopen only for an actual regression, not a fresh map audit.

## Delivered overnight

PR #270 is merged. The desktop **CSGO Rewrite Playtest** opens a single match;
F8 changes weapons/knives, solo/ten-player sessions and maps inside the game.
It also includes classic shots, the lower local AK shot mix, compact HUD/feed,
airborne easing, footstep attenuation and loading work. The original profile
is preserved. PR #271 also merged and is staged through the same launcher:
fire intersections, bot jump/portal recovery, material decal exclusions,
dropped-gun bounds and parallel prop-lighting queries. One installed Dust II
PLAY comparison reduced world/prop preparation from 45.1s to 17.7s with identical
geometry/material counts. These increments do not complete the queue: walking,
remaining airborne pauses, hit-feedback audibility, fire card edges, fuller
weapon handling/combat presentation and bot behavior remain active as relevant.

## Delivery policy (explicit owner correction, October 20 evening)

Work in useful combined batches. Implement, run focused checks plus one combined
build/test pass, inspect ordinary play where relevant, then move on. Do not
repeat full Release builds for every small knob, launch exhaustive native
matrices, proliferate diagnostic helpers/reports or turn uncertainty into a
research project. Reasonable first passes are welcome; perfection comes later.
Keep clean-room/content/player-data protections and sim authority. Record real
limitations honestly without engineering caveats in player-facing descriptions.
Continue after a merged increment until interrupted or this approved queue is
finished; then discuss further scope rather than inventing expansion.

## Current direction: October 18

The overnight queue is superseded by this feedback. Nuke and old Dust II
performance/lighting passed the owner's test; Inferno and old Inferno are largely
accepted, and Mirage performance is good enough for now despite some variation.
Further general FPS optimization is **low priority**. Preserve these gains.
Core-map support with ten players and acceptable performance is still **high**.
Do not present this acceptance as proof of steady above-300 FPS on every map.

**High priority:**

- **Classic maps and playability:** prepare the correct 2015-era core map set,
  including old Cache/pre-remake Nuke and appropriate versions of the other
  requested maps. Verify historical identity, dependencies, lighting,
  navigation and ordinary ten-player matches. Broaden coverage to Cache,
  Mirage, Cobblestone, Vertigo, Train and Overpass. Fix failed ladders and blocked
  intended passages across maps. Nuke vent ladder and Mirage ladder-room/window
  access are examples, not separate outcomes. Preserve working doors, vents,
  heaven/palace ladders and accepted maps. Retain newly reported material
  defects within this map pass.
- **Movement feel:** correct overly fast walking and investigate crouch/backward
  speed against original behavior. Do not change accepted input sensitivity or
  responsiveness without evidence.
- **Viewmodel finish:** keep accepted grounded gun/utility motion; restore the
  original airborne and landing response instead of a frozen pose.
- **Combat hit audio:** investigate inconsistent head/body/helmet/armor cues
  across guns and maps. Some cues demonstrably work (M4A1-S on Inferno); trace
  the existing behavior and original audibility instead of inventing sounds.
- **Reliability:** investigate the grenade-practice unresponsive/failed launch.
  Logs are retained; its cause is not established.

**Medium:** footsteps through walls and vertical positional audio; Molotov
visible triangles/flicker (to rise after current high work). Existing decoy
cadence remains unresolved despite accepted bounce sounds. Other older work
retains its prior band unless the owner changes it.

**Low:** further general FPS gains; cosmetic vent/glass break sounds, shards and
fracture embellishment. Functional destruction and traversability remain high.
Old Inferno's alt-mid door squeak remains recorded without an explicit new band.

The owner is home Sunday and can test coherent batches every few hours when
useful. Do not return for tiny frequent tests or repeat an overnight run. State
exact next scope in chat after feedback so the owner can adjust it. Prioritize
building actual fixes with focused checks; no verification-framework or backlog
GUI/audit pass. Preserve settings, content and accepted behavior.

## Objective and working scope

Make the game playable, fun and convincingly like original CS:GO. It is still
laggy, looks wrong and feels wrong. Work on complete player outcomes, not one
ticket per map, screenshot, symptom or test. Feedback examples explain defects;
they do not restrict fixes to those circumstances. One outcome can be split
among agents internally without multiplying the owner's backlog.

The saved categories are rough guidance for **right now**, not a permanent
ranking or exact ordering of small items. Parallel work within current
playability priorities is authorized. Reassess through chat as the game improves.

## Critical: core playability

1. **Performance / FPS.** Preserve the October 18 owner-accepted playable
   performance. The older 300-500 FPS aspiration across all maps with ten players
   is not declared complete, but chasing further general gains is low priority
   now. A newly admitted map still must be playable with ten players; regressions
   remain part of high-priority map playability.
2. **Viewmodel and animation feel.** Consistent original look/walk/run motion
   across guns and grenades; continuous jumping and landing; stable hands
   through firing, reload, death, spectating and respawn. The accepted grounded
   M4A4 feel is a reference, not permission to guess coefficients.
3. **Mouse sensitivity and response.** Match CS counts-to-angle and raw mouse
   feel exactly; sensitivity defaults to 1 for new settings. Preserve saved
   preferences. Good high-FPS response does not solve bad frame pacing.

## High: original look and feel

- **Movement:** walking/running, jumping/landing, bunnyhopping/surfing and
  hit slowdown/view response where present in the original.
- **Weapon behavior:** firing/recoil/recovery, tap/burst timing, secondary
  modes, aiming/optics and moving-sniper blur across the arsenal. Preserve
  accepted AWP and spray fixes; numerical accuracy still needs parity.
- **Lighting and rendering:** correct materials, interior/exterior lighting,
  shadows, reflections and surface presentation across maps. Bright boxes,
  arches and yellow beams are examples of broader faults.
- **HUD:** one coherent original HUD, including overall scale/layout, radar,
  health/armor, ammo, player display, weapon selection, icons and kill feed.
  Component complaints are its acceptance checklist, not separate outcomes.
- **Audio:** original combat hit-cue consistency is high priority now. Footstep
  occlusion/vertical placement is medium. Broader classic mixing and event cues
  retain their existing unfinished status.
- **Grenade throwing:** correct release and trajectories for normal,
  alternate, running and jump throws. The October 18 owner direction
  explicitly targets original CS:GO throw feel and trajectories.
- **Map interactions:** working ladders, doors/sounds, vents and relevant
  breakable objects on all supported maps through authoritative simulation.
- **Classic presentation:** retain the requested legitimate original Valve
  startup movie/audio. It does not displace critical playability above.

## Medium: complete the playable base

- **Grenade gameplay/effects:** smoke shape/expansion, fire appearance/spread,
  HE and utility lifecycle. Preserve accepted ignition, extinguishing and fade.
  Decoy cadence/contact/presence are retained with their lower current urgency.
- **Bots:** natural locomotion/airborne animation and navigation recovery;
  credible movement without moonwalking or sliding.
- **Combat presentation:** original material-dependent impact particles,
  consistent decals, dropped-weapon toss/contact/settling and remaining knife
  inspection/blending details.
- **Menus/settings:** responsive original flows/presentation, loading screens,
  controls UI, match navigation and translated CS2 settings import. Preserve
  accepted persistence/binding/transition fixes. Do not requeue old passes.
- **Inventory/cosmetics/progression:** classic inventory/cases, dated content,
  knife presentation and earned, persisted rewards after core playability.

## Low for now: later scope

- Wider multiplayer, server discovery/hosting, Steam-free social identity and
  remaining round modes. Preserve existing multiplayer seams.
- VR, content packs/mod interoperability, private distribution, Intervention
  and Overworld. Low **now**, not abandoned forever.
- Optional death embellishments/Molotov cloth, extra console/crosshair
  customization and expansion beyond the requested core classic map set. The
  2015-era core map set itself is high priority after October 18 feedback.

## Resolved work and acceptance

October 18 additionally accepts Nuke/old Dust II performance and lighting,
mostly improved Inferno/old Inferno lighting, grounded weapon/utility motion,
working Nuke/Cache doors and Nuke breakable vent covers, and restored decoy
bounce sounds. Airborne motion, particular ladders, decoy cadence and named
material defects remain open; do not erase the owner's corrections or requeue
the accepted parts as entirely missing.


Do not keep resolved work active because an old ledger says open. The owner
explicitly confirms **AWP 10-round ammunition and bolt sound are resolved**,
along with tested zoom cycling, cooldown and bullet marks. Mouse-side-button
binding works; keyboard persistence and HUD rebinding labels previously passed.
Menu overlap, black transition flashes and the broken Escape/Pause sequence
passed. Existing ragdolls are acceptable. Preserve results and reopen only on a
real regression. All-surface decals and moving-scope blur remain separate scope.

Inspect current implementation before declaring features missing. Deduplicate
before scheduling. Verification alone is not a new feature ticket. Keep checks
proportional and deliver improvements instead of proliferating diagnostic tasks.
The owner's follow-up requires a better balance: finish concrete implementation
and use existing, focused checks; do not let verification infrastructure consume
the work or repeatedly ask the owner to test unchanged behavior.

## Historical input and player data

Raw owner rankings (revision 108) and the original inventory are retained
privately in `C:/Users/shawn/csr-fidelity/priority-board-20261017/` as historical
input. They do not supersede this clarification. Do not restart that GUI.
Preserve the tested player build and latest settings at
`C:/Users/shawn/csr-playtest/feedback-20261017-34ea6f7-staged/userdata`.
