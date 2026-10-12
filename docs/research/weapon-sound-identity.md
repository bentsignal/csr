# Weapon shot identity and the 2015 sound target

The owner's manual playtest reported an unsuppressed M4A1-S shot and an AK-47
sound that did not match the requested 2015 era. These are separate issues:
the first has an installed-schema selection bug; the second needs historical
waveform provenance before an era override can claim fidelity.

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
These are candidate/provenance facts, not a claim that the retained variant is
an exact 2015 waveform. No modern file is relabeled as classic.

Historical public soundscript/directory metadata and observed reference
comparisons remain under investigation. Any eventual era policy must read the
player's own verified bytes at runtime; no third-party sound is committed or
downloaded into this repository. The default-suppressor correction does not
complete the requested 2015 AK-47 fidelity target.

## Verification

Generated catalog regressions cover inherited suppressor state, item override
to an unsuppressed weapon, permanent suppression, absent/empty special sounds,
unrecognized suppressor metadata, and unrelated special actions.
Build/test execution is pending the coordinated Cargo slot.

Required checks are workspace tests, release Dust II probe/sim and the existing
soundcheck decoding every weapon waveform. Targeted identity proof should
assert installed M4A1-S/USP-S/M4A4/AK-47/MP5SD catalog entries and their existing
sound-library wave paths, then compare M4A1-S/USP-S fire logs with owned Legacy
`snd_showstart` probes using ordinary inputs. Wave selection, decode success
and 2015 sound fidelity are distinct assertions.

Private extraction metadata and waveform hashes are in
`C:/Users/shawn/csr-fidelity/weapon-sound-identity`; wave bytes were read only in
memory. No native game launch, config write, Cargo or GPU run was performed for
this initial checkpoint. No game function bodies or SDK source were inspected.
