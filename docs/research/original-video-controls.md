# Original Video controls with working display backends

The original Video page currently has no connected rows. This slice connects
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
are separate gates. Verification is pending; this is not yet a delivered UI.
