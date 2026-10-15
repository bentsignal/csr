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

An existing native Legacy 2023 1600x900 viewmodel capture provides a useful
same-install font-height cross-check without another game launch. A strict
near-white mask of the health `100` has y=867..888 (22 pixels); the initialized
1920x1080 movie has y=1033..1064 (32 pixels). Normalizing the latter to 900 pixels
and applying the observed shared `hud_scaling=0.85` yields about 22.7 pixels.
This supports investigating the missing per-module HUD scale, as with notices;
it does not establish an exact transform, safe-zone setting, native cvar value
for that capture, or 2015 reference fidelity. Original layout must be measured
and preserved rather than inventing offsets from these two images.
