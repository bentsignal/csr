//! Bot-only planning against admitted authoritative grounded primary releases.
//! Numerical aim convergence is not a native bot-AI or trajectory parity claim.
use super::*;

impl World {
    fn bot_lob_definition<'a>(&self, bot: &'a crate::Player, definition: u32)
        -> Option<&'a weapons::WeaponDefinition> {
        if !bot.is_bot || !bot.movement.on_ground || bot.tracked.is_some() || bot.aim.is_some() {
            return None;
        }
        let weapon = bot.weapons.iter().find(|w| w.def.def_index == definition)?;
        if weapon.grenade_throw.is_some_and(|s| s.mode != ThrowMode::Overhand) { return None; }
        let def = weapon.def.as_ref();
        // Every known utility kind can carry an admitted primary profile.
        // Unknown names must not borrow the solver merely by carrying metadata.
        (Kind::from_name(&def.class_name).is_some()
            && def.grenade_launch.as_ref()?.grounded_overhand_velocity.is_some()).then_some(def)
    }

    // The closest horizontal crossing and its vertical error use the same
    // mean-velocity gravity displacement as authoritative free flight.
    fn bot_lob_target_error(&self, bot: &crate::Player, definition: &weapons::WeaponDefinition,
        target: Vec3, angles: Vec3) -> Option<(glam::Vec2, f32, Vec3, Vec3)> {
        let (from, velocity) = self.grenade_release(bot, definition, Some(ThrowMode::Overhand), angles)?;
        let horizontal = velocity.truncate();
        let delta = (target - from).truncate();
        let time = delta.dot(horizontal) / horizontal.length_squared();
        if !time.is_finite() || time <= 0.0 || horizontal.length_squared() < 1.0 { return None; }
        let end = from + velocity * time - Vec3::Z * 160.0 * time * time;
        let across = horizontal.normalize().perp_dot((end - target).truncate());
        let error = glam::Vec2::new(end.z - target.z, across);
        error.is_finite().then_some((error, time, from, velocity))
    }

    pub(crate) fn bot_lob_angles(&self, bot: &crate::Player, definition: u32, target: Vec3) -> Option<Vec3> {
        let mut angles = lob_angles(bot.eye(), target)?;
        let Some(definition) = self.bot_lob_definition(bot, definition) else { return Some(angles); };
        // Bounded two-angle Newton solve; these are numerical tolerances, not
        // new movement, launch or native fidelity constants.
        for _ in 0..8 {
            let (error, _, _, _) = self.bot_lob_target_error(bot, definition, target, angles)?;
            if error.length_squared() <= 0.01 { return Some(angles); }
            let dp = (self.bot_lob_target_error(bot, definition, target,
                angles + Vec3::X * 0.1)?.0 - error) / 0.1;
            let dy = (self.bot_lob_target_error(bot, definition, target,
                angles + Vec3::Y * 0.1)?.0 - error) / 0.1;
            let determinant = dp.x * dy.y - dy.x * dp.y;
            if !determinant.is_finite() || determinant.abs() < 0.00001 { return None; }
            angles.x -= ((error.x * dy.y - dy.x * error.y) / determinant).clamp(-12., 12.);
            angles.y -= ((dp.x * error.y - error.x * dp.y) / determinant).clamp(-12., 12.);
            if !angles.is_finite() || angles.x.abs() >= 89.0 { return None; }
        }
        None
    }

    pub(crate) fn clear_bot_lob(&self, bot: &crate::Player, definition: u32,
        target: Vec3, angles: Vec3) -> bool {
        let Some(definition) = self.bot_lob_definition(bot, definition) else {
            return self.clear_lob(bot.eye(), target, angles);
        };
        let Some((error, duration, from, velocity)) = self.bot_lob_target_error(bot, definition, target, angles) else { return false; };
        if error.length_squared() > 0.01 { return false; }
        let mut previous = from;
        for step in 1..=24 {
            let time = duration * step as f32 / 24.0;
            let next = from + velocity * time - Vec3::Z * 160.0 * time * time;
            let tr = self.collision.trace_box(previous, next, Vec3::splat(-2.0),
                Vec3::splat(2.0), contents::MASK_SOLID);
            if tr.hit() { return next.distance(target) < 45.0 && !tr.start_solid; }
            previous = next;
        }
        true
    }
}
