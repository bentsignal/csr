# Current owner priorities

Status: **owner morning playtest accepted on 2026-10-18**. This is the scheduling
authority. VISION defines the product. The complete newest observations and
corrections are in [the October 18 playtest record](PLAYTEST-2026-10-18.md).
Maintain priorities through conversation; the GUI is retired.

## Approved work order: October 18 follow-up

The owner explicitly approved implementation in this order:

1. **Classic map availability and playability**, including old Train and all
   requested core historical versions, traversal and material/lighting defects.
2. **Movement and viewmodel finish**: walking/crouching/backward movement and
   airborne/landing response while preserving accepted grounded motion.
3. **Combat hit-sound consistency** across weapons, armor states and maps.
4. **Grenades**, including original CS:GO release/trajectory/feel and remaining
   behavior/effect defects. The owner revoked the earlier CS2 throw-feel target.
   Use clean-room original-game observations and old smoke lineup videos on the
   matching old maps as end-to-end checks; cover other grenades separately too.

The owner retested bot matches through another launch route and considers the
reported bot/mode availability issue resolved, probably due to the weapon-motion
shortcut. Do not spend a new track on it without a normal-menu reproduction.
Keep ordinary ten-player map support as an admission requirement.

This is authorization to start; do not wait for another scope confirmation.
Prepare coherent daytime playtests rather than tiny frequent checks.

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
