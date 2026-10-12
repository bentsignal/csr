# Weapon shot identity and the 2015 sound target

The owner's manual playtest reported an unsuppressed M4A1-S shot and an AK-47
sound that did not match the requested 2015 era. These are separate issues:
the first has an installed-schema selection bug; the second uses a verified
retained waveform pair, while broader era mixing/DSP fidelity remains unmeasured.

## Installed selection evidence

Read-only source: CS:GO Legacy app 4465480, version 1.38.8.1,
`C:/Program Files (x86)/Steam/steamapps/common/csgo legacy/csgo`.
`scripts/items/items_game.txt` SHA256:
`510e09b68a01d88edba2342972960025fd6484aaaa58365fcf47d8513de79623`.
The sound manifest resolves the entries below from
`scripts/game_sounds_weapons.txt` in `pak01_dir.vpk`.

| Definition | Resolved `has silencer` | `sound_single_shot` | `sound_special1` |
|---|---|---|---|
| `weapon_m4a1_silencer` | 1 | `Weapon_M4A1.SilencedSingle` | `Weapon_M4A1.Silenced` |
| `weapon_usp_silencer` | 1 | `Weapon_USP.Single` | `Weapon_USP.SilencedShot` |
| `weapon_mp5sd` | 2 | `Weapon_MP5.Single` | `Weapon_MP5.Single` |
| `weapon_m4a1` (M4A4) | 0 | `Weapon_M4A1.Single` | `Weapon_M4A1.Silenced` |
| `weapon_ak47` | 0 | `Weapon_AK47.Single` | absent |

`SimWorld::fire` emits `GameEvent::Fire` with the weapon class. The client
effects system looks up that definition and plays its `fire_sound` entry.
The catalog previously populated this field unconditionally from
`sound_single_shot`, ignoring the suppressor metadata. Despite its name,
`Weapon_M4A1.SilencedSingle` selects `m4a1_us_01.wav` through `m4a1_us_04.wav`;
the separate suppressed entry selects `m4a1_silencer_01.wav`. USP-S similarly
selected `usp_unsilenced_01.wav` through `usp_unsilenced_03.wav` instead of the
suppressed `usp_01.wav` through `usp_03.wav`.

The fix selects a nonempty `sound_special1` when the resolved numeric
`has silencer` attribute is positive, otherwise preserving the ordinary-shot
fallback. It respects prefab inheritance and item overrides, does not infer
identity from names, and leaves M4A4/AK-47 and unrelated special actions alone.
The simulation does not yet model removing/reattaching a suppressor: this field
describes the default equipped sound, not a new suppressor-toggle mechanic.

## 2015 provenance investigation

Valve's [September 28, 2016 release notes](https://blog.counter-strike.net/2016/09/16156/)
document rifle fire-sound changes, including the AK-47 and both M4A1-S states.
That establishes an era boundary, not the identity of individual retained files.
Current Legacy `Weapon_AK47.Single` selects `weapons/ak47/ak47_01.wav`:
502,676 bytes, stereo 44,100 Hz, SHA256
`6fc24ceac0d1aa625467ef2405433031d1e8fb2eff08c93fd5d962b19a55fd55`.

The same install retains `weapons/ak47/ak47-1.wav` and explicitly references it
from `3rd_Weapon_AK47.Single`. It is 288,436 bytes, stereo 44,100 Hz, SHA256
`a504312e586d54332bc42b533242bb7b57ba8704439138a624bf4edc18f71903`.
Its RIFF metadata contains no dated origin field. The newer file includes an
ACID Pro software comment, which also does not establish its release date.
The [public release-data snapshot at December 31, 2015](https://github.com/SteamTracking/GameTracking/commit/fcb66c41571b716820b1e1c148e0cba9a318f772)
records the earlier shot bindings in
[soundscript metadata](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/scripts/game_sounds_weapons.txt)
and their size/CRC in the
[VPK directory metadata](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/pak01_dir.txt).
Both downloaded text blobs were checked against their Git object hashes
(`f5aa5ecb564d6117213086d47285e25438f44a35` and
`3fe114205f6bc2f3aad1690bab6246e0ca4b01ea`) and kept private. This is a
SteamTracking capture of publicly shipped Valve data, not an official Valve
hosting location or a binary waveform download.

| 2015 entry | Retained path under `sound/weapons/` | Historical bytes | Historical CRC32 | Computed installed match |
|---|---|---:|---|---|
| `Weapon_AK47.Single` | `ak47/ak47-1.wav` | 288436 | `e0fb8223` | size and CRC |
| `Weapon_AK47.SingleDistant` | `ak47/ak47-1-distant.wav` | 203206 | `1207e18a` | size and CRC |
| `Weapon_M4A1.Silenced` | `m4a1/m4a1-1.wav` | 44438 | `c909e01e` | size and CRC |
| `Weapon_M4A1.SilencedDistant` | `m4a1/m4a1-1-distant.wav` | 23122 | `b994e5b9` | size and CRC |

The retained M4A1-S near wave also has an iXML origin date of 2013-07-10.
That corroborates its age; the historical entry binding and matching directory
size/CRC provide the stronger era evidence. CRC/size are historical identifiers,
not cryptographic proof of a complete original build. The runtime policy pins
the independently computed current retained-file SHA256 values as well.

USP-S remains a separate content gap: the 2015 `usp/usp1.wav` was 44068 bytes /
CRC `2e40d830`, but the installed file is 45830 bytes / CRC `4199d4d7`.
Its distant file also differs (historical 44068 / `3c84cb85`, installed
144098 / `b58cd43a`). It receives the suppressed current-Legacy mapping fix,
but no 2015 waveform substitution or era-fidelity label. Removing only the
installed near file's iXML metadata and repairing RIFF length produces the
historical 44068-byte / `2e40d830` identity; the far file still does not match.
That is a narrower metadata-only near-file observation, not a verified pair.

## Runtime classic policy

`SoundLibrary::load` defaults to `SoundEra::Classic2015`, with no player setup
step. `load_with_era(InstalledLegacy)` is an explicit comparison seam.
The profile applies only to exact `csgo_weapon_ak47` /
`Weapon_AK47.Single` and `csgo_weapon_m4a1_silencer` /
`Weapon_M4A1.Silenced` identities. Custom content IDs or shot bindings bypass it.
Public soundscript entries and catalog definitions remain intact.

Own internal entries clone the current parsed mixing/limits/crossfade metadata
and replace only the verified waveforms. AK's complete near/distant pair must
pass expected byte size, SHA256 and the actual audio decoder before either
entry is published. Current Legacy's suppressed M4 entry has **no distant
partner** and no `Weapon_M4A1.SilencedDistant` declaration. Its profile therefore
replaces only the verified near waveform and preserves `None` routing. The
historical M4 distant file remains fingerprinted in the audit (SHA256
`fe23d5c351f5b095437c4326a715693cd380c8e81b1f0f8fa2793a2b9970f80c`);
its routing/mix is unmeasured, and no distant entry/gain is invented.
Missing/changed/unsupported data or reserved entry-name
collisions preserve the installed shot with at most two fixed diagnostics per
load. No partial classic/modern pair is constructed.

Verified decoded bytes are retained immutably, with separate internal audio
cache keys. A subsequently mounted map or pack cannot shadow the old path and
change those selected bytes. No sound data is committed or downloaded into the
repository. **The cloned modern mix/DSP metadata has not been verified against
2015.** This restores measured waveform identity for two stock weapon states;
it does not claim complete 2015 audio/gameplay fidelity or all-weapon coverage.

## Verification

Generated catalog regressions cover inherited suppressor state, item override
to an unsuppressed weapon, permanent suppression, absent/empty special sounds,
unrecognized suppressor metadata, and unrelated special actions.
The four focused weapons tests passed at `5aa6144` (three new cases plus the
existing prefab test). New classic regressions cover atomic fallback,
fingerprint mismatch, unsupported bytes, custom ID/binding exclusion,
unchanged public entries, and both actual Bevy AudioSource byte identities
after a synthetic pack shadows the verified paths. No audio device or GPU is
needed for those generated tests; no copyrighted fixture bytes are used.
The near-only regression asserts that spatial playback produces exactly one
verified source with no constructed distant alias or route.

The first actual installed audit exposed a stale normal weapon-library artifact
in the shared target, while unit tests had rebuilt separately. Freshening all
owned workspace Rust source mtimes, without changing bytes or cleaning Cargo,
proved the corrected M4 binding. It then exposed the real absent-distant
distinction above; the corrected audit explicitly checks that case. Frozen
runtime verification must freshen member source timestamps when switching
worktrees that share a target, or use an isolated target.

An owned native Legacy observation in private
`classic-reference-20261012` confirmed visible suppressed M4A1-S ammo 20 to 17
playing `m4a1_silencer_01.wav` at 73dB / volume 0.90, with no M4A4 wave in that
interval. The separate M4A4 ammo 30 to 26 interval used `m4a1_01.wav` /
`m4a1_02.wav`. HUD/model/ammo distinguish the variant; entity classname alone
does not. This confirms current suppressed identity, not native 2015 mixing.

Required checks are workspace tests, release Dust II probe/sim and the existing
soundcheck decoding every weapon waveform. Targeted identity proof should
assert installed M4A1-S/USP-S/M4A4/AK-47/MP5SD catalog entries and their existing
sound-library wave paths (`cargo run -p launcher --example weapon_sound_identity`),
then compare M4A1-S/USP-S fire logs with owned Legacy
`snd_showstart` probes using ordinary inputs. Wave selection, decode success
and 2015 sound fidelity are distinct assertions.

Private extraction metadata and waveform hashes are in
`C:/Users/shawn/csr-fidelity/weapon-sound-identity`; wave bytes were read only in
memory. Classic client tests, the installed identity audit and full required
checks await the next coordinated build slot. No native game launch or config
write was performed; focused catalog tests used the agreed shared target.
No game function bodies or SDK source were inspected.
