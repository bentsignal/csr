//! Read-only installed catalog -> selected near/distant waveform audit.
//! No audio device, game window, config writes or third-party bytes exported.
use client::audio::{SoundEra, SoundLibrary};

fn main() {
    let (_, fs) = level::mount_game(None).expect("CS:GO Legacy install");
    let text = fs
        .read_string("scripts/items/items_game.txt")
        .expect("installed item schema");
    let catalog = weapons::Catalog::from_items_game(&text).expect("valid item schema");
    let installed = SoundLibrary::load_with_era(&fs, SoundEra::InstalledLegacy);
    let classic = SoundLibrary::load(&fs);
    assert_eq!(classic.era(), SoundEra::Classic2015);
    for (weapon, binding, historical) in [
        (
            "weapon_ak47",
            "Weapon_AK47.Single",
            Some((
                "sound/weapons/ak47/ak47-1.wav",
                "sound/weapons/ak47/ak47-1-distant.wav",
            )),
        ),
        (
            "weapon_m4a1_silencer",
            "Weapon_M4A1.Silenced",
            Some((
                "sound/weapons/m4a1/m4a1-1.wav",
                "sound/weapons/m4a1/m4a1-1-distant.wav",
            )),
        ),
        ("weapon_usp_silencer", "Weapon_USP.SilencedShot", None),
        ("weapon_m4a1", "Weapon_M4A1.Single", None),
        ("weapon_mp5sd", "Weapon_MP5.Single", None),
    ] {
        let definition = catalog
            .get(weapon)
            .unwrap_or_else(|| panic!("Missing {weapon}"));
        assert_eq!(
            definition.fire_sound, binding,
            "default equipped mapping: {weapon}"
        );
        let selected = classic.weapon_shot_entry(definition);
        assert_eq!(installed.weapon_shot_entry(definition), binding);
        assert_eq!(
            installed.waves(binding),
            classic.waves(binding),
            "public entry changed"
        );
        let near = classic.waves(selected).expect("selected shot entry");
        if let Some((near_path, distant_path)) = historical {
            assert_ne!(
                selected, binding,
                "verified classic profile unavailable: {weapon}"
            );
            assert_eq!(near, &[near_path.to_string()]);
            let distant = classic
                .distant_entry(selected)
                .expect("verified distant partner");
            assert_eq!(classic.waves(distant).unwrap(), &[distant_path.to_string()]);
            println!(
                "{} default={binding} selected={selected} near={near_path} distant={distant_path}; retained bytes verified+decoded",
                definition.id
            );
        } else {
            assert_eq!(selected, binding, "unverified era override: {weapon}");
            println!(
                "{} default={binding} selected={selected} installed waves={near:?}; no verified 2015 override",
                definition.id
            );
        }
        let mut private = definition.clone();
        private.id = format!("mod_weapon_{}", definition.id.name)
            .parse()
            .unwrap();
        assert_eq!(
            classic.weapon_shot_entry(&private),
            binding,
            "private ID altered"
        );
        private.id = definition.id.clone();
        private.fire_sound = "Private.CustomShot".into();
        assert_eq!(
            classic.weapon_shot_entry(&private),
            "Private.CustomShot",
            "custom binding altered"
        );
    }
    println!(
        "weapon sound identity: 5 default mappings, 2 verified classic pairs, custom definitions preserved; failures: 0"
    );
}
