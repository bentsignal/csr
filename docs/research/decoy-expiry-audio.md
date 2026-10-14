# Decoy expiry audio

Two owned, stationary Legacy 1.38.8.1/build 8802 observations used the existing
`snd_showstart 2`/NetCon sound log. The accepted AK case had active-player,
stable-position, controlled AK fire, and real decoy pin/throw postconditions.
The AWP case had real pin/throw and a separately reset map. The earlier AK
case without a verified throw was rejected.

Both accepted decoys requested HE-style near/distant terminal sounds from the
same sound-source entity as their gunfire. AK requested
`weapons/hegrenade/hegrenade_detonate_02.wav` and distant `_01.wav`;
AWP requested near `_03.wav` and distant `_02.wav`. These paths match the
installed `BaseGrenade.Explode` random-wave set and its
`BaseGrenade.ExplodeDistant` operator partner. These are sound-start
observations, not PCM onset, audible-mix or damage measurements. Private raw
receipts remain outside the repository (`ak-only-20261014-020417` and
`ak-awp-20261014-015235`).

The runtime VFS imports that entry under `csgo_sound_decoy_expire`, preserving
its authored gain, pitch, wave selection and distant partner. A mounted pack
can supply the soundscript and referenced audio; no installed bytes are
bundled. Playback uses the authoritative field position, even for the owner.
Missing data stays silent. Existing spatial mixing remains subject to fidelity
verification.

Protocol 14 appends `DecoyExpired { player, pos }` after protocol 13's
`UtilityBounced`. A Decoy field emits it once when its existing `until` is
reached, immediately before removal. Activation and round cleanup do not emit
it. Admission requires a nonzero owner and finite bounded position; earlier
protocol peers are rejected. No snapshot fields change.

The existing 15-second field lifetime and fixed half-second shot scheduler are
unchanged and provisional. Observed AK and AWP intervals differ from that
scheduler, but two traces do not establish its random rules. No terminal
extra gunshot, visual explosion, or expiry damage is inferred here. Original
2015 behaviour, exact cadence and lifetime still require evidence.
