# Scaleform spike: playing CS:GO's own menu movies with Ruffle

Track C feasibility spike, 2026-10-11. Question: can we show the classic
Scaleform menu (VISION §7, `docs/reference/`) by running Valve's original
movies from the player's CS:GO Legacy install in
[Ruffle](https://github.com/ruffle-rs/ruffle), instead of rebuilding the
layout by hand?

**Decision: yes, option (a).** With three small Ruffle fixes and a bridge
that plays the engine's part, the install's `mainmenu.swf` and its 25
dependent movies render the 2018 home screen at 1920x1080 with the right
layout, fonts, art, text and interactivity. Outside the two areas that are
not Scaleform content (the embedded blog web page and Steam avatars), the
frame differs from the reference by a mean of **6.0 / 255 per channel**. We
integrate Ruffle as the menu layer; a hand-built native layout (option b) is
not needed.

Nothing from the install is committed. The spike tool reads the movies at
runtime; renders go to a scratch folder outside the repository.

## Result

`tools/scaleform-spike` (standalone crate, outside the workspace) boots the
menu headless and saves a frame:

```text
cd tools/scaleform-spike
cargo run --release -- render D:\scratch\menu.png 90
cargo run --release -- diff D:\scratch\menu.png ..\..\docs\reference\menu-2018-home.png
```

Difference against `docs/reference/menu-2018-home.png` (1920x1080, per
channel, 0-255):

| Stage of the spike | MAE | RMSE | PSNR | pixels off by >32 |
|---|---|---|---|---|
| Background only (movies loaded, no engine data) | 131.8 | 148.5 | 4.7 dB | 90.8% |
| Layout scaled to 1080p, every engine call `undefined` | 24.5 | 55.4 | 13.3 dB | 20.5% |
| Profile and friends data | 22.6 | 52.1 | 13.8 dB | 18.6% |
| All home-screen data and engine events (final) | **19.3** | 47.1 | 14.7 dB | 12.4% |
| Final, masking web content and avatars | **6.0** | 14.5 | 24.9 dB | 3.1% |

The mask (`SPIKE_DIFF_MASK`) leaves out five rectangles that are not
Scaleform content: the blog web page inside the News panel
(734,392)-(1582,1053), the `net_graph` text at the bottom right, and the Steam
avatars (profile and the two friend columns). Per-region MAE on a 4x3 grid
(final, unmasked):

```text
  7.44  11.83   5.14   5.62     top: background, top bar, profile, Overwatch, store
  8.60  12.25  11.31   8.15     middle: friends list, News panel frame
 10.23  52.55  73.58  24.64     bottom: News panel web content dominates
```

### What matches

Measured by overlaying our frame on the reference; everything below lines up
to the pixel unless noted:

- **Top bar:** full width, home icon, PLAY, INVENTORY, WATCH, AWARDS,
  OPTIONS, power; tab separators and the Stratum2 Bold labels. The PLAY tab
  opens the original dropdown (FIND A GAME, PLAY WITH FRIENDS, ..., OFFLINE
  WITH BOTS, WEAPONS COURSE) when clicked.
- **Background:** `background.swf`'s vignette and faded CT emblem, scaled to
  cover the screen.
- **Profile card:** "Prime Account" badge, name, six coins and the right
  arrow, commend counts 13/12/12, "Private Rank 3", the XP bar at 70% with
  "Rank 4 + Drop", the Gold Nova III emblem, and the 439 wins trophy.
- **Friends panel:** tab badges 50 / mail / 23 / 10, the "Friends" heading
  with the add-friend button, the two-column list with name colours (green
  for in-game, blue for online), statuses, the scrolling rich-presence
  marquee for in-game friends, and the scroll arrow.
- **Overwatch panel:** title, "Overwatch F.A.Q." link, DOWNLOAD EVIDENCE,
  "You have a pending case to review.", "Case ID: OWC#XXXXXXX023".
- **Store carousel:** the StatTrak music-kit offer tile with -35% badges,
  "View Your Offers" and "Offers Available Till: 1 Day 1 Hour 31 Mins 19
  Secs"; the Horizon Case double tile with its hint text and "View On
  Market"; the Name Tag tile with £1.45; five page dots and arrows.
- **News panel frame:** title bar with browser buttons.

### What does not match (yet)

| Gap | Cause | Fix |
|---|---|---|
| News panel content | It is a web page (blog.counter-strike.net) that the engine rendered with Chromium and handed to Flash as an image (`Blog.InitChromeHTMLRenderTarget`). Not Scaleform content. | Render our own news (GitHub releases) to an image in that slot. The page's scrollbar is engine-driven (`UpdateHTMLScrollbar`). |
| Avatars | Steam pictures; the spike serves a flat tile. Friend avatars also stay hidden in Ruffle (below). | Our server's player avatars. |
| Background clouds | `background.swf` animates for 840 frames; the reference was captured at a different point. | None needed (animation phase). |
| Text edges | Ruffle rasterizes glyph outlines with its own anti-aliasing; Scaleform used its own glyph cache. Visible as 1 px fringes in the heat map. | Small; revisit when tuning. |
| Featured flair coin over the avatar, the store search icon, "Beta" after "Prime Account" | Engine values the spike does not supply yet (`GetFriendDisplayItemDefFeatured`, a store-search button condition, the 2018 token text). | Bridge data. |
| Exact coins and music kits | The reference's items are not identifiable at this size; the spike uses close stand-ins from the install. | Real inventory data. |

## How CS:GO hosted its Scaleform UI (facts from the shipped movies)

- **Root movie.** `mainuirootmovie.swf` (front end) and `gameuirootmovie.swf`
  (in game) define the helpers (`_global.ConstructString`,
  `AutosizeTextDown`, `FormatSecondsToDaysHourString`, ...), the
  `Lib.ResizeManager` / `NavManager` / `TintManager` singletons, and
  `_global.ElementLoaders`, a table from element name to movie and level.
- **Loading an element.** The engine called
  `_global.RequestElement(name, gameAPI)`. `MainMenu` loads `Background.swf`
  into level 1 and `MainMenu.swf` into level 31 through
  `LoadExternalElement`, which attaches `gameAPI` to the loaded clip, calls
  `gameAPI.OnLoadFinished(clip, UISlot)` and then `clip.onLoaded()`.
  Other levels: StartScreen 2, Legals/credits 12, SinglePlayer 15, Lobby 16,
  LobbyBrowser 17, OptionsMenu 21, HowToPlay 24, MedalStatsScreen 25,
  LeaderBoards 30, CallVote 44, PlayerDetails 98, OverwatchResolution 99,
  PauseMenu 100 (PauseBackground 3), MotionCalibration 125, ItemPickup 200,
  message boxes from 500, LoadingScreen 1000.
- **Engine objects.** `_global.GameInterface` and 29
  `_global.CScaleformComponent_<Name>` objects (Blog, CompetitiveMatch,
  EmbeddedStream, Fantasy, FriendsList, GameTypes, IME, ImageCache,
  Inventory, ItemData, Leaderboards, Loadout, MatchDraft, MatchInfo,
  MatchList, MatchStats, Medals, Missions, MyPersona, News, Overwatch,
  PartyBrowser, PartyList, Predictions, SteamOverlay, Store, Streams,
  Teammates, UGC), 545 distinct methods across all 117 movies. The home
  screen calls about 70 of them.
- **Engine to movie.** Events are plain functions called by name on the
  movie root (`_global.MainMenuMovie.ScaleformComponent_Store_PriceSheetChanged()`);
  parameters are read back with
  `gameAPI.GetScaleformComponentEventParamString(event, key)`. Keyboard
  input arrived as `_global.KeyDownEvent(key, vkey, slot, binding)`; mouse
  input was native Flash.
- **Images.** Loaded by URL relative to `resource/flash/` (files in
  `pak01_dir.vpk`, e.g. `econ/status_icons/skillgroup9.png`), plus engine
  schemes `img://avatar_<xuid>`, `img://inventory_<id>`,
  `vmt://stickerpreview_<id>`.
- **Return conventions matter.** Ruffle, like Flash, ignores
  `_visible = undefined` and `undefined == 0` is false, so returning
  `undefined` shows ban warnings, the licence upsell and the update bar.
  Examples of the exact types the movies test: `IsVacBanned` is a number (0
  none, 1 VAC, else game ban); `GetLicenseType` must be `"purchased"`;
  `IsNewClientAvailable` a boolean; `GetFriendStatusBucket` one of
  `PlayingCSGO` / `Online` / `AwaitingLocalAccept` / `AwaitingRemoteAccept`;
  competitive rank 9 is Gold Nova III; `GetActiveSeasionIndexValue` (sic)
  -1 means no operation.

## Scaleform features Ruffle lacks, and what the spike does instead

| Scaleform feature | Used for | Spike |
|---|---|---|
| Native engine objects (`GameInterface`, `CScaleformComponent_*`, `gameAPI`) | All data | A bridge shim in our own ActionScript 2 (`src/shim.as`, compiled at runtime by [rascal](https://crates.io/crates/rascal), the AS2 compiler Ruffle itself uses) defines them as `__resolve` proxies that forward every call through `ExternalInterface` to Rust. The host calls back into ActionScript through `ExternalInterface.addCallback`. |
| Font library (`gfxfontlib.swf` + `fontmapping.cfg`) | Every text field imports `$TextFont`, `$TextFontBold` or `$BigButtonFont` from a virtual `gfxfontlib.swf` | Built in memory from `fontlib.swf`: the mapped Stratum2 fonts are copied, renamed to the alias and exported under it. `$BodyText` maps to "PF DinText Pro", which `fontlib.swf` does not contain; "Arial Unicode MS" (54k glyphs, 0.4 s per import) is left out. |
| Translator (`#TOKEN` text becomes the localized string) | Static text and ~580 `htmlText = "#..."` assignments | Rewritten when a movie loads: text field initial text, and string constants in each action block's constant pool (compilers put the pool first, so no jump crosses it). 1,310 constants and 150 text fields across the menu. Tokens fall back to the 2023 file's `_1` names. |
| `Stage.visibleRect` / `Stage.originalRect` | `ResizeManager` scales the 1280x720 authored layout to the screen | Defined by the shim. Without them the menu draws at 1280x720 in the corner. |
| Load visibility | `avatar.swf` hides the avatar clip, then loads the picture into it; Scaleform showed it | The shim makes `img://avatar_*` loads visible. Friend avatars are still hidden in the spike (open item). |

## Ruffle

- **Version.** Upstream `ba92d5af` (2026-10-11). Not on crates.io; the spike
  pins a git revision.
- **wgpu.** Ruffle uses wgpu 30; Bevy 0.19 uses wgpu 29. Ruffle went from
  wgpu 27 straight to 30 (`f64085ef`, 2026-08-22), so no Ruffle revision
  matches. The spike renders with its own wgpu 30 device into an offscreen
  `TextureTarget` and reads the frame back.
- **AVM1 coverage.** Every ActionScript path the home screen runs works:
  classes from `__Packages`, `MovieClipLoader`, `attachMovie` of imported
  symbols, `duplicateMovieClip`, `setInterval`, `__resolve`, text
  `htmlText`, `hscroll` marquees, `ExternalInterface`. No AVM1 errors remain
  besides harmless warnings (missing frame labels the movies themselves
  reference, "stack underflow" in a few packages).
- **Bugs fixed** (fork `bentsignal/ruffle`, branch `csgo-scaleform`, three
  commits, ~60 lines, all upstreamable):
  1. *Nested ImportAssets.* A movie loaded for ImportAssets that has its
     own ImportAssets (every panel imports `sharedlib.swf`, which imports
     `iconlib.swf`, which imports `loadingiconslib.swf`) stopped preloading
     at that tag, and its exports never reached the importer. Panels
     appeared as "non-registered character" errors.
  2. *Importer export IDs.* Ruffle copied the importer's exports into the
     imported movie under the importer's character IDs. IDs are local to a
     SWF, so they collided; a colliding `DefineSprite` stopped preloading.
  3. *duplicateMovieClip of an imported clip* looked the ID up in the
     parent's library and panicked.
- **Load time.** `MainMenu` finishes loading after 15 frames, 4.7 to 7.5 s
  on this PC. Most of it is Ruffle parsing `sharedlib.swf` (7 MB) and
  `iconlib.swf` again for each importer. The spike already merges duplicate
  ImportAssets tags (128 merged; `loadoutpanel.swf` imported `sharedlib.swf`
  seven times). Sharing one parsed library between importers would remove
  most of the rest (a Ruffle change).
- **Frame cost.** Advancing one frame, rendering 1920x1080 and reading it
  back to the CPU: 30 to 70 ms in this headless tool. Fine to prove the
  pipeline; the integration must not pay it every Bevy frame (below).

## Integration plan (option a)

1. **`crates/scaleform`** (new): the host from the spike as a library:
   Ruffle player, navigator over the game VFS with the load-time rewrites,
   the bridge shim, and a `Bridge` trait the client implements with real
   data. Ruffle stays out of `sim` and the other crates.
2. **Rendering.** First: Ruffle renders with its own wgpu 30 device, only
   when the movie changed (`Player::needs_render`), and the frame is copied
   into a Bevy `Image` shown by a full-screen UI node. Later, for the HUD
   and scoreboard movies that draw every frame in a match: backport Ruffle's
   renderer to wgpu 29 in the fork and draw straight into a Bevy texture on
   Bevy's device (no copy).
3. **Input.** Bevy mouse and keyboard events become Ruffle `PlayerEvent`s
   (window pixels map 1:1 because the stage is `noScale`, top-left), and
   keys also go to `_global.KeyDownEvent` as the engine did.
4. **Bridge to real data.** Settings and options (`GameInterface` convars,
   `optionsmenu.swf`), inventory and cases from `client::cosmetics`
   (`CScaleformComponent_Inventory`, `ItemData`, `Store`, `Loadout`), the
   map list (`GameTypes`), our server's players as the friends list, news
   from GitHub releases into the blog slot, then the server browser. UI
   sounds: `GameInterface.PlaySound("UI\\buttonrollover.wav")` and friends
   map onto our audio.
5. **App state machine** (menu, loading, match) replacing the
   relaunch-the-process flow, coordinated with Track A's connect flow.
6. **Screenshot check.** The spike's `diff` (MAE, RMSE, PSNR, share of
   pixels off, per-region grid, masks) moves into the integration's
   screenshot mode so menu work reports a number against
   `docs/reference/`.

Next screens in the same way: case opening (`inventorypanel.swf`, the
second reference), scoreboard, buy menu, HUD, pause menu.

## Owner decisions

- **Ruffle fork.** The fixes live in `github.com/bentsignal/ruffle`
  (`csgo-scaleform`), created for this spike. Offering them upstream to
  ruffle-rs would let us drop the fork; that is a public contribution under
  the owner's account, so it is the owner's call.
- **News panel content.** In 2018 it was the Counter-Strike blog. Proposed:
  our GitHub release notes rendered into the same slot.
- **Binary size and build time.** Ruffle adds a large dependency tree and a
  second wgpu version to the client until the renderer backport lands.
