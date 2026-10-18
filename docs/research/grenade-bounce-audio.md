# Grenade contact audio

Legacy 1.38.8.1's installed sound manifest and `game_sounds_weapons.txt`
declare `HEGrenade.Bounce`, `Flashbang.Bounce`, `SmokeGrenade.Bounce` and
`IncGrenade.Bounce`. Their referenced waves exist in the installed VPK.
Flash/smoke share identical wave bytes, as do HE/incendiary. No dedicated Decoy or
Molotov bounce entry or item binding was found. The later owned decoy observation
below establishes its flashbang contact binding; Molotov remains unsupported.
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

### Decoy binding and final contacts (October 18 source candidate)

The retained owned Legacy 1.38.8.1 AK and AWP decoy packets both request
`weapons/flashbang/grenade_hit1.wav` from the identified decoy entity, at
volume 0.60, channel 3 and soundlevel 75. Private originals are under
`csr-fidelity/native-decoy-cadence/ak-only-20261014-020417` and
`ak-awp-20261014-015235`. This runtime evidence supplies the previously missing
decoy binding: import the selected VFS's `Flashbang.Bounce` under a separate
decoy ContentId, preserving its authored wave list and cue properties. Missing
entries stay silent. No captured audio or installed bytes are committed.

Final soft contacts now emit once even when the solver sets outgoing velocity
to zero. The existing pre-step `settled` state suppresses later gravity/nudge
chatter, while existing support-loss logic permits a subsequent real fall and
contact. Fire impact ignition still suppresses a duplicate contact cue. This
does not reproduce duplicate native sound requests or establish exact contact
cadence, ballistics or original mixing. The focused 64/128 Hz final-tap and
rest-cycle regression and VFS binding checks are **UNRUN**, under the parent's
exclusive performance slot. No Cargo, game or native executable was run.
