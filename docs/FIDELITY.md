# Fidelity: measuring against the real game

VISION §7 asks for 1:1 fidelity, measured, never eyeballed. This page is how
we measure it and what the measurements say. The reference is CS:GO Legacy
(Steam app 4465480) installed on the dev machine; never CS2.

**Never commit captures.** Screenshots and recordings of the real game
contain Valve art. The harness writes them to `%USERPROFILE%\csr-fidelity`
(outside the repository; it refuses a folder inside it). Only numbers and
observations go into this file.

`CSR_FRAME_LOG=<milliseconds>` enables the client's existing schedule scopes
and Bevy 0.19.1's `RenderDiagnosticsPlugin`. Once per second, `render diagnostic`
lines report the retained-history mean and sample count for recently updated
render passes. The `/elapsed_cpu` and `/elapsed_gpu` paths distinguish CPU pass
encoding from GPU execution; pipeline-statistics paths retain their original
units. Missing GPU paths mean no supported result was supplied, not zero GPU
cost. Nested pass times are not additive. Diagnostic query collection adds
overhead, so use these runs to locate bottlenecks rather than claim normal-play
FPS gains. Normal play without this switch installs neither the scopes nor
the render-diagnostics plugin.

## The harness

`tools/fidelity/fidelity.ps1` (PowerShell 5.1, no extra installs; the C#
helpers in `FidelityKit.cs` are compiled on the fly):

```powershell
cargo build --release -p launcher
powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 all      # real + ours + compare
powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 ours     # re-render ours only
powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 compare  # metrics for existing pairs
powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 sounds   # real-game sound logs
powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 sounds-ours # matching client sound logs
powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 quit     # close the game, clean up
```

`-Only name1,name2` limits a run to some viewpoints or sound probes;
`-KeepGame` leaves the game running between runs.
`-Csr target\play\csr.exe` selects the day-to-day build. Sound probes use
isolated player data under the output folder, ordinary `TickInput`s and a
simulation-time capture deadline. `CSR_SOUND_PROBE` is ignored in interactive
sessions. Console commands are acknowledged before a probe's waits begin;
the real game restarts after freeze-time settings are applied.

Before a native launch, the harness snapshots the active Steam account's
Legacy-named config/video files and their existence under both supported app
IDs. Exact bytes are retained privately in the session record. After its owned
process exits, it restores those bytes or removes only known files that did not
exist before launch. Restoration refuses reparse points, invalid snapshots and
any live Legacy process. Older sessions without snapshots cannot reconstruct
or restore their previous preferences. The fake-directory session tests cover
this boundary without reading the runner's real player profile.

The harness records its process ID/start time and the existing byproduct
paths before launch. It closes only its own game process, preserves existing
files, skips junctions and removes new known byproducts without recursive
directory deletion. `test-session.ps1` checks this on a temporary fixture in CI.

How it works:

- **Launch.** `csgo.exe -insecure -novid -nojoy -windowed -noborder -w 1600
  -h 900 -netconport 27999 +map de_dust2`. Steam must be running (it is
  needed to start the game, not to play ours). `-netconport` opens the
  game's console on a local TCP port; the script sends commands and reads
  every console line with a timestamp. It waits until `status` shows the
  local player as active.
- **Scene setup** (console): `sv_cheats 1`, `jointeam 3 1`, `bot_quota 0`,
  `bot_kick`, `mp_warmup_end`, `mp_freezetime 0`,
  `mp_ignore_round_win_conditions 1`, `mp_backup_round_auto 0`,
  `cl_drawhud 0`, `r_drawviewmodel 0`, `cl_showpos 0`, `cl_showfps 0`,
  `net_graph 0`.
- **Viewpoints** live in `tools/fidelity/viewpoints.txt` as eye positions.
  `getpos` prints the eye (origin + 64.0626, CS:GO's standing view offset);
  `setpos` takes the origin. The script sets `noclip`, `setpos`, `setang`,
  waits, reads `getpos` back (the engine may nudge the player) and records
  the eye it actually used in `viewpoints-resolved.txt`; ours renders from
  exactly that point.
- **Capture.** The script captures the window itself (Win32 `PrintWindow`
  with `PW_RENDERFULLCONTENT`, falling back to a screen copy); the game's
  own screenshot command is not used.
- **Ours.** `csr play <map> --bots 0 --size 1600 900 --camera x y z pitch
  yaw --screenshot <file>` with `CSR_NO_HUD=1` and `CSR_NO_SELF=1` (hides
  our own body, which otherwise stands at spawn while the noclip camera
  flies). Our vertical FOV is 73.74°, which is CS:GO's 90° at 4:3 widened to
  16:9; matched pairs line up to the pixel (checked on `ct_garage`).
- **Metrics** per pair (`compare`, written to `metrics-<time>.tsv`): mean
  absolute error over RGB (0-255), block SSIM on luma (8x8 windows, 1 =
  identical structure), mean luma and mean RGB of each image, and luma
  percentiles p05/p50/p95 (real/ours) for contrast and tonemapping. A
  false-colour `_diff.png` is written beside each pair.
- **Sounds.** `sounds` runs the scenarios in `tools/fidelity/soundprobes.txt`
  with `sv_max_allowed_developer 1; developer 1; snd_showstart 2`. Every
  sound start is printed with its wave, entity, channel, soundlevel, volume,
  engine time and per-speaker gain (`lvol`/`rvol`). Logs go to
  `csr-fidelity\sounds\<scenario>.log`. For distance curves,
  `snd_playsounds <entry> x y z` plays an entry at a chosen point and
  `playgamesound <entry>` plays it at the listener.
- **Cleanup.** The install is read-only for us, but a session makes the game
  write round backups (`backup_roundNN.txt`), a nav graph cache
  (`maps\graphs\*.ain`) and a copy of every Workshop map the Steam account
  subscribes to (`maps\workshop`, 4.7 GB on the dev machine). `quit` (and
  every command that started the game) deletes exactly those, if created
  after the session began. Nothing else in the install is touched.

## Visual baseline (2026-10-11, de_dust2, before Track D fixes)

| Viewpoint | MAE | SSIM | Luma real/ours | p05 real/ours | p95 real/ours |
|---|---|---|---|---|---|
| ct_garage | 15.0 | 0.626 | 51.9 / 55.8 | 14.2 / 12.2 | 90.7 / 103.3 |
| ct_spawn_mid | 11.6 | 0.717 | 41.2 / 42.4 | 10.5 / 10.1 | 77.1 / 69.4 |
| a_site | 19.8 | 0.704 | 107.2 / 115.2 | 45.6 / 60.2 | 178.3 / 168.7 |
| a_long | 23.7 | 0.641 | 125.4 / 138.0 | 52.9 / 77.1 | 193.7 / 192.4 |
| long_doors | 18.5 | 0.767 | 38.8 / 56.3 | 15.5 / 23.7 | 55.5 / 106.7 |
| outside_long | 21.0 | 0.659 | 139.0 / 142.9 | 48.6 / 61.6 | 201.6 / 193.0 |
| mid_top | 23.7 | 0.639 | 129.9 / 140.3 | 55.5 / 64.5 | 201.4 / 195.1 |
| upper_tunnel | 43.0 | 0.594 | 54.4 / 97.7 | 21.3 / 39.8 | 132.7 / 180.6 |
| b_site | 30.0 | 0.531 | 123.5 / 138.5 | 40.6 / 68.1 | 188.4 / 180.5 |
| t_spawn | 26.6 | 0.614 | 121.8 / 139.0 | 50.0 / 74.0 | 202.6 / 199.2 |
| sky_a | 16.9 | 0.868 | 158.8 / 160.0 | 108.1 / 127.6 | 201.9 / 188.9 |
| **mean** | **22.7** | **0.669** | | | |

What the pairs show, largest first:

1. **Too little contrast.** Ours lifts the shadows (p05 is 10-28 higher in
   every sunlit view) and compresses highlights (p95 lower). Surfaces look
   flat and washed out.
2. **Static props far too bright and too neutral.** Crates, barrels and
   doors are near-white in ours and dark, warm grey in the real game
   (`long_doors`, `upper_tunnel`). Props use an ambient cube plus sun
   instead of the baked per-vertex lighting (`sp_hdr_*.vhv`).
3. **Missing or mis-lit lightmap regions.** Some brush faces are fully lit
   in ours and shaded in the real game (the wall behind the long-doors gate;
   the sun shafts on the upper-tunnel walls).
4. **Ground textures soft and low in detail** at grazing angles (b_site,
   t_spawn): the real game shows crisp pebbles and paving.
5. **No detail sprites** (grass tufts under the CT-spawn ledge) and no
   self-illuminated lamp bulbs.
6. Smaller: cables, colour correction (`materials/correction/cc_dust2.raw`
   is not applied), decal blobs.

## Sound audit

Facts measured with the harness (`snd_showstart`) on CS:GO Legacy
1.38.8.1, compared with what our client played before Track D.

### What plays, per event

| Event | Real game | Ours (before) |
|---|---|---|
| Own gunfire | `Weapon_X.Single` (CHAN_STATIC) plus `Weapon_X.SingleDistant`, cross-faded by distance; at most 2 overlapping shots per entity (`CS_distance_weapon_start`) | `Single` only, unlimited overlap |
| Others' gunfire | Same, spatial; gain flat to 400 u, then falls to ~0.05 at 3937 u; distant layer fades in from 800 to 2800 u | rodio inverse-square with 1 m = 157 u: −26 dB too quiet at 1000 u, −30 dB at 2000 u, no distant layer |
| Reload | `Weapon_AK47.Clipout`, `.Clipin`, `.BoltPull` at animation events (clip-out to clip-in 0.699 s, clip-in to bolt 0.671 s) | nothing |
| Deploy | `Weapon_X.Draw` (vol 0.3) then animation-event foley (AK bolt pull 0.32 s after draw; P2000 slide back/release at 0.22/0.42 s); knife `knife_deploy1` | nothing |
| Footsteps | `CT_`/`T_` + surface `stepleft`/`stepright` (e.g. `CT_Gravel.StepLeft`), alternating, plus `CT_Default.Suit` (vol 0.01-0.05). Period 0.297 s at 250 u/s, 0.390 s at 140-215 u/s; first step 0.44 s after starting; silent at walk speed (130 u/s); audible from ~140 u/s | nothing |
| Jump / land | `Default.WalkJump` on take-off; `Land_<Surface>.StepLeft` (e.g. `land_gravel_04`) plus `CT_Default.SuitLand` (vol 0.2) on landing | nothing |
| Bullet impacts | Surface's `bulletimpact` entry from `scripts/surfaceproperties_cs.txt` via the material's `$surfaceprop` (e.g. dust ground: `sand_impact_bullet`), plus ricochets (`weapons/fx/rics`) and near-miss whizzes (`weapons/fx/nearmiss`) | guessed from texture-name substrings (four families); no ricochets or whizzes |
| Announcer, UI | `Event.TERWin`, `UIPanorama.*`, radio | played, but too loud (see levels) |
| Ambience | `env_soundscape` (Dust II: 57 entities, `dust2_new.*`): wind loops, distant city, random rockfall, cable stress, sand gusts | nothing |
| Music | Valve kit cues (`roundtenseccount`, round start, bomb, won/lost) | nothing |

### Levels at the listener

`lvol` at the listener, divided by the entry volume and normalised to the
AK-47 shot (16.83 / 0.9). Ours divides the same way using the soundscript
volume alone.

| Sound | Real (per unit volume) | Ours (before) |
|---|---|---|
| Gunfire (`Weapon_AK47.Single`) | 1.00 | 1.00 |
| Reload / damage foley (`Clipout`, `Player.DamageKevlar`) | 1.00 | — / 1.00 |
| Announcer (`Event.TERWin`), UI (`buymenu_purchase`, `cant_buy`) | 0.58 | 1.00 (+4.7 dB) |
| Player voice (`sas.radio.letsgo01`) | 0.29 | 1.00 (+10.7 dB) |
| Bullet impact (`Concrete.BulletImpact`) | 0.29 | 1.00 (+10.7 dB) |
| Own footstep | 0.18 | — |

### Distance curves

Per-speaker gain relative to the same sound 30-100 u in front, measured with
`snd_playsounds` along the open Mid corridor:

| Distance (u) | 150 | 200 | 300 | 400 | 600 | 800 | 1000 | 1400 | 2000 |
|---|---|---|---|---|---|---|---|---|---|
| Gunfire (`CS_update_weapon`) | 1.0 | 1.0 | 1.0 | 1.0 | 0.63 | 0.56 | 0.48 | 0.30 | 0.13 |
| Footsteps (`CS_update_footsteps`) | 0.72 | 0.65 | 0.54 | 0.42 | 0.24 | 0.14 | | | |
| Foley (`CS_update_foley`) | 1.0 | 1.0 | 1.0 | 1.0 | 0.40 | 0.23 | 0.06 | | |
| Impacts (`CS_update_rics`) | 0.94 | 0.83 | 0.62 | 0.45 | | | | | |
| Plain entry, `SNDLVL_60dB` (C4 beep) | 0.22 | 0.16 | 0.10 | | | | | | |
| Plain entry, `SNDLVL_65dB` (knife hit) | 0.40 | 0.30 | 0.18 | 0.14 | 0.08 | 0.06 | | | |
| Plain entry, `SNDLVL_75dB` (smoke bounce) | 0.96 | 0.91 | 0.75 | 0.44 | 0.29 | 0.20 | 0.13 | 0.10 | |
| Plain entry, `SNDLVL_80dB` (`c4.plant`) | 0.95 | 0.93 | 0.87 | 0.75 | 0.41 | 0.28 | 0.23 | 0.13 | |
| Ours before, any entry (1/d², 1 m = 157 u) | 1.0 | 0.62 | 0.27 | 0.15 | 0.07 | 0.04 | 0.02 | 0.01 | 0.006 |

Observations that shape the model:

- The operator stacks in `scripts/sound_operator_stacks.txt` define
  `calc_falloff_curve` with a min and max distance per stack (weapon
  400/3937, foley 400/1100, footsteps 120/1100, impacts 120/600). Measured
  gain depends only on `x = (d - min) / (max - min)`: weapons, foley and
  footsteps all follow ~`0.78 · e^(-2.5x)` past `min` (curve amount 0.7);
  impacts (curve amount 0.1) follow ~`e^(-1.3x)`.
- Entries without a stack attenuate as `min(1, r / d)` with
  `r ≈ 32 u · 10^((soundlevel - 60) / 22)` (empirical radius fit; inverse distance, 6 dB per
  doubling), measured at 60, 65, 75 and 80 dB. `SNDLVL_NONE`/`0` does not
  attenuate (`c4.explode`, voice lines).
- Gunfire cross-fades with the `…SingleDistant` entry: the near entry is
  scaled by `1 - ((d - 800) / 2000)^1.2`, the distant one by
  `1 - ((2800 - d) / 2000)^1.2` (the stack's `math_remap_float` and `pow`
  operators, confirmed by the measured distant gain).
- Pitch and volume ranges (`"100, 105"`, `"0.1, 0.6"`) are drawn per play.
- A source in front at 30-400 u is 5 dB quieter per ear than the same sound
  at the listener (stereo panning of a centred source).

### Track D sound foundation review (2026-10-11)

The client now reads soundscript mixing parameters and MDL sound events,
resolves surface properties for steps and impacts, and plays jump/landing
foley. Footsteps and animation-event scheduling follow simulation ticks,
including ticks processed together in one rendered frame. Initial weapon
deployment is audible too. Channel limits discard finished entities instead
of assuming a fixed lifetime.

Remote footsteps consume elapsed authoritative ticks, rather than one tick
per received snapshot. Duplicate ticks do not advance cadence; death,
removal, changed life epochs and clock discontinuities discard old animation
queues and cadence. New motion starts its measured timer when observed.
Offline per-tick onset, thresholds and periods remain unchanged. Jump and
landing cues continue to come from reliable authoritative events.

For steady movement sampled every four or eight ticks at 64/128 Hz, tests
verify the same alternating steps as per-tick playback, delayed until the
next observed snapshot (at most interval minus one ticks). Intermediate
speed, grounding and surface changes cannot be recovered from sparse
snapshots. Large gaps emit at most one current step and discard whole-period
overruns, without bursts of missing steps. This is a bounded fallback, not a
claim of exact fidelity across unknown movement or long gaps.

Matched Dust II probes from Legacy 1.38.8.1 and the client are reproducible
with `sounds` / `sounds-ours`. Private evidence is outside the repository
under `%USERPROFILE%\csr-fidelity\sound-final`; an earlier comparison is in
`sound-reviewed`. The earlier comparison measured:

| Probe | Legacy | Client |
|---|---|---|
| AK reload | Clipout → Clipin 0.696 s; Clipin → BoltPull 0.666 s | Same files, 0.693 / 0.660 s (within one 128 Hz tick) |
| Running over CT-spawn gravel | Alternating gravel steps, mean period 0.297 s | Alternating `CT_Gravel` entries, mean period 0.299 s |
| Walking / crouching | No gravel steps | No gravel steps |
| Jump / land | `sand5.wav`, `land_gravel_*`, CT suit landing | Same file families and events |
| CT-spawn impacts | Concrete and sand bullet-impact families | Material surface properties select the same families |

Two backend corrections have tests against **actual rodio samples**: with
its inverse-square falloff bypassed, rodio 0.22's distance-difference panner
boosts the farther virtual ear, requiring exchanged ear positions; its
spatial downmix sums channels without averaging, requiring division by the
input channel count. Local stereo is preserved. Audio is validated with the
decoder before creating a Bevy source. `soundcheck` now covers weapons,
feedback and foley: 1,425 files decode, none are unsupported or fail.

This lands a sound foundation, not a claim of 1:1 audio parity. Draw-to-foley
timing still differs (P2000 slide-back 0.277 s versus 0.219 s, AK bolt 0.367 s
versus 0.322 s in that comparison); first-step onset also differs and needs
controlled movement measurements. The fixture can produce an initial
landing after teleporting above the floor. Gain curves are approximate fits;
DSP, occlusion, ricochets/near misses, soundscape integration and music remain
open. The raw-wave/fade helpers are groundwork, not an ambient implementation.
