# Whole HUD composition candidate

Source-only implementation based at `a9cd2e8eab75afeddf3a626c939f996d278facba`.
The owner wants one coherent original HUD outcome. This change addresses its
common composition rules together. **It is unbuilt and unrendered; whole-HUD
acceptance and pixel parity remain open.** Root reserved Cargo/native/GPU access.

## Evidence and inference

Retained private evidence was read directly, including the actual PNGs:

- `post-playtest-20261016/controls-hints-awp-R3A1/ordinary-awp.png`: current
  1920x1080 HUD with effective keyboard/mouse labels, crowded individual utility
  rows, small radar, large original health/ammo and isolated top avatar.
- `post-playtest-20261016/hud-geometry-reference-5a06cf3/`: original tutorial
  frames254s and260s, the prior research report, installed declarative metadata
  and pixel measurements. The source is 3kliksphilip's recorded
  [2015 HUD tutorial](https://www.youtube.com/watch?v=rYVZezwknL8&t=260s).
  Its settings frames record HUD Scale0.85 and, in an earlier scene, Radar HUD
  Size1.24. The scenes have different unstated settings; they are not a matched
  before/after pair. The metadata records all relevant movies'1280x720 stages.
- `authored-killfeed-20261015/reference-20150311/original-331s.png` and
  `row-contract-dfc9beb/called-1.png`: actual old USP-S/headshot footage versus
  current two-row fixture. The original source is the
  [March2015 grenade tutorial](https://www.youtube.com/watch?v=xoatYCUn4tw&t=331s).
  The retained public row contract independently supports the existing
  approximately37.36px1080p row pitch. It does not support another blanket shrink.
- Both `docs/reference/` PNGs: classic menu/case references, useful for era and
  presentation identity, not gameplay HUD coordinates.

Prior measured health100 ink is67x33 at x52,y1033 versus original57x28 at
x44,y1040. Armor100 is66x32 at x289 versus original57x28 at x246. A single
0.85 transform about the bottom-left corner predicts x44.2/y1040.05 and armor
x245.65, with28.05/27.2px heights. These are **predictions, not new renders**.
The previous adapter only called `onResize`: neither bottom module received
the HUD scale already applied to notice rows. This strongly constrains the
missing composition step, but does not prove every historical host CVar rule.

## Implementation

`hud_layout.as` owns the missing bottom-corner composition. It resets our
module container before calling the original public resize, then applies
`hud_scaling` about the viewport's bottom-left or bottom-right corner. It never
fits mutable content bounds, adjusts individual digits or scales the raster
again in Bevy. Repeated resize cannot accumulate transforms. Original authored
methods/timelines are untouched. Show8 receipts, capture fences, per-component
fallback, player-life gating and parked worker ownership are retained.

`client::hud_layout` expresses Bevy geometry/fonts in the same720-high space,
using viewport-height units instead of desktop logical pixels. The default
scale comes from the same `scaleform::HUD_SCALING` constant as the engine answer.
Already-rasterized surfaces instead cancel both DPI and Bevy UiScale, retaining
their exact integer physical crop dimensions even at odd window sizes.

The common layout includes:

- Independent persisted `radar_hud_size` / `cl_hud_radar_scale`, with a neutral
  fallback of1.0 for new settings and files missing that field. R2 established
  that the1.24 reference is an unmatched edited scene, so it cannot justify an
  ordinary default. At1080 the revised fallback predicts249.9px, near the
  retained ordinary reference's250px circle; this is not a native-default or
  1:1 claim. Explicit saved values, including1.24, are retained. Existing map
  zoom, rotation and saved input settings remain independent. Markers share
  the viewport transform, and money/buy indication flow below it.
- Weapon selection grouped by real simulation selection slots. Utility gets
  one horizontal row and one effective binding label. Stable icon planes and
  approximately82px row pitch replace variable-width icon+label flex rows and
  approximately38px pitch. Default three-slot pistol/knife/utility tops predict
  y784/866/947 at1080, near the original260s observations y782/862/950.
  Weapon images fit uniformly in a128x32 authored-unit plane; wide and narrow
  silhouettes retain their aspect ratio. Active localized weapon names return.
  Full accepted keyboard/mouse alternative labels still refresh the signature.
  R3 explicitly disables soft wrapping and anchors their auto width at the
  fixed right edge, so long hints extend left. The active full weapon name is
  also nonwrapping, inside a descendant-clipping vertical band from authored
  y32 to48, ending at the next row's hint anchor. Actual font fit remains a
  render gate; stored Node dimensions do not prove disjoint glyph bounds.
- One common top strip: approximately46x51px player allocations, a narrow central
  timer above the two scores, and teams anchored immediately beside it. Team
  changes now invalidate the tree. This corrects the earlier large central gap
  and horizontal score/timer/score arrangement. Tile art remains approximate.
  R2 identified that the nominal260s DM tile is about43x28px, unlike these
  allocated46x51px boxes. ImageNode::Auto contains the avatar within its box;
  allocation is not image ink. No mode-specific replacement is justified yet.
  DM geometry and competitive height remain separately unverified, and R3
  leaves these allocations open for root's actual mode/contain comparisons.
- Death notices start below that strip at approximately66px1080, rather than
  the isolated movie's25px anchor. Native row scale/pitch and installed weapon
  and headshot exports remain. The compatibility background is fitted to the
  text+tab span in the correct parent coordinate space, using the existing
  right inset on both sides instead of preserving excess leading allowance
  from a second plain-text measurement. It resizes only original background
  and highlight clips. This spacing policy requires actual installed renders.

The numerical constants above are derived candidate geometry constrained by
the cited old frames, not recovered engine constants. No CS2 reference, new art,
leaked code or Source SDK implementation was used. No extra Scaleform worker,
render-frame gameplay or public game assets were introduced.

## Source regressions, all UNRUN

- `scaleform/tests/hud_layout.rs` compiles our actual adapters into asset-free
  synthetic SWFs and checks real MovieClip bounds: corner anchoring, three
  repeated layouts, unrelated content bounds, odd/wide viewports, first notice
  placement and nonuniform TextPanel-to-row conversion. The three existing
  background/highlight clips must agree through changing spans/repeated fit.
- `hud_art` runs the actual ECS inventory update through ordinary keyboard
  rebinding, Mouse4, a persisted Control R plus all five mouse alternatives,
  unbind and unchanged-frame caching. It checks full strings, explicit NoWrap
  and right anchors, unchanged authored slot allocations and the name's
  descendant-clipping policy. Team/death appearance properties refresh in DM
  and competitive. MinimalPlugins does not run text shaping, UI layout or image
  extraction: these assertions do not prove glyph fit, clipping pixels or
  native tile geometry. A separate grouping regression covers all five slots.
- `combat_hud` runs the actual display system with cropped regions at normal
  and odd physical sizes, non-unit DPI and UiScale. Every coordinate/extent
  must resolve back to its exact physical source rectangle.
- `hud_layout` checks Bevy's viewport-unit resolution and raster pixel mapping;
  `settings` covers the1.0 new/missing-field fallback and explicit saved sizes
  (including1.24), independently of zoom, rotation and input settings.

`git diff --check` passed. Cargo, rustc, rustfmt, tests, builds, interpreters,
native/UI/GPU operation and CI were not run. Root must execute source checks,
then clean/build under its exclusive slot from the real integration cwd and
review an immutable executable's actual renders before any merge/acceptance.

## Remaining whole-HUD acceptance

This is one candidate toward the whole outcome, not acceptance of individual
complaints. Exact safe-zone/CVar handling, historical artwork, fonts/color,
radar frame/location/objectives, money background, player identities/dead/local
states, weapon selection decoration/fade and utility/bomb ordering are still
incomplete. Bevy inventory/avatar/fallback art still comes from the existing
Legacy Panorama paths; the validated classic health/ammo/kill-feed movie art
is retained. Missing classic kill-feed icon exports still use per-row fallback;
this change does not establish complete icon coverage.

Root's actual images must cover competitive5v5 and deathmatch; short/long and
rebound labels; full utility and bomb; local/nonlocal/mixed kills with every
supported icon and headshots; health/ammo changes and burst indicators;
death/respawn/spectating; Pause/Resume/Home/new match; real resize and DPI.
Compare original references at the same resolution, retaining unknown-CVar
limits. In particular inspect the new notice inset against installed text-field
bounds and avoid accepting a fallback as a successful authored row. Existing
black-flash/no-overlay, binding persistence, Show8/raster-fence and idle-parking
passes must hold. No frame-time or FPS improvement is claimed.
