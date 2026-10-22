//! Bounded private pump-wave restoration; authored events and mixing stay intact.
use super::{Entry, FrozenWave, WaveSpec, historical::Historical, verified_wave};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Spec<'a> {
    binding: &'static str,
    historical: WaveSpec<'a>,
    installed: WaveSpec<'a>,
}

// Same exact event-to-path bindings in the shipped June 2015 and installed
// Legacy declarations. PCM payloads differ, not merely RIFF headers. Only
// these recovered pump waves are opted in; no scripts or assets are embedded.
// Keep MAG-7 back/forward atomic so a partial pack cannot mix their eras.
const NOVA: Spec<'static> = Spec {
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
};

const GROUPS: &[&[Spec<'static>]] = &[
    &[NOVA],
    // Draw and shotgun reload finish use Pump, while firing uses
    // PumpDuringShot. Both declarations name this same verified wave in 2015
    // and Legacy. Validate independently: an unknown alias must not disable
    // the other recognized binding (unlike MAG-7's atomic two-wave action).
    &[Spec { binding: "weapon_nova.pump", ..NOVA }],
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::{private_pack, wav};
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, sync::Arc};

    fn entries(bindings: &[&str]) -> HashMap<String, Entry> {
        bindings.iter().map(|binding| {
            let doc = kv::parse(&format!(r#""{binding}" {{
                "wave" "weapons/test/current.wav" "channel" "CHAN_ITEM"
                "volume" "0.5, 1.0" "pitch" "97, 105" "soundlevel" "SNDLVL_65dB"
                "operator_stacks" {{ "update_stack" {{ "import_stack" "CS_update_foley" }} }}
            }}"#)).unwrap();
            let entry = super::super::super::parse_entry(doc.block(binding).unwrap()).unwrap();
            (binding.to_string(), entry)
        }).collect()
    }

    #[test]
    fn nova_aliases_validate_independently_and_only_replace_bytes() {
        let old = wav(7000);
        let current = wav(1000);
        let old_hash = format!("{:x}", Sha256::digest(&old));
        let current_hash = format!("{:x}", Sha256::digest(&current));
        let primary = Spec {
            binding: "weapon_nova.pumpduringshot",
            historical: WaveSpec { path: "sound/old_distant.wav", size: old.len(), sha256: &old_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
        };
        let alias = Spec { binding: "weapon_nova.pump", ..primary };
        // Missing/changed aliases must not disable the primary; likewise a
        // missing primary cannot disable a separately recognized draw/reload.
        for case in ["valid", "missing alias", "changed alias", "missing primary", "changed bytes", "corrupt old", "old pack"] {
            let payload = if case == "corrupt old" { wav(6999) } else { old.clone() };
            let historical = Historical::from_pack(
                // The near resource stands in for the existing shots; old
                // packs remain valid without the optional handling resource.
                private_pack(&current, (case != "old pack").then_some(payload.as_slice()), "classic2015", false),
                &[WaveSpec { path: "sound/old_near.wav", size: current.len(), sha256: &current_hash }, primary.historical],
            ).unwrap();
            let mut fs = vfs::Vfs::new();
            fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                (primary.installed.path.into(), Arc::from(if case == "changed bytes" { wav(999) } else { current.clone() })),
            ])));
            let mut entries = entries(&[primary.binding, alias.binding]);
            match case {
                "missing alias" => { entries.remove(alias.binding); }
                "changed alias" => { entries.get_mut(alias.binding).unwrap().waves[0] = "sound/custom.wav".into(); }
                "missing primary" => { entries.remove(primary.binding); }
                _ => {}
            }
            let before = entries.clone();
            install_groups(&fs, &mut entries, &historical, &[&[primary], &[alias]]);
            for (binding, mut entry) in entries {
                let expected = match binding.as_str() {
                    "weapon_nova.pumpduringshot" => matches!(case, "valid" | "missing alias" | "changed alias"),
                    _ => matches!(case, "valid" | "missing primary"),
                };
                assert_eq!(entry.frozen_wave.is_some(), expected, "{case}: {binding}");
                if let Some(wave) = entry.frozen_wave.take() {
                    assert_eq!(wave.bytes.as_ref(), old);
                    assert_eq!(wave.channels, 1);
                }
                assert_eq!(format!("{entry:?}"), format!("{:?}", before[&binding]), "metadata: {case}");
            }
        }
    }

    #[test]
    fn mag_pumps_still_publish_only_as_a_complete_verified_pair() {
        let old_back = wav(7000);
        let old_forward = wav(-8000);
        let current = wav(1000);
        let back_hash = format!("{:x}", Sha256::digest(&old_back));
        let forward_hash = format!("{:x}", Sha256::digest(&old_forward));
        let current_hash = format!("{:x}", Sha256::digest(&current));
        let specs = [Spec {
            binding: "weapon_mag7.pumpback",
            historical: WaveSpec { path: "sound/old_near.wav", size: old_back.len(), sha256: &back_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
        }, Spec {
            binding: "weapon_mag7.pumpforward",
            historical: WaveSpec { path: "sound/old_distant.wav", size: old_forward.len(), sha256: &forward_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
        }];
        for case in ["valid", "missing partner", "corrupt partner", "changed binding"] {
            let far = if case == "corrupt partner" { wav(-7999) } else { old_forward.clone() };
            let historical = Historical::from_pack(
                private_pack(&old_back, (case != "missing partner").then_some(far.as_slice()), "classic2015", false),
                &[specs[0].historical, specs[1].historical],
            ).unwrap();
            let mut fs = vfs::Vfs::new();
            fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                (specs[0].installed.path.into(), Arc::from(current.clone())),
            ])));
            let mut entries = entries(&[specs[0].binding, specs[1].binding]);
            if case == "changed binding" { entries.get_mut(specs[1].binding).unwrap().waves.clear(); }
            install_groups(&fs, &mut entries, &historical, &[&specs]);
            for (spec, expected) in specs.iter().zip([&old_back, &old_forward]) {
                let wave = entries[spec.binding].frozen_wave.as_ref();
                assert_eq!(wave.is_some(), case == "valid", "{case}");
                if let Some(wave) = wave { assert_eq!(wave.bytes.as_ref(), expected.as_slice()); }
            }
        }
    }
}

fn verified_group(
    fs: &vfs::Vfs,
    entries: &HashMap<String, Entry>,
    historical: &Historical,
    specs: &[Spec<'_>],
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
    install_groups(fs, entries, historical, GROUPS);
}

fn install_groups(
    fs: &vfs::Vfs,
    entries: &mut HashMap<String, Entry>,
    historical: &Historical,
    groups: &[&[Spec<'_>]],
) {
    for group in groups {
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
