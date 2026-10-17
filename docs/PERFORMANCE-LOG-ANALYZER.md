# Offline gameplay performance log analysis

`tools/perf_report.py` reads the **actual diagnostic text contract** in
`5dc2b5064aa6914eb83e49b060f7d8645fe2c607:crates/client/src/frame_report.rs`
and `input_latency.rs` (included in diagnostics baseline `0e9aa51`). It uses
only Python's standard library. It never launches a game, queries Steam,
changes configuration, builds Rust, or inspects a running process. Fixtures
are synthetic text, with no third-party assets or source.

This tool implements analysis infrastructure for PF01/PF02. It establishes
no performance improvement, FPS target, native parity, or latency result.
The frozen owner candidate is unaffected. Root owns actual run admission.

## Root commands after collecting logs

Run these from the checkout containing this change, outside the shared build
target. The input paths are placeholders for root's future collected evidence.
`--out` must name a **new directory**; existing evidence is never overwritten.

```powershell
python -B -m unittest discover -s tools/tests -p test_perf_report.py -v
python -B tools/perf_report.py D:/private/baseline/raw.log --receipt D:/private/baseline/run.json --out D:/private/analysis/baseline
python -B tools/perf_report.py D:/private/baseline/raw.log --receipt D:/private/baseline/run.json --compare-log D:/private/candidate/raw.log --compare-receipt D:/private/candidate/run.json --out D:/private/analysis/pair
```

Without `--receipt`, the analyzer still archives/parses the log and reports
interval exclusions, but invents no early/late windows. Exit codes: `0` means
the supplied descriptive windows/metadata passed validation; `2` means a
log, receipt, window, or paired metadata check failed; `1` means input/output
failure. **Exit 0 is never performance admission.** With current logs,
`paired_admission` is always false, including for metadata-aligned pairs.

Outputs include byte-for-byte `raw.log` (ANSI and original timestamps retained),
`receipt.json`, `report.json`, and `report.md`. SHA-256 and byte length identify
the raw evidence; receipt bytes get their own hash. Pair mode also retains
candidate inputs/reports and emits `comparison.json`/`comparison.md`.
The analyzer does not read executables or follow paths embedded in receipts.
Build hashes, clean source, immutable copy, and intended camera are assertions
from root's build/run receipts, not independently verified by this tool.

## Helper receipt contract (version 1)

The collection helper must preserve its actual phase transitions with UTC
and simulation-time bounds. It must write a small run receipt **after the raw
log is complete**, binding its SHA-256. Combine the immutable build receipt
with case and phase records; a bare `immutable/build.json` is insufficient.
For the root baseline helper, copy its `head`, `sha256`, `profile`, `exe`,
`source_working_directory` and `source_clean` fields unchanged into `build`.
Set `immutable_executable: true` only when the executed binary is the retained
immutable copy, rather than a mutable shared-target executable.

The following is a synthetic schema example, not measured evidence. Replace
all example values, including hash strings, with actual recorded values.

```json
{
  "schema_version": 1,
  "log_sha256": "<64 hex characters: SHA-256 of final raw log bytes>",
  "build": {
    "head": "<40 hex characters: source commit>",
    "sha256": "<64 hex characters: executed immutable executable>",
    "profile": "release",
    "exe": "D:/private/baseline/immutable/csr.exe",
    "source_working_directory": "D:/dev/global-offensive-post-playtest",
    "source_clean": true,
    "immutable_executable": true
  },
  "case": {
    "map": "de_nuke",
    "bots": 2,
    "players": 3,
    "tick_rate": 128,
    "competitive": false,
    "resolution": [1920, 1080],
    "present_requested": "Immediate",
    "frame_latency_hint": 1,
    "camera": {"position": [1, 2, 3], "pitch": 0, "yaw": 90}
  },
  "windows": [
    {
      "name": "early",
      "phase": "stable_match",
      "utc_start": "2026-10-16T23:00:01Z",
      "utc_end": "2026-10-16T23:00:03Z",
      "sim_start_s": 1,
      "sim_end_s": 3
    },
    {
      "name": "late",
      "phase": "stable_match",
      "utc_start": "2026-10-16T23:00:04Z",
      "utc_end": "2026-10-16T23:00:06Z",
      "sim_start_s": 4,
      "sim_end_s": 6
    }
  ]
}
```

`camera` describes the intended matched pose/aim, never proof that the camera
stayed there. Pair metadata must match the full case (including intended
camera), build **profile**, window names, and simulation-time bounds. Build
hashes/commits can differ between baseline and candidate and remain retained
for review. Each case's actual map, bots/players, tick rate, competitive mode,
physical resolution, requested present mode, and latency hint must agree with
its logged observations. Present mode/hint are engine policy, not evidence of
actual OS presentation, compositor behavior, scanout, or external latency.

Collector phase rules:

1. Preserve startup, loading, play, pause, focus changes and transitions as
   distinct helper phases. Choose `early` only after independently observing
   stable Match play; do not rename the beginning of a process as gameplay.
2. Bound each `stable_match` phase with helper UTC timestamps **and actual
   logged `sim_seconds`**. Do not infer simulation age from process wall time,
   frames, or the requested tick rate. Use the same simulation-age ranges for
   paired runs. `early` and `late` must be ordered and disjoint.
3. Leave a complete prior stable report as a boundary witness. Each selected
   interval needs a prior matching context/case and advancing simulation
   endpoint. Loading followed by a Match endpoint is insufficient. Each
   window needs at least two complete contiguous eligible intervals.
4. Review selected `interval_lines`, observed simulation/UTC bounds and edge
   coverage. Intervals crossing helper boundaries are omitted, not split or
   prorated. More than one interval of uncovered UTC/simulation edge invalidates
   the window. Any fully contained mixed/excluded interval invalidates it.

UTC is parsed from Bevy RFC3339 log prefixes after stripping ANSI for parsing
only. `utc_start` is an **estimate** from the report line UTC minus its printed
`report_wall_s`; simulation start is bracketed by the previous census endpoint.
Logger execution and millisecond printing introduce boundary uncertainty;
helper bounds should allow margin. The tool rejects reordered/backwards UTC,
real-clock resets, gaps between reports, and disagreement of successive
UTC/elapsed-real/report-wall deltas above 50 ms. This is a conservative
integrity gate, not clock synchronization proof. Review any exclusion rather
than raising the tolerance to make a run pass. Tick/tick-rate consistency is
checked against actual `sim_seconds` within 2 ms of printed/f32 precision.

## Interpretation and hard admission limits

- `frames` and `mean_ms` cover all valid interval samples; the window mean
  weights interval means by `frames`. `sampled_real_s` is the sum of sampled
  frame deltas and stays separate from wall-clock report duration. A second
  sample-time-derived mean exposes their printed precision. The window maximum
  is the maximum of all reported interval maxima.
- Source quantiles use exact nearest rank: sorted index `ceil(n*p/100)-1`.
  Raw per-frame samples are **not** logged, so the analyzer preserves and
  validates the source's interval values; it cannot reconstruct a window
  distribution. It reports the min/max **range of interval quantiles**, never
  their average as an overall p95. `overall_quantiles_ms` is null. The source's
  8192-sample overflow retains the all-frame mean/max, but publishes no
  quantiles; the analyzer rejects any purported prefix quantiles on overflow.
- Mixed context, unfocused window, non-Match state, menu page, screenshot,
  disabled input, noclip, spectating, absent window/local data, rejected
  samples, missing context/censuses, and inconsistent metadata exclude an
  interval. Counts remain endpoint observations. An invalid interval inside a
  helper window prevents that window passing, rather than disappearing from
  its mean. Clock/log integrity failures invalidate all descriptive windows.
- CPU scopes are wall time, including waits. `render.prepare`, `render.submit`,
  `render.surface`, character work, and simulation subscopes can overlap or
  nest under `main`/`render`. Per-name calls, totals, mean and maximum remain
  separate; adding all names double-counts work. No CPU/GPU bottleneck verdict
  follows from the scope with the largest number.
- Render diagnostics retain Bevy history (including overlapping histories
  between reports). Sample counts and units are preserved; means are presented
  as history-mean ranges, never summed or relabeled interval GPU totals/p95.
- Census first/last/min/max, plateau/directional patterns, slope per actual
  simulation second, and correlation with mean frame time describe drift.
  A constant count has undefined correlation (null). Early/late deltas and
  correlations establish co-occurrence, not causation, leak proof or a fix.
- **Camera pose/aim and local-player alive state are NOT LOGGED.** Aggregate
  `alive`, a stable local id and a receipt's intended camera do not supply
  those observations. They remain explicit missing paired-admission
  requirements. Later instrumentation/independent aligned evidence needs a
  separately reviewed admission contract; this analyzer never silently
  accepts receipt intentions as actual camera/local-alive measurements.
- No external presentation trace is available here. Merely finding `wpr.exe`
  does not create one. These reports cannot establish mouse-to-display latency.

Synthetic tests cover the actual Rust debug spelling, ANSI/UTC byte retention,
sample math/nearest rank/overflow, nested scopes, render-history semantics,
context/receipt rejection, clock reordering/reset/drift, phase boundaries,
body/asset plateaus followed by retained-count and frame-mean drift, and
read-only input/new-output CLI behavior. Actual new logs still need root review
after the admitted immutable diagnostic binary runs.
