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

## Cadence remains unmeasured

The existing twice-per-second schedule is retained as an explicit unfinished
approximation. A weapon's ordinary `cycle_time` does not establish how a decoy
groups shots or pauses. The owner's recollection of alternating bursts/taps is
a measurement target, not grounds for introducing a new random scheduler.
Use a bounded offline Legacy observation or clearly dated original-game footage
to establish the AK/AWP/pistol cadence before changing that law. Modern CS2
footage and leaked/decompiled game code are not references for this work.

The focused sim test drives pin/hold/release through `TickInput` at 64 and 128
ticks, checks primary/pistol selection, changes the owner's inventory/life, and
compares restored in-flight/active-field state and subsequent sound events.
