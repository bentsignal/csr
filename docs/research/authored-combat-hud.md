# Authored health, armor and ammunition HUD

The owner requires the original CS:GO presentation rather than approximate
health/ammo text and manually positioned icons. This track follows the authored
kill-feed overlay: load original movies from the player's Legacy install, call
observed public interfaces, and preserve their art, fonts, layout and scaling.
No game assets or authored instructions enter this repository.

## Scope and evidence gates

- Identify the installed health/armor and weapon/ammunition movie contracts
  using file/import/export metadata and public runtime properties/callbacks.
  Initial candidates are `hudhealtharmormodule.swf`, `hudweaponmodule.swf` and
  `hudweaponselected.swf`; filenames alone do not establish their API/role.
- Drive presentation only from authoritative local-player snapshots. Damage,
  ammunition, reload and weapon changes remain simulation/TickInput concerns.
- Reuse the existing transparent HUD worker where practical. The current notice
  surface copies only the top half; any full-viewport extension must preserve
  that existing API behavior and avoid another constantly rendering player.
- Send updates on snapshot/viewport revisions, retain a valid raster between
  changes, and park idle work after required authored animation has settled.
  Bound loader lifetime, timers and owned children; clear on leaving the match.
- Keep current health/ammo presentation until the matching native revision is
  admitted. Missing/unsupported content must fall back per component, without
  disabling independently working notices or erasing usable gameplay values.
- Verify actual changing health/armor/ammo, weapon switch, reload, death/respawn,
  resizing and leaving/reentering a match, including explicit saved PNG review.
  Record frame cost and idle behavior; tests alone do not admit visual fidelity.

## Starting point

The existing kill feed uses one bare transparent Ruffle player, original
`huddeathnotification.swf`, keyed revision/fallback receipts and parked idle
state. Current health/armor/ammo still use Bevy text and extracted icon fragments.
No new runtime contract, rendered improvement or complete HUD fidelity is
claimed at this initial checkpoint. Full Audio settings content remains open
in the menu research/owner playtest ledger; this track does not add dummy rows.

## Installed public movie contract, first observation

A bounded two-stage 1920x1080 runtime observation loaded the three candidate
movies into separate owned, locked-root children of the existing transparent
main UI. Only standard `createEmptyMovieClip`/`loadMovie` and passive public
property inspection were used. It exited 0 with no ERROR/panic and unchanged
owner settings. The saved actual raster shows the original health/armor at the
bottom left and ammunition at the bottom right, still containing authored sample
values because no gameplay setter or initializer had been called.

- `hudhealtharmormodule.swf`: public `updateValues`, `SetStyle`,
  `AmmoStyleUpdate`, `setColorText`, `showNow`, `hideNow`, `setVisible`,
  `onLoaded`, `onResize`, `onUnload`; its actual child is `HudPanel`, not `Panel`.
  Uninitialized bounds were x=0..615.1, y=1018..1083.05 at 1920x1080.
- `hudweaponmodule.swf`: public `updateAmmo`, `switchWeaponName`,
  `weaponFired`, `updateAmmoColor`, `ShowBomb`, `updateKillEater`, `SetStyle`,
  `AmmoStyleUpdate`, `setNumberKills`, `showNow`, `hideNow`, `setVisible`,
  `onLoaded`, `onResize`, `onUnload`, with `HudPanel`.
  Uninitialized bounds were x=1336.35..1925.05, y=874.3..1102.8.
- `hudweaponselected.swf`: separate inventory public `AddPanel`, `RemovePanel`,
  `AddWeaponData`, `AddNextPanel`, `AddNextWeaponData`, `SetWeaponName`,
  `resizeRelative`, and `Anchor`. It is not required to display health/ammo;
  inventory remains a separate fidelity item.

These are runtime property names and primitive geometry, not authored
instructions. Bounds and sample art are not an initialized layout or fidelity
pass. Argument meanings, original initialization/scaling and animation settling
remain evidence gates. Public `Function.length` metadata, if exposed by the
runtime, can establish arity but cannot establish argument meaning.

The existing `Menu::spawn_named` already uses transparent window rendering and
returns the complete viewport. `spawn_overlay` differs only by copying its top
half. A shared full-viewport worker therefore requires no new public renderer
mode and can preserve the existing top-half API for other consumers. Component
rasters must retain independent readiness/revision/fallback; ordinary ammo
updates must not blank valid notice or health presentation.

A separate three-stage observation called only those observed lifecycle methods,
then inspected the initialized roots. It also exited 0 with clean logs and
unchanged settings. `HudPanel.HealthArmorPanel` and `HudPanel.WeaponPanel` are
the actual presentation containers. Initialization queries
`GameInterface.GetConvarNumber("cl_hud_healthammo_style")`; our current shared
default returns 0. Optional public `Function.length` reads produced no scalar
result, so parameter arity is **not** admitted. The actual initialized raster
still contains sample values and unsupported sample bomb/kill-counter art;
these must be replaced/cleared before any production admission.

## Numeric and bar admission

The first experimental variadic setter call is a retained negative: numeric
sample fields did not update, and `updateAmmo(7,53)` tried four `icon-undefined`
exports. Those incomplete inputs are not used in production. Public static
DefineEditText metadata has empty variable names. Named instance placements
identify `Health.TextBox`, `HealthRed.TextBox`, `Armor.TextBox`,
`AmmoCountClip.TextBox`, and `AmmoCountTotal.TextBox` under the observed panels.
Only text/font/placement metadata was inspected; all action payloads were skipped.

Our adapter retains each original field's `getTextFormat`, changes its `text`,
and restores its current and default text formats. A separate immutable8d3
four-stage probe actually rendered73health/41armor/7clip/53reserve, then19/0/0/12,
then cleared both complete roots. Public string reads and actual PNGs agree;
two component-ready and two successful-update callbacks, clean logs, process0
and unchanged owner settings were checked. This admits numeric fields/cleanup,
not the unrelated template bomb, skull, weapon-name or kill-counter art.

A separate original timeline observation establishes100-frame foreground bars:
100 selects62units of fill,73 selects45.2,41 selects25.3,19 selects11.6. Frame1
has zero width/height, no drawn child and an actually empty armor0 raster.
The production adapter selects those original frames, with values clamped to
the observed1..100 range, and keeps the original red trail layer hidden during
steady presentation. Red damage-trail/pulse timing remains unmeasured. No bar
rectangles, SVGs or replacement bitmap art are drawn.

## Scaling limits

An existing native Legacy2023 1600x900 viewmodel capture has a near-white health
100 glyph y=867..888 (22pixels), while our initialized1920x1080 movie has
y=1033..1064 (32pixels). A blanket0.85 multiplier would approximately reconcile
those captures, but their native HUD scaling/safe-zone settings were not recorded.
The already saved primary [March2015 tutorial frame331s](https://www.youtube.com/watch?v=xoatYCUn4tw&t=331s)
has a720p yellow health glyph y=690..710 (21pixels); our **unscaled** initialized
movie normalizes to21.3pixels at720. These observations conflict unless CVars
are accounted for. The notice-row0.85 transform is therefore not blindly applied
to these separate modules: the candidate retains original `onResize` layout.
Compression, unknown original CVars, 2023-versus2015 differences and safe-zone
placement prevent an exact pixel-parity claim.

## Production ownership and remaining gates

The production change shares one transparent full-viewport player with
notices. Three nonoverlapping physical-pixel regions retain independent raster
receipts/fallbacks. Snapshot commands are sent on actual value/life/viewport
changes. A readiness callback alone never hides ordinary values; a successful
matching raster is required. Ordinary value updates retain same-life pixels;
death/respawn/view-target/leave/resize cannot reuse stale pixels. Per-component
load/result failure preserves other components. Only worker failure is global.
Idle admission parks the worker; no second continually rendering HUD player is
created. Unchanged image regions and Node/Visibility values are retained.

Initial native numeric fields and steady bar art are admitted. The bounded
production checks below do not admit every original state: low-health colors, damage
pulse/trail, low/empty-ammo tint, shot/eject animation, grenade/bomb/defuser/burst
subcomponents and weapon-name/kill-counter state remain open fidelity work.
Unsupported sample decorations are hidden, never presented as authoritative
state. Original inventory/radar/money/round HUD work remains separate. Full Audio
settings content remains open; no dummy option rows are added by this track.

The first production92 render admitted the real M4-S magazine20shots?clip0/80,
health100/armor100 and the original death notice in the same transparent player,
with no sample decorations or duplicate fallback. It exited0 with clean logs and
unchanged settings. Its numeric HealthRed visibility override was unadmitted and
made health visibly dimmer than the earlier positively observed numeric template;
that override is removed in the next candidate. Only the separately measured bar
trail is excluded from steady presentation. The92 image is retained as partial
production proof rather than an overall final-visual pass.

Public FrameLabel metadata independently resolves parking: both original
15-frame HudPanel timelines have StartShow at2, **Show at8**, StartHide at9 and
Hide at15. Production callbacks repeatedly report current8/total15. The last
frame is the hidden state, not a universal settling target. Idle and lifecycle
runtime checks remain required, but parking the observed Show state does not
require inventing an animation duration or waiting for the hidden frame.

The first ordinary underhand HE self-damage render on immutable14 is a retained
negative. The actual simulation and native raster changed health/armor100/100
to56/57, and the worker parked, but result callbacks were still at HudPanel
frames2/1/2/6 rather than Show8. The actual screenshot has dim armor and ammo:
the worker had parked during the original entrance animation. The previous
real-kill case had a live notice keeping that same worker active, masking this
idle-path defect. Its process exited0 with clean logs and unchanged settings;
the helper's Show8 gate correctly failed. This is not an overall visual pass.

The correction defers a visible component's success receipt until its original
HudPanel reaches the observed Show8 state. An empty child owned by this adapter
observes frames; authored handlers and timeline actions remain untouched. The
pending receipt is fenced by part identity, generation and the latest revision,
replaced on newer values, and cancelled on clear/load failure. It deletes its
own observer when complete. Unsupported or stuck timelines fail after a bounded
60 observer turns, retaining component fallback rather than running forever.
This guard is not a claimed animation duration. Rust's existing pending-receipt
state keeps the shared worker active until both completion and raster admission.
The corrected actual damage and lifecycle observations follow below. Idle
parking is measured; a controlled frame-cost comparison remains open.

Immutable4ba's exact ordinary HE repeat passed the previously failing gate:
health/armor changed to56/57 through simulation, the native raster shows those
values and matching steady bars, and the automatically selected M4-S shows
20/80. Every visible component completion callback is now Show8/15 before
parking; the hidden grenade-ammo component legitimately reports1/15. Actual
1920x1080 PNG inspection confirms readable steady ammo without the earlier
partial fade, no duplicate fallback or sample art. The process exited0 with
clean logs and unchanged settings. Historical colors and low-health behavior
remain uncalibrated. Later ordinary fire/reload/Pause/new-match observations
are recorded below; a controlled frame-cost comparison remains open.

A newer stamped command also clears only that component's previous result
rejection. Otherwise an earlier failure could park a retry while its owned
Show observer is pending. One behavioral regression exercises failure, newer
pending retry, ignored old-stamp failure, success without raster, and final
matching raster before parking. Other components and load failures are unchanged.

## Production lifecycle evidence

Frozen333 passed that six-test component suite and the launcher build. Its
full workspace/installed-data CI was green. A separate all-workspace-package
RELEASE clean/build and the required Dust II probe/simulation both completed
with `failures: 0`. Immutable executable/source/cwd/hash receipts and original
failed14 receipts remain private; no game screenshots or assets are published.

The normal engine-input five-bot run rendered four actual1920x1080 images:
initial100health/100armor with20/80ammo, ordinary firing19/80, Pause with the HUD
hidden, and Resume with20/79 after reload. The capture requested immediately
when reload completed retained the last valid19/80 raster; the matching reload
raster arrived38ms later and is visible in the resumed image. Authoritative
snapshots agree, and no other component or fallback duplicated the native art.
The worker parked between value changes. The observed frame cadence was
7.973ms mean/10.069ms p95, with77.5ms initial Pause response; these are bounded
engine-input observations, not OS-input, GPU-present or comparative gain claims.

Normal Home startup then exercised Dust II, Pause, Audio, Back, Resume, Home,
and a second match on Inferno. All eight actual images were reviewed. Both
matches rendered100health/0armor with the original empty armor bar and13/52
P2000 ammunition. The HUD hid during Pause/Audio and cleared on Home; both
components admitted fresh second-match rasters. Home readiness was101.7ms.
Only one ready callback per component occurred across both matches: the same
parsed modules were retained, cleared and reused. Whole-client map-loading
memory peaked at12,395.5MiB private and ended at1,485.8MiB; this is not an
isolated HUD memory-cost measurement or a general leak-test claim.

Two existing ordinary underhand Molotov input cases then exercised real
self-death and respawn, with no artificial damage or life changes. The6s image
has actual life1/dead0health and no native health/ammo; the9s image has actual
life2/alive28health/100armor and20/80ammo. Continued fire damage explains the
low respawn health. The image agrees with its9s snapshot, while a later log
records20health after capture. The existing periodic secondary-input fixture
also starts a suppressor transition after weapon selection; that animation is
not a HUD admission. All cases exited0 with clean logs and unchanged settings.
The agent and parent independently reviewed all production lifecycle images.

Remaining fidelity work includes historical color/low-health pulse and trail,
low/empty-ammo tint and ejection, extra native HUD states, live window-resize
behavior, exact historical scaling/CVars, and a controlled isolated cost check.
The menu screenshots also retain known incomplete Audio content (Master Volume
only and an empty microphone panel) and Home placeholder/clipped news content.
Those are open defects, not complete-menu or typography passes. Original
inventory, radar, money and round HUD presentation remain separate work.
