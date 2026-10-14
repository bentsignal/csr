//! Stable sound identities imported from the selected VFS, never bundled waves.
use content::{ContentId, ContentKind};
use sim::utility::Kind;
use super::SoundLibrary;

const BOUNCES: &[(Kind, &str, &str)] = &[
    (Kind::He, "hegrenade_bounce", "HEGrenade.Bounce"),
    (Kind::Flash, "flashbang_bounce", "Flashbang.Bounce"),
    (Kind::Smoke, "smokegrenade_bounce", "SmokeGrenade.Bounce"),
    (Kind::Incendiary, "incgrenade_bounce", "IncGrenade.Bounce"),
];

pub(super) fn bounce_id(kind: Kind) -> Option<ContentId> {
    BOUNCES.iter().find(|(k, _, _)| *k == kind)
        .map(|(_, name, _)| ContentId::csgo(ContentKind::Sound, name))
}

pub(super) fn import(library: &mut SoundLibrary) {
    for (_, name, entry) in BOUNCES {
        if let Some(sound) = library.entries.get(&entry.to_ascii_lowercase()).cloned() {
            library.entries.insert(ContentId::csgo(ContentKind::Sound, name).to_string(), sound);
        }
    }
    // No Decoy/Molotov bounce identity was found in installed soundscripts.
    // Missing cues stay silent rather than borrowing an unsupported sound.
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
                &b"HEGrenade.Bounce { wave private/contact.wav volume 0.37 pitch 98 soundlevel SNDLVL_65dB }"[..])),
        ].into_iter().collect()));
        let sounds = SoundLibrary::load(&fs);
        let id = bounce_id(Kind::He).unwrap().to_string();
        let source = sounds.entries.get("hegrenade.bounce").unwrap();
        let imported = sounds.entries.get(&id).unwrap();
        assert_eq!(imported.waves, source.waves);
        assert_eq!(imported.volume, source.volume);
        assert_eq!(imported.pitch, source.pitch);
        assert_eq!(imported.soundlevel, source.soundlevel);
        assert!(bounce_id(Kind::Decoy).is_none() && bounce_id(Kind::Molotov).is_none());
        assert!(!sounds.entries.contains_key(&bounce_id(Kind::Flash).unwrap().to_string()));
    }
}
