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
- The offline map picker defaults to Competitive. Unsupported Casual, Wingman
  and War Games buttons are disabled using their authored controls. Mapgroups
  are filtered against installed maps and the original shared-library thumbnail
  exports, then indexed without gaps. Eight classic cards render on this install.
- Audio and Keyboard/Mouse controls come from the installed settings scripts.
  Master Volume, Mouse Sensitivity and Reverse Mouse use their native slider and
  choice callbacks and persist through the existing settings store. Values are
  applied after the movie's asynchronous widget layout completes; unanswered or
  malformed callbacks cannot reset a saved value. Unimplemented controls are
  withheld. A pointer drag from 50% to 76% updates both the volume pip and its
  number without resetting the drag. Slider focus still needs further work.

## Remaining integration

The offline GO callback still needs to reach the load request. Its full
menu-to-match-to-menu-to-second-map flow needs a rendered proof. Slider focus,
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

The combined integration checkpoint passes 262 workspace tests (two installed-data
tests ignored), three menu-probe tests, release Dust II probe and ten-round sim
with zero failures, and the 1,425-file weapon/feedback/foley decode audit with
zero unsupported or crashing sounds. These checks do not establish completion
of the remaining UI flows above.
