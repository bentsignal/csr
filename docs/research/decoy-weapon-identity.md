# Decoy weapon identity

The prior implementation emitted a universal AK cue twice per second. That
played the wrong gun for an AWP, a pistol, or a suppressed M4 loadout.

The selected Legacy install's `resource/csgo_english.txt` contains the public
item description under `CSGO_Item_Desc_Decoy_Grenade`: it describes imitating the
carrier's most powerful gun. Valve's [August 17, 2017 release notes](https://blog.counter-strike.net/2017/08/19239/)
also record a fix for decoys selecting the wrong weapon. Neither source gives
a burst-scheduling formula or establishes every detail of the 2015 behavior.

The sim now selects a carried primary firearm, otherwise the carried pistol,
at the actual release event. If neither is carried, it uses the current team's
starter definition when that definition exists in the selected catalog; it does
not invent an AK fallback for an unknown content pack. This starter fallback
follows the rewrite's current loadout policy, rather than independently proving
the original no-firearm/loadout-selection edge case.

The weapon's ContentId travels with the projectile, then the activated field,
then each `DecoyShot`. Switching weapons, dying, or restoring a snapshot cannot
substitute the owner's later inventory. The client resolves that identity
through its runtime catalog and uses the same sound-entry/era selection as a
normal shot, including suppressed and content-pack definitions. A decoy stays
world audio with no player voice owner or firing animation.

Protocol 12 is required because the replicated projectile, field and event
payloads changed. Older clients are rejected during admission. Malformed or
non-weapon identities and identities attached to non-decoy utility are rejected.

## Empirical cadence candidate (October 18, source only)

The owned Legacy 1.38.8.1 decoy packets now supply actual irregular inter-shot
observations. Private originals are `native-decoy-cadence/ak-only-20261014-020417`
and `ak-awp-20261014-015235` under `csr-fidelity`. The latter's failed AK setup
is excluded; its AWP case and the separately accepted AK-only case identify
the current decoy entity and near/distant sound requests. The separate
`grenade-observation-research-20261016` review preserves their log hashes and
negative statement that the native gameplay scheduling law is not established.

The new AK/AWP candidate deterministically resamples the respective recorded
printed-time gaps (AK 0.135..4.027 seconds; AWP 1.464..5.217). This is explicitly
an **empirical approximation**, not recovery of native burst correlations,
probabilities, random-number generator or a historical 2015 scheduler. No gap
is invented from the gun's cycle time. The first-shot delay retains the previous
half-second scale as an unmeasured compatibility choice, now measured from
field activation. Gun identities without observations retain their existing
2 Hz schedule, including pistols and other-origin content-pack identities.

Each active field derives timing from its replicated id, owner and birth, so
separate decoys have independent gap sequences, and restored snapshots and
prediction reproduce the same events without serialized timer/RNG additions.
The existing 15-second lifetime bounds recomputation. Field expiry, body
presence, captured gun identity and sound/era selection remain unchanged.
Native CS2 footage is not used for decoy behavior: the owner's exception only
covers grenade throwing. No installed sound bytes or capture artifacts ship.

Focused source regressions cover variable intervals, independent identities,
64/128 Hz quantization and compatibility schedules. The existing ordinary
TickInput throw/death/snapshot regression covers event reproduction. All tests,
compilation and audible review are **UNRUN** while the parent holds the exclusive
performance slot. Exact native scheduling and other guns remain open.

The focused sim test drives pin/hold/release through `TickInput` at 64 and 128
ticks, checks primary/pistol selection, changes the owner's inventory/life, and
compares restored in-flight/active-field state and subsequent sound events.
