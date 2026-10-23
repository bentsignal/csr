//! Practical thin-cover policy, NOT the native CS:GO penetration formula.
//! Installed modifiers provide material ordering; the inch budget and linear
//! retention curve below are deliberate bounded gameplay approximations.
//! The known Wood declaration uses a prefactor fitted to original thin-post
//! shots; this calibrates the existing curve, not the native damage algorithm.
use collision::{bullet_exit::BulletSurface, CollisionWorld, Trace};
use glam::Vec3;

pub(super) const MAX_SLABS: usize = 4;

// Three original Legacy shots through the same ~4-unit wood post: AK
// 36 entry / 7.8 lost, M4A4 33 / 7.3, AWP 115 / 20.1. Using their mounted
// powers (2, 2, 2.5), least squares gives 0.925315; rounded to 0.925, the
// existing thickness curve fits these labels within 0.6 raw damage. A separate
// 30-degree AK shot (11.9cm / 7.9 lost) differs by 0.15. This remains an
// approximation; other thicknesses extrapolate the same linear curve.
fn observed_wood(surface: BulletSurface) -> bool {
    surface.material == b'W' && surface.penetration_modifier == 0.9
        && surface.damage_modifier == 0.6
}

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
    let mut calibrated_wood = observed_wood(entry);
    for surface in exit.crossed_surfaces {
        if world.is_sky(surface) { return None; }
        let crossed = world.bullet_surface(surface)?;
        if !supported(crossed) { return None; }
        calibrated_wood &= observed_wood(crossed);
        modifier = modifier.min(crossed.penetration_modifier);
        damage_modifier = damage_modifier.min(crossed.damage_modifier);
    }
    let budget = (16. * weapon_power * modifier).min(64.);
    if exit.thickness >= budget { return None; }
    let prefactor = if calibrated_wood { 0.925 } else { damage_modifier };
    let retention = prefactor * (1. - exit.thickness / budget);
    (retention > 0.).then_some((exit.next_start, retention))
}
