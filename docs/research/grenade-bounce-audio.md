# Grenade contact audio

Legacy 1.38.8.1's installed sound manifest and `game_sounds_weapons.txt`
declare `HEGrenade.Bounce`, `Flashbang.Bounce`, `SmokeGrenade.Bounce` and
`IncGrenade.Bounce`. Their referenced waves exist in the installed VPK.
Flash/smoke share identical wave bytes, as do HE/incendiary. No Decoy or
Molotov bounce entry or item binding was found; their contacts stay silent.
This data audit does not establish original runtime cadence or 2015 mixing.

The selected VFS soundscripts supply all wave paths, volume, pitch,
soundlevel and channel data. The importer assigns the stable sound IDs
`csgo_sound_hegrenade_bounce`, `csgo_sound_flashbang_bounce`,
`csgo_sound_smokegrenade_bounce` and `csgo_sound_incgrenade_bounce`.
Audio resolves through the same VFS as mounted private content, with no
game bytes or install paths compiled into the feature. No new loudness curve
or speed multiplier is applied. Each cue plays at the authoritative grenade
contact position, including grenades thrown by the listener.

Protocol 13 appends `UtilityBounced { player, kind, pos }` to the event enum.
Existing event discriminants and snapshot fields remain unchanged. Contact
events require a valid kind, nonzero owner and finite bounded position;
older peers are rejected at admission. Clients never infer contacts from
render interpolation. Fire grenades that detonate on contact do not also
emit a bounce cue.

The existing solver's zero-velocity floor settlement suppresses soft final
contacts. This conservative policy also prevents gravity/separation-nudge
chatter at rest, without adding a threshold or replicated resting state.
Final soft settling taps are not yet reproduced. Native contact cadence,
ballistics and exact original mix remain unmeasured; no parity is claimed.
