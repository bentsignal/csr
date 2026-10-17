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
  means a reported ordinary Match endpoint was seen, not camera/life admission.
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

Known game executable names, any process under `steamapps/common`, the selected
binary name and non-CI compiler/build executables block launch or stop only the
owned case. CI ancestry is established from `Runner.Worker.exe` through parent
PIDs; an idle `Runner.Listener.exe` alone does not contaminate. Observed CI work
marks all windows contaminated even after it finishes. CI games still block the
case as other games. This inventory is not universal identification of games
outside known names/Steam paths; the root must also ensure no other game is
running. Short activity between guard samples and inaccessible process paths
cannot be ruled out. Unknown telemetry errors stay explicit.

All windows have `admitted: false`. At diagnostics commit `5dc2b50`, reports
cover focus, app state/page, input enabled, screenshot mode, map, viewport,
spectating/noclip and mixed context. They **lack camera origin, camera angles
and local-player alive state**. The helper flags `needed_metadata` for these
fields. External root evidence and/or added diagnostics must establish camera,
life state, actual focus/Match/input and window coverage before admitting any
comparison. Simulation-clock regression also contaminates every window.

Fixture verification (no games/builds/Windows backend/owner data):

```powershell
python -B -m unittest discover -s tools/fidelity -p test_performance_case.py -v
```

The fixtures verify orchestration, path scoping, guards, report classification,
cancel/deadline cleanup, read-only preservation and missing-metadata behavior.
They do not validate Windows CIM availability, real game startup/input/rendering,
the native slot under contention, runtime map/config consumption, or actual
CPU/GPU counters. Root must validate those using one granted native slot.
