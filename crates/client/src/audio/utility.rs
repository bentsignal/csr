//! Stable sound identities imported from the selected VFS, never bundled waves.
use content::{ContentId, ContentKind};
use sim::utility::Kind;
use super::SoundLibrary;

const BOUNCES: &[(Kind, &str, &str)] = &[
    (Kind::He, "hegrenade_bounce", "HEGrenade.Bounce"),
    (Kind::Flash, "flashbang_bounce", "Flashbang.Bounce"),
    (Kind::Smoke, "smokegrenade_bounce", "SmokeGrenade.Bounce"),
    (Kind::Incendiary, "incgrenade_bounce", "IncGrenade.Bounce"),
    // Owned Legacy decoy observations request the flashbang contact waves.
    (Kind::Decoy, "decoy_bounce", "Flashbang.Bounce"),
];

pub(super) fn bounce_id(kind: Kind) -> Option<ContentId> {
    BOUNCES.iter().find(|(k, _, _)| *k == kind)
        .map(|(_, name, _)| ContentId::csgo(ContentKind::Sound, name))
}

pub(super) fn decoy_expiry_id() -> ContentId {
    ContentId::csgo(ContentKind::Sound, "decoy_expire")
}

pub(super) fn import(library: &mut SoundLibrary) {
    // Observed terminal decoy waves match this installed near/distant entry.
    // Cloning retains its authored partner, gain, pitch and operator metadata.
    if let Some(sound) = library.entries.get("basegrenade.explode").cloned() {
        library.entries.insert(decoy_expiry_id().to_string(), sound);
    }
    for (_, name, entry) in BOUNCES {
        if let Some(sound) = library.entries.get(&entry.to_ascii_lowercase()).cloned() {
            library.entries.insert(ContentId::csgo(ContentKind::Sound, name).to_string(), sound);
        }
    }
    // Molotov contact identity remains unobserved. Missing entries stay silent.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_vfs_import_keeps_authored_sound_and_missing_cues_silent() {
        let mut fs = vfs::Vfs::new();
        fs.push_front("private-content", vfs::Mount::Memory([
            ("scripts/game_sounds_manifest.txt".into(), std::sync::Arc::from(
                &b"game_sounds_manifest { precache_file scripts/contact.txt }"[..])),
            ("scripts/contact.txt".into(), std::sync::Arc::from(
                &br#"HEGrenade.Bounce { wave private/contact.wav volume 0.37 pitch 98 soundlevel SNDLVL_65dB }
                    Flashbang.Bounce { wave private/decoy_contact.wav volume 0.6 pitch 100 soundlevel SNDLVL_75dB }
                    BaseGrenade.Explode { wave private/expiry.wav volume 0.81 pitch 99 soundlevel SNDLVL_140dB
                        operator_stacks { start_stack { play_distant_version_1 { entry_name BaseGrenade.ExplodeDistant } } } }
                    BaseGrenade.ExplodeDistant { wave private/expiry_distant.wav }"#[..])),
        ].into_iter().collect()));
        let sounds = SoundLibrary::load(&fs);
        let id = bounce_id(Kind::He).unwrap().to_string();
        let source = sounds.entries.get("hegrenade.bounce").unwrap();
        let imported = sounds.entries.get(&id).unwrap();
        assert_eq!(imported.waves, source.waves);
        assert_eq!(imported.volume, source.volume);
        assert_eq!(imported.pitch, source.pitch);
        assert_eq!(imported.soundlevel, source.soundlevel);
        let expiry = sounds.entries.get(&decoy_expiry_id().to_string()).unwrap();
        let original = sounds.entries.get("basegrenade.explode").unwrap();
        assert_eq!(expiry.waves, original.waves);
        assert_eq!(expiry.volume, original.volume);
        assert_eq!(expiry.pitch, original.pitch);
        assert_eq!(expiry.soundlevel, original.soundlevel);
        assert_eq!(expiry.distant.as_deref(), Some("basegrenade.explodedistant"));
        assert!(sounds.entries.get(expiry.distant.as_ref().unwrap()).unwrap().distant_layer);
        let decoy = sounds.entries.get(&bounce_id(Kind::Decoy).unwrap().to_string()).unwrap();
        let flash = sounds.entries.get("flashbang.bounce").unwrap();
        assert_eq!(decoy.waves, flash.waves);
        assert_eq!(decoy.volume, flash.volume);
        assert_eq!(decoy.pitch, flash.pitch);
        assert_eq!(decoy.soundlevel, flash.soundlevel);
        assert!(bounce_id(Kind::Molotov).is_none());
        assert!(!sounds.entries.contains_key(&bounce_id(Kind::Smoke).unwrap().to_string()));
    }
}
