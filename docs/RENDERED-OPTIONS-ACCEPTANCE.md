# Opt-in rendered Options and controls acceptance

Implementation base: `fix/controls-options-integrated` at
`0d08dad80ad0d679b24505d4cbe7f9aeed49b3f1`. This harness has been authored and
source reviewed only. No compilation, test execution, installed movie session,
native run or screenshot review was performed by its implementation agent.
Root owns all builds, execution, private fixtures, preservation checks and PRs.

Set `CSR_TEST_FLOW=1` and select exactly one `CSR_TEST_FLOW_CASE`:

| Case | Authored acceptance route |
| --- | --- |
| `home-options` | Home -> Options -> Audio -> Home, then Audio -> Play -> Offline with Bots -> Back -> Home. Repeat twice in the same process, including cached Options/picker opens. |
| `match-options` | Existing authored Dust II launch, public Escape -> Pause -> Audio -> Escape -> Pause -> Escape -> game with both installed HUD surfaces visible and simulation advancing. Repeat Escape/Pause/game, then cancel Audio and close/reopen Pause on consecutive key edges before waiting for its raster; reopen Audio -> authored Back -> Pause -> Escape -> game. |
| `controls` | Home's authored Keyboard / Mouse route -> actual Bevy panel at 1920x1080 -> capture first binding -> F11 -> Clear -> recapture -> Escape cancel -> mouse button and wheel choice buttons -> wheel scroll -> End -> bottom row -> resize through public Window state to 1280x720 -> End -> bottom row -> Home key -> top row -> Restore -> Apply -> disk reload comparison -> Back -> authored route reopens panel with saved defaults. |

Omitting `CSR_TEST_FLOW_CASE` runs the existing `frontend_flow.rs` unchanged.
These cases do not change production navigation, settings, geometry or cadence.
In particular, no production fix is made for the invalid zero-bot launch fixture.

## Root invocation

Under root's exclusive Cargo/build slot, clean the explicit shared target/profile
before building from this worktree, as required by AGENTS.md. Copy the resulting
executable to an immutable private path and retain the real build cwd/commit.
Run each case separately with normal menu startup, using that copied executable:

```powershell
# ROOT ONLY: choose a fresh private output folder for EACH case, outside the repo.
$env:CSR_TEST_FLOW = '1'
$env:CSR_TEST_FLOW_CASE = 'controls' # or home-options / match-options
$env:CSR_TEST_FLOW_OUTPUT = 'C:\Users\shawn\csr-fidelity\options-acceptance\controls'
$env:CSR_DATA_DIR = Join-Path $env:CSR_TEST_FLOW_OUTPUT 'userdata'
# Prepare OUTPUT/userdata/config/user.cfg before launch; do not copy player data.
# Launch from OUTPUT/userdata (the launcher also selects CSR_DATA_DIR).
& 'C:\absolute\immutable\csr.exe' play de_dust2 --bots 3 --size 1920 1080
```

The caller must create OUTPUT and its `userdata/config` directory and seed a
private `user.cfg`. The case checks that both cwd and canonical `CSR_DATA_DIR`
equal OUTPUT/userdata, that the config exists and loads to the initial runtime
Settings, that startup is windowed at 1920x1080, and that neither `--direct` nor
`--screenshot` was requested. Use `--bots 3` for the existing launch's authored
difficulty-3 selection; `--bots 0` is an incompatible fixture. Clear unrelated
test/proof environment flags before each run. No Steam launch/config edits or
backend OS focus/input automation are used by this harness.

For the controls case, seed nondefault controls and unrelated preferences so a
whole-settings reset is observable. For example, root may use this fixture text
inside the private config (this is documentation, not a checked-in game config):

```text
fullscreen=false
resolution=2
vsync=true
exposure=3.25
msaa=false
volume=0.73
snd_mute_losefocus=0
snd_menumusic_volume=0.27
menu_blur=false
cl_showfps=1
cross_color=3
cross_size=13
radar_scale=0.8
sensitivity=3.375
invert_y=true
bind0=ArrowUp
```

Root should retain the initial/final private config and independently verify
protected real-player and Steam files remain unchanged. The harness snapshots
initial Settings, requires Restore to leave the private disk config untouched
until Apply, then compares the full reloaded Settings to initial Settings with
only the eight controls fields replaced by defaults. It checks the full equality
again after Back and reopening the editor. `Settings::load()` is a real disk
parse, **not a second-process restart**; root can additionally relaunch the saved
fixture for process-startup verification. No harness setter replaces Settings.

## Evidence and bounds

All output uses existing `CSR_TEST_FLOW_OUTPUT`: case/cycle/phase numbered PNGs
and the caller's redirected log. Ordinary phases capture settled state before
input and wait for screenshot readback, dimensions and synchronous PNG save.
A predicate still pending after one second gets one `waiting` capture. The two
explicit consecutive Escape-edge pressure phases deliberately skip this delay
and capture barrier. The next settled Pause/Audio captures show their result.

Each case has a 240-second process bound and 40-second ordinary phase bound;
the reused match launch retains its own original deadline. Movie observations
are limited to 32 samples per phase, at most one per fresh raster and no more
often than 200 ms. Settlement requires a continuously valid predicate across
three distinct rasters and 350 ms; controls/HUD predicates use application frames
because the inactive menu worker need not rasterize during gameplay. Late probe
replies from a previous phase cannot satisfy the next phase. No arbitrary sleep
loop, per-frame full trace or movie state/capture/Interaction mutation is used.

The read-only `flow_probe.as` diagnostic samples the registered authored roots'
public visibility, searches visible display-list text for the exact English
label, and reports its public transformed bounds. Ambiguous labels, out-of-window
coordinates, mismatched Stage/window dimensions, absent clips and exhausted
search budgets fail closed. Navigation sends actual Scaleform MouseMove/Down/Up.
The only reference coordinates are the measured 1920x1080 Home and Play topbar
centers already used by the original harness/reference. If an installed label
differs, the bounded sample reports up to 24 observed labels for correction;
there is no fallback controller call that could turn a missed click into a pass.

Bevy input follows the pinned 0.19.1 sources: First writes KeyboardInput,
MouseButtonInput and MouseWheel; InputPlugin processes these in PreUpdate before
UI focus and the existing menu controller in RunFixedMainLoop. Window's public
cursor/viewport state is used without OS cursor APIs. Controls targets are
read-only Action locators plus measured ComputedNode/UiGlobalTransform bounds;
keyboard/adjust buttons must fit inside the scroll viewport and all footer
buttons must fit inside the window. The actual controller owns capture, Clear,
Restore, Save and settings changes. No Interaction is assigned by the harness.

Required root checks include the authored client fixtures and the asset-free
Scaleform diagnostic fixture, then the repository's full required checks and
the three actual rendered cases. A runtime `failures: 0` log means these
synthetic functional assertions completed. Root must inspect PNGs, especially
waiting/settled transitions, top/bottom editor rows and both final game HUDs.
Natural delayed rasters are observed; this harness does not force GPU/worker
delays or prove every adversarial raster ordering. The existing synthetic
ownership tests remain necessary for deterministic stale-callback coverage.

Physical input latency, native Legacy behavior, 0.022 sensitivity gain, historical
audio tone/mixing, full pixel fidelity, in-match controls editing, repeated
edit/save at both sizes, and a fresh-process reload remain outside these cases.
The previously successful installed Audio Back/Pause/Resume/Home/second-Inferno
flow remains separate evidence; it is not a pass result for this new harness.
