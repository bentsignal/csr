# Classic front-end integration

Track C is in progress in [PR #10](https://github.com/bentsignal/global-offensive/pull/10).
The client plays the original Scaleform movies directly from CS:GO Legacy.
Ruffle renders offscreen and transfers changed frames to a persistent Bevy UI
image. The UI camera stops drawing when a match runs with its menu closed.
No movie, font, image or sound from the install is checked into the repository.

The installed movies need engine-side data and lifecycle calls, as well as
their shared-library exports. Missing dynamic imports are supplied by reading
public SWF symbol tables and allocating unused character ids; authored movie
instructions are untouched. Diagnostic probes enumerate runtime properties,
observe bounded getter/missing-property reads and exercise public callbacks.
They do not decode game function bodies.

## Verified foundation

- The home screen and inventory render in the Bevy window at 1920x1080.
  Inventory names, opaque item ids, default weapons, owned cosmetics and case
  stacks come from local data. Native `img://inventory_<id>` requests resolve
  only known items to their installed images. Alphabetical and rarity sorts
  are supported; case stacks lack the history needed for acquired-order sort.
- Mouse, wheel, physical keys, focus changes, text input and text-field editing
  cross the bridge. Engine UI sounds use the UI mixer and survive map teardown.
- Menu, loading and match have separate lifetimes in one process. One map worker
  may run at a time. Cancellation discards its result; a newer selection waits
  for the existing worker instead of creating unbounded concurrent loads.
  Loading status, Cancel and Escape are available while the movie loads.
- Match entities and map resources are removed on leaving a match; persistent
  UI, inventory and settings survive. Tests cover teardown, late cancelled
  errors and a new selection waiting for an existing worker.
- The authored pause movie renders during a match and follows menu visibility.
  Closing pause through a movie callback resumes the simulation clock, captures
  the cursor and blocks the closing click from firing a weapon. A regression
  test covers that external transition. A physical Exit click returns to the
  home screen in the same process. The rendered lifecycle proof launches Dust II,
  opens pause, resumes its clock, returns home and launches Inferno. Pause is
  opened through engine state in that probe; both launches, Resume and Exit use
  the authored pointer controls. Cached movie roots retain their original scale
  when reopened and hide their launch-dialog descendants during a match.
- The offline map picker defaults to Competitive. Unsupported Casual, Wingman
  and War Games buttons are disabled using their authored controls. Mapgroups
  are filtered against installed maps and the original shared-library thumbnail
  exports, then indexed without gaps. Eight classic cards render on this install.
- The original GO dialog validates serialized offline settings, submits that
  exact query and commits through its native `OnOk`. Requests resolve only
  supported modes and filtered installed maps, then use the existing map worker.
  A real pointer sequence launches Dust II in the same process. No Bots and
  Medium Bots are accepted; other difficulty choices receive visible feedback
  until implemented. The current bot AI has not been measured for original
  difficulty parity.
- Gameplay HUD nodes explicitly target the match's default UI camera. The
  higher-order menu camera can be inactive without hiding radar, health or ammo.
- Audio and Keyboard/Mouse controls come from the installed settings scripts.
  Master Volume, Mouse Sensitivity and Reverse Mouse use their native slider and
  choice callbacks and persist through the existing settings store. Values are
  applied after the movie's asynchronous widget layout completes; unanswered or
  malformed callbacks cannot reset a saved value. Unimplemented controls are
  withheld. A pointer drag from 50% to 76% updates both the volume pip and its
  number without resetting the drag. Slider focus still needs further work.

## Remaining integration

Slider focus,
keyboard navigation, numeric entry and restore-defaults
behavior need further checks against the original movie's runtime contract.

The server browser, rendered network joins, loadout actions, classic case
opening, online profile/friends/news data and original HUD/buy/scoreboard
movies remain open. The older native menu is still transitional fallback code.
The archived-map download flow is withheld until its cancellation and material
dependencies are ready. This work is not a release-ready classic menu yet.

## Development probes

Use the normal client capture with `CSR_TEST_MENU=main` for the front end:

```powershell
$env:CSR_TEST_MENU = 'main'
$env:CSR_MENU_PROBE = 'inventory' # also play, offline or video
csr play de_dust2 --size 1920 1080 --screenshot C:\Users\you\csr-fidelity\inventory.png
```

`CSR_MENU_CLICKS` accepts `x,y;x,y` in reference 1920x1080 coordinates, plus
ordered `down:x,y;move:x,y;up:x,y` sequences for pointer interaction checks.
`CSR_UI_TRACE=1` logs first engine calls. Keep capture output outside the repo:
the frames contain the player's game art. `scaleform`'s `menu_probe` example
provides offscreen inspection and ordered callback/pointer probes, and its
`diff` module provides the reference comparison metrics.

`CSR_TEST_FLOW=1` with a normal 1920x1080 screenshot run exercises both match
launches and pause transitions in one process. It captures each stage beside
the requested screenshot, checks resumed ticks and match teardown, then exits
with success only after Inferno loads. Live window input is isolated from its
injected pointer sequence. This switch is for development verification.

On the development machine, inspected evidence is in the private
`C:\Users\shawn\csr-fidelity\menu-client` folder. `inventory-3.png` shows
the original weapon thumbnails after native image resolution; `match-2.png`
shows the corrected 3D target after deactivating the UI camera during play.
`offline-4.png` shows the filtered classic picker. `keyboard-choice-1.png`
shows Reverse Mouse changing to On through a physical pointer click, with the
saved config checked afterward. `audio-5.png` shows the correctly initialized
volume slider, and `audio-drag-4.png` shows the pointer-driven 76% result with
its matching number and saved config. The home capture's masked MAE is 19.77 against the reference;
this integration has not reached the earlier offscreen spike's fidelity.
`pause-2.png` shows the original pause movie with its Resume and Exit controls.
`offline-launch-1.png` shows Dust II after the original GO sequence;
`pause-exit-1.png` shows the return to home, and `match-hud-1.png` verifies the
match camera with radar, crosshair, round timer, health and ammo visible.
`flow-dust2.png`, `flow-pause.png`, `flow-resumed.png`,
`flow-pause-exit.png`, `flow-returned-home.png` and `flow-inferno.png` are the
inspected single-process proof; its log reports zero lifecycle failures.

The combined integration checkpoint passes 288 workspace tests (four installed-data/helper
tests ignored), three menu-probe tests, release Dust II probe and ten-round sim
with zero failures, and the 1,425-file weapon/feedback/foley decode audit with
zero unsupported or crashing sounds. These checks do not establish completion
of the remaining UI flows above.
