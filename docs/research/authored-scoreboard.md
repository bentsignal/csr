# Original scoreboard adapter (October 22 source pass)

The client now loads the player's `resource/flash/scoreboard.swf` in its own
bare-root Scaleform worker. Tab, rebound scoreboard controls, match-over display
and the existing privacy-filtered `scoreboard::board` snapshot are retained.
The original row typography/background/team headings/score fields are populated;
no replacement scoreboard is drawn by the adapter. Existing Bevy presentation
remains available until a successful movie receipt and fresh raster agree on
snapshot revision and viewport. Failure or unsupported content retains it.
The kill-feed worker, opacity and scale are untouched.

## Evidence and declarative contract

Offline reads of the installed Legacy movie inspected only public SWF
PlaceObject2/3 instance names, DefineEditText metadata, ExportAssets names and
FrameLabel tags. No ActionScript bodies, action records or bytecode were read.
Existing repository references establish this as the retained classic movie;
no new native/game/runtime execution was performed by the child.

- Root: `ScoreBoard.InnerScoreBoard`; outer labels include `StartShow`,
  `StartHide`, `Hide`.
- Inner labels: `Standard`, `StandardTenPlayer`, plus halftime/overtime and
  sixteen-player/co-op variants not used by this first pass.
- Inner fields: `ServerName.ServerName`, `CT_TeamName.CT_TeamNameText`,
  `T_TeamName.T_TeamNameText`, `CT_Score.CT_Score`, `T_Score.T_Score`.
- `CT_Scoretable.CT_ScoreRow_0..11` and matching T table hold original rows.
- Rows expose `Money`, `Kills`, `Death`, `Assists`, `Score`, `Mvp`, `Ping`,
  `Player_Name.Player_Name`, `Bomb`, `defuser` (CT), `Skull`, `DeadBG`,
  `PlayerOutline`, and optional cosmetic/rank/avatar children.
- Name timeline labels: `Normal`, `Dead`, `Local`, `DeadLocal`; row label `Up`.

The adapter freezes owned movie timelines, selects the observed standard layout,
writes literal text while preserving authored TextFormats, hides unused rows
and sample/cosmetic panels, and fits the original background to the viewport.
No function signatures from the game are guessed. There is no synthetic score,
ping, assist, MVP, rank or award data. Enemy money and equipment are absent
before the snapshot crosses into the worker. Team scores are the same values
as the working fallback. Rosters above twelve per side use the fallback.

The worker parks when hidden or when its current snapshot is admitted. Ordinary
kills/deaths/money updates retain the last admitted raster while its replacement
is rendered, so they do not briefly switch back to the custom layout. Resize,
viewer/team changes and changed match headings invalidate that retained raster.
A per-receipt raster fence excludes a pre-callback readback.
Loading/presentation timeouts retain the functional fallback. The developer
switch `CSR_NO_CLASSIC_SCOREBOARD` selects that fallback explicitly.

## Root verification and limits

Root compiled the adapter, passed its focused regressions and workspace checks,
and inspected original-movie captures with both small and full rosters. The CT
heading is above its roster and the board keeps a consistent maximum display
width; decorative layout is still a practical first pass. Sources cover original
row replacement/hiding, viewer/team invalidation and revision/viewport/raster-fence
admission. Natural Tab interaction and all optional panels are not qualified by
the screenshot controls.

The observed declarative clips must survive authored first-frame initialization
in the installed/private variant. If they do not, the adapter rejects it and
the fallback remains; inspect the runtime receipt before changing field paths.
The bounded pass does not implement voting, awards, rank progression, round
history, context menus, Steam profile avatars, or spectator rows. Existing
server/local simulation authority and player-data ownership are unchanged.

Root's parallel HUD capture found generic user-head avatars in the prior pass.
Offline extraction/viewing confirmed the installed scoreboard team avatar PNGs
are CT/T emblems, while `teamcounter_botavatar.png` is a pale generic bot image.
The top tiles now use `materials/panorama/images/icons/scoreboard/avatar-ct.png`
and `avatar-terrorist.png` for bots and local players without Steam avatars.
Only asset paths are committed; inspected images stayed in the private temp
folder and no game art entered the repository.
