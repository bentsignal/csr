//! Import timing from installed model declarations; no animation/engine code port.
use sha1::{Digest,Sha1};
use weapons::{GrenadeAnimations,GrenadeReleaseAnimation,WeaponClass};

fn release(model: &mdl::Model, activity: &str) -> Option<GrenadeReleaseAnimation> {
    let sequence=model.sequences.iter().find(|s|s.activity.eq_ignore_ascii_case(activity))?;
    let animation=model.animations.get(usize::try_from(*sequence.anims.first()?).ok()?)?;
    let duration=animation.duration();
    let events:Vec<_>=sequence.events.iter().filter(|e|e.id==3005||e.name=="EVENT_WEAPON_THROW").collect();
    if events.len()!=1 || !duration.is_finite() || duration<=0. || duration>30. {return None;}
    let cycle=events[0].cycle;
    if !cycle.is_finite() || !(0. ..=1.).contains(&cycle) {return None;}
    Some(GrenadeReleaseAnimation {duration,release_time:cycle*duration})
}

pub(super) fn import(catalog: &mut weapons::Catalog, fs: &vfs::Vfs, previous_hash: Option<[u8;20]>) -> Option<[u8;20]> {
    let mut digest=Sha1::new();digest.update(previous_hash?);
    digest.update(b"grenade-animation-timing-v1");
    let mut definitions:Vec<_>=catalog.weapons.iter_mut().filter(|w|w.class==WeaponClass::Grenade).collect();
    if definitions.is_empty() {return previous_hash;}
    definitions.sort_by(|a,b|a.class_name.cmp(&b.class_name));
    for weapon in definitions {
        digest.update(weapon.class_name.as_bytes());digest.update([0]);
        let Some(bytes)=fs.read(&weapon.view_model) else {digest.update(b"unavailable");continue};
        digest.update(Sha1::digest(&bytes));
        let Some(model)=mdl::Model::load_header(&bytes).ok() else {continue};
        let profile=(|| {
            let pull=model.sequences.iter().find(|s|s.activity.eq_ignore_ascii_case("ACT_VM_PULLPIN"))?;
            let animation=model.animations.get(usize::try_from(*pull.anims.first()?).ok()?)?;
            let duration=animation.duration();
            if !duration.is_finite() || duration<=0. || duration>30. {return None;}
            Some(GrenadeAnimations {pull_pin_duration:duration,overhand:release(&model,"ACT_VM_THROW")?,underhand:release(&model,"ACT_VM_RELEASE")?})
        })();
        weapon.grenade_animations=profile;
    }
    Some(digest.finalize().into())
}
