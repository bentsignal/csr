# Automatic-fire tick quantization

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
catalogs with a different scheduling policy would predict different shots.

The target is an aggregate magazine duration within one tick of the authored
2.9 seconds, supported by the observed native sound spans. This does not claim
exact native fire ticks, a native 64-tick measurement, or measured parity for
trigger presses during deploy/reload, moving/airborne firing or arbitrary
custom-pack cycles. Recoil profiles, angles, spread and suppressor selection
are outside this change.

## Verification

Focused tests use actual `GameEvent::Fire` ticks for a full magazine, assert
one bullet per tick, preserve ammo, cover newly admitted presses and stale
gaps, and compare authority/restored/predicted snapshots at both 64 and 128
ticks. Their execution and the installed `recoilcheck` actual-ray observer
are pending the shared Cargo slot. Required workspace/Legacy checks and any
rendered observation remain pending; no after-result is claimed yet.

Native raw logs and aggregate receipts remain private under
`measured-recoil-20261015/native-near-shot-timestamp-spans-private.json`.
