# Offline picker power-button availability

Owner feedback UI08: the authored top-right power button is disabled while
Offline with Bots is open. It should remain available and require confirmation
before exiting, avoiding accidental shutdown.

## Current evidence and scope

The cached Home/Match worker lifecycle and owned picker epochs are already on
the base branch. Preserve those ownership fences and the player's original
installed Scaleform widgets; do not add a replacement power icon or dialog.

Static review finds no dedicated quit-confirmation route in the current
frontend bridge. Generic BasePanelRunCommand dispatch closes other pages and
passes unknown commands to the main thread; quit/quitgame then exits directly.
That does not prove which authored power-button callback is missing or whether
the picker blocks the original hit target. Actual public callback/property and
rendered evidence is needed before choosing a fix.

The next observation will inspect the original Home power target, open the
original picker and compare its public enable/input state. Establish the
authored confirmation widget/callback through ordinary public API observations
and legitimate references. Do not inspect authored function bodies or bytecode.

No runtime pass or original-confirmation fidelity is claimed yet. Private
captures remain outside the repository. Builds and graphical observations use
the parent agent's shared resource leases.

## Historical confirmation reference

Cuperschmid and Hildebrand's own CS:GO interface evaluation in the
[SBGames 2013 proceedings, Figure 3, printed page 373](https://www.sbgames.org/sbgames2013/proceedings/artedesign/44-dt-paper.pdf)
shows the original exit confirmation: a black dialog titled `EXIT GAME`, a
confirmation question, and `OK` / `Cancel` controls. This supports restoring an
original confirmation instead of immediately exiting from a menu click.

The paper identifies a Mac OS X test environment. Its dated screenshot proves
the historical dialog and labels, not exact 2015/2018 PC geometry or picker
input behavior. A private copy/render was inspected; copyrighted screenshots
are not committed. The installed widget still needs a public runtime contract.

## First public callback observation (dfc9beb)

At 1920x1080, the visible power glyph's center is (1584, 28). Both the Home
click and a click after requesting StartSinglePlayer emitted the original
`MainMenu.BasePanelRunCommand("Quit", "false")`. No confirmation appeared:
the standalone probe records commands without implementing application exit.
Production currently forwards that command to immediate application exit, so
the original confirmation handoff is missing from our bridge.

This is not a production picker-availability pass. The standalone picker
request rendered partly behind Home panels; its callback only establishes
that the power control still received input with that movie loaded. The
native Escape function attempt returned false and changed no visible dialog;
Escape/cancel delivery was not established.

The process exited 0, preserved owner settings and left no game running, but
the strict helper failed on missing modern map thumbnail exports while the
original picker loaded. That raw failure remains preserved. Public MessageBox
metadata is the next prerequisite for implementing its original controls.

## Original MessageBox and implementation checkpoint

The installed `RequestElement("MessageBox")` loader positively reports
`OnLoadFinished("mc:_level500", ...)` and `OnReady`. It has no observed
`MessageBoxMovie` alias. Public runtime properties expose `SetTitle`,
`SetMessage`, `SetButtonLegend`, `SetFlagOk`, `SetFlagCancel`,
`SetFlagTertiary`, `showPanel` and immediate/animated hide methods. Both
metadata-only observations exited cleanly without ERROR/panic. They do not
prove initialized dialog layout or button behavior.

The implementation intercepts the authored `Quit` before generic navigation
teardown. It requests the installed dialog, supplies original localized text
through those setters, and assigns an opening-bound native API to that exact
registered movie. Cancellation retains the picker. Parking/navigation revokes
the opening; unowned, malformed and cancelled callbacks cannot exit. The
first rendered production check still must establish the actual button
callback names, original visibility/navigation and cancel/reopen/confirm.

Separate existing follow-up: the release lifecycle Audio screenshot again
shows only Master Volume and a large empty rounded panel with a white point.
Its capture may precede steady state. Full Audio content/layout remains open;
the slider/navigation fixes are not a full-page visual pass.

## Missing localized MessageBox glyphs

The first production confirmation draws the original box and controls but drops
letters in its question. A delayed-setter public observation reproduces that
failure, so delaying text assignment is not a fix. Metadata-only inspection of
the installed movie identifies its eleven-glyph `Stratum2 Regular` subset:
`#DTaegilotx`. The question's missing letters, space and question mark are absent
from that subset. The same install's font library contains the complete family.
No authored instructions or function bodies were inspected.

The runtime compatibility change is limited to `message-box.swf`, its observed
font id/family/flags/code sequence and the observed full-family library metadata.
It appends missing installed glyphs and matching alignment records. Existing
glyph indices, raw outline bytes, advances, bounds, global metrics, kerning and
alignment records stay unchanged. Other movie tags, including instructions,
pass through byte-for-byte. No font or movie assets enter the repository.

This checkpoint is not a rendered fix claim. The original Cancel click emitted
`MessageBox.OnButtonPress(52)`; the confirm value and the dialog object's stored
API still need the bounded verbose public observation. The provisional callback
binding is not accepted as complete until actual Cancel/reopen/OK behavior and
the full question render have been checked.

## Observed button contract and owned adapter

The bounded verbose public observation identifies `OnButtonPress(48)` for the
actual OK pixel and `OnButtonPress(52)` for Cancel. Those values are our bridge's
existing `ValveKeyTable` entries for `KEY_ENTER` and `KEY_ESCAPE`. The dialog
record exposes text/flags, `mLevel` and `mPanel`, with no native API field.
Rebinding the movie's `gameAPI` did not replace its retained engine receiver.

The corrected adapter changes only `OnButtonPress` on our cached MessageBox
proxy. It preserves any prior own property, translates through our own key table
and assigns an immutable opening receiver. Release restores that property only
if it still owns it. The ineffective root/MessageBoxAPI replacement is removed.
A retained old callback closure remains tied to its old epoch; a future authored
lookup of the current property obtains the current wrapper. No claim is made
that arbitrary delayed authored lookups can be assigned a historical epoch.

The Rust bridge requires a current shown confirmation, active action epoch and
menu scene. Raw native callbacks, loading-only confirmations, malformed values,
cancelled openings and parked workers cannot request application exit. Actual
production Cancel/reopen/OK and complete question rendering remain pending the
combined candidate's immutable build and graphical check.


## Frozen 15337c8 diagnostic result and remaining limits

The new in-memory subset completion renders the full original question
coherently at 1920x1080. Existing glyphs/indices, outlines, bounds, advances,
global metrics, kerning and alignment records are preserved; the installed-data
and independent SWF-reader regression passed. This fixes missing letters as a
bounded usability improvement, not exact typography admission.

The donor and subset have the same family name but different flags (133 versus
140), global metrics and semantic contour geometry. Common donor advances are
uniformly 20 Font3 units smaller; contour extrema differ by up to 350 units
(about 0.46 pixels at the displayed 27-pixel font size). No inferred advance,
outline or weight correction was applied. Existing metrics remain original.
The same-family library fallback mechanism is an inference; exact original
Scaleform substitution/typography remains open.

The public-runtime check positively records actual Cancel through `OwnedQuit1`,
the retained old callback still reporting `OwnedQuit1` after rebinding, and
actual OK through `OwnedQuit2`. The Rust ownership regression separately rejects
that stale receiver. The probe exited 0, preserved settings and produced five
images with no ERROR/panic. Its strict helper nevertheless failed a substring
gate: the expected diagnostic property-path string contained
`MessageBox.OnButtonPress`; an anchored receiver check found zero raw calls.
The original failed receipt remains unchanged, with a separate classification.

The reopened probe raster is negative: its same-batch hide/show omitted the
production bridge's explicit visibility restoration and settled cancellation.
The later pictures show Home without the dialog, despite callback label proof.
Do not call that a reopened visibility pass. Production Home question,
Cancel/reopen/OK exit and Offline picker retention still require their bounded
actual-client checks. Full Audio settings content/layout also remains open;
the Master-only/empty-panel release capture is retained as independent evidence.

## Production Home pass and native picker control correction

The a61f67e actual-client checks establish Home Cancel followed by reopening:
the reopened original box and complete question are visible at 1920x1080.
A fresh actual OK click emits the current owned Enter callback and exits 0
before the screenshot fixture can fire. These are bounded Home passes.

The same artifact's production picker check is a preserved real failure: its
power glyph is dim, no Quit callback is emitted, and the later Cancel point
instead hits a map tile. The production bridge honors `bHidePanel` by invoking
`MainMenu.hidePanelImmediate`; the earlier standalone picker request did not.

Installed PlaceObject2 metadata names the native `QuitButton`. A bounded public
runtime check reproduces the production hide, observes that exact button,
invokes its public `setDisabled(false)` and then clicks the actual power pixel.
The glyph becomes bright without changing its authored geometry and emits the
original Quit command. The correction calls that same setter after showing the
owned picker, for both cold-ready and cached paths. It does not add a hit region,
new control, overlay, generic visibility rule or authored instruction change.

That standalone check also retains nine exact missing modern-thumbnail errors;
those are an explicit separate asset-negative, not a general clean-log pass.
The corrected production picker route still needs its new-build check. Passed
Home cases will not be repeated merely to obtain another receipt. Exact font
substitution and full Audio page content/layout remain open.


## Corrected production picker verification (407e5b0)

The single failed route was rerun against the corrected immutable candidate:
Play -> Offline -> Competitive -> first map tile -> power -> Cancel. The actual
original Quit callback loads MessageBox, the current owned Escape callback
cancels it, and the 1920x1080 raster returns to the picker. Inferno remains
selected, GO remains available and the original power glyph is enabled. No
MessageBox, Audio or Home panel overlays the picker. The client exits 0 with
strict no-ERROR/panic checks, exact callback counts and unchanged owner settings.
The actual image was inspected; the earlier a61 failure is retained separately.

This establishes the requested bounded control/navigation behavior alongside
the earlier actual Home cancel/reopen and real confirmation-exit checks. It
is not a claim of complete original menu fidelity. The same-family appended
font's exact typography remains open, as does the full Audio content/layout
gap and the other menu requirements in the playtest ledger. No passed Home
case was repeated for the picker-only correction.
