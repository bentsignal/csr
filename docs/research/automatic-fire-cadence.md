# Automatic-fire cadence and empty-trigger reload

Three admitted installed CS:GO Legacy AK observations recorded 30 local near
gunshot starts per magazine, excluding the distant sound layer. Their first
to last logged engine-sound spans were 2.905, 2.906 and 2.896 seconds. The first
two were slow-time captures expressed in engine-log time; the third was a
normal-speed capture. These are sound starts, not exact server fire ticks.
The installed item schema declares a 0.1-second AK cycle, or 2.9 seconds
between the first and thirtieth shots.

The rewrite's actual authoritative ray observer previously emitted that
magazine over 2.9453125 seconds at 128 ticks and 3.171875 seconds at 64 ticks.
Reassigning `next_attack = actual_tick_time + cycle` after every shot rounds
each interval upward independently. Its rounding error accumulates across
the magazine, even though the aim-ray profile itself remains correct.

## Bounded correction

An ordinary full-auto shot with the trigger still held carries the preceding
deadline forward by the authored cycle **only when it is at most one tick
overdue**. A newly admitted trigger press or a stale deadline starts a fresh
interval from the actual shot time. Cycles shorter than a tick keep the
existing one-bullet-per-tick behavior; there is no backlog or catch-up volley.
Burst-mode scheduling and its imported intervals remain unchanged.

This uses already replicated attack input and `next_attack`, with no new
weapon state fields. Protocol 22 nevertheless rejects older peers: identical
catalogs with a different scheduling or reload policy predict different state.

The target is an aggregate magazine duration within one tick of the authored
2.9 seconds, supported by the observed native sound spans. This does not claim
exact native fire ticks, a native 64-tick measurement, or measured parity for
trigger presses during deploy/reload, moving/airborne firing or arbitrary
custom-pack cycles. Recoil profiles, angles, spread and suppressor selection
are outside this change.

## Normal-mode released-empty reload

Existing admitted native captures also provide an independent reload signal
before the observer issues its next manual reload. Times below are ordinary
console-receipt seconds from each run, not exact server reload-start ticks.

| Native case | Primary released | Local clip-out sound received | Next manual `+reload` sent |
| --- | --- | --- | --- |
| AK slow magazine 1 | 122.725 | 123.040 | 123.613 |
| M4A4 slow magazine 1 | 123.638 | 124.082 | 124.327 |
| M4A4 slow magazine 2 | 158.249 | 158.739 | 158.953 |

In each case the local gun's reload sound starts before any manual reload
command, establishing an automatic reload following release. The admitted
AK also remains visibly at 0/89 while primary is held through approximately
eight wall-clock seconds after the last near shot at verified timescale 0.1;
its clip-out cue occurs only after release. This complements the normal-speed
Glock/FAMAS burst observations in [weapon-lifecycle.md](weapon-lifecycle.md).
The normal-speed final M4/AK observations close too soon to establish idle
reload, and normal-mode M4 held-empty behavior remains unmeasured.

The existing generic empty-magazine readiness/reload guards now apply on
released attack inputs for normal ammo weapons as well as burst mode. A
continuous empty trigger no longer starts automatic reload; the previous
fallback for a fresh click on an already empty weapon is preserved. There are
no gun-class-name exceptions or invented delays. Secondary-held, depleted
reserve/dry-click and other weapon-specific nuances remain open.

## Verification

Focused tests use actual `GameEvent::Fire` ticks for a full magazine, assert
one bullet per tick, preserve ammo, cover newly admitted presses and stale
gaps, and compare authority/restored/predicted snapshots at both 64 and 128
ticks. The initial cadence-only code `e806e06` passed 127 simulation and 96
netcode tests (one existing test ignored), following an all-23-workspace-package
play-profile cleanup. The installed `recoilcheck` observer emitted 30 actual
authoritative AK rays at each rate: both first-to-last spans are **2.90625
seconds**, with maximum angular error **0.000002 degrees** against the unchanged
attached recoil trajectory. Its immutable executable, source/cwd/hash receipts
and raw output are retained privately.

The subsequent generic idle-reload correction adds normal AK/M4 released/held
and snapshot replay coverage at both tick rates. Its checks, final integrated
workspace/Legacy CI and any rewritten rendered reload observation are pending.
The measured cadence result is not presented as verification of that later
reload change or as exact native tick parity.

Native raw logs and aggregate receipts remain private under
`measured-recoil-20261015/native-near-shot-timestamp-spans-private.json`.
