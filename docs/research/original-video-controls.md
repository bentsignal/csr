# Original Video controls with working display backends

The original Video page previously had no connected rows. This slice connects
the installed Display Mode and Wait for Vertical Sync widgets to the existing
saved/live display preferences, and exposes `mat_vsync` and `mat_antialias`
through the console. It does not change camera aim, input sampling or the
renderer's default presentation policy.

Installed `scripts/video_options.txt` supplies the original labels, dropdown
type, priorities and values. Display Mode offers Windowed0 and Fullscreen
Windowed2. Exclusive fullscreen1 is omitted because the existing backend is
borderless fullscreen, not exclusive. The vsync row offers Disabled0 and
Enabled1; the latter uses the original generic Enabled localization token
instead of claiming Source's double-buffer implementation. Triple buffering2
is omitted. These are bounded functional controls, not complete native Video
Settings or buffer-count fidelity.

Both original widget values and console aliases update the same persistent
Settings fields that `frontend_display::DisplayState` applies to the actual
window. MSAA console support accepts only0/off and4/four samples, matching the
existing renderer. Unsupported modes are rejected, not silently mapped to a
different preference. Fullscreen continues using the OS monitor size and keeps
the saved windowed size for returning to a window.

Gamma/TV calibration, aspect ratio, dynamic resolution enumeration, exclusive
fullscreen, other MSAA modes and the remaining video controls are unfinished.
Shader/material fidelity, input-to-photon latency and native buffering behavior
are separate gates.

PR161 was merged after final-head CI, full workspace checks and combined
RELEASE verification. Frozen union53d88aa (crates81d6918925a13bfab95df1e34bc802ce4a275de9)
produced immutable RELEASE9197101C. Its ordinary Home/DustII/Pause/Video/Back/
Resume/Home/Inferno lifecycle exited0 and preserved the owner's settings.
Actual original dropdown pointer callbacks changed saved vsync false/true/false,
and the live Window policy changed AutoNoVsync/AutoVsync/AutoNoVsync accordingly.
The three actual1920x1080 Video page images were personally reviewed: captions
match, and the stray controller dead-zone panel is absent. This verifies the
bounded controls and production route, not physical display latency or Source's
buffer-count implementation. Fullscreen pointer switching remains unverified.

Two earlier negative fixture packets remain private and unchanged. Keyboard
navigation did not select the opened dropdown. The following pointer packet
updated the saved setting, but --screenshot deliberately skips live display
application; a normal launch with fixture output supplied separately resolved
that test-setup error without changing the production display policy.
