# Normal-startup drawable/raster proof

`CSR_TEST_DISPLAY_PROOF=<absolute private output directory>` installs an
opt-in diagnostic fixture in the normal menu startup path. It requires
`CSR_DATA_DIR=<output>/userdata`, that directory must already exist, and the
launcher's working directory must match it. The fixture rejects direct map or
`--screenshot` startup before changing Settings. It never drives synthetic
focus/cursor/input, changes Winit scheduling, or writes settings outside the
isolated fixture data folder.

The fixture changes the production Settings resource through windowed
1600×900, 1920×1080, 2560×1440, borderless fullscreen, a volume-only change
while fullscreen, and return to the saved 2560×1440 windowed dimensions.
Fullscreen dimensions come from the current Winit `OnMonitor`/`Monitor`
relationship and the actual client rectangle, never from the saved windowed
resolution. If the desktop cannot realize a requested large window, the
fixture fails its dimension gate rather than claiming that size was proved.

Each stage requires the ECS physical window size, current frontend viewport,
last assigned raster texture size and native HWND client rectangle to agree.
The HWND query executes on the UI thread, holds Bevy's owned raw-handle
wrapper, and checks process/thread ownership and handle validity. Fullscreen
must also match the current monitor's physical dimensions; its volume-only
stage must retain the preceding fullscreen size. The log records logical
size, DPI scale, natural focus, mode, monitor, serial and every measured size.

Only after two distinct fresh matching raster assignments and a one-second
settle does the fixture request a primary-window PNG. The next Settings
transition waits for that specific screenshot callback and validates its
dimensions. Six callbacks are required before successful exit. A deadline or
any ownership/capture mismatch exits with a failure. Pure tests prevent
reused rasters or a mismatched asynchronous native resize from satisfying
readiness.

The source checkpoint has not yet been compiled or run: Cargo and GPU are
coordinated by the root agent. Capture files remain private. Visual QA must
inspect the settled PNGs, compare the 1920×1080 Home image with the installed
classic reference, and assess text/edge quality at native sizes. These checks
prove raster/drawable agreement, not OS input latency, compositor latency,
all display modes, every DPI configuration, or gameplay HUD visibility. HUD
needs a separate normal full-window match proof after admission/respawn.
