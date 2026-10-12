# Owned Pause/Options surface bookkeeping

The host-address panel must disappear before native Options opens. Bevy's
`Menu.page` does not change when the native bridge opens Options, so it is not
sufficient to select the visible movie page.

## Read-only observations

Two completed owned offscreen runs at 1920x1080 requested the installed Pause
and Options movies through an existing diagnostic helper. Public primitive
property reads used only `_visible` and `_alpha`; no original function bodies
or instruction streams were inspected. Audio Options used the existing owned
supported-settings fixture. Pointer Move/Down/Up at `(380,972)` hit its original
Back control and emitted **`OptionsMenu.OnCancel()` with zero arguments**.
No `OptionsMenu.OnUnload` callback was observed.

| Observation | Public properties | Visible result |
| --- | --- | --- |
| Explicit Pause `showPanel` | Pause root `_visible=true`, `_alpha=100` | Original Pause controls appeared. |
| Requested/shown Options | Options root `_visible=true`, `_alpha=100` | Audio Options appeared; this older fixture also retained Pause above it. |
| Original Back pointer | Options root stayed `true/100` | `OnCancel()` reached the mock engine. Its undefined response did not close Options. |
| Public Options root `HidePanel` | Root and `TopPanel` stayed `true/100` | This probe did not demonstrate a successful Options close. |
| Public Pause root `hidePanel` | Root and `Panel` stayed `true/100` | Pause controls disappeared despite those unchanged properties. |

Consequently these properties alone do **not** detect authored panel presence
or internal animations. An unanswered Cancel is not a measured close. The
integration must explicitly own root visibility at its existing show/hide
boundaries: suspend before Options is requested, hide the exact Options root
on the observed Cancel/scene teardown, and restore Pause through its verified
public show boundary. Sampling then verifies **our own bookkeeping**, rather
than inventing an outward native visibility callback or a pixel-perfect
animation contract. Current-source rendered proof remains necessary.

## Tracker and sampler contract

`frontend_pause_surface::Tracker` retains registered loader roots across
active-scope changes, but forgets them on a movie-worker owner change. The
caller supplies a fresh worker generation before `Menu::spawn`; host lifetime
changes must not clear cached movie roots. Enable sampling only in Match with
the caller's legacy Pause/Main page. Register every `OnLoadFinished`, including
reused level paths, to advance the root epoch.

`options_opened` immediately invalidates pending proof and sets the desired
Options lifecycle. `options_requested` is independent of active Pause scope;
gate late Options `OnReady`/show with it. `options_cancelled` revokes that
desired lifecycle and invalidates proof. Close-before-load can recover through
a fresh Pause sample; a later Options load still advances the epoch and must
remain hidden when it is no longer wanted.

`request(presented_serial)` permits one outstanding request and at most one
request per advanced, actually presented movie frame. The own shim function
`csrBindings.samplePauseSurface` emits `OwnedPauseSurface.Visibility` with
five literal request strings (worker, epoch, request serial and two recorded
root paths), followed by each root's exists/visible/alpha primitives. The
tracker requires exact stamps/root paths, rejects late replies, and never
coerces missing values to false or zero. A matched malformed reply fails
closed. Pause must be explicitly visible at alpha 100; a known Options root
must be explicitly hidden. Alpha zero alone does not remove Flash hit testing.
Requested Options always suppresses the panel, including a temporarily hidden
cached root while a cold show is pending. No gameplay or authority changes
occur in this tracker or sampler.

## Evidence and limits

Private measurement directories:
`C:\Users\shawn\csr-fidelity\pause-surface-properties-3` and `-4` contain
complete invocations, callback/property logs, screenshots and exit results.
Both exited 0; owned PIDs 53560 and 71420 were verified absent. The copied older
helper has SHA256
`20179468A2D09B7D292576A6C687C429F9A03FF1BBD1020CE04670F709C5B2B3`.
Its precise source-to-executable mapping is unavailable: it came from private
diagnostic work above `d63f7da` and predates the final import-alias pin. These
observations are therefore qualified runtime evidence, not a current-client
fidelity or successful Options-close proof. Two earlier attempted invocations
used unsupported request syntax and provide no lifecycle evidence.

The new sampler was separately executed in pinned public Ruffle
`5b20c6507c51657dabdad25e172cc81c8587d898` using only generated own MovieClips,
the actual shim, and Null backends. Five EI cases cover absent Options, visible
Options, explicitly hidden Options, hidden/partial-alpha Pause and missing
roots, including actual `Undefined` values. The same cases are retained in
`crates/scaleform/tests/pause_surface.rs`. Seven tracker regressions cover
worker/root epoch changes, same-path reload, Options opening/cancel-before-ready,
strict primitives, request coalescing and exhausted counters. Ten Scaleform
library tests also passed, including compilation of the actual new shim.
The private proof source/log/hash index is under
`C:\Users\shawn\csr-fidelity\pause-surface-synthetic`; tracker harness logs
are under `pause-surface-tracker-tests`.

No assets are committed or copied from the install. The original measurements
used offscreen engine pointer fixtures, not OS input or the original engine.
The synthetic regression needs no game install, GPU or external assets. No
player configuration, server/game action or system clipboard was changed.
