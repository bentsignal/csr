# Classic death-notification integration

Work in progress for WP10 in the October 14 playtest ledger. The current
text feed is not the requested classic HUD.

The player's Legacy install contains `resource/flash/huddeathnotification.swf`.
Its public export table includes the original weapon/headshot icons and
`death-text`; none of this artwork is copied into the repository. Public
runtime property inspection exposes `AddPanel`, `SetPanelText`, `RemovePanel`,
`UpdateWidth`, `onLoaded`, and `onResize`. No authored function bodies,
instructions, or leaked engine source have been inspected.

Public values observed at 1920 x 1080 include CT color `#6f9ce6`, T color
`#eabe54`, seven notices, five-second lifetime, a 1.5 local-player lifetime
modifier, and one-second fade. These are observations of the installed
historical movie, not a claim of exact 2015 runtime parity.

The first four bounded private probes established that the movie loads and
initializes, but did **not** display a notice. Calling a public function is
not evidence that its argument contract is correct. The next probe checks
standard Flash root scoping before changing production presentation. Setting
`_lockroot` did not by itself produce a visible notice. Public function arity
and returned-clip observations are now exposed by the explicitly invoked
diagnostic bridge; normal menus never call that path.

That call-result probe positively returned the clip
`Panel.Notice.DeathText1.NoticePanel1` from `AddPanel`. Its public children
are `TextPanel`, `IDTextBG`, `PanelVictim`, and `PanelAttacker`. Function
`length` is undefined in this runtime, so no parameter-count claim is made.
An explicit `__csrMovieClip` host value now allows a public setter to receive
the returned clip without treating arbitrary player strings as handles.

Private evidence is under `C:\Users\shawn\csr-fidelity\` in
`killfeed-*-runtime-20261015`. Those captures contain installed game artwork
and must never be committed or attached to a public release.

Implementation starts by retaining authoritative weapon/headshot values in
the bounded client feed, alongside its existing immutable death receipt.
This does not change simulation, network messages, or the visible HUD yet.
Authored rendering, real kill-event admission, local emphasis, resizing,
expiration, and frame-cost checks remain required before completion.


## Integration checkpoint, October 15

The original row now has a positive public-runtime argument contract: the
returned AddPanel movie clip is passed to SetPanelText, made visible, and
removed through RemovePanel. Public boolean observations distinguish local
victim fill from local attacker outline. Bitmap exports attach through owned
movie-clip containers; Ruffle does not render the original inline HTML images.
Text gutter/spacing is still being established, so this remains unfinished.

The client integration is a draft: a dedicated transparent bare-root worker
receives immutable kill receipts, bounded lifetime, causal shot mode, and
revision-fenced rasters. It receives no keyboard/pointer input and pauses when
no notices are present. CPU copies/uploads use the viewport's upper half;
GPU readback still covers the whole target. The existing text feed remains the
fallback until a current authored raster is admitted; failed loads or rows
retain that fallback and disconnect only the dedicated worker. Rust compilation and the initial real-kill renders passed on `6a2932d`.
Repeated resize/lifetime, complete icon coverage and frame-cost validation
remain open.

## Real receipts and dated scale reference

The dedicated worker drew real authoritative kills on `6a2932d`: attached
M4A1-S at1920x1080 and AK-47 at1600x900, with original panel art, local-attacker
outline, colored names and installed weapon/headshot bitmaps. Both process and
capture checks passed without fallback; the coordinating agent viewed both.
An earlier T-spawn AK fixture aimed above its target on a slope, produced no
kill, and correctly failed admission. Its failed receipt was retained; the
successful retry used the already verified CT-spawn fixture without changing
simulation or injecting a kill event.

The original row-height safety fit made icons too large. A dated primary
reference is 3kliksphilip's own [CS GO Grenade Tutorial, March11 2015, frame331s](https://www.youtube.com/watch?v=xoatYCUn4tw&t=331s).
Its USP-S/headshot ink was compared with the native-font glyph height, rather
than assuming the video's unknown HUD scaling equals ours. Public bitmap
metadata gives a32-pixel canvas height for these exports; the original field
uses font size16. Canvas-to-font-height scaling is a bounded candidate supported
by the matching USP-S and shared headshot observations. The new implementation
uses the measured field font size, bounded by the existing row height and
original bitmap resolution. Compressed footage, exact historic HUD scaling,
safe-zone position and bitmap differences limit the comparison; this is not a
claim of complete pixel parity. New scaled renders are pending.

Raw video/captures, bitmap bytes and account data remain private outside the
repository. Only public metadata, derived measurements and bridge code ship.
`CSR_KILL_FEED_TRACE=1` logs revision/receipt/raster admission for owned checks;
`CSR_NO_CLASSIC_KILL_FEED=1` isolates its cost against the existing text feed.
Neither switch changes simulation or weapon behavior.
