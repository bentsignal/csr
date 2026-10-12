# Menu drawable resolution

The classic movies are rasterized by Ruffle at the Bevy window's physical
client dimensions. `frontend` supplies these dimensions at worker creation
and on resize; `Host::resize` updates Ruffle's viewport, whose wgpu backend
reallocates the texture target. The composed ImageNode fills the UI camera's
viewport. Bevy 0.19.1's default image sampler is already linear. The authored
1280×720 stage and its own ResizeManager scaling are retained.

The display-settings application had two API-level defects. Startup uses
`WindowResolution::new`, whose dimensions are physical pixels, but subsequent
settings application used `WindowResolution::set`, whose dimensions are
logical pixels. At DPI scale 1.5 a saved 1920×1080 therefore requested
2880×1620. It also requested the saved windowed size while enabling borderless
fullscreen, and repeated these requests after unrelated settings changes or
local-player admission.

The correction tracks the last applied fullscreen, windowed-resolution and
vsync preferences separately. It requests saved sizes with
`set_physical_resolution` only on initial windowed setup, an explicit windowed
resolution change, or return from fullscreen. Borderless fullscreen leaves
actual dimensions to Winit's selected monitor and its resize notifications.
Audio, sensitivity and player admission do not overwrite a settled drawable
or a manually resized window. Changing the saved resolution while fullscreen
retains that preference for a later windowed transition.

Source evidence is Bevy 0.19.1 `bevy_window/src/window.rs` (resolution new/set),
`bevy_winit/src/system.rs` (mode and deferred resize reconciliation),
`bevy_image/src/image.rs` (linear default), Winit 0.30.13 Windows
`request_inner_size`, and the pinned Ruffle player/wgpu viewport setters.
Windows issues an asynchronous size request and returns `None`; a later
Resized event supplies actual dimensions. A conflicting request can therefore
change the window or temporarily disagree with the previous drawable. This
API evidence does not alone establish the cause of the player's screenshot.

Three asset-free regressions cover high-DPI physical windowed dimensions,
fullscreen monitor-size preservation through audio/player reapplication and
windowed return, and manual resize/DPI preservation through unrelated changes.
They are awaiting coordinated compilation. No game assets are included.

Visual verification is pending. Use private normal-startup data at actual
1600×900, 1920×1080 and 2560×1440 if available; record the physical drawable,
DPI scale, worker frame dimensions and image dimensions for each capture.
Compare the 1920×1080 Home image with `docs/reference/menu-2018-home.png`.
Exercise windowed → fullscreen → windowed, ordinary resizing, DPI changes,
volume adjustment while fullscreen, map admission and cached Pause reopening.
Check HUD/radar/feed visibility and input alignment as well as captions and
native geometry. Repeat the normal moving-menu performance fixture at the
larger drawable; do not substitute an upscaled low-resolution image or claim
1:1 fidelity from source inspection. No unsupported panels are expanded.