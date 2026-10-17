# Private ordinary-input performance cases

Root/operator helper for paired immutable diagnostic builds. It does not build,
change a player bundle, drive input, activate a window, or take screenshots.
Use the available ComputerUse skill or an approved repository fidelity workflow
for activation, ordinary input and external captures. The helper never admits a
performance window or claims an FPS improvement.

After the root has verified and copied the release binary, execute one case at a
time from PowerShell (replace every example path/hash with actual receipts):

```powershell
& 'C:\checkout\tools\fidelity\performance-case.ps1' `
  -NativeSlotGranted `
  -Executable 'C:\private-proof\baseline\immutable\csr.exe' `
  -Sha256 '<full SHA256 from build.json>' -Head '<full source commit>' `
  -Manifest 'C:\private-proof\baseline\immutable\build.json' `
  -PrivateRoot 'C:\private-proof\cases\baseline-nuke-bots3' `
  -OwnerData 'C:\frozen-player-bundle\userdata' `
  -SteamRoot 'C:\Program Files (x86)\Steam' `
  -GameDir 'C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo' `
  -Map de_nuke -Bots 3 -MaxSeconds 900 -MinFreeGiB 12
```

Run the helper in a root-owned background process if the root needs to use
ComputerUse concurrently. Keep its process/session receipt and observe
`PrivateRoot\status.json`; create the file `PrivateRoot\CANCEL` to stop it.
Do not stop by executable name or kill another session's process tree.

The switch is an explicit root grant, and the wrapper acquires the same
`Local\CSGORewrite.NativeFidelity` mutex as `with-native-slot.ps1`, without
waiting. It is reentrant if invoked by that wrapper from a private invocation
script. It refuses an occupied mutex. The Python executable entry point also
requires the wrapper's inherited parent-PID token. Do not call the Python entry
point directly. As with the existing native helpers, every concurrent CI/native
fidelity-session check must use the shared slot. CI is never cancelled.

The exact launch command is recorded in `status.json`:

```text
csr.exe play de_nuke --direct --deathmatch --bots 3 --size 1920 1080
```

`--size` is a **CLI request**, not an observed physical resolution. Copied
fullscreen preferences can override it: the retained `0e9aa51` Nuke run reported
2560×1440 despite this request. The helper never edits those preferences to
force a size. By default it recognizes an ordinary startup endpoint at any
positive logged `WindowContext.size`, recording `requested_cli_resolution`
separately from `actual_physical_resolution` in each record and
`observed_physical_resolutions` in status/windows. It never labels an observed
1440p run as 1080p.

To require a particular physical size, add **both** `-ExpectedWidth 2560
-ExpectedHeight 1440` to the wrapper invocation (Python receives
`--expected-resolution 2560 1440`). A mismatch prevents that report from
qualifying as ready/ordinary; its actual size and error remain recorded. This
does not change the CLI request. Pair only independently reviewed windows with
matching actual sizes. Resolution changes remain visible per record and in each
window's distinct-size list; ready is a startup milestone, not a continuing
validation of every later interval.

WindowContext reports the physical **window** size. New camera endpoints also
retain `viewport` and `physical_viewport_rect` as raw Bevy Debug values under
`parsed.world_camera_endpoint.raw_debug_fields`. The parsed rectangle is also in
`endpoint_metadata.actual_camera_viewport_pixels` as min/max/size in pixels
(null when unavailable). Window size is not proof of a
camera viewport. Old logs have no camera viewport observation; the missing field
is explicit. `viewport=None` is a logged default, distinct from an omitted field;
review `physical_viewport_rect` rather than inventing dimensions from CLI intent.

Cases are `de_dust2_2017` or `de_nuke`, each with `--bots 0` or `--bots 3`.
The latter requests three enemies and two teammates: **five bots**, not three
total. The runtime census remains the authority for actual players/bots.
`CSR_PERF_REPORT=1` is set; all inherited `CSR_*` switches are stripped first,
including `CSR_FRAME_LOG`, watch-bot and autofire switches. `RUST_LOG=info`
keeps diagnostics visible without trace spam. No screenshot, camera override,
noclip or input injection flags are accepted. Only the private data directory,
explicit Legacy source directory, no-update and private dummy Steam directory
are passed alongside the performance flag.

`build.json` uses the existing root baseline receipt schema: full `head`,
`sha256`, absolute `exe`, `profile: "release"`, `source_clean: true`,
`owner_config_unchanged: true`, and absolute `checks`. `checks` is a nonempty
JSON array of check receipts with matching `head` and zero `exit_code`.
The executable hash and verification-receipt hashes are checked again before
launch. This binds a case to the root's evidence; the helper does not rerun the
checks or prove that a manifest is honest. Shared `target`/debug executables and
installed Velopack layouts are refused. Baseline and later optimized binaries
each require their own root-verified immutable manifest.

PrivateRoot must be new, absolute, outside Git checkouts, binary bundles,
Steam, game sources and owner data (also outside its frozen bundle when named
`userdata`). Reparse paths are refused. Selected `.cfg` files are copied into a
new `userdata\config`, excluding locks/temp files. Dust II copies only the
already cached `workshop\maps\de_dust2_2017.bsp` and optional `.nav`; missing
BSP is an error, not permission to download. Nuke reads the explicit installed
Legacy source. All source copies are ordinary independent files, not links.

Protected hashes cover **all files** under the supplied owner's `config`,
Steam `config` and Steam `userdata`, plus Steam's
`steamapps\libraryfolders.vdf` and the selected cached Dust II map files.
Missing roots/files are recorded too. This includes every account's Steam
shortcuts and game configs, including CS2 configs, strictly as opaque hashes.
The runner never writes/restores any of them. Their before/after hash receipts
stay private. External changes mark the run contaminated and failed, and remain
untouched. Large Steam userdata trees can make the prelaunch/final hash audit
expensive; this audit is not part of a cadence window. These are supplied-root
audits, not a claim to snapshot arbitrary player folders elsewhere on disk or
every byte of the Steam installation.

Outputs stay private:

- `status.json` and `phases.jsonl`: started, ready, warmup, measurement and closed,
  with timestamps, stable process identity, exact CLI and close reason. Ready
  means a reported ordinary Match endpoint satisfying the optional physical-size
  contract was seen, never interval/camera admission. Status retains startup
  reports without a simulation census under `reports_without_simulation`; these
  are unbounded and cannot be assigned to simulation windows.
- `game.log`: unaltered merged stdout/stderr, including per-second diagnostics.
- `telemetry.jsonl`: wall/monotonic timestamps, latest simulation time, process
  private/working bytes, cumulative process CPU seconds, and per-engine GPU
  utilization plus dedicated/shared process GPU counters where available.
  Missing/failed counters are explicit. These are not GPU render durations,
  total GPU utilization, or authoritative GPU allocation accounting.
- `windows.json`: raw whole-interval report records conservatively bounded by
  successive simulation endpoints inside 30–60, 300–330 and 600–630 seconds.
  Boundary-straddling intervals are excluded. No pooled percentiles are derived
  from per-second percentiles. Check coverage/gaps and all raw context flags.
  `ordinary_context_endpoint` is a report classification; the existing
  `context_valid_without_camera_alive` key is retained as its compatibility alias.
- `protected-before.json`, `protected-after.json`: hashes, never config contents.

The guard checks available RAM and inventories processes about every two seconds;
telemetry is sampled about every five seconds, never per frame. Query durations
are recorded because observation has cost. Windows CIM subprocesses have an
eight-second timeout; a blocking query can delay the next guard/deadline check
by that bound. Compare builds under the same collection conditions. The owned
game loop has a bounded 660–1200 second wall deadline (default 900), minimum
12 GiB free RAM, and cancel marker. It closes only the original retained Popen
process handle; no PID reopening, name-based stop, child-tree kill or CI kill.
Shutdown has a further eight-second wait bound. Prelaunch and final hash audits
are outside the game loop deadline. A forced close may lose the buffered tail
of the log; captured full report records are retained.

Known game executable names, unknown processes under `steamapps/common`, the selected
binary name and non-CI compiler/build executables block launch or stop only the
owned case. CI ancestry is established from `Runner.Worker.exe` through parent
PIDs; an idle `Runner.Listener.exe` alone does not contaminate. Observed CI work
marks all windows contaminated even after it finishes. CI games still block the
case as other games. This inventory is not universal identification of games
outside known names/Steam paths; the root must also ensure no other game is
running. Short activity between guard samples and inaccessible process paths
cannot be ruled out. Unknown telemetry errors stay explicit.
The narrowly recognized Wallpaper Engine `wallpaper32.exe`/`wallpaper64.exe`
under its own `steamapps/common/wallpaper_engine` directory are desktop utilities;
the guard recognizes them without stopping them or treating them as another game.

All windows have `admitted: false`. Parsing is shared with `../perf_report.py`;
keep that module beside this helper's parent directory when copying the tools.
Both old `0e9aa51` and new `cc66e5e` FrameContext shapes are supported. Old logs
lack actor alive/weapon/life metadata and camera pose. New contexts include
`authoritative_local` and `authoritative_view_target` discrete actor fields
(id, life epoch, alive, team, active slot, weapon def index). They participate in
the producer's mixed-context detection; mixed or rejected samples still prevent
ordinary classification. New context actor absence, death or identity mismatch
also prevents it. Missing old observations are never filled from aggregate counts.

New separate player records contain **local and view-target endpoints only**,
including authoritative actor, weapon class/ContentId and Source-space
origin/angles. They are not per-bot pose records. View intent is a separate
endpoint. The world-camera endpoint retains Bevy transform/global matrix,
projection, FOV, target, layers and viewport debug fields; it is explicitly
`source=main_world_last interval_camera_stability=unproven`. The helper validates
numeric arrays/roles and rejects conflicting or malformed known records. Raw
Debug fields remain available without guessed coordinate conversions.

Each record has `endpoint_metadata.missing_fields`; missing keys and explicit
`None` values are also distinguished by each parsed endpoint's `missing_fields`
and `none_fields`. New fields can be absent even in a new-format context. All
windows still require `whole_interval_camera_pose` and independent coverage /
focus / input validation in `needed_metadata`. An endpoint camera is **never**
whole-interval pose evidence. External root review of full-interval integrity,
life/identity, camera, actual focus/Match/input and coverage remains necessary.
Simulation-clock regression still contaminates every window. This runner does
not replace the offline analyzer's stricter UTC/real-clock and window checks.

Fixture verification (no games/builds/Windows backend/owner data):

```powershell
python -B -m unittest discover -s tools/fidelity -p test_performance_case.py -v
```

The fixtures verify orchestration, path scoping, guards, old/new report parsing,
physical-vs-requested resolution, endpoint absence/conflicts, mixed contexts,
cancel/deadline cleanup, read-only preservation and no admission even with complete
endpoint metadata. They share synthetic format text with the analyzer tests.
They do not validate Windows CIM availability, real game startup/input/rendering,
the native slot under contention, runtime map/config consumption, or actual
CPU/GPU counters. Root must validate those using one granted native slot.
