//! Bounded private pump-wave restoration; authored events and mixing stay intact.
use super::{Entry, FrozenWave, WaveSpec, historical::Historical, verified_wave};
use std::collections::HashMap;

struct Spec {
    binding: &'static str,
    historical: WaveSpec<'static>,
    installed: WaveSpec<'static>,
}

// Same exact event-to-path bindings in the shipped June 2015 and installed
// Legacy declarations. PCM payloads differ, not merely RIFF headers. Only
// these recovered pump waves are opted in; no scripts or assets are embedded.
// Keep MAG-7 back/forward atomic so a partial pack cannot mix their eras.
const GROUPS: &[&[Spec]] = &[
    &[Spec {
        binding: "weapon_nova.pumpduringshot",
        historical: WaveSpec {
            path: "sound/weapons/nova/nova_pump.wav",
            size: 39236,
            sha256: "7dc2881cc8add49de540c42e0c5300e80548baa32be9d2d7dda594311c6cf68f",
        },
        installed: WaveSpec {
            path: "sound/weapons/nova/nova_pump.wav",
            size: 35914,
            sha256: "ae8b0f14117978612839be08ce3bf6c241604b6f04057714a6e527b5c263b9ba",
        },
    }],
    &[
        Spec {
            binding: "weapon_mag7.pumpback",
            historical: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_back.wav",
                size: 22858,
                sha256: "452b27f399cd355744c50d7c01aa28d14935e56c005a08ff241c5dc73f008ea3",
            },
            installed: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_back.wav",
                size: 12244,
                sha256: "c1d42fa0fa679b76dd16e3738838b000fb0ca2596b7e7980c183ae768c6deb5b",
            },
        },
        Spec {
            binding: "weapon_mag7.pumpforward",
            historical: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_forward.wav",
                size: 16422,
                sha256: "acdd8d4bae0d957f99495c1369d53ffa6202ffc89d05dcbe4310bc5f9f41f945",
            },
            installed: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_forward.wav",
                size: 9930,
                sha256: "0cebb88bea3723a9282f9e0c54428b94c57c69622857a7e8728c4ad8483d2cb0",
            },
        },
    ],
];

pub(super) fn waves() -> impl Iterator<Item = &'static WaveSpec<'static>> {
    GROUPS.iter().flat_map(|group| group.iter().map(|spec| &spec.historical))
}

fn verified_group(
    fs: &vfs::Vfs,
    entries: &HashMap<String, Entry>,
    historical: &Historical,
    specs: &[Spec],
) -> Result<Vec<(&'static str, FrozenWave)>, &'static str> {
    specs.iter().map(|spec| {
        let entry = entries.get(spec.binding).ok_or("installed handling entry missing")?;
        if entry.waves.len() != 1 || entry.waves[0] != spec.installed.path
            || entry.distant.is_some() || entry.frozen_wave.is_some()
            || entry.category != super::super::Category::Foley
        {
            return Err("installed handling binding differs");
        }
        // Refuse changed installs/selected-pack overrides instead of applying
        // a historical wave to an unrecognized installed sound declaration.
        verified_wave(fs, &spec.installed)?;
        let wave = historical.wave(spec.historical.path)?;
        Ok((spec.binding, wave))
    }).collect()
}

pub(super) fn install(
    fs: &vfs::Vfs,
    entries: &mut HashMap<String, Entry>,
    historical: &Historical,
) {
    for group in GROUPS {
        // Validate every group member before publishing any of them. An old
        // shot-only pack, absent/invalid resource or changed declaration leaves
        // all installed handling in this group untouched.
        if let Ok(waves) = verified_group(fs, entries, historical, group) {
            for (binding, wave) in waves {
                if let Some(entry) = entries.get_mut(binding) {
                    // Existing foley dispatch uses these bindings directly.
                    // Freeze bytes only: preserve pitch, gain, channel, limits,
                    // fades, distance and the MDL scheduler's authored timing.
                    entry.frozen_wave = Some(wave);
                }
            }
        }
    }
}
