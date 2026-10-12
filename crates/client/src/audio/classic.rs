//! Derived waveform identity only; modern mixing metadata is retained.
//! Historical scripts/assets are never embedded in this module.
use super::{Entry, decoded_audio};
use content::{ContentId, ContentKind};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoundEra {
    InstalledLegacy,
    #[default]
    Classic2015,
}

#[derive(Clone, Debug)]
pub(super) struct FrozenWave {
    pub bytes: Arc<[u8]>,
    pub channels: u16,
}

struct Selection {
    binding: &'static str,
    alias: &'static str,
}

#[derive(Default)]
pub(super) struct Profile {
    era: SoundEra,
    selections: HashMap<ContentId, Selection>,
}

struct WaveSpec<'a> {
    path: &'a str,
    size: usize,
    sha256: &'a str,
}

struct WeaponSpec<'a> {
    weapon: &'static str,
    binding: &'static str,
    distant: &'static str,
    alias: &'static str,
    alias_distant: &'static str,
    near_wave: WaveSpec<'a>,
    distant_wave: WaveSpec<'a>,
}

// Names/size/hash are derived observations from the player's retained files,
// compared with the published 2015 directory's size/CRC and shot bindings.
const STOCK: [WeaponSpec<'static>; 2] = [
    WeaponSpec {
        weapon: "ak47",
        binding: "Weapon_AK47.Single",
        distant: "weapon_ak47.singledistant",
        alias: "csr.classic2015.ak47.single",
        alias_distant: "csr.classic2015.ak47.distant",
        near_wave: WaveSpec {
            path: "sound/weapons/ak47/ak47-1.wav",
            size: 288436,
            sha256: "a504312e586d54332bc42b533242bb7b57ba8704439138a624bf4edc18f71903",
        },
        distant_wave: WaveSpec {
            path: "sound/weapons/ak47/ak47-1-distant.wav",
            size: 203206,
            sha256: "260c3237643c74e4950ba2a96d1179228023f9e759235c43e4d1bc34f5ab95b9",
        },
    },
    WeaponSpec {
        weapon: "m4a1_silencer",
        binding: "Weapon_M4A1.Silenced",
        distant: "weapon_m4a1.silenceddistant",
        alias: "csr.classic2015.m4a1_silencer.single",
        alias_distant: "csr.classic2015.m4a1_silencer.distant",
        near_wave: WaveSpec {
            path: "sound/weapons/m4a1/m4a1-1.wav",
            size: 44438,
            sha256: "8548c3b0937bf07656c73021439b76a946a4a8bb339b208508af6e916f072195",
        },
        distant_wave: WaveSpec {
            path: "sound/weapons/m4a1/m4a1-1-distant.wav",
            size: 23122,
            sha256: "fe23d5c351f5b095437c4326a715693cd380c8e81b1f0f8fa2793a2b9970f80c",
        },
    },
];

fn verified_wave(fs: &vfs::Vfs, spec: &WaveSpec<'_>) -> Result<FrozenWave, &'static str> {
    let bytes = fs.read(spec.path).ok_or("retained waveform missing")?;
    if bytes.len() != spec.size {
        return Err("retained waveform size differs");
    }
    if format!("{:x}", Sha256::digest(&bytes)) != spec.sha256 {
        return Err("retained waveform fingerprint differs");
    }
    let (bytes, channels) = decoded_audio(bytes).ok_or("retained waveform is unsupported")?;
    Ok(FrozenWave {
        bytes: Arc::from(bytes),
        channels,
    })
}

impl Profile {
    pub(super) fn load(fs: &vfs::Vfs, entries: &mut HashMap<String, Entry>, era: SoundEra) -> Self {
        let mut profile = Self {
            era,
            ..Default::default()
        };
        if era == SoundEra::Classic2015 {
            for spec in &STOCK {
                if let Err(reason) = profile.install(fs, entries, spec) {
                    // Fixed identifiers/reasons, at most two diagnostics per load.
                    bevy::log::warn!(
                        "Classic2015 {} unavailable: {}; using installed shot",
                        spec.weapon,
                        reason
                    );
                }
            }
        }
        profile
    }

    fn install(
        &mut self,
        fs: &vfs::Vfs,
        entries: &mut HashMap<String, Entry>,
        spec: &WeaponSpec<'_>,
    ) -> Result<(), &'static str> {
        if entries.contains_key(spec.alias) || entries.contains_key(spec.alias_distant) {
            return Err("profile entry name already declared");
        }
        let mut near = entries
            .get(&spec.binding.to_ascii_lowercase())
            .cloned()
            .ok_or("installed shot entry missing")?;
        if near.distant.as_deref() != Some(spec.distant) {
            return Err("installed distant binding differs");
        }
        let mut distant = entries
            .get(spec.distant)
            .cloned()
            .ok_or("installed distant entry missing")?;
        // Validate the complete pair before publishing anything. Never mix a
        // classic near shot with a modern distant waveform after a partial failure.
        let near_audio = verified_wave(fs, &spec.near_wave)?;
        let distant_audio = verified_wave(fs, &spec.distant_wave)?;
        near.waves = vec![spec.near_wave.path.to_string()];
        near.frozen_wave = Some(near_audio);
        near.distant = Some(spec.alias_distant.to_string());
        distant.waves = vec![spec.distant_wave.path.to_string()];
        distant.frozen_wave = Some(distant_audio);
        // Retain the measured current mixer/limits; era-specific DSP is unknown.
        // The alias distant layer must not recurse into an unrelated entry.
        distant.distant = None;
        distant.distant_layer = true;
        entries.insert(spec.alias.to_string(), near);
        entries.insert(spec.alias_distant.to_string(), distant);
        self.selections.insert(
            ContentId::csgo(ContentKind::Weapon, spec.weapon),
            Selection {
                binding: spec.binding,
                alias: spec.alias,
            },
        );
        Ok(())
    }

    pub(super) fn era(&self) -> SoundEra {
        self.era
    }

    pub(super) fn select(&self, id: &ContentId, binding: &str) -> Option<&str> {
        self.selections
            .get(id)
            .filter(|entry| entry.binding.eq_ignore_ascii_case(binding))
            .map(|entry| entry.alias)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::GameOrigin;
    use std::collections::BTreeMap;

    fn wav(sample: i16) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&38u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&44100u32.to_le_bytes());
        bytes.extend_from_slice(&88200u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&sample.to_le_bytes());
        bytes
    }

    fn entries() -> HashMap<String, Entry> {
        let doc = kv::parse(
            r#"
            "Test.Single" { "wave" "weapons/test/current.wav" "volume" "0.9"
                "operator_stacks" { "start_stack" { "import_stack" "CS_distance_weapon_start"
                    "play_distant_version_1" { "entry_name" "Test.Distant" } }
                    "update_stack" { "import_stack" "CS_update_weapon" } } }
            "Test.Distant" { "wave" "weapons/test/current_distant.wav" "volume" "0.2" }
        "#,
        )
        .unwrap();
        doc.iter()
            .filter_map(|(name, value)| {
                value
                    .as_block()
                    .and_then(super::super::parse_entry)
                    .map(|entry| (name.to_ascii_lowercase(), entry))
            })
            .collect()
    }

    fn spec<'a>(near: &'a str, distant: &'a str) -> WeaponSpec<'a> {
        WeaponSpec {
            weapon: "ak47",
            binding: "Test.Single",
            distant: "test.distant",
            alias: "csr.test.near",
            alias_distant: "csr.test.distant",
            near_wave: WaveSpec {
                path: "sound/old_near.wav",
                size: 46,
                sha256: near,
            },
            distant_wave: WaveSpec {
                path: "sound/old_distant.wav",
                size: 46,
                sha256: distant,
            },
        }
    }

    fn fs(near: Vec<u8>, distant: Vec<u8>) -> vfs::Vfs {
        let mut fs = vfs::Vfs::new();
        fs.push_back(
            "synthetic",
            vfs::Mount::Memory(BTreeMap::from([
                ("sound/old_near.wav".into(), Arc::from(near)),
                ("sound/old_distant.wav".into(), Arc::from(distant)),
            ])),
        );
        fs
    }

    #[test]
    fn verified_pair_preserves_original_entries_and_only_selects_exact_stock_identity() {
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        let fs = fs(near.clone(), distant.clone());
        let mut entries = entries();
        let mut profile = Profile::default();
        profile
            .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
            .unwrap();
        assert_eq!(
            entries["test.single"].waves,
            ["sound/weapons/test/current.wav"]
        );
        assert_eq!(
            entries["test.distant"].waves,
            ["sound/weapons/test/current_distant.wav"]
        );
        assert!(entries["test.single"].frozen_wave.is_none());
        assert_eq!(
            entries["csr.test.near"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            near
        );
        assert_eq!(
            entries["csr.test.distant"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            distant
        );
        assert_eq!(
            entries["csr.test.near"].volume,
            entries["test.single"].volume
        );
        assert_eq!(entries["csr.test.near"].limit, Some(2));
        assert_eq!(
            entries["csr.test.near"].distant.as_deref(),
            Some("csr.test.distant")
        );
        assert!(entries["csr.test.distant"].distant_layer);
        let id = ContentId::csgo(ContentKind::Weapon, "ak47");
        assert_eq!(profile.select(&id, "TEST.single"), Some("csr.test.near"));
        assert_eq!(profile.select(&id, "Custom.Single"), None);
        assert_eq!(
            profile.select(
                &ContentId::new(GameOrigin::Mod, ContentKind::Weapon, "ak47"),
                "Test.Single"
            ),
            None
        );
        assert_eq!(
            profile.select(&ContentId::csgo(ContentKind::Sound, "ak47"), "Test.Single"),
            None
        );
        assert_eq!(
            profile.select(
                &ContentId::csgo(ContentKind::Weapon, "usp_silencer"),
                "Test.Single"
            ),
            None
        );
    }

    #[test]
    fn missing_corrupt_and_unsupported_variants_never_publish_half_a_pair() {
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        for fs in [
            vfs::Vfs::new(),
            fs(near.clone(), wav(-1999)),
            fs(vec![0; 46], distant.clone()),
        ] {
            let mut entries = entries();
            let mut profile = Profile::default();
            assert!(
                profile
                    .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
                    .is_err()
            );
            assert_eq!(entries.len(), 2);
            assert!(profile.selections.is_empty());
        }
        let bad = vec![0u8; 46];
        let bad_hash = format!("{:x}", Sha256::digest(&bad));
        let mut entries = entries();
        let mut profile = Profile::default();
        assert_eq!(
            profile.install(
                &fs(bad, distant),
                &mut entries,
                &spec(&bad_hash, &distant_hash)
            ),
            Err("retained waveform is unsupported")
        );
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn later_pack_shadow_cannot_replace_verified_near_or_distant_bytes() {
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        let mut fs = fs(near.clone(), distant.clone());
        let mut entries = entries();
        let mut profile = Profile::default();
        profile
            .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
            .unwrap();
        fs.push_front(
            "selected pack",
            vfs::Mount::Memory(BTreeMap::from([
                ("sound/old_near.wav".into(), Arc::from(wav(9000))),
                ("sound/old_distant.wav".into(), Arc::from(wav(8000))),
            ])),
        );
        assert_ne!(fs.read("sound/old_near.wav").unwrap(), near);
        assert_ne!(fs.read("sound/old_distant.wav").unwrap(), distant);
        assert_eq!(
            entries["csr.test.near"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            near
        );
        assert_eq!(
            entries["csr.test.distant"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            distant
        );
    }

    #[test]
    fn explicit_installed_mode_and_reserved_alias_collision_leave_definitions_untouched() {
        let mut entries = entries();
        let profile = Profile::load(&vfs::Vfs::new(), &mut entries, SoundEra::InstalledLegacy);
        assert_eq!(profile.era(), SoundEra::InstalledLegacy);
        assert_eq!(entries.len(), 2);
        let declared = entries["test.single"].clone();
        entries.insert("csr.test.near".into(), declared);
        let mut profile = Profile::default();
        assert_eq!(
            profile.install(&vfs::Vfs::new(), &mut entries, &spec("", "")),
            Err("profile entry name already declared")
        );
        assert_eq!(entries.len(), 3);
        assert!(entries["csr.test.near"].frozen_wave.is_none());
        assert!(profile.selections.is_empty());
    }

    #[test]
    fn actual_audio_sources_use_verified_pair_after_a_pack_shadows_both_paths() {
        use bevy::{
            audio::{AudioPlayer, AudioSource},
            ecs::world::CommandQueue,
            prelude::*,
        };
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        let mut fs = fs(near.clone(), distant.clone());
        let mut entries = entries();
        let mut profile = Profile::default();
        profile
            .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
            .unwrap();
        fs.push_front(
            "selected pack",
            vfs::Mount::Memory(BTreeMap::from([
                ("sound/old_near.wav".into(), Arc::from(wav(9000))),
                ("sound/old_distant.wav".into(), Arc::from(wav(8000))),
            ])),
        );
        let mut library = super::super::SoundLibrary {
            entries,
            classic: profile,
            ..Default::default()
        };
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut sources = Assets::<AudioSource>::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            library.play_with(
                &mut commands,
                &fs,
                &mut sources,
                "csr.test.near",
                super::super::Emit {
                    at: Some(glam::Vec3::ZERO),
                    ..Default::default()
                },
            );
        }
        queue.apply(&mut world);
        let played: Vec<Vec<u8>> = world
            .query::<&AudioPlayer<AudioSource>>()
            .iter(&world)
            .map(|player| sources.get(&player.0).unwrap().bytes.to_vec())
            .collect();
        assert_eq!(played.len(), 2);
        assert!(played.contains(&near));
        assert!(played.contains(&distant));
    }
}
