//! Practical thin-cover policy, NOT the native CS:GO penetration formula.
//! Installed modifiers provide material ordering; the inch budget and linear
//! retention curve below are deliberate bounded gameplay approximations.
use collision::{bullet_exit::BulletSurface, CollisionWorld, Trace};
use glam::Vec3;

pub(super) const MAX_SLABS: usize = 4;

fn supported(surface: BulletSurface) -> bool {
    // Installed concrete/metal declarations may admit thin cover under this
    // same approximation; their material codes alone never authorize an exit.
    matches!(surface.material, b'W' | b'U' | b'L' | b'Y' | b'G' | b'C' | b'M')
        && surface.penetration_modifier.is_finite() && surface.penetration_modifier > 0.
        && surface.damage_modifier.is_finite() && (0. ..=1.).contains(&surface.damage_modifier)
}

pub(super) fn through(world: &CollisionWorld, start: Vec3, end: Vec3, hit: &Trace,
    weapon_power: f32) -> Option<(Vec3, f32)> {
    if world.is_sky(hit.surface) || !weapon_power.is_finite() || weapon_power <= 0. { return None; }
    let entry = world.bullet_surface(hit.surface)?;
    if !supported(entry) { return None; }
    let exit = world.bullet_exit(start, end, hit, 64.)?;
    // Include internal faces from every component, even a convex wholly hidden
    // inside wood. Visible endpoint materials cannot authorize that backing.
    let mut modifier = entry.penetration_modifier;
    let mut damage_modifier = entry.damage_modifier;
    for surface in exit.crossed_surfaces {
        if world.is_sky(surface) { return None; }
        let crossed = world.bullet_surface(surface)?;
        if !supported(crossed) { return None; }
        modifier = modifier.min(crossed.penetration_modifier);
        damage_modifier = damage_modifier.min(crossed.damage_modifier);
    }
    let budget = (16. * weapon_power * modifier).min(64.);
    if exit.thickness >= budget { return None; }
    let retention = damage_modifier * (1. - exit.thickness / budget);
    (retention > 0.).then_some((exit.next_start, retention))
}
