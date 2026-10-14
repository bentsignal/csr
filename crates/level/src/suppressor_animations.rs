//! Runtime declarations only; no weapon/engine implementation is imported.
use sha1::{Digest, Sha1};
use weapons::{SuppressorAnimation, SuppressorAnimations};

fn transition(model: &mdl::Model, activity: &str, complete: &str) -> Option<SuppressorAnimation> {
    let sequence = model.sequences.iter().find(|s| s.activity.eq_ignore_ascii_case(activity))?;
    let animation = model.animations.get(usize::try_from(*sequence.anims.first()?).ok()?)?;
    let duration = animation.duration();
    if !duration.is_finite() || duration <= 0. || duration > 30. { return None; }
    let completions: Vec<_> = sequence.events.iter().filter(|e| e.name.eq_ignore_ascii_case(complete)).collect();
    if completions.len() != 1 { return None; }
    let mut visibility: Vec<_> = sequence.events.iter().filter_map(|e| {
        let visible = if e.name.eq_ignore_ascii_case("AE_CL_SHOW_SILENCER") { true }
            else if e.name.eq_ignore_ascii_case("AE_CL_HIDE_SILENCER") { false }
            else { return None; };
        Some((e.cycle * duration, visible))
    }).collect();
    visibility.sort_by(|a, b| a.0.total_cmp(&b.0));
    if visibility.first()?.0 != 0. { return None; }
    Some(SuppressorAnimation { duration, complete_time: completions[0].cycle * duration, visibility })
}

fn draw(model: &mdl::Model, attached: bool) -> Option<String> {
    // The installed M4 label/activity suffixes are counterintuitive. Use the
    // declared first-frame body visibility rather than guessing from a suffix.
    model.sequences.iter().find(|s| s.activity.starts_with("ACT_VM_DRAW") && s.events.iter().any(|e|
        e.cycle == 0. && e.name.eq_ignore_ascii_case(if attached {
            "AE_CL_SHOW_SILENCER"
        } else { "AE_CL_HIDE_SILENCER" }))).map(|s| s.activity.to_ascii_uppercase())
}

pub(super) fn import(catalog: &mut weapons::Catalog, fs: &vfs::Vfs,
    previous_hash: Option<[u8; 20]>) -> Option<[u8; 20]> {
    let mut digest = Sha1::new();
    digest.update(previous_hash?);
    digest.update(b"suppressor-animation-timing-v1");
    let mut definitions: Vec<_> = catalog.weapons.iter_mut()
        .filter(|w| w.attr_f32("has silencer").is_some_and(|v| v > 0.)).collect();
    definitions.sort_by(|a, b| a.class_name.cmp(&b.class_name));
    for weapon in definitions {
        digest.update(weapon.class_name.as_bytes()); digest.update([0]);
        let Some(bytes) = fs.read(&weapon.view_model) else { digest.update(b"unavailable"); continue; };
        digest.update(Sha1::digest(&bytes));
        let Some(model) = mdl::Model::load_header(&bytes).ok() else { continue; };
        weapon.suppressor_animations = (|| Some(SuppressorAnimations {
            attach: transition(&model, "ACT_VM_ATTACH_SILENCER", "AE_CL_ATTACH_SILENCER_COMPLETE")?,
            detach: transition(&model, "ACT_VM_DETACH_SILENCER", "AE_CL_DETACH_SILENCER_COMPLETE")?,
            draw_attached: draw(&model, true)?, draw_detached: draw(&model, false)?,
        }))();
    }
    Some(digest.finalize().into())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires the player's installed Legacy declarations; no GPU"]
    fn installed_removable_suppressors_have_complete_profiles() {
        let level = crate::Level::load("de_dust2", None).unwrap();
        for name in ["weapon_m4a1_silencer", "weapon_usp_silencer"] {
            let weapon = level.catalog.get(name).unwrap();
            let profile = weapon.suppressor_animations.as_ref().expect(name);
            for animation in [&profile.attach, &profile.detach] {
                assert!((0. ..=animation.duration).contains(&animation.complete_time));
                assert!(animation.duration > 0.);
                assert_eq!(animation.visibility.first().unwrap().0, 0.);
            }
            assert_ne!(profile.draw_attached, profile.draw_detached);
            assert_ne!(weapon.fire_sound, weapon.unsuppressed_fire_sound);
            println!("{name}: {profile:?}; {} / {}", weapon.fire_sound, weapon.unsuppressed_fire_sound);
        }
        assert!(level.catalog.get("weapon_mp5sd").unwrap().suppressor_animations.is_none());
    }
}
