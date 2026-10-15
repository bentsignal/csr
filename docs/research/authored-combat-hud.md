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
