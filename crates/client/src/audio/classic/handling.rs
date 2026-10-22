//! Bounded classic handling waves and declaration metadata; authored events stay intact.
use super::{Entry, FrozenWave, WaveSpec, historical::Historical, verified_wave};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Spec<'a> {
    binding: &'static str,
    historical: WaveSpec<'a>,
    installed: WaveSpec<'a>,
    volume: f32,
    channel: u8,
}

// Same exact event-to-path bindings in the shipped June 2015 and installed
// Legacy declarations. Pump PCM payloads differ, not merely RIFF headers.
// No scripts or assets are embedded.
// Keep MAG-7 back/forward atomic so a partial pack cannot mix their eras.
// Literal June 2015 declarations: all four use PITCH_NORM, CHAN_ITEM and
// SNDLVL_65dB, with no operator stacks, entry limit or explicit fade override.
// Depot 731 manifest 4730361390205743786, game_sounds_weapons.txt SHA1
// 8ecba5f796256eabca6068086f70bd0157e759fb (derived values only).
// Preserve the renderer's existing foley curve/mixer rather than guessing a
// historical mixing law. Only the verified group's volume/pitch/channel/limit
// change; installed/old-pack fallback keeps all installed metadata.
const NOVA: Spec<'static> = Spec {
    binding: "weapon_nova.pumpduringshot",
    volume: 0.3,
    channel: 3,
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
    &[Spec { binding: "weapon_nova.pump", volume: 1.0, ..NOVA }],
    &[
        Spec {
            binding: "weapon_mag7.pumpback",
            volume: 1.0,
            channel: 3,
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
            volume: 1.0,
            channel: 3,
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
    // M4A4 and M4A1-S share draw/clip bindings. Only M4A4 invokes these
    // BoltBack/BoltForward entries; Legacy's M4A1S.* bolts and ClipHit have
    // no matching June 2015 declarations and must not be aliased to them.
    // Same script provenance above: PITCH_NORM, SNDLVL_65dB, no operator
    // stacks, entry limit or explicit fade override for all five entries.
    // Keep the recovered M4 set atomic, including its declaration metadata.
    &[
        Spec {
            binding: "weapon_m4a1.draw", volume: 0.5, channel: 6,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_draw.wav", size: 64994,
                sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_draw.wav", size: 90312,
                sha256: "f8a54f39f8efd71d8ef7f5703ec99fe7e439f0b8dcb3c1375fc382cfa950cec9",
            },
        },
        Spec {
            binding: "weapon_m4a1.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipout.wav", size: 15872,
                sha256: "a99c89083ce31767c915f62a1668b999330fc6222d1674144d0abb8dd9c656df",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipout.wav", size: 33120,
                sha256: "b3cdbb7d21f6e3285d7d76838dfe7378ce46a7112f2755ba003c4c2d32e05625",
            },
        },
        Spec {
            binding: "weapon_m4a1.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipin.wav", size: 13294,
                sha256: "71c849aeeb9b145035937f22624dda7267063e8ab31ade2951c978df30164fc7",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipin.wav", size: 36750,
                sha256: "654f310c41f97021006110af429922556fee149c51881c3d4a967c6793a17acc",
            },
        },
        Spec {
            binding: "weapon_m4a1.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltforward.wav", size: 21604,
                sha256: "da38a834809278b98df5ddf61e056518cf253ac7039f86be460686c824a53bee",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltforward.wav", size: 33120,
                sha256: "cdfecd48ca661ae5d2014499d570235089c916eea07a1f1b753fc9b5f5c34be6",
            },
        },
        Spec {
            binding: "weapon_m4a1.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltback.wav", size: 25870,
                sha256: "7b16e1f0fc354063d915a110de2cbe9bae0bd9d14c143c2e774bf8739227ab99",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltback.wav", size: 33120,
                sha256: "1af86f38556f0ed0ec643530ced345cfc79fdf0b1407ee5cb26c2388c4bd3993",
            },
        },
    ],
];

struct RetainedSpec<'a> {
    binding: &'static str,
    wave: WaveSpec<'a>,
    volume: f32,
    channel: u8,
}

// These four installed AK waves are byte-identical to archive 054 in the
// same June 2015 depot (directory length/CRC and full SHA256 checked).
// Reuse the verified installed bytes; no duplicate private assets required.
// Draw/BoltPull use CHAN_STATIC, clips CHAN_ITEM. All use PITCH_NORM and
// have no entry-limit stack. The shared M4 Draw is also CHAN_STATIC.
const RETAINED_RIFLES: &[RetainedSpec<'static>] = &[
    RetainedSpec {
        binding: "weapon_ak47.draw", volume: 0.5, channel: 6,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_draw.wav", size: 39884,
            sha256: "8118407efcf748c119e459d6dd87369fa714a36863dc11e735dc59c857376cd2",
        },
    },
    RetainedSpec {
        binding: "weapon_ak47.boltpull", volume: 1.0, channel: 6,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_boltpull.wav", size: 28846,
            sha256: "d6253073be8c071eb6112cdbf8212ac58f377dd7c5ee623c48b4b673dec5f36c",
        },
    },
    RetainedSpec {
        binding: "weapon_ak47.clipout", volume: 1.0, channel: 3,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_clipout.wav", size: 16706,
            sha256: "17953c6c9e57d9c050169830c18cd46edc7e55467c7b690298f43c4a7e0d72ec",
        },
    },
    RetainedSpec {
        binding: "weapon_ak47.clipin", volume: 1.0, channel: 3,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_clipin.wav", size: 15724,
            sha256: "bbf5d5429d709d422d2488189e7375e99437b88c7ef7d579e65edd2e4cdf825c",
        },
    },
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
                "operator_stacks" {{
                    "start_stack" {{ "import_stack" "CS_limit_start" }}
                    "update_stack" {{ "import_stack" "CS_update_foley" }}
                }}
            }}"#)).unwrap();
            let entry = super::super::super::parse_entry(doc.block(binding).unwrap()).unwrap();
            (binding.to_string(), entry)
        }).collect()
    }

    #[test]
    fn nova_aliases_restore_metadata_only_with_their_verified_wave() {
        let old = wav(7000);
        let current = wav(1000);
        let old_hash = format!("{:x}", Sha256::digest(&old));
        let current_hash = format!("{:x}", Sha256::digest(&current));
        let primary = Spec {
            binding: "weapon_nova.pumpduringshot",
            historical: WaveSpec { path: "sound/old_distant.wav", size: old.len(), sha256: &old_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
            ..NOVA
        };
        let alias = Spec {
            historical: primary.historical, installed: primary.installed,
            ..GROUPS[1][0]
        };
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
                let mut expected_entry = before[&binding].clone();
                if expected {
                    let volume = if binding == primary.binding { 0.3 } else { 1.0 };
                    expected_entry.volume = (volume, volume);
                    expected_entry.pitch = (100.0, 100.0);
                    expected_entry.channel = 3; // CHAN_ITEM
                    expected_entry.limit = None;
                }
                assert_eq!(format!("{entry:?}"), format!("{expected_entry:?}"), "metadata: {case}: {binding}");
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
            ..GROUPS[2][0]
        }, Spec {
            binding: "weapon_mag7.pumpforward",
            historical: WaveSpec { path: "sound/old_distant.wav", size: old_forward.len(), sha256: &forward_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
            ..GROUPS[2][1]
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
            entries.get_mut(specs[0].binding).unwrap().pitch = (150.0, 150.0);
            for spec in &specs { entries.get_mut(spec.binding).unwrap().channel = 6; }
            if case == "changed binding" { entries.get_mut(specs[1].binding).unwrap().waves.clear(); }
            let before = entries.clone();
            install_groups(&fs, &mut entries, &historical, &[&specs]);
            for (spec, expected) in specs.iter().zip([&old_back, &old_forward]) {
                let mut entry = entries[spec.binding].clone();
                let wave = entry.frozen_wave.take();
                assert_eq!(wave.is_some(), case == "valid", "{case}");
                if let Some(wave) = wave { assert_eq!(wave.bytes.as_ref(), expected.as_slice()); }
                let mut expected_entry = before[spec.binding].clone();
                if case == "valid" {
                    expected_entry.volume = (1.0, 1.0);
                    expected_entry.pitch = (100.0, 100.0);
                    expected_entry.channel = 3;
                    expected_entry.limit = None;
                }
                assert_eq!(format!("{entry:?}"), format!("{expected_entry:?}"), "metadata: {case}: {}", spec.binding);
            }
        }
    }

    #[test]
    fn retained_ak_metadata_requires_the_recognized_binding_and_identical_bytes() {
        let current = wav(1000);
        let hash = format!("{:x}", Sha256::digest(&current));
        assert_eq!(RETAINED_RIFLES.len(), 4);
        for (stock, (binding, volume, channel)) in RETAINED_RIFLES.iter().zip([
            ("weapon_ak47.draw", 0.5, 6),
            ("weapon_ak47.boltpull", 1.0, 6),
            ("weapon_ak47.clipout", 1.0, 3),
            ("weapon_ak47.clipin", 1.0, 3),
        ]) {
            assert_eq!((stock.binding, stock.volume, stock.channel), (binding, volume, channel));
            let spec = RetainedSpec {
                binding, volume: stock.volume, channel: stock.channel,
                wave: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &hash },
            };
            for case in ["valid", "changed bytes", "missing bytes", "changed binding", "multiple waves", "distant", "wrong category", "already frozen"] {
                let mut fs = vfs::Vfs::new();
                if case != "missing bytes" {
                    fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                        (spec.wave.path.into(), Arc::from(if case == "changed bytes" { wav(999) } else { current.clone() })),
                    ])));
                }
                let mut entries = entries(&[binding]);
                let entry = entries.get_mut(binding).unwrap();
                entry.channel = channel;
                match case {
                    "changed binding" => entry.waves[0] = "sound/custom.wav".into(),
                    "multiple waves" => entry.waves.push(spec.wave.path.into()),
                    "distant" => entry.distant = Some("custom.distant".into()),
                    "wrong category" => entry.category = super::super::super::Category::Weapon,
                    "already frozen" => entry.frozen_wave = Some(FrozenWave { bytes: Arc::from(current.clone()), channels: 1 }),
                    _ => {}
                }
                let mut expected = entry.clone();
                install_retained_rifles(&fs, &mut entries, std::slice::from_ref(&spec));
                let mut actual = entries[binding].clone();
                if case == "valid" {
                    assert_eq!(actual.frozen_wave.take().unwrap().bytes.as_ref(), current);
                    expected.volume = (volume, volume);
                    expected.pitch = (100.0, 100.0);
                    expected.channel = channel;
                    expected.limit = None;
                }
                assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{binding}: {case}");
            }
        }
    }

    #[test]
    fn m4_handling_restores_metadata_only_as_a_complete_verified_set() {
        let old = wav(7000);
        let current = wav(1000);
        let old_hash = format!("{:x}", Sha256::digest(&old));
        let current_hash = format!("{:x}", Sha256::digest(&current));
        let expected = [
            ("weapon_m4a1.draw", 0.5, 6),
            ("weapon_m4a1.clipout", 1.0, 3),
            ("weapon_m4a1.clipin", 1.0, 3),
            ("weapon_m4a1.boltforward", 1.0, 3),
            ("weapon_m4a1.boltback", 1.0, 3),
        ];
        assert_eq!(GROUPS[3].len(), expected.len());
        let specs: Vec<_> = GROUPS[3].iter().zip(expected).enumerate().map(|(i, (spec, expected))| {
            assert_eq!((spec.binding, spec.volume, spec.channel), expected);
            Spec {
                historical: WaveSpec { path: if i == 4 { "sound/old_distant.wav" } else { "sound/old_near.wav" }, size: old.len(), sha256: &old_hash },
                installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
                ..*spec
            }
        }).collect();
        for case in ["valid", "old pack", "corrupt partner", "changed bytes", "changed binding", "missing binding"] {
            let far = if case == "corrupt partner" { wav(6999) } else { old.clone() };
            let historical = Historical::from_pack(
                private_pack(&old, (case != "old pack").then_some(far.as_slice()), "classic2015", false),
                &[specs[0].historical, specs[4].historical],
            ).unwrap();
            let mut fs = vfs::Vfs::new();
            fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                (specs[0].installed.path.into(), Arc::from(if case == "changed bytes" { wav(999) } else { current.clone() })),
            ])));
            let mut bindings: Vec<_> = specs.iter().map(|spec| spec.binding).collect();
            // The actual M4A1-S model invokes its own bolts and the newer
            // ClipHit; these must survive even when the recovered set succeeds.
            bindings.extend(["weapon_m4a1s.boltback", "weapon_m4a1s.boltforward", "weapon_m4a1.cliphit"]);
            let mut entries = entries(&bindings);
            if case == "changed binding" { entries.get_mut(specs[4].binding).unwrap().waves.clear(); }
            if case == "missing binding" { entries.remove(specs[4].binding); }
            let before = entries.clone();
            install_groups(&fs, &mut entries, &historical, &[&specs]);
            for (binding, mut entry) in entries {
                let mut expected_entry = before[&binding].clone();
                if case == "valid" {
                    if let Some((_, volume, channel)) = expected.iter().find(|(name, _, _)| *name == binding) {
                        assert_eq!(entry.frozen_wave.take().unwrap().bytes.as_ref(), old);
                        expected_entry.volume = (*volume, *volume);
                        expected_entry.pitch = (100.0, 100.0);
                        expected_entry.channel = *channel;
                        expected_entry.limit = None;
                    }
                }
                assert_eq!(format!("{entry:?}"), format!("{expected_entry:?}"), "{binding}: {case}");
            }
        }
    }

    #[test]
    fn installed_mode_leaves_all_handling_metadata_untouched() {
        let bindings: Vec<_> = GROUPS.iter().flat_map(|group| group.iter().map(|spec| spec.binding))
            .chain(RETAINED_RIFLES.iter().map(|spec| spec.binding)).collect();
        let mut entries = entries(&bindings);
        entries.get_mut("weapon_mag7.pumpback").unwrap().pitch = (150.0, 150.0);
        let before = entries.clone();
        super::super::Profile::load(&vfs::Vfs::new(), &mut entries, super::super::SoundEra::InstalledLegacy);
        for (binding, entry) in entries {
            assert_eq!(format!("{entry:?}"), format!("{:?}", before[&binding]), "{binding}");
        }
    }
}

fn verified_group(
    fs: &vfs::Vfs,
    entries: &HashMap<String, Entry>,
    historical: &Historical,
    specs: &[Spec<'_>],
) -> Result<Vec<FrozenWave>, &'static str> {
    specs.iter().map(|spec| {
        // Refuse changed installs/selected-pack overrides instead of applying
        // a historical wave to an unrecognized installed sound declaration.
        verified_installed(fs, entries, spec.binding, &spec.installed)?;
        historical.wave(spec.historical.path)
    }).collect()
}

fn verified_installed(
    fs: &vfs::Vfs,
    entries: &HashMap<String, Entry>,
    binding: &str,
    wave: &WaveSpec<'_>,
) -> Result<FrozenWave, &'static str> {
    let entry = entries.get(binding).ok_or("installed handling entry missing")?;
    if entry.waves.len() != 1 || entry.waves[0] != wave.path
        || entry.distant.is_some() || entry.frozen_wave.is_some()
        || entry.category != super::super::Category::Foley
    {
        return Err("installed handling binding differs");
    }
    verified_wave(fs, wave)
}

fn restore(entry: &mut Entry, volume: f32, channel: u8, wave: FrozenWave) {
    entry.volume = (volume, volume);
    entry.pitch = (100.0, 100.0);
    entry.channel = channel;
    entry.limit = None;
    entry.frozen_wave = Some(wave);
}

pub(super) fn install(
    fs: &vfs::Vfs,
    entries: &mut HashMap<String, Entry>,
    historical: &Historical,
) {
    install_groups(fs, entries, historical, GROUPS);
    // The caller admits handling only in Classic2015 with a valid private
    // pack. Existing shot-only/pump packs also permit identical-byte AK
    // restoration; missing M4 resources still retain the entire installed M4.
    install_retained_rifles(fs, entries, RETAINED_RIFLES);
}

fn install_retained_rifles(
    fs: &vfs::Vfs,
    entries: &mut HashMap<String, Entry>,
    specs: &[RetainedSpec<'_>],
) {
    for spec in specs {
        if let Ok(wave) = verified_installed(fs, entries, spec.binding, &spec.wave) {
            if let Some(entry) = entries.get_mut(spec.binding) {
                restore(entry, spec.volume, spec.channel, wave);
            }
        }
    }
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
            for (spec, wave) in group.iter().zip(waves) {
                if let Some(entry) = entries.get_mut(spec.binding) {
                    // Existing foley dispatch uses these bindings directly.
                    // Publish historical metadata only with the verified wave;
                    // failed groups never alter installed entries.
                    restore(entry, spec.volume, spec.channel, wave);
                }
            }
        }
    }
}
