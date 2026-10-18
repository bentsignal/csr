# Current owner priorities

Status: **work resumed by the owner on 2026-10-17**. This is the scheduling
authority. VISION defines the product; older feedback supplies examples and
regression cases. Maintain priorities through conversation. The GUI is retired.

Current owner-approved overnight queue (October 17, before bed): finish the
current lighting/material build, then continue independently through the order
below. Do not stop after one completed item or PR. The owner returns in the
morning; another owner test is not required to continue authorized work.

1. **Performance**, the top goal. Steady 200–300 FPS on the owner's machine
   would be substantial progress. Steady above 300 FPS is sufficient to move
   on, preserving the existing ten-player, across-map and sustained-pacing
   conditions. Do not chase 500 FPS before progressing through this queue.
2. **Viewmodels and animations.** Reuse the motion the owner already tested
   and liked, then make it consistent across weapons and utility. Inspect the
   existing implementation before redoing completed work; match the original.
3. **Map interactions.** Ladders, doors and squeaky sounds, breakable vents,
   firing through doors and visible bullet holes, and normal access to
   apartments. The owner reports invisible barriers at all modern Inferno
   apartment entrances and is unsure whether old Inferno is affected too.
4. **Grenades**, ahead of HUD: throw feel/trajectories and remaining gameplay
   and effect defects, within the existing CS2 throw-feel exception.
5. **HUD**, one coherent original-game presentation.
6. **Audio**, last in this overnight order.

Prioritize implementation and use focused regression checks. The owner's
playtest is the main acceptance feedback; do not consume the night building
verification machinery. Preserve config/content and prior accepted fixes.
The owner will identify obsolete backlog items as testing proceeds; no backlog
audit is part of this work.

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

1. **Performance / FPS.** Long-term target **300–500 FPS on the owner's PC, every
   supported map, with 10 total players (human and/or bots)**, with smooth,
   predictable frame pacing throughout sustained play. A good solo spawn view
   or one optimized Nuke/Dust II case does not complete this outcome. Preserve
   fidelity. Test representative scenes, combat and long sessions; retain worst
   cases as regressions. Support reasonable lower-end playability too. The
   target has not been achieved. For the current queue, the owner's newer
   steady-above-300 stopping threshold takes precedence over chasing 500.
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
- **Audio:** classic sound identity and faithful mix, DSP, distance, occlusion,
  footsteps and animation/event cues throughout the game.
- **Grenade throwing:** correct release and trajectories for normal,
  alternate, running and jump throws. The owner's CS2 throw-feel exception
  remains; assets, UI and general fidelity remain old CS:GO.
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
  customization and broader historical map/content expansion.

## Resolved work and acceptance

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
