# Classic death notifications

The draft client displays real kills using the player's installed classic
`resource/flash/huddeathnotification.swf`: original boxes, colored names,
local highlights, and runtime weapon/headshot bitmaps. Multiple-row positioning
and repeated-resize sizing are currently broken and block integration. This
implements WP10 in [the playtest ledger](../PLAYTEST-2026-10-14.md); full HUD
fidelity remains open.

## Runtime contract and ownership

Clean-room public-runtime inspection established `AddPanel`, `SetPanelText`,
`RemovePanel`, `UpdateWidth`, `onLoaded`, and `onResize`. Returned movie clips
are passed back through explicit host clip references, never by interpreting
player text as an object. No authored function bodies, instructions, leaked
source, or game artwork are included in the repository.

The installed movie exposes CT color `#6f9ce6`, T color `#eabe54`, seven notices,
five-second lifetime, a 1.5 local-player lifetime modifier, and one-second fade.
These are installed historical-movie observations, not proof of 2015 runtime
values. SetPanelText's local-victim flag selects the filled highlight; its
local-attacker flag selects the outline.

Ruffle does not display the original inline HTML bitmap images. Compatibility
code reserves space in the original text field using observed tab stops,
attaches the installed bitmap exports to owned containers, and fits the
original background/highlight art to measured text. It draws no replacement
boxes. Fonts and team colors remain those of the original field.

Simulation supplies immutable death identity, weapon and headshot facts.
Suppressor state is retained only from a causally preceding matching Fire
in the same event batch; the renderer does not infer a past shot's mode from
an actor's later weapon state. No gameplay or network protocol changes are
introduced by this presentation layer.

## Worker and fallback

A dedicated transparent bare-root worker receives bounded receipts and
revision-fenced raster requests. It receives no keyboard or pointer input.
It finishes in-flight loading, then parks when no drawable notices remain;
rejected rows receive no fade calls. CPU copies/uploads use the viewport's
upper half; GPU readback still covers the complete target. Odd physical
heights preserve the crop's pixel dimensions at the window's actual scale.

The ordinary text feed remains until the current authored raster is admitted.
Load/worker failure preserves that complete fallback. A content-specific row
failure instead retains only that receipt as text, below the cropped HUD when
healthy authored rows are present. The helper hides/removes only its owned
failed panel. Keyed successful relayout can recover a rejected row. Rejection
state is bounded and pruned against live receipts; an unknown icon cannot
disable the whole feed until restart.

## Observed renders and remaining defects

Actual authoritative M4A1-S and AK kills rendered at 1920x1080 and1600x900 on
`988033c`. Both saved images were personally inspected with intact original
boxes, local-attacker outlines, names, weapon icons and headshot icons.
Launcher/menu-probe builds and client/Scaleform checks passed. An earlier
T-spawn AK fixture aimed above its target on a slope and correctly failed
admission without a kill; that negative evidence remains preserved.

`7db8dbb` adds per-row fallback/cleanup/recovery and includes current M4A4
recoil. Its clean-workspace launcher/menu-probe builds and client/Scaleform
checks passed. A six-stage public API probe then established that unknown
icons reject explicitly without removing healthy rows, individual removal
leaves its sibling, and clear removes visible notices. It also exposed real
failures: two valid rows overlap and repeated resize stretches their
backgrounds. Inherited enumerable `isNavigable` was incorrectly treated as
a receipt during iteration; owned-key/panel filtering now excludes it.
The original probe remains failed: its strict raw-error guard also caught the
intentionally unavailable test icon. Explicit callbacks, actual pixels and
owned-map inspection are authoritative; some public return diagnostics are
stale, so they are not used alone as acceptance evidence.

An additional bounded public-contract observation will distinguish separate
row allocation/positioning and original width refresh before choosing a fix.
No complete multi-row, actual viewport-resize, expiration, coverage or frame
cost parity claim is made.

## Dated icon-scale reference

The original row-height fit made icons too large. The dated primary reference
is 3kliksphilip's [CS GO Grenade Tutorial, March11 2015, frame331s](https://www.youtube.com/watch?v=xoatYCUn4tw&t=331s).
USP-S/headshot ink is compared with native font glyph height rather than
assuming the video's unknown HUD scale matches ours. Public bitmap metadata
shows a32-pixel canvas and the installed field uses font size16. Font-height
canvas sizing agrees with both reference observations within their bounds,
so fitting uses the actual field font size, original resolution and row-height
limit. Both new scaled real-kill images on988033c were personally inspected.
Compressed footage, historical HUD scale, safe-zone placement and bitmap
changes still prevent a complete pixel-parity claim.

Raw video, captures, bitmap bytes and account data stay private under
`C:\Users\shawn\csr-fidelity\`. Only observations, derived measurements
and our bridge code ship. `CSR_KILL_FEED_TRACE=1` logs revision/receipt/raster
admission; `CSR_NO_CLASSIC_KILL_FEED=1` isolates its presentation cost against
the ordinary text feed. Neither switch changes simulation or weapon behavior.
