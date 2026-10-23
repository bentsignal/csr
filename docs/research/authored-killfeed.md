# Classic death notifications

The draft client displays real kills using the player's installed classic
`resource/flash/huddeathnotification.swf`: original boxes, colored names,
local highlights, and runtime weapon/headshot bitmaps. Multiple-row positioning
and repeated-resize sizing now pass a bounded public API/rendered check. This
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

`dfc9beb` corrects those defects using the original public UpdateWidth reset,
owned insertion ordering, the existing public HUD scale0.85, and the movie's
nScaledHeight metric converted into each row's parent coordinates. A six-stage
probe draws two separate rows with opposite local highlights, preserves both
after unknown/duplicate rejection, keeps them unchanged through two relayouts,
reflows the survivor after removal, and clears the feed. Both agents viewed
the actual images. Before/after values are identical: original parent yscale
150.42735042735, nScaledHeight37.3575, row y0/24.9 and yscale85, background
widths203/224. The expanded parent height75.05 does not corrupt its scale.
Exactly one deliberately missing-export error is classified separately; all
other errors/panics remain failures. Earlier failed evidence stays unchanged.

The same frozen source passes clean-workspace launcher/menu-probe builds and
client/Scaleform checks. A final real authoritative M4A1-S kill at1920x1080
exits0, admits its1920x540 raster and displays the corrected original panel,
name, local outline and icons; the coordinating agent viewed the actual image.
No actual viewport-resize, complete expiration/icon coverage or frame-cost
parity claim follows from the public relayout check.

## Dated icon-scale reference

The original row-height fit made icons too large. The dated primary reference
is 3kliksphilip's [CS GO Grenade Tutorial, March11 2015, frame331s](https://www.youtube.com/watch?v=xoatYCUn4tw&t=331s).
USP-S/headshot ink is compared with native font glyph height rather than
assuming the video's unknown HUD scale matches ours. Public bitmap metadata
shows a32-pixel canvas and the installed field uses font size16. Font-height
canvas sizing agrees with both reference observations within their bounds,
so fitting uses the actual field font size, original resolution and row-height
limit. Both new scaled real-kill images on988033c were personally inspected.
A separate primary multi-kill reference is [ESL Archives' Cologne2015 final,
uploaded August23 2015, around1190s](https://www.youtube.com/watch?v=02I5vVxlJhU&t=1190s).
The original720p frame shows four notice lines at approximately25px pitch
(with a conservative2px inspection/compression bound). The public movie's
37.3575px stage pitch at1080 normalizes to24.905px at720. This independently
supports the chosen public metric. The original single-row box/font heights
also agree with applying the existing HUD scale, rather than introducing a
new tuned scale. The video's spectator HUD, unknown original CVars, safe-zone
placement and bitmap/compression differences still prevent a complete
pixel-parity claim.

Raw video, captures, bitmap bytes and account data stay private under
`C:\Users\shawn\csr-fidelity\`. Only observations, derived measurements
and our bridge code ship. `CSR_KILL_FEED_TRACE=1` logs revision/receipt/raster
admission; `CSR_NO_CLASSIC_KILL_FEED=1` isolates its presentation cost against
the ordinary text feed. Neither switch changes simulation or weapon behavior.

## October 22 source-only owner-feedback pass

Public ExportAssets metadata in the installed death-notification movie lacks
`icon-revolver.png`, `icon-cz75a.png`, and `icon-mp5sd.png`. All three equipment
SVGs exist in Legacy's VPK. The host now adds missing bitmap exports in memory
from the bounded client SVG decoder; existing authored/imported symbols win,
and allocated IDs exclude both local definitions and imports. This keeps the
same original notice panel for later guns without substituting another gun's
icon. No asset bytes are committed. Background opacity and scale are unchanged.

Receipt updates now retain the admitted raster until a replacement arrives,
instead of alternating raster/text while the worker catches up. Resize still
invalidates old geometry. Exceptional text uses intrinsic width at the right
anchor. Narrow regression sources cover retention/resize and export completion,
ID collisions, and idempotence. F8 has a stable guns-before-grenades partition
with a source regression. None of these tests/builds were run by the source-only
child; root owns the serial checks and ordinary-play visual acceptance.

The same pass selects runtime `materials/vgui/avatar_default_64.vtf` and
`avatar_default-t_64.vtf` for team tiles (existing bot image remains a missing-art
fallback), and replaces opaque damage strips with Legacy's alpha-shaped
`materials/panorama/images/hud/damageindicator/damage-segment.png`. Direction,
strength and lifetime still use the existing damage receipt. Segment placement
and avatar appearance need root's visual check; this is not a measured classic
HUD parity claim.

Scoreboard remains the custom renderer. `resource/flash/scoreboard.swf` exists;
its public export table exposes Vote, Star, info_icon, superscript digits,
DefaultLocalAvatar and library classes, but not a ready row-render adapter.
The next concrete seam is a separate scoreboard movie worker following
`kill_feed::Bridge`/`combat_hud::Presentation`, fed the existing privacy-filtered
`scoreboard::board` snapshot. Establish its public runtime row/setter contract
without reading bytecode, add a revision-fenced full-board surface, and switch
Tab from the Bevy board only after successful raster admission. Do not feed
scoreboard pixels through the kill feed's top-half crop. Source-only inspection
cannot establish those runtime calls, so this pass leaves the working board.


October 22 follow-up: the scoreboard adapter is now implemented in
`scoreboard/movie.rs` and `scaleform/src/scoreboard.as`; see
[authored-scoreboard.md](authored-scoreboard.md) for the declarative contract
and remaining runtime checks. Root's capture rejected the generic user-head
avatars above. Team tiles now select the actual installed scoreboard team
avatars (`icons/scoreboard/avatar-ct.png`, `avatar-terrorist.png`), verified by
offline image inspection as the original CT and T emblems. They apply to bots
and offline local players alike; no generic Steam head is loaded.
