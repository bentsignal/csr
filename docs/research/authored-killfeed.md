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

