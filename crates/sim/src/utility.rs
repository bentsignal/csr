//! Deterministic grenades. Inventory, collision and effects are shared by humans and bots.
use crate::{GameEvent, PlayerId, World};
use collision::contents;
use glam::Vec3;
use serde::{Deserialize, Serialize};

const FIRE_DAMAGE_RADIUS: f32 = 140.0;
// Owned Legacy 1.38.8.1 console exposes molotov_throw_detonate_time = 2.0.
// Keep the existing projectile-age clock and per-tick polling; the observed
// airborne bursts do not independently establish native birth/think cadence.
const FIRE_DETONATE_TIME_SECONDS: f32 = 2.0;

mod throw_state;
pub use throw_state::{ThrowMode,ThrowPhase,ThrowState};
mod bot_lob;
mod decoy;
#[cfg(test)]
mod lifecycle_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Smoke,
    Flash,
    He,
    Molotov,
    Incendiary,
    Decoy,
}
impl Kind {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "weapon_smokegrenade" => Self::Smoke,
            "weapon_flashbang" => Self::Flash,
            "weapon_hegrenade" => Self::He,
            "weapon_molotov" => Self::Molotov,
            "weapon_incgrenade" => Self::Incendiary,
            "weapon_decoy" => Self::Decoy,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Smoke => "weapon_smokegrenade",
            Self::Flash => "weapon_flashbang",
            Self::He => "weapon_hegrenade",
            Self::Molotov => "weapon_molotov",
            Self::Incendiary => "weapon_incgrenade",
            Self::Decoy => "weapon_decoy",
        }
    }
    pub fn fire(self) -> bool {
        matches!(self, Self::Molotov | Self::Incendiary)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Projectile {
    pub id: u32,
    pub owner: PlayerId,
    pub kind: Kind,
    pub pos: Vec3,
    pub velocity: Vec3,
    /// Supported rest after the solver's soft floor stop. Smoke activation,
    /// presentation and contact audio share this state through the solver's
    /// small gravity/contact cycles. Never skips ballistic integration.
    pub settled: bool,
    pub born: f32,
    /// Immutable weapon ContentId captured when a decoy leaves the hand.
    /// Other grenade kinds carry no copied gun identity.
    pub decoy_weapon: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: u32,
    pub owner: PlayerId,
    pub kind: Kind,
    pub pos: Vec3,
    pub born: f32,
    pub until: f32,
    pub decoy_weapon: Option<String>,
}

/// Low ballistic arc, rejecting out-of-range targets. Source angles, gravity 320.
pub fn lob_angles(from: Vec3, to: Vec3) -> Option<Vec3> {
    let delta = to - from;
    let x = delta.truncate().length();
    if x < 32.0 {
        return None;
    }
    let v2: f32 = 750.0 * 750.0;
    let disc = v2 * v2 - 320.0 * (320.0 * x * x + 2.0 * delta.z * v2);
    if disc < 0.0 {
        return None;
    }
    let elevation = ((v2 - disc.sqrt()) / (320.0 * x)).atan();
    Some(Vec3::new(
        -elevation.to_degrees(),
        delta.y.atan2(delta.x).to_degrees(),
        0.0,
    ))
}

pub fn crosses_smoke(a: Vec3, b: Vec3, center: Vec3, radius: f32) -> bool {
    let d = b - a;
    let t = ((center - a).dot(d) / d.length_squared().max(0.001)).clamp(0.0, 1.0);
    (a + d * t).distance_squared(center) < radius * radius
}

impl World {
    pub fn smoke_occludes(&self, from: Vec3, to: Vec3) -> bool {
        self.utility_fields.iter().any(|f| {
            f.kind == Kind::Smoke
                && f.until > self.time()
                && crosses_smoke(
                    from,
                    to,
                    f.pos + Vec3::Z * 55.0,
                    140.0 * ((self.time() - f.born) / 0.6).clamp(0.0, 1.0),
                )
        })
    }
    /// Current burn hazard at a prospective actor's feet/origin, not its eye
    /// or chest. NAV centers/portals and tactical stand points use this space.
    pub fn fire_at(&self, point: Vec3) -> bool {
        self.utility_fields.iter().any(|f| {
            f.kind.fire()
                && f.until > self.time()
                && self.utility_reaches(f.pos + Vec3::Z * 12.0,
                    point + Vec3::Z * 32.0, FIRE_DAMAGE_RADIUS).is_some()
        })
    }
    pub fn flash_amount(&self, id: PlayerId) -> f32 {
        self.player(id).map_or(0.0, |p| {
            ((p.flash_until - self.time()) / 1.5).clamp(0.0, 1.0)
        })
    }
    /// Validate the actual throw arc; bots don't throw through ceilings/walls.
    pub fn clear_lob(&self, from: Vec3, target: Vec3, angles: Vec3) -> bool {
        let (f, _, _) = movement::angle_vectors(angles);
        let duration =
            from.truncate().distance(target.truncate()) / (f.truncate().length() * 750.0).max(1.0);
        let mut previous = from;
        for step in 1..=24 {
            let t = duration * step as f32 / 24.0;
            let next = from + f * 750.0 * t - Vec3::Z * 160.0 * t * t;
            let tr = self.collision.trace_box(
                previous,
                next,
                Vec3::splat(-2.0),
                Vec3::splat(2.0),
                contents::MASK_SOLID,
            );
            if tr.hit() {
                return next.distance(target) < 45.0 && !tr.start_solid;
            }
            previous = next;
        }
        true
    }
    /// Same release calculation for authoritative spawning and bot prediction.
    /// Only callers choose a mode; metadata/content admission stays on the definition.
    fn grenade_release(&self, p: &crate::Player, definition: &weapons::WeaponDefinition,
        mode: Option<ThrowMode>, angles: Vec3) -> Option<(Vec3, Vec3)> {
        let (from, _) = p.aim_origin_angles();
        let (forward, _, _) = movement::angle_vectors(angles);
        let tr = self.collision.trace_box(from, from + forward * 18.0,
            Vec3::splat(-2.0), Vec3::splat(2.0), contents::MASK_SOLID);
        if tr.start_solid { return None; }
        let pos = from + forward * 18.0 * tr.fraction;
        Some((pos, Self::grenade_launch_velocity(p, definition, mode, angles)))
    }
    fn grenade_launch_velocity(p: &crate::Player, definition: &weapons::WeaponDefinition,
        mode: Option<ThrowMode>, angles: Vec3) -> Vec3 {
        let (forward, _, _) = movement::angle_vectors(angles);
        let strength=mode.map_or(1.,ThrowMode::strength);
        // Read the release tick's actual movement, not the pull-pin tick. This
        // includes running/backpedalling/jumping; crouch uses the current eye.
        // Button endpoint velocities are independent observations. Only the
        // independently admitted modes receive a measured movement response;
        // retain each absent component's prior response independently.
        let mut inherited = p.movement.velocity * 0.5;
        let mut launch = forward * (750.0 * strength);
        if let Some(profile) = definition.grenade_launch.as_ref() {
            let (horizontal_inheritance, vertical_inheritance, velocity_profile) = if p.movement.on_ground {
                let horizontal = match mode {
                    Some(ThrowMode::Overhand) => Some(profile.grounded_overhand_horizontal_inheritance),
                    Some(ThrowMode::Underhand) => profile.grounded_underhand_horizontal_inheritance,
                    Some(ThrowMode::Combined) => profile.grounded_combined_horizontal_inheritance,
                    None => None,
                };
                let velocity = match mode {
                    Some(ThrowMode::Overhand) => profile.grounded_overhand_velocity.as_ref(),
                    Some(ThrowMode::Underhand) => profile.grounded_underhand_velocity.as_ref(),
                    Some(ThrowMode::Combined) => profile.grounded_combined_velocity.as_ref(),
                    None => None,
                };
                (horizontal, None, velocity)
            } else {
                match mode {
                    Some(ThrowMode::Overhand) => (
                        profile.airborne_overhand_horizontal_inheritance,
                        profile.airborne_overhand_vertical_inheritance,
                        profile.airborne_overhand_velocity.as_ref()),
                    Some(ThrowMode::Underhand) => (
                        profile.airborne_underhand_horizontal_inheritance,
                        profile.airborne_underhand_vertical_inheritance,
                        profile.airborne_underhand_velocity.as_ref()),
                    Some(ThrowMode::Combined) => (
                        profile.airborne_combined_horizontal_inheritance,
                        profile.airborne_combined_vertical_inheritance,
                        profile.airborne_combined_velocity.as_ref()),
                    None => (None, None, None),
                }
            };
            if let Some(inheritance) = horizontal_inheritance {
                inherited.x = p.movement.velocity.x * inheritance;
                inherited.y = p.movement.velocity.y * inheritance;
            }
            if let Some(inheritance) = vertical_inheritance {
                inherited.z = p.movement.velocity.z * inheritance;
            }
            if let Some(profile) = velocity_profile {
                // Keep the separately uncalibrated spawn trace on the raw
                // aim ray above. Only the release velocity uses this law.
                let mut launch_angles = angles;
                launch_angles.x -= profile.upward_pitch_bias_degrees
                    * (1. - angles.x.abs() / 90.);
                let (direction, _, _) = movement::angle_vectors(launch_angles);
                launch = direction * profile.speed;
            }
        }
        launch + inherited
    }
    fn spawn_grenade(&mut self, i: usize) -> bool {
        let time = self.time();
        let p = &self.players[i];
        let Some(kind) = p
            .active_weapon()
            .and_then(|w| Kind::from_name(&w.def.class_name))
        else {
            return false;
        };
        let (_, angles) = p.aim_origin_angles();
        let definition = &p.active_weapon().unwrap().def;
        let mode = p.active_weapon().and_then(|w| w.grenade_throw).map(|s| s.mode);
        let Some((pos, velocity)) = self.grenade_release(p, definition, mode, angles) else { return false; };
        let owner = p.id;
        let decoy_weapon = (kind == Kind::Decoy).then(|| {
            p.weapons.iter()
                .filter(|w| matches!(w.def.class, weapons::WeaponClass::Pistol |
                    weapons::WeaponClass::Smg | weapons::WeaponClass::Rifle |
                    weapons::WeaponClass::Sniper | weapons::WeaponClass::Shotgun |
                    weapons::WeaponClass::MachineGun))
                .min_by_key(|w| crate::weapon::slot(&w.def))
                .map(|w| w.def.id.to_string())
                .or_else(|| self.catalog.get(if p.team == crate::Team::Terrorist {
                    "weapon_glock"
                } else { "weapon_usp_silencer" }).map(|w| w.id.to_string()))
        }).flatten();
        self.next_item += 1;
        self.projectiles.push(Projectile {
            id: self.next_item,
            owner,
            kind,
            pos,
            velocity,
            settled: false,
            born: time,
            decoy_weapon,
        });
        self.events.push(GameEvent::UtilityThrown {
            player: owner,
            kind,
            pos,
        });
        true
    }
    pub(crate) fn step_utility(&mut self) {
        const CONTACT_NUDGE: f32 = 0.1;
        const FLOOR_NORMAL_Z: f32 = 0.65;
        let time = self.time();
        let dt = self.dt();
        let mut exploded = Vec::new();
        for p in &mut self.projectiles {
            let was_settled = p.settled;
            let was_moving = p.velocity != Vec3::ZERO;
            let previous_velocity = p.velocity;
            p.velocity.z -= 320.0 * dt;
            // Observed Legacy pre-contact positions follow the mean vertical
            // velocity over the tick. Using only the final velocity adds an
            // extra half-tick of gravity displacement on every flight step.
            let end = p.pos + (previous_velocity + p.velocity) * (0.5 * dt);
            let tr = self.collision.trace_box(
                p.pos,
                end,
                Vec3::splat(-2.0),
                Vec3::splat(2.0),
                contents::MASK_SOLID,
            );
            p.pos = p.pos.lerp(end, tr.fraction);
            let floor = tr.hit() && tr.normal.z > FLOOR_NORMAL_Z;
            if tr.hit() {
                p.pos += tr.normal * CONTACT_NUDGE;
                p.velocity = (p.velocity - 2.0 * p.velocity.dot(tr.normal) * tr.normal) * 0.45;
                if floor && p.velocity.length() < 35.0 {
                    p.velocity = Vec3::ZERO;
                    p.settled = true;
                } else {
                    p.settled = false;
                }
            } else if p.settled {
                // The unchanged solver still applies gravity after a soft stop.
                // Keep the visual rest mark through its separation/contact cycle,
                // but wake immediately if a dynamic world removes the support.
                // Probe only the existing nudge + collision skin, projected onto
                // the shallowest floor normal accepted by this same solver.
                let support = self.collision.trace_box(
                    p.pos,
                    p.pos - Vec3::Z * ((CONTACT_NUDGE + collision::DIST_EPSILON) / FLOOR_NORMAL_Z),
                    Vec3::splat(-2.0),
                    Vec3::splat(2.0),
                    contents::MASK_SOLID,
                );
                // Rest integration can only add downward gravity to a stopped
                // projectile. An upward or horizontal impulse invalidates the
                // old rest mark even while the previous support is still near.
                p.settled = previous_velocity.x == 0. && previous_velocity.y == 0.
                    && previous_velocity.z <= 0.
                    && support.hit() && !support.start_solid && support.normal.z > FLOOR_NORMAL_Z;
            }
            let age = time - p.born;
            // Retain the existing contact slope/age gate pending independent
            // surface-law evidence. Expiry alone must not create ground fire.
            // No guessed downward projection range is introduced here.
            let ignite = p.kind.fire() && floor && age > 0.15;
            let detonate = if p.kind.fire() {
                ignite || age > FIRE_DETONATE_TIME_SECONDS
            } else if p.kind == Kind::Smoke {
                // A slow apex and an expired age timer are not landings.
                // Reuse the floor stop/support check above: support loss must
                // wake the projectile before its activation is considered.
                age > 1.5 && p.settled && p.velocity.length() < 20.0
            } else if p.kind == Kind::Decoy {
                age > 1.5 && (p.velocity.length() < 20.0 || age > 3.0)
            } else {
                age > 1.5
            };
            // Include the actual final settling contact. The pre-step rest mark
            // distinguishes it from the solver's later gravity/nudge chatter.
            // No new speed threshold or velocity-to-loudness curve is added.
            if was_moving && !was_settled && tr.hit() && !tr.start_solid
                && !(p.kind.fire() && detonate)
            {
                self.events.push(GameEvent::UtilityBounced {
                    player: p.owner, kind: p.kind, pos: p.pos,
                });
            }
            if detonate {
                exploded.push((p.clone(), ignite));
            }
        }
        self.projectiles
            .retain(|p| !exploded.iter().any(|(e, _)| e.id == p.id));
        for (p, ignite) in exploded {
            self.events.push(GameEvent::UtilityDetonated {
                player: p.owner,
                kind: p.kind,
                pos: p.pos,
            });
            match p.kind {
                Kind::Flash => {
                    for i in 0..self.players.len() {
                        let player = &self.players[i];
                        let delta = p.pos - player.eye();
                        if !player.alive
                            || delta.length() > 1800.0
                            || !self.visible(player.eye(), p.pos)
                        {
                            continue;
                        }
                        let (f, _, _) = movement::angle_vectors(player.view_angles);
                        let facing = f.dot(delta.normalize_or_zero());
                        let duration = (1.0 - delta.length() / 1800.0)
                            * (if facing > 0.5 {
                                4.8
                            } else if facing > -0.3 {
                                2.0
                            } else {
                                0.5
                            });
                        self.players[i].flash_until =
                            self.players[i].flash_until.max(time + duration);
                    }
                }
                Kind::He => self.blast_utility(p.owner, p.kind, p.pos, 350.0, 98.0),
                // Aerial fire bursts share the existing terminal event, but
                // have no persistent hazard/field (observed with no inferno).
                kind if kind.fire() && !ignite => {},
                kind => self.utility_fields.push(Field {
                    id: p.id,
                    owner: p.owner,
                    kind,
                    pos: p.pos,
                    born: time,
                    until: time
                        + if kind == Kind::Smoke {
                            18.0
                        } else if kind == Kind::Decoy {
                            decoy::LIFETIME_SECONDS
                        } else {
                            7.0
                        },
                    decoy_weapon: p.decoy_weapon,
                }),
            }
        }
        // Smoke extinguishes fire, including fire created after the smoke.
        let smokes: Vec<_> = self
            .utility_fields
            .iter()
            .filter(|f| f.kind == Kind::Smoke && f.until > time)
            .map(|f| f.pos)
            .collect();
        // Only timed decoy expiry emits this cue. Round cleanup clears fields
        // independently, and must never sound like a live decoy has expired.
        for f in self.utility_fields.iter().filter(|f| f.kind == Kind::Decoy && f.until <= time) {
            self.events.push(GameEvent::DecoyExpired { player: f.owner, pos: f.pos });
        }
        self.utility_fields.retain(|f| {
            f.until > time && !(f.kind.fire() && smokes.iter().any(|s| s.distance(f.pos) < 180.0))
        });
        for f in self.utility_fields.iter().filter(|f| f.kind == Kind::Decoy) {
            if decoy::shot_due(f, self.tick, self.tick_rate) {
                if let Some(weapon) = &f.decoy_weapon { self.events.push(GameEvent::DecoyShot {
                    player: f.owner,
                    pos: f.pos,
                    weapon: weapon.clone(),
                }); }
            }
        }
        if self.tick % (13 * self.tick_rate as u64 / 64).max(1) == 0 {
            let fields = self.utility_fields.clone();
            for f in fields.iter().filter(|f| f.kind.fire()) {
                self.blast_utility(f.owner, f.kind, f.pos + Vec3::Z * 12.0, FIRE_DAMAGE_RADIUS, 8.0);
            }
        }
    }
    /// Return the existing radial damage distance only when the same source
    /// can reach the target. Hazard queries and actual burns share this test.
    fn utility_reaches(&self, pos: Vec3, point: Vec3, radius: f32) -> Option<f32> {
        let dist = point.distance(pos);
        if dist >= radius || !self.visible(pos, point) { return None; }
        Some(dist)
    }
    fn blast_utility(&mut self, owner: PlayerId, kind: Kind, pos: Vec3, radius: f32, base: f32) {
        let attacker = self.players.iter().position(|p| p.id == owner);
        for i in 0..self.players.len() {
            let p = &self.players[i];
            let chest = p.movement.origin + Vec3::Z * 32.0;
            if !p.alive { continue; }
            let Some(dist) = self.utility_reaches(pos, chest, radius) else { continue; };
            let friendly = attacker.is_some_and(|a| a != i && self.players[a].team == p.team);
            let raw =
                base * (if kind.fire() {
                    1.0
                } else {
                    (1.0 - dist / radius).powf(1.2)
                }) * if friendly { 0.35 } else { 1.0 };
            let absorbed = if kind == Kind::He {
                (raw * 0.5).min(p.armor as f32)
            } else {
                0.0
            };
            let damage = (raw - absorbed).ceil() as i32;
            let victim = p.id;
            self.players[i].armor -= absorbed as i32;
            self.players[i].health -= damage;
            self.events.push(GameEvent::Damage {
                attacker: owner,
                victim,
                health: damage,
                group: crate::weapon::HitGroup::Chest,
                pos: chest,
                protection: if absorbed > 0.0 { crate::weapon::HitProtection::Kevlar } else { crate::weapon::HitProtection::Flesh },
            });
            if self.players[i].health <= 0 {
                self.kill(i, attacker, kind.name(), false, None);
                if let Some(a) = attacker {
                    if friendly || a == i {
                        self.players[a].kills = self.players[a].kills.saturating_sub(1);
                    } else if self.competitive() {
                        self.players[a].money =
                            (self.players[a].money + 300).min(self.rules.max_money);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Team;

    #[test]
    fn free_flight_replays_observed_legacy_he_positions_without_tick_rate_sag() {
        use std::sync::Arc;
        // Existing owned Legacy 1.38.8.1, 128 Hz public projectile dumps:
        // public-grenade-state-20261015-170424 (private fidelity packet).
        // Anchor at a received state, not an inferred launch/spawn timestamp.
        // The first segment spans 21 native ticks; the second spans 10 and
        // also admits a 5-step 64 Hz resampling (not a native 64 Hz claim).
        for (rate, steps, from, velocity, observed_end, observed_velocity_z) in [
            (128, 21, Vec3::new(1094.15625, 2380., 174.96875),
                Vec3::new(670.435852, 0., -123.362717),
                Vec3::new(1204.15625, 2380., 150.4375), -175.862717),
            (128, 10, Vec3::new(1125.59375, 2380., 168.84375),
                Vec3::new(670.435852, 0., -138.362717),
                Vec3::new(1177.96875, 2380., 157.0625), -163.362717),
            (64, 5, Vec3::new(1125.59375, 2380., 168.84375),
                Vec3::new(670.435852, 0., -138.362717),
                Vec3::new(1177.96875, 2380., 157.0625), -163.362717),
        ] {
            let mut collision = collision::CollisionWorld::new();
            collision.finalize();
            let mut world = World::new(Arc::new(collision));
            world.set_tick_rate(rate);
            world.projectiles.push(Projectile {
                id: 1, owner: crate::WORLD, kind: Kind::He, pos: from,
                velocity, settled: false, born: 0., decoy_weapon: None,
            });
            let mut restored = world.clone();
            restored.restore(world.decode(&world.encode_state()).unwrap());
            for _ in 0..steps {
                world.step(&[]);
                restored.step_prediction(&[]);
                assert_eq!(world.state(), restored.state());
                assert!(world.events.is_empty(), "pre-contact flight has no impact or detonation");
            }
            let projectile = &world.projectiles[0];
            // Conservatively allow one 1/32 unit per received position
            // endpoint. The previous final-velocity integration has >0.10
            // units of vertical error on both 128 Hz segments.
            let error = (projectile.pos - observed_end).abs();
            assert!(error.max_element() < 2. / 32., "rate {rate}, steps {steps}: {error:?}");
            assert_eq!(projectile.velocity.truncate(), velocity.truncate());
            assert!((projectile.velocity.z - observed_velocity_z).abs() < 0.0001);
            assert!(!projectile.settled);
        }
    }

    fn launch_he(velocity: Vec3, grounded: bool, mode: ThrowMode, measured: bool,
        angles: Vec3, vector: Option<weapons::GrenadeLaunchVelocity>) -> Projectile {
        launch_he_with_mode_inheritance(velocity, grounded, mode, measured, angles, vector, None)
    }

    fn launch_he_with_mode_inheritance(velocity: Vec3, grounded: bool, mode: ThrowMode, measured: bool,
        angles: Vec3, vector: Option<weapons::GrenadeLaunchVelocity>, horizontal: Option<f32>) -> Projectile {
        use std::sync::Arc;
        let mut world = arena();
        world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#""items_game" { "items" { "44" { "name" "weapon_hegrenade" "attributes" { "primary default clip size" "1" } } } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = measured.then(|| weapons::GrenadeLaunch {
            reference: "native-grounded-he-button-vectors".into(),
            grounded_overhand_horizontal_inheritance: 1.25,
            grounded_underhand_horizontal_inheritance: (mode == ThrowMode::Underhand).then_some(horizontal).flatten(),
            grounded_combined_horizontal_inheritance: (mode == ThrowMode::Combined).then_some(horizontal).flatten(),
            grounded_overhand_velocity: (mode == ThrowMode::Overhand).then(|| vector.clone()).flatten(),
            grounded_underhand_velocity: (mode == ThrowMode::Underhand).then(|| vector.clone()).flatten(),
            grounded_combined_velocity: (mode == ThrowMode::Combined).then_some(vector).flatten(),
            airborne_overhand_velocity: None,
            airborne_overhand_horizontal_inheritance: None,
            airborne_overhand_vertical_inheritance: None,
            airborne_underhand_velocity: None,
            airborne_underhand_horizontal_inheritance: None,
            airborne_underhand_vertical_inheritance: None,
            airborne_combined_velocity: None,
            airborne_combined_horizontal_inheritance: None,
            airborne_combined_vertical_inheritance: None,
        });
        let mut weapon = crate::weapon::WeaponState::new(Arc::new(definition));
        weapon.grenade_throw = Some(ThrowState { phase: ThrowPhase::Throwing, mode,
            pull_started_tick: 0, throw_started_tick: Some(0), release_requested: true, spawned: false });
        world.players[0].weapons = vec![weapon];
        world.players[0].active = 0;
        world.players[0].movement.on_ground = grounded;
        world.players[0].movement.velocity = velocity;
        world.players[0].view_angles = angles;
        assert!(world.spawn_grenade(0));
        world.projectiles[0].clone()
    }

    #[test]
    fn bot_primary_plan_reaches_target_through_tick_input_and_rejects_calibrated_path_blocker() {
        use std::sync::Arc;
        use collision::{CollisionWorld, Convex, Plane};
        use crate::TickInput;
        let solid = |mins: Vec3, maxs: Vec3| Convex { plane_surfaces: Vec::new(),
            mins, maxs,
            planes: [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
                .into_iter().zip([maxs.x, -mins.x, maxs.y, -mins.y, maxs.z, -mins.z])
                .map(|(normal, dist)| Plane { normal, dist }).collect(),
            contents: contents::SOLID, surface: 0,
        };
        let scene = |blocker: Option<Vec3>| {
            let mut collision = CollisionWorld::new();
            collision.add_convex(solid(Vec3::new(-2000., -2000., -16.), Vec3::new(2000., 2000., 0.)));
            if let Some(point) = blocker {
                collision.add_convex(solid(point - Vec3::splat(2.), point + Vec3::splat(2.)));
            }
            collision.finalize();
            Arc::new(collision)
        };
        // Preserve the same world-space lateral wish while solver yaw changes.
        let input = |angles: Vec3, moving: bool, buttons: u32| {
            let (forward, right, _) = movement::angle_vectors(Vec3::new(0., angles.y, 0.));
            let wish = if moving { Vec3::Y * 245. } else { Vec3::ZERO };
            TickInput { view_angles: angles, forward_move: wish.dot(forward),
                side_move: wish.dot(right), buttons, ..Default::default() }
        };
        let check = |rate: u32, name: &str, index: u32, kind: Kind, moving: bool| {
            let mut catalog = weapons::Catalog::from_items_game(&format!(r#"items_game {{ items {{
                {index} {{ name {name} attributes {{ "primary default clip size" 1 }} }}
            }} }}"#)).unwrap();
            let def = &mut catalog.weapons[0];
            def.max_player_speed = 245.;
            def.grenade_launch = Some(weapons::GrenadeLaunch {
                reference: "admitted-grounded-primary-profile-fixture".into(),
                grounded_overhand_horizontal_inheritance: 1.25,
                grounded_underhand_horizontal_inheritance: None,
                grounded_combined_horizontal_inheritance: None,
                grounded_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                    speed: 675., upward_pitch_bias_degrees: 10.,
                }),
                grounded_underhand_velocity: None, grounded_combined_velocity: None,
                airborne_overhand_velocity: None,
                airborne_overhand_horizontal_inheritance: None,
                airborne_overhand_vertical_inheritance: None,
                airborne_underhand_velocity: None,
                airborne_underhand_horizontal_inheritance: None,
                airborne_underhand_vertical_inheritance: None,
                airborne_combined_velocity: None,
                airborne_combined_horizontal_inheritance: None,
                airborne_combined_vertical_inheritance: None,
            });
            let release = weapons::GrenadeReleaseAnimation {
                duration: 0.5, release_time: 0.3, command_release_time: Some(0.1),
            };
            def.grenade_animations = Some(weapons::GrenadeAnimations {
                pull_pin_duration: 0.5, overhand: release.clone(), underhand: release,
            });
            let mut world = World::with_catalog(scene(None), Arc::new(catalog));
            world.set_tick_rate(rate);
            let owner = world.add_player("thrower", Team::Terrorist);
            let def = Arc::new(world.catalog.weapons[0].clone());
            world.players[0].weapons = vec![crate::weapon::WeaponState::new(def)];
            world.players[0].active = 0;
            world.players[0].is_bot = true;
            world.players[0].movement.origin = Vec3::Z * 0.03125;
            world.players[0].movement.on_ground = true;
            for _ in 0..rate {
                world.step(&[(owner, input(Vec3::ZERO, moving, movement::buttons::ATTACK))]);
            }
            assert!(world.projectiles.is_empty());
            assert_eq!(world.players[0].active_weapon().unwrap().grenade_throw.unwrap().phase, ThrowPhase::Holding);
            let target = Vec3::new(700., 120., 8.);
            for unsupported in ["air", "air-underhand", "air-combined", "underhand", "combined", "no-profile", "no-velocity", "human",
                "tracked", "aim", "unknown-definition", "unknown-kind"] {
                let mut actor = world.players[0].clone();
                let mut definition = index;
                match unsupported {
                    "air" | "air-underhand" | "air-combined" => {
                        actor.movement.on_ground = false;
                        actor.weapons[0].grenade_throw.as_mut().unwrap().mode = match unsupported {
                            "air-underhand" => ThrowMode::Underhand,
                            "air-combined" => ThrowMode::Combined,
                            _ => ThrowMode::Overhand,
                        };
                        // UNADMITTED alternate-air candidates do not expand
                        // the existing grounded-only bot planner's scope.
                        let air = observed_smoke_air_buttons_launch();
                        let profile = Arc::make_mut(&mut actor.weapons[0].def).grenade_launch.as_mut().unwrap();
                        profile.airborne_overhand_velocity = air.airborne_overhand_velocity;
                        profile.airborne_overhand_horizontal_inheritance = air.airborne_overhand_horizontal_inheritance;
                        profile.airborne_overhand_vertical_inheritance = air.airborne_overhand_vertical_inheritance;
                        profile.airborne_underhand_velocity = air.airborne_underhand_velocity;
                        profile.airborne_underhand_horizontal_inheritance = air.airborne_underhand_horizontal_inheritance;
                        profile.airborne_underhand_vertical_inheritance = air.airborne_underhand_vertical_inheritance;
                        profile.airborne_combined_velocity = air.airborne_combined_velocity;
                        profile.airborne_combined_horizontal_inheritance = air.airborne_combined_horizontal_inheritance;
                        profile.airborne_combined_vertical_inheritance = air.airborne_combined_vertical_inheritance;
                    },
                    "underhand" => actor.weapons[0].grenade_throw.as_mut().unwrap().mode = ThrowMode::Underhand,
                    "combined" => actor.weapons[0].grenade_throw.as_mut().unwrap().mode = ThrowMode::Combined,
                    "no-profile" => Arc::make_mut(&mut actor.weapons[0].def).grenade_launch = None,
                    "no-velocity" => Arc::make_mut(&mut actor.weapons[0].def).grenade_launch.as_mut().unwrap()
                        .grounded_overhand_velocity = None,
                    "human" => actor.is_bot = false,
                    "tracked" => actor.tracked = Some(crate::TrackedPose::default()),
                    "aim" => actor.aim = Some(crate::AimRay::default()),
                    "unknown-definition" => definition += 10000,
                    "unknown-kind" => Arc::make_mut(&mut actor.weapons[0].def).class_name = "weapon_unknown_grenade".into(),
                    _ => unreachable!(),
                }
                let fallback = lob_angles(actor.eye(), target).unwrap();
                assert_eq!(world.bot_lob_angles(&actor, definition, target), Some(fallback), "{unsupported}");
                assert_eq!(world.clear_bot_lob(&actor, definition, target, fallback),
                    world.clear_lob(actor.eye(), target, fallback), "{unsupported}");
            }
            // Stop immediately before the ordinary command-relative release tick.
            world.step(&[(owner, input(Vec3::ZERO, moving, 0))]);
            let started = world.players[0].active_weapon().unwrap().grenade_throw.unwrap().throw_started_tick.unwrap();
            let delay = world.players[0].active_weapon().unwrap().def.grenade_animations.as_ref().unwrap()
                .overhand.projectile_delay();
            while world.tick < started + (delay / world.dt()).ceil() as u64 {
                world.step(&[(owner, input(Vec3::ZERO, moving, 0))]);
            }
            assert!(world.projectiles.is_empty());
            // Isolate planning/release agreement at the release actor state. A
            // cloned TickInput step supplies post-movement origin/velocity; this
            // does not assert that bot decisions anticipate future movement.
            let mut release_actor = world.clone();
            release_actor.step_prediction(&[(owner, input(Vec3::ZERO, moving, 0))]);
            let actor = &release_actor.players[0];
            assert!(actor.movement.on_ground);
            if moving { assert!((actor.movement.velocity.y - 245.).abs() < 0.001); }
            let angles = world.bot_lob_angles(actor, index, target).unwrap();
            assert!(world.clear_bot_lob(actor, index, target, angles));
            let (from, velocity) = world.grenade_release(actor, &actor.weapons[0].def,
                Some(ThrowMode::Overhand), angles).unwrap();
            let duration = (target - from).truncate().dot(velocity.truncate()) / velocity.truncate().length_squared();
            let flight_ticks = (duration / world.dt()).ceil() as u32 + 1;
            // Reach the target before the existing shortest HE/Flash fuse.
            // This tests free-flight planning, not each kind's detonation law.
            assert!(duration > 0. && flight_ticks as f32 * world.dt() < 1.5,
                "{name}, {rate} Hz, moving={moving}: target must precede the fuse");
            let midpoint = from + velocity * (duration * 0.5) - Vec3::Z * 160. * (duration * 0.5).powi(2);
            let mut blocked = world.clone();
            blocked.collision = scene(Some(midpoint));
            assert!(!blocked.clear_bot_lob(actor, index, target, angles));
            let fallback = lob_angles(actor.eye(), target).unwrap();
            assert!((angles - fallback).length() > 1., "must solve beyond the raw-750 fallback");
            assert!(blocked.clear_lob(actor.eye(), target, fallback), "old prediction misses this path blocker");
            let direction = velocity.truncate().normalize();
            let mut previous = from;
            let mut crossing = None;
            let mut hit_blocker = false;
            for step in 1..=flight_ticks {
                for candidate in [&mut world, &mut blocked] {
                    candidate.step(&[(owner, input(angles, moving, 0))]);
                }
                if step == 1 {
                    assert!((world.players[0].movement.origin - actor.movement.origin).length() < 0.001);
                    assert!((world.players[0].movement.velocity - actor.movement.velocity).length() < 0.001);
                    assert!(world.events.iter().any(|e| matches!(e,
                        GameEvent::UtilityThrown { kind: actual, .. } if *actual == kind)));
                    assert_eq!(world.projectiles[0].kind, kind);
                    assert!((world.projectiles[0].velocity - (velocity - Vec3::Z * 320. * world.dt())).length() < 0.001);
                }
                // Fire may terminate on a floor-facing contact instead of
                // bouncing. Either event must identify this projectile's kind,
                // owner and the actual midpoint blocker, not a later floor hit.
                hit_blocker |= blocked.events.iter().any(|e| match e {
                    GameEvent::UtilityBounced { player, kind: actual, pos }
                    | GameEvent::UtilityDetonated { player, kind: actual, pos } =>
                        *player == owner && *actual == kind && pos.distance(midpoint) < 10.,
                    _ => false,
                });
                let p = world.projectiles.first().expect("free-flight projectile before target");
                let time = step as f32 * world.dt();
                let planned = from + velocity * time - Vec3::Z * 160. * time * time;
                assert!((p.pos - planned).length() < 0.01, "{name}, {rate} Hz, moving={moving}");
                let a = (previous - target).truncate().dot(direction);
                let b = (p.pos - target).truncate().dot(direction);
                if a < 0. && b >= 0. { crossing = Some(previous.lerp(p.pos, -a / (b-a))); break; }
                previous = p.pos;
            }
            assert!((crossing.expect("ordinary primary throw crosses the target plane") - target).length() < 1.0);
            assert!(hit_blocker, "the same TickInput launch hits the rejected path blocker");
        };
        for rate in [64, 128] {
            for (name, index, kind) in [
                ("weapon_hegrenade", 44, Kind::He), ("weapon_smokegrenade", 45, Kind::Smoke),
                ("weapon_flashbang", 43, Kind::Flash), ("weapon_molotov", 46, Kind::Molotov),
                ("weapon_incgrenade", 48, Kind::Incendiary), ("weapon_decoy", 47, Kind::Decoy),
            ] {
                for moving in [false, true] { check(rate, name, index, kind, moving); }
            }
        }
    }

    fn observed_smoke_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "native-grounded-smoke-button-response-128hz".into(),
            grounded_overhand_horizontal_inheritance: 1.25,
            grounded_underhand_horizontal_inheritance: Some(1.25),
            grounded_combined_horizontal_inheritance: Some(1.25),
            grounded_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            grounded_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            grounded_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_velocity: None,
            airborne_overhand_horizontal_inheritance: None,
            airborne_overhand_vertical_inheritance: None,
            airborne_underhand_velocity: None,
            airborne_underhand_horizontal_inheritance: None,
            airborne_underhand_vertical_inheritance: None,
            airborne_combined_velocity: None,
            airborne_combined_horizontal_inheritance: None,
            airborne_combined_vertical_inheritance: None,
        }
    }

    // Independently qualified Legacy Smoke primary-air response. See the
    // bounded native observations in research/smoke-horizontal-inheritance.md.
    fn observed_smoke_air_primary_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "legacy-1.38.8.1-smoke-ground-and-air-primary-response-128hz".into(),
            airborne_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_horizontal_inheritance: Some(1.25),
            airborne_overhand_vertical_inheritance: Some(1.25),
            ..observed_smoke_launch()
        }
    }

    // Independently observed secondary and combined airborne responses; these
    // do not borrow their component admission from the primary observations.
    fn observed_smoke_air_buttons_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "legacy-1.38.8.1-smoke-ground-and-air-button-response-128hz".into(),
            airborne_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            airborne_underhand_horizontal_inheritance: Some(1.25),
            airborne_underhand_vertical_inheritance: Some(1.25),
            airborne_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_combined_horizontal_inheritance: Some(1.25),
            airborne_combined_vertical_inheritance: Some(1.25),
            ..observed_smoke_air_primary_launch()
        }
    }

    fn launch_utility_tick_input(name: &str, rate: u32, mode: ThrowMode,
        profile: Option<weapons::GrenadeLaunch>, running: bool, jump: bool, pitch: f32) -> (Projectile, Vec3) {
        use std::sync::Arc;
        use crate::{TickInput, weapon::WeaponState};
        use movement::buttons;
        let mut collision = collision::CollisionWorld::new();
        collision.add_convex(collision::Convex { plane_surfaces: Vec::new(),
            mins: Vec3::new(-8192., -8192., -128.), maxs: Vec3::new(8192., 8192., 0.),
            planes: [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
                .into_iter().zip([8192., 8192., 8192., 8192., 0., 128.])
                .map(|(normal, dist)| collision::Plane { normal, dist }).collect(),
            contents: contents::SOLID, surface: 0,
        });
        collision.finalize();
        let mut catalog = weapons::Catalog::from_items_game(&format!(r#"items_game {{ items {{
            7343 {{ name {name} attributes {{ "primary default clip size" 1 }} }}
        }} }}"#)).unwrap();
        let def = &mut catalog.weapons[0];
        def.max_player_speed = 245.;
        def.grenade_animations = Some(weapons::GrenadeAnimations {
            pull_pin_duration: 0.25,
            overhand: weapons::GrenadeReleaseAnimation { command_release_time: Some(0.1), duration: 0.5, release_time: 0.125 },
            underhand: weapons::GrenadeReleaseAnimation { command_release_time: Some(0.1), duration: 0.5, release_time: 0.125 },
        });
        def.grenade_launch = profile;
        let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
        world.set_tick_rate(rate);
        let id = world.add_player("thrower", Team::Terrorist);
        let weapon = WeaponState::new(Arc::new(world.catalog.weapons[0].clone()));
        let player = world.player_mut(id).unwrap();
        player.alive = true;
        player.weapons = vec![weapon]; player.active = 0;
        player.movement.on_ground = true;
        let forward_move = if running { 450. } else { 0. };
        let hold = match mode {
            ThrowMode::Overhand => buttons::ATTACK,
            ThrowMode::Underhand => buttons::ATTACK2,
            ThrowMode::Combined => buttons::ATTACK | buttons::ATTACK2,
        };
        for _ in 0..rate {
            world.step(&[(id, TickInput { view_angles: Vec3::new(pitch, 0., 0.), forward_move, buttons: hold, ..Default::default() })]);
        }
        assert!(world.projectiles.is_empty());
        let state = world.player(id).unwrap().active_weapon().unwrap().grenade_throw.unwrap();
        assert_eq!(state.phase, ThrowPhase::Holding);
        assert_eq!(state.mode, mode);
        for _ in 0..rate / 2 {
            world.step(&[(id, TickInput { view_angles: Vec3::new(pitch, 0., 0.), forward_move,
                buttons: if jump { buttons::JUMP } else { 0 }, ..Default::default() })]);
            if let Some(projectile) = world.projectiles.first() {
                let player = world.player(id).unwrap();
                assert_eq!(player.movement.on_ground, !jump);
                assert_eq!(world.projectiles.len(), 1);
                assert_eq!(projectile.owner, id);
                assert!(world.events.iter().any(|e| matches!(e,
                    GameEvent::UtilityThrown { kind, .. } if *kind == projectile.kind)));
                return (projectile.clone(), player.movement.velocity);
            }
        }
        panic!("ordinary hold/release must create one {name}");
    }

    fn observed_ground_primary_launch(name: &str) -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: format!("observed-{name}-ground-primary-angular-horizontal-128hz"),
            grounded_overhand_horizontal_inheritance: 1.25,
            grounded_underhand_horizontal_inheritance: None,
            grounded_combined_horizontal_inheritance: None,
            grounded_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            grounded_underhand_velocity: None,
            grounded_combined_velocity: None,
            airborne_overhand_velocity: None,
            airborne_overhand_horizontal_inheritance: None,
            airborne_overhand_vertical_inheritance: None,
            airborne_underhand_velocity: None,
            airborne_underhand_horizontal_inheritance: None,
            airborne_underhand_vertical_inheritance: None,
            airborne_combined_velocity: None,
            airborne_combined_horizontal_inheritance: None,
            airborne_combined_vertical_inheritance: None,
        }
    }

    fn observed_ground_buttons_launch(name: &str) -> weapons::GrenadeLaunch {
        let mut launch = observed_ground_primary_launch(name);
        launch.grounded_underhand_velocity = Some(weapons::GrenadeLaunchVelocity {
            speed: 202.5, upward_pitch_bias_degrees: 10.,
        });
        launch.grounded_combined_velocity = Some(weapons::GrenadeLaunchVelocity {
            speed: 438.75, upward_pitch_bias_degrees: 10.,
        });
        launch.grounded_underhand_horizontal_inheritance = Some(1.25);
        launch.grounded_combined_horizontal_inheritance = Some(1.25);
        launch
    }

    #[test]
    fn flash_ground_buttons_follow_independent_native_vectors_through_tick_input() {
        for rate in [64, 128] {
            for (mode, stationary, moving) in [
                (ThrowMode::Underhand, [199.423584, 162.429962, 155.124008], 505.673584),
                (ThrowMode::Combined, [432.084351, 351.931519, 336.101959], 738.334351),
            ] {
                for (pitch, running, native_x) in [(0., false, stationary[0]),
                    (-30., false, stationary[1]), (45., false, stationary[2]), (0., true, moving)] {
                    let (actual, player_velocity) = launch_utility_tick_input("weapon_flashbang", rate, mode,
                        Some(observed_ground_buttons_launch("weapon_flashbang")), running, false, pitch);
                    let (prior, prior_player_velocity) = launch_utility_tick_input("weapon_flashbang", rate, mode,
                        Some(observed_ground_primary_launch("weapon_flashbang")), running, false, pitch);
                    assert_eq!(actual.kind, Kind::Flash);
                    assert_eq!(actual.born, prior.born);
                    assert_eq!(player_velocity, prior_player_velocity);
                    assert!((player_velocity.x - if running {245.} else {0.}).abs() < 0.001);
                    assert!((actual.velocity.x - native_x).abs() < 0.002,
                        "{rate}Hz {mode:?} pitch={pitch} running={running}");
                    assert!((actual.pos - prior.pos - (actual.velocity - prior.velocity) / rate as f32).length() < 0.002,
                        "release time and raw-aim spawn trace stay fixed");
                }
            }
        }
    }

    #[test]
    fn remaining_ground_buttons_follow_own_native_vectors_through_tick_input() {
        // Each kind has its own eight native cases, with identical VX endpoints.
        for (name, kind) in [("weapon_decoy", Kind::Decoy),
            ("weapon_molotov", Kind::Molotov), ("weapon_incgrenade", Kind::Incendiary)] {
            for rate in [64, 128] {
                for (mode, stationary, moving) in [
                    (ThrowMode::Underhand, [199.423584, 162.429962, 155.124008], 505.673584),
                    (ThrowMode::Combined, [432.084351, 351.931519, 336.101959], 738.334351),
                ] {
                    for (pitch, running, native_x) in [(0., false, stationary[0]),
                        (-30., false, stationary[1]), (45., false, stationary[2]), (0., true, moving)] {
                        let (actual, player_velocity) = launch_utility_tick_input(name, rate, mode,
                            Some(observed_ground_buttons_launch(name)), running, false, pitch);
                        let (prior, prior_player_velocity) = launch_utility_tick_input(name, rate, mode,
                            Some(observed_ground_primary_launch(name)), running, false, pitch);
                        assert_eq!(actual.kind, kind);
                        assert_eq!(actual.born, prior.born);
                        assert_eq!(player_velocity, prior_player_velocity);
                        assert!((player_velocity.x - if running {245.} else {0.}).abs() < 0.001);
                        assert!((actual.velocity.x - native_x).abs() < 0.002,
                            "{name} {rate}Hz {mode:?} pitch={pitch} running={running}");
                        assert!((actual.pos - prior.pos - (actual.velocity - prior.velocity) / rate as f32).length() < 0.002,
                            "release time and raw-aim spawn trace stay fixed");
                    }
                }
            }
        }
    }

    #[test]
    fn flash_ground_buttons_preserve_primary_air_missing_modes_and_independent_fallbacks() {
        let candidate = observed_ground_buttons_launch("weapon_flashbang");
        for rate in [64, 128] {
            for mode in [ThrowMode::Overhand, ThrowMode::Underhand, ThrowMode::Combined] {
                for jump in [false, true] {
                    if !jump && mode != ThrowMode::Overhand {continue;}
                    for running in [false, true] {
                        assert_eq!(launch_utility_tick_input("weapon_flashbang", rate, mode,
                            Some(candidate.clone()), running, jump, -30.),
                            launch_utility_tick_input("weapon_flashbang", rate, mode,
                                Some(observed_ground_primary_launch("weapon_flashbang")), running, jump, -30.));
                    }
                }
            }
            for mode in [ThrowMode::Underhand, ThrowMode::Combined] {
                let mut absent = candidate.clone();
                match mode {
                    ThrowMode::Underhand => {
                        absent.grounded_underhand_velocity = None;
                        absent.grounded_underhand_horizontal_inheritance = None;
                    }
                    _ => {
                        absent.grounded_combined_velocity = None;
                        absent.grounded_combined_horizontal_inheritance = None;
                    }
                }
                assert_eq!(launch_utility_tick_input("weapon_flashbang", rate, mode,
                    Some(absent), true, false, -30.),
                    launch_utility_tick_input("weapon_flashbang", rate, mode,
                        Some(observed_ground_primary_launch("weapon_flashbang")), true, false, -30.));
            }
        }
        let mut world = arena();
        let id = world.add_player("missing mode", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            43 { name weapon_flashbang attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(candidate);
        let mut prior = definition.clone();
        prior.grenade_launch = Some(observed_ground_primary_launch("weapon_flashbang"));
        assert_eq!(world.grenade_release(world.player(id).unwrap(), &definition, None, Vec3::ZERO),
            world.grenade_release(world.player(id).unwrap(), &prior, None, Vec3::ZERO));
    }

    #[test]
    fn four_kinds_ground_primary_angles_and_independent_inheritance_follow_tick_input() {
        for (name, kind, horizontal) in [
            ("weapon_flashbang", Kind::Flash, 1.25), ("weapon_decoy", Kind::Decoy, 1.25),
            ("weapon_molotov", Kind::Molotov, 1.25), ("weapon_incgrenade", Kind::Incendiary, 1.25),
        ] {
            for rate in [64, 128] {
                let dt = 1. / rate as f32;
                // Each kind independently measured these horizontal values.
                // Vz here is the candidate family's construction value, never
                // a claim that a received native Vz is an initial velocity.
                for (pitch, native_x, candidate_z) in [
                    (0., 664.745239, 117.212517),
                    (-30., 541.433167, 403.082062),
                    (45., 517.080017, -433.881622),
                ] {
                    let mut stationary: Option<Vec3> = None;
                    for running in [false, true] {
                        let (actual, player_velocity) = launch_utility_tick_input(name, rate, ThrowMode::Overhand,
                            Some(observed_ground_primary_launch(name)), running, false, pitch);
                        let (prior, prior_player_velocity) = launch_utility_tick_input(name, rate, ThrowMode::Overhand,
                            None, running, false, pitch);
                        assert_eq!(player_velocity, prior_player_velocity);
                        if running { assert!((player_velocity.x - 245.).abs() < 0.001); }
                        else { assert_eq!(player_velocity, Vec3::ZERO); }
                        let inherited = Vec3::new(player_velocity.x * horizontal,
                            player_velocity.y * horizontal, player_velocity.z * 0.5);
                        let expected = Vec3::new(native_x, 0., candidate_z) + inherited;
                        // Production spawning advances one ordinary free-flight
                        // tick before inspection. Undo only that known sim tick.
                        assert!((actual.velocity + Vec3::Z * 320. * dt - expected).length() < 0.002,
                            "{name}, {rate}Hz, pitch={pitch}, running={running}");
                        if pitch == 0. && running {
                            assert!((actual.velocity.x - 970.995239).abs() < 0.002,
                                "each kind's independent steady-forward native level target");
                        }
                        assert_eq!(actual.kind, kind);
                        assert_eq!(actual.born, prior.born);
                        assert!((actual.pos - prior.pos - (actual.velocity - prior.velocity) * dt).length() < 0.002,
                            "only velocity changes; the raw-aim spawn trace stays fixed");
                        if let Some(velocity) = stationary {
                            assert!((actual.velocity - velocity - inherited).length() < 0.002,
                                "movement inheritance retains each kind's existing coefficient");
                        } else { stationary = Some(actual.velocity); }

                        let mut absent_velocity = observed_ground_primary_launch(name);
                        absent_velocity.grounded_overhand_velocity = None;
                        let (absent, absent_player_velocity) = launch_utility_tick_input(name, rate, ThrowMode::Overhand,
                            Some(absent_velocity), running, false, pitch);
                        assert_eq!(absent_player_velocity, prior_player_velocity);
                        let raw = movement::angle_vectors(Vec3::new(pitch, 0., 0.)).0 * 750.;
                        assert!((prior.velocity + Vec3::Z * 320. * dt - raw - player_velocity * 0.5).length() < 0.002,
                            "missing metadata retains the complete raw-aim fallback");
                        assert!((absent.velocity + Vec3::Z * 320. * dt - raw - inherited).length() < 0.002,
                            "an absent angular component preserves independent inheritance");
                        assert_eq!(absent.born, prior.born);
                        assert_eq!(absent.kind, prior.kind);
                        assert!((absent.pos - prior.pos - (absent.velocity - prior.velocity) * dt).length() < 0.002);
                    }
                }
            }
        }
    }

    #[test]
    fn four_kinds_ground_primary_profiles_preserve_other_modes_air_and_missing_mode() {
        for name in ["weapon_flashbang", "weapon_decoy", "weapon_molotov", "weapon_incgrenade"] {
            for rate in [64, 128] {
                for running in [false, true] {
                    for pitch in [0., -30., 45.] {
                        for mode in [ThrowMode::Overhand, ThrowMode::Underhand, ThrowMode::Combined] {
                            for jump in [false, true] {
                                if !jump && mode == ThrowMode::Overhand { continue; }
                                assert_eq!(launch_utility_tick_input(name, rate, mode,
                                    Some(observed_ground_primary_launch(name)), running, jump, pitch),
                                    launch_utility_tick_input(name, rate, mode, None, running, jump, pitch),
                                    "{name}, {rate}Hz, pitch={pitch}, running={running}, mode={mode:?}, jump={jump}");
                            }
                        }
                    }
                }
            }
            // Missing mode is defensive release behavior, unreachable from the
            // ordinary hold/release producer; exercise the same production seam.
            let mut world = arena();
            let id = world.add_player("thrower", Team::Terrorist);
            let catalog = weapons::Catalog::from_items_game(&format!(r#"items_game {{ items {{
                7343 {{ name {name} attributes {{ "primary default clip size" 1 }} }}
            }} }}"#)).unwrap();
            let mut definition = catalog.weapons[0].clone();
            definition.grenade_launch = Some(observed_ground_primary_launch(name));
            let mut fallback = definition.clone();
            fallback.grenade_launch = None;
            for grounded in [false, true] {
                let player = world.player_mut(id).unwrap();
                player.movement.on_ground = grounded;
                player.movement.velocity = Vec3::new(245., 80., 100.);
                for pitch in [0., -30., 45.] {
                    let angles = Vec3::new(pitch, 0., 0.);
                    assert_eq!(world.grenade_release(world.player(id).unwrap(), &definition, None, angles),
                        world.grenade_release(world.player(id).unwrap(), &fallback, None, angles));
                }
            }
        }
    }

    #[test]
    fn molotov_and_smoke_tick_input_preserve_inheritance_with_admitted_smoke_button_vectors() {
        fn launch(name: &str, rate: u32, mode: ThrowMode, measured: bool, running: bool, jump: bool, pitch: f32) -> (Projectile, Vec3) {
            let profile = measured.then(|| {
                if name == "weapon_smokegrenade" { observed_smoke_launch() }
                else { observed_ground_primary_launch(name) }
            });
            launch_utility_tick_input(name, rate, mode, profile, running, jump, pitch)
        }
        for name in ["weapon_molotov", "weapon_smokegrenade"] {
            for rate in [64, 128] {
                let (stationary, _) = launch(name, rate, ThrowMode::Overhand, true, false, false, 0.);
                let (moving, player_velocity) = launch(name, rate, ThrowMode::Overhand, true, true, false, 0.);
                assert!((player_velocity.x - 245.).abs() < 0.001);
                // Independently observed for each grenade:970.995239 -664.745239 =306.25.
                assert!((moving.velocity.x - stationary.velocity.x - 306.25).abs() < 0.002);
                assert!((stationary.velocity.x - 664.745239).abs() < 0.002);
                assert!((moving.velocity.x - 970.995239).abs() < 0.002);
                assert_eq!(moving.velocity.z, stationary.velocity.z);
                for mode in [ThrowMode::Overhand, ThrowMode::Underhand, ThrowMode::Combined] {
                    assert_eq!(launch(name, rate, mode, true, true, true, 0.), launch(name, rate, mode, false, true, true, 0.),
                        "air retains the complete uncalibrated release for every mode");
                }
                for (mode, native_x) in [
                    (ThrowMode::Underhand, [199.423584, 162.429962, 155.124008]),
                    (ThrowMode::Combined, [432.084351, 351.931519, 336.101959]),
                ] {
                    if name == "weapon_smokegrenade" {
                        for (pitch, native_x) in [0., -30., 45.].into_iter().zip(native_x) {
                            let (stationary, player_velocity) = launch(name, rate, mode, true, false, false, pitch);
                            assert_eq!(player_velocity, Vec3::ZERO);
                            assert_eq!(stationary.kind, Kind::Smoke);
                            assert!((stationary.velocity.x - native_x).abs() < 0.002);
                            assert!(stationary.velocity.y.abs() < 0.002);
                            let (moving, player_velocity) = launch(name, rate, mode, true, true, false, pitch);
                            assert!((player_velocity.x - 245.).abs() < 0.001);
                            assert!((moving.velocity.x - stationary.velocity.x - 306.25).abs() < 0.002,
                                "independent stationary/steady-forward Smoke pairs establish both button responses");
                            assert_eq!(moving.velocity.z, stationary.velocity.z);
                            if pitch == 0. {
                                let native_moving_x = if mode == ThrowMode::Underhand { 505.673584 } else { 738.334351 };
                                assert!((moving.velocity.x - native_moving_x).abs() < 0.002);
                            }
                        }
                    } else {
                        assert_eq!(launch(name, rate, mode, true, true, false, 0.), launch(name, rate, mode, false, true, false, 0.),
                            "Molotov secondary/combined retain their complete prior release");
                    }
                }
            }
        }
    }

    // Independently qualified HE primary-air response; alternate air modes stay absent.
    fn observed_he_air_primary_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-he-air-primary-response-128hz".into(),
            airborne_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_horizontal_inheritance: Some(1.25),
            airborne_overhand_vertical_inheritance: Some(1.25),
            ..observed_ground_buttons_launch("weapon_hegrenade")
        }
    }

    // Each HE alternate mode has independently measured air angles and movement controls.
    fn observed_he_air_buttons_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-he-ground-and-air-button-response-128hz".into(),
            airborne_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            airborne_underhand_horizontal_inheritance: Some(1.25),
            airborne_underhand_vertical_inheritance: Some(1.25),
            airborne_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_combined_horizontal_inheritance: Some(1.25),
            airborne_combined_vertical_inheritance: Some(1.25),
            ..observed_he_air_primary_launch()
        }
    }

    // Independently qualified Flash primary-air response; alternate air modes stay absent.
    fn observed_flash_air_primary_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-flash-air-primary-response-128hz".into(),
            airborne_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_horizontal_inheritance: Some(1.25),
            airborne_overhand_vertical_inheritance: Some(1.25),
            ..observed_ground_buttons_launch("weapon_flashbang")
        }
    }

    // Flash's own secondary/combined angular, steady-forward and constant-VZ controls.
    fn observed_flash_air_buttons_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-flash-air-button-response-128hz".into(),
            airborne_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            airborne_underhand_horizontal_inheritance: Some(1.25),
            airborne_underhand_vertical_inheritance: Some(1.25),
            airborne_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_combined_horizontal_inheritance: Some(1.25),
            airborne_combined_vertical_inheritance: Some(1.25),
            ..observed_flash_air_primary_launch()
        }
    }

    // Independently qualified Decoy primary-air response; alternate air modes stay absent.
    fn observed_decoy_air_primary_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-decoy-air-primary-response-128hz".into(),
            airborne_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_horizontal_inheritance: Some(1.25),
            airborne_overhand_vertical_inheritance: Some(1.25),
            ..observed_ground_buttons_launch("weapon_decoy")
        }
    }

    // Decoy's own secondary/combined angular, steady-forward and constant-VZ controls.
    fn observed_decoy_air_buttons_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-decoy-air-button-response-128hz".into(),
            airborne_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            airborne_underhand_horizontal_inheritance: Some(1.25),
            airborne_underhand_vertical_inheritance: Some(1.25),
            airborne_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_combined_horizontal_inheritance: Some(1.25),
            airborne_combined_vertical_inheritance: Some(1.25),
            ..observed_decoy_air_primary_launch()
        }
    }

    // Independently qualified Molotov primary-air response; alternate air modes stay absent.
    fn observed_molotov_air_primary_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-molotov-air-primary-response-128hz".into(),
            airborne_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_horizontal_inheritance: Some(1.25),
            airborne_overhand_vertical_inheritance: Some(1.25),
            ..observed_ground_buttons_launch("weapon_molotov")
        }
    }

    // Molotov's own secondary/combined angular, steady-forward and constant-VZ controls.
    fn observed_molotov_air_buttons_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-molotov-air-button-response-128hz".into(),
            airborne_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            airborne_underhand_horizontal_inheritance: Some(1.25),
            airborne_underhand_vertical_inheritance: Some(1.25),
            airborne_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_combined_horizontal_inheritance: Some(1.25),
            airborne_combined_vertical_inheritance: Some(1.25),
            ..observed_molotov_air_primary_launch()
        }
    }

    // Independently qualified Incendiary primary-air response; alternate air modes stay absent.
    fn observed_incendiary_air_primary_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-incendiary-air-primary-response-128hz".into(),
            airborne_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_horizontal_inheritance: Some(1.25),
            airborne_overhand_vertical_inheritance: Some(1.25),
            ..observed_ground_buttons_launch("weapon_incgrenade")
        }
    }

    // Incendiary's own secondary/combined angular, steady-forward and constant-VZ controls.
    fn observed_incendiary_air_buttons_launch() -> weapons::GrenadeLaunch {
        weapons::GrenadeLaunch {
            reference: "observed-incendiary-air-button-response-128hz".into(),
            airborne_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            airborne_underhand_horizontal_inheritance: Some(1.25),
            airborne_underhand_vertical_inheritance: Some(1.25),
            airborne_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_combined_horizontal_inheritance: Some(1.25),
            airborne_combined_vertical_inheritance: Some(1.25),
            ..observed_incendiary_air_primary_launch()
        }
    }

    #[test]
    fn observed_air_primary_components_follow_ordinary_jump_release_independently() {
        for (name, kind, prior_profile, candidate) in [
            ("weapon_smokegrenade", Kind::Smoke, observed_smoke_launch(), observed_smoke_air_primary_launch()),
            ("weapon_hegrenade", Kind::He, observed_ground_buttons_launch("weapon_hegrenade"), observed_he_air_primary_launch()),
            ("weapon_flashbang", Kind::Flash, observed_ground_buttons_launch("weapon_flashbang"), observed_flash_air_primary_launch()),
            ("weapon_decoy", Kind::Decoy, observed_ground_buttons_launch("weapon_decoy"), observed_decoy_air_primary_launch()),
            ("weapon_molotov", Kind::Molotov, observed_ground_buttons_launch("weapon_molotov"), observed_molotov_air_primary_launch()),
            ("weapon_incgrenade", Kind::Incendiary, observed_ground_buttons_launch("weapon_incgrenade"), observed_incendiary_air_primary_launch()),
        ] {
            for rate in [64, 128] {
                let dt = 1. / rate as f32;
                for running in [false, true] {
                    // Native airborne VX at each angle; base VZ is calculated from
                    // the qualified angular family, not a normal-gravity age fit.
                    for (pitch, observed_x, base_z) in [
                        (0., 664.745239, 117.212517),
                        (-30., 541.433167, 403.082062),
                        (45., 517.080017, -433.881622),
                    ] {
                        let (prior, player_velocity) = launch_utility_tick_input(name, rate,
                            ThrowMode::Overhand, Some(prior_profile.clone()), running, true, pitch);
                        assert!(player_velocity.z > 0., "ordinary JUMP must still be ascending at release");
                        if running { assert!((player_velocity.x - 245.).abs() < 0.001); }
                        else { assert_eq!(player_velocity.truncate(), glam::Vec2::ZERO); }
                        for components in 0..8 {
                            let mut profile = candidate.clone();
                            if components & 1 == 0 { profile.airborne_overhand_velocity = None; }
                            if components & 2 == 0 { profile.airborne_overhand_horizontal_inheritance = None; }
                            if components & 4 == 0 { profile.airborne_overhand_vertical_inheritance = None; }
                            let (actual, actual_player_velocity) = launch_utility_tick_input(name, rate,
                                ThrowMode::Overhand, Some(profile), running, true, pitch);
                            assert_eq!(actual_player_velocity, player_velocity);
                            let mut expected = if components & 1 != 0 {
                                Vec3::new(observed_x, 0., base_z)
                            } else { movement::angle_vectors(Vec3::new(pitch, 0., 0.)).0 * 750. };
                            expected += Vec3::new(
                                player_velocity.x * if components & 2 != 0 { 1.25 } else { 0.5 },
                                player_velocity.y * if components & 2 != 0 { 1.25 } else { 0.5 },
                                player_velocity.z * if components & 4 != 0 { 1.25 } else { 0.5 });
                            // First emitted projectile has already advanced through
                            // one ordinary free-flight tick; do not fit spawn age.
                            assert!((actual.velocity + Vec3::Z * 320. * dt - expected).length() < 0.002,
                                "{name} {rate}Hz, running={running}, pitch={pitch}, components={components}");
                            assert!((actual.pos - prior.pos - (actual.velocity - prior.velocity) * dt).length() < 0.002,
                                "air velocity changes preserve the existing spawn trace");
                            assert_eq!(actual.born, prior.born);
                            assert_eq!(actual.kind, kind);
                            if components == 0 { assert_eq!(actual, prior); }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn observed_air_buttons_follow_ordinary_jump_release_with_independent_fallbacks() {
        // Each mode's independent native airborne VX; base VZ is calculated
        // from its qualified angular family, not a normal-gravity age fit.
        for (name, kind, primary, buttons) in [
            ("weapon_smokegrenade", Kind::Smoke,
                observed_smoke_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_smoke_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_hegrenade", Kind::He,
                observed_he_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_he_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_flashbang", Kind::Flash,
                observed_flash_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_flash_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_decoy", Kind::Decoy,
                observed_decoy_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_decoy_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_molotov", Kind::Molotov,
                observed_molotov_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_molotov_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_incgrenade", Kind::Incendiary,
                observed_incendiary_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_incendiary_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
        ] {
            for (mode, raw_speed, observations) in [
                (ThrowMode::Underhand, 225., [
                    (0., 199.423584, 35.16376),
                    (-30., 162.429962, 120.92462),
                    (45., 155.124008, -130.16449),
                ]),
                (ThrowMode::Combined, 487.5, [
                    (0., 432.084351, 76.18813),
                    (-30., 351.931519, 262.00333),
                    (45., 336.101959, -282.02304),
                ]),
            ] {
                for rate in [64, 128] {
                    let dt = 1. / rate as f32;
                    for running in [false, true] {
                        for (pitch, observed_x, base_z) in observations {
                            let (prior, player_velocity) = launch_utility_tick_input(name, rate,
                                mode, Some(primary()), running, true, pitch);
                            assert!(player_velocity.z > 0., "ordinary standing/running JUMP must be ascending");
                            if running { assert!((player_velocity.x - 245.).abs() < 0.001); }
                            else { assert_eq!(player_velocity.truncate(), glam::Vec2::ZERO); }
                            for components in 0..8 {
                                let mut profile = buttons();
                                let (velocity, horizontal, vertical) = match mode {
                                    ThrowMode::Underhand => (&mut profile.airborne_underhand_velocity,
                                        &mut profile.airborne_underhand_horizontal_inheritance,
                                        &mut profile.airborne_underhand_vertical_inheritance),
                                    ThrowMode::Combined => (&mut profile.airborne_combined_velocity,
                                        &mut profile.airborne_combined_horizontal_inheritance,
                                        &mut profile.airborne_combined_vertical_inheritance),
                                    ThrowMode::Overhand => unreachable!(),
                                };
                                if components & 1 == 0 { *velocity = None; }
                                if components & 2 == 0 { *horizontal = None; }
                                if components & 4 == 0 { *vertical = None; }
                                // Primary and the other alternate mode stay populated:
                                // absent components must never borrow their values.
                                let (actual, actual_player_velocity) = launch_utility_tick_input(name, rate,
                                    mode, Some(profile), running, true, pitch);
                                assert_eq!(actual_player_velocity, player_velocity);
                                let mut expected = if components & 1 != 0 {
                                    Vec3::new(observed_x, 0., base_z)
                                } else { movement::angle_vectors(Vec3::new(pitch, 0., 0.)).0 * raw_speed };
                                expected += Vec3::new(
                                    player_velocity.x * if components & 2 != 0 { 1.25 } else { 0.5 },
                                    player_velocity.y * if components & 2 != 0 { 1.25 } else { 0.5 },
                                    player_velocity.z * if components & 4 != 0 { 1.25 } else { 0.5 });
                                // Undo the first ordinary free-flight gravity tick;
                                // this does not assert an observed native spawn age.
                                assert!((actual.velocity + Vec3::Z * 320. * dt - expected).length() < 0.002,
                                    "{name}, {mode:?}, {rate}Hz, running={running}, pitch={pitch}, components={components}");
                                assert!((actual.pos - prior.pos - (actual.velocity - prior.velocity) * dt).length() < 0.002);
                                assert_eq!(actual.born, prior.born);
                                assert_eq!(actual.kind, kind);
                                if components == 0 { assert_eq!(actual, prior); }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn observed_air_buttons_preserve_ground_modes_and_admitted_primary_air() {
        for (name, primary, buttons) in [
            ("weapon_smokegrenade", observed_smoke_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_smoke_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_hegrenade", observed_he_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_he_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_flashbang", observed_flash_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_flash_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_decoy", observed_decoy_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_decoy_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_molotov", observed_molotov_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_molotov_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
            ("weapon_incgrenade", observed_incendiary_air_primary_launch as fn() -> weapons::GrenadeLaunch,
                observed_incendiary_air_buttons_launch as fn() -> weapons::GrenadeLaunch),
        ] {
            for rate in [64, 128] {
                for running in [false, true] {
                    for pitch in [0., -30., 45.] {
                        for mode in [ThrowMode::Overhand, ThrowMode::Underhand, ThrowMode::Combined] {
                            for jump in [false, true] {
                                if jump && mode != ThrowMode::Overhand { continue; }
                                assert_eq!(
                                    launch_utility_tick_input(name, rate, mode,
                                        Some(buttons()), running, jump, pitch),
                                    launch_utility_tick_input(name, rate, mode,
                                        Some(primary()), running, jump, pitch),
                                    "{name}, {rate}Hz, running={running}, pitch={pitch}, mode={mode:?}, jump={jump}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn absent_air_components_preserve_other_kinds_through_ordinary_jump_release() {
        for name in ["weapon_hegrenade", "weapon_flashbang", "weapon_molotov", "weapon_incgrenade", "weapon_decoy"] {
            for rate in [64, 128] {
                for running in [false, true] {
                    for pitch in [0., -30., 45.] {
                        for mode in [ThrowMode::Overhand, ThrowMode::Underhand, ThrowMode::Combined] {
                            // Grounded metadata must not supply absent air data.
                            assert_eq!(
                                launch_utility_tick_input(name, rate, mode, Some(observed_smoke_launch()), running, true, pitch),
                                launch_utility_tick_input(name, rate, mode, None, running, true, pitch),
                                "{name}, {rate}Hz, running={running}, pitch={pitch}, mode={mode:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn smoke_air_button_modes_match_independent_native_constant_vz_and_moving_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            45 { name weapon_smokegrenade attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_smoke_air_buttons_launch());
        for (mode, observed_x, moving_x, controls) in [
            (ThrowMode::Underhand, 199.423584, 505.673584, [
                (true, 0., 35.163757), (false, 223.868378, 314.999237), (false, 130.118378, 197.811737),
            ]),
            (ThrowMode::Combined, 432.084351, 738.334351, [
                (true, 0., 76.188133), (false, 217.618378, 348.211121), (false, 130.118378, 238.836121),
            ]),
        ] {
            // Each native zero-G session reads back gravity=0 and equal player
            // VZ before/after. No physical projectile age is subtracted.
            for (grounded, player_vz, observed_z) in controls {
                let actor = world.player_mut(id).unwrap();
                actor.movement.on_ground = grounded;
                actor.movement.velocity = Vec3::Z * player_vz;
                let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                    &definition, Some(mode), Vec3::ZERO);
                assert!((actual - Vec3::new(observed_x, 0., observed_z)).length() < 0.002);
            }
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = false;
            actor.movement.velocity = Vec3::new(245., 0., 211.368378);
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(mode), Vec3::ZERO);
            // Normal-gravity steady-forward VX only; its initial VZ is unknown.
            assert!((actual.x - moving_x).abs() < 0.002);
        }
    }

    #[test]
    fn he_air_button_modes_match_own_native_constant_vz_and_moving_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            44 { name weapon_hegrenade attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_he_air_buttons_launch());
        for (mode, observed_x, moving_x, controls) in [
            (ThrowMode::Underhand, 199.423584, 505.673584, [
                (true, 0., 35.163757), (false, 223.868378, 314.999237), (false, 123.868378, 189.999237),
            ]),
            (ThrowMode::Combined, 432.084351, 738.334351, [
                (true, 0., 76.188133), (false, 217.618378, 348.211121), (false, 123.868378, 231.023621),
            ]),
        ] {
            // Each native zero-G session reads back gravity=0 and equal player
            // VZ before/after. No physical projectile age is subtracted.
            for (grounded, player_vz, observed_z) in controls {
                let actor = world.player_mut(id).unwrap();
                actor.movement.on_ground = grounded;
                actor.movement.velocity = Vec3::Z * player_vz;
                let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                    &definition, Some(mode), Vec3::ZERO);
                assert!((actual - Vec3::new(observed_x, 0., observed_z)).length() < 0.002);
            }
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = false;
            actor.movement.velocity = Vec3::new(245., 0., 211.368378);
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(mode), Vec3::ZERO);
            // Normal-gravity steady-forward VX only; its initial VZ is unknown.
            assert!((actual.x - moving_x).abs() < 0.002);
        }
    }

    #[test]
    fn flash_air_button_modes_match_own_native_constant_vz_and_moving_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            43 { name weapon_flashbang attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_flash_air_buttons_launch());
        for (mode, observed_x, moving_x, controls) in [
            (ThrowMode::Underhand, 199.423584, 505.673584, [
                (true, 0., 35.163757), (false, 223.868378, 314.999237), (false, 123.868378, 189.999237),
            ]),
            (ThrowMode::Combined, 432.084351, 738.334351, [
                (true, 0., 76.188133), (false, 223.868378, 356.023621), (false, 123.868378, 231.023621),
            ]),
        ] {
            // Own zero-G player VZ is constant before/after release, and each
            // projectile VZ is constant across three precontact samples.
            for (grounded, player_vz, observed_z) in controls {
                let actor = world.player_mut(id).unwrap();
                actor.movement.on_ground = grounded;
                actor.movement.velocity = Vec3::Z * player_vz;
                let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                    &definition, Some(mode), Vec3::ZERO);
                assert!((actual - Vec3::new(observed_x, 0., observed_z)).length() < 0.002);
            }
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = false;
            actor.movement.velocity = Vec3::new(245., 0., 211.368378);
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(mode), Vec3::ZERO);
            // Normal-gravity forward samples qualify horizontal velocity only.
            assert!((actual.x - moving_x).abs() < 0.002);
        }
    }

    #[test]
    fn decoy_air_button_modes_match_own_native_constant_vz_and_moving_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            47 { name weapon_decoy attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_decoy_air_buttons_launch());
        for (mode, observed_x, moving_x, controls) in [
            (ThrowMode::Underhand, 199.423584, 505.673584, [
                (true, 0., 35.163757), (false, 223.868378, 314.999237), (false, 123.868378, 189.999237),
            ]),
            (ThrowMode::Combined, 432.084351, 738.334351, [
                (true, 0., 76.188133), (false, 223.868378, 356.023621), (false, 123.868378, 231.023621),
            ]),
        ] {
            // Own zero-G player VZ is constant before/after release, and each
            // projectile VZ is constant across three precontact samples.
            for (grounded, player_vz, observed_z) in controls {
                let actor = world.player_mut(id).unwrap();
                actor.movement.on_ground = grounded;
                actor.movement.velocity = Vec3::Z * player_vz;
                let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                    &definition, Some(mode), Vec3::ZERO);
                assert!((actual - Vec3::new(observed_x, 0., observed_z)).length() < 0.002);
            }
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = false;
            actor.movement.velocity = Vec3::new(245., 0., 211.368378);
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(mode), Vec3::ZERO);
            // Normal-gravity forward samples qualify horizontal velocity only.
            assert!((actual.x - moving_x).abs() < 0.002);
        }
    }

    #[test]
    fn molotov_air_button_modes_match_own_native_constant_vz_and_moving_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            46 { name weapon_molotov attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_molotov_air_buttons_launch());
        for (mode, observed_x, moving_x, controls) in [
            (ThrowMode::Underhand, 199.423584, 505.673584, [
                (true, 0., 35.163757), (false, 223.868378, 314.999237), (false, 130.118378, 197.811737),
            ]),
            (ThrowMode::Combined, 432.084351, 738.334351, [
                (true, 0., 76.188133), (false, 223.868378, 356.023621), (false, 123.868378, 231.023621),
            ]),
        ] {
            // Own zero-G player VZ is constant before/after release, and each
            // projectile VZ is constant across three precontact samples.
            for (grounded, player_vz, observed_z) in controls {
                let actor = world.player_mut(id).unwrap();
                actor.movement.on_ground = grounded;
                actor.movement.velocity = Vec3::Z * player_vz;
                let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                    &definition, Some(mode), Vec3::ZERO);
                assert!((actual - Vec3::new(observed_x, 0., observed_z)).length() < 0.002);
            }
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = false;
            actor.movement.velocity = Vec3::new(245., 0., 211.368378);
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(mode), Vec3::ZERO);
            // Normal-gravity forward samples qualify horizontal velocity only.
            assert!((actual.x - moving_x).abs() < 0.002);
        }
    }

    #[test]
    fn incendiary_air_button_modes_match_own_native_constant_vz_and_moving_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            48 { name weapon_incgrenade attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_incendiary_air_buttons_launch());
        for (mode, observed_x, moving_x, controls) in [
            (ThrowMode::Underhand, 199.423584, 505.673584, [
                (true, 0., 35.163757), (false, 223.868378, 314.999237), (false, 130.118378, 197.811737),
            ]),
            (ThrowMode::Combined, 432.084351, 738.334351, [
                (true, 0., 76.188133), (false, 223.868378, 356.023621), (false, 130.118378, 238.836121),
            ]),
        ] {
            // Own zero-G player VZ is constant before/after release, and each
            // projectile VZ is constant across three precontact samples.
            for (grounded, player_vz, observed_z) in controls {
                let actor = world.player_mut(id).unwrap();
                actor.movement.on_ground = grounded;
                actor.movement.velocity = Vec3::Z * player_vz;
                let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                    &definition, Some(mode), Vec3::ZERO);
                assert!((actual - Vec3::new(observed_x, 0., observed_z)).length() < 0.002);
            }
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = false;
            actor.movement.velocity = Vec3::new(245., 0., 211.368378);
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(mode), Vec3::ZERO);
            // Normal-gravity forward samples qualify horizontal velocity only.
            assert!((actual.x - moving_x).abs() < 0.002);
        }
    }

    #[test]
    fn smoke_primary_matches_independent_constant_vz_native_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            45 { name weapon_smokegrenade attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_smoke_air_primary_launch());
        // Native sv_gravity=0 was read back after jump onset, leaving two
        // constant player-Vz plateaus. No unknown flight age is subtracted.
        for (grounded, player_vz, observed_z) in [
            (true, 0., 117.212517),
            (false, 217.618378, 389.235504),
            (false, 123.868378, 272.048004),
        ] {
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = grounded;
            actor.movement.velocity = Vec3::Z * player_vz;
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(ThrowMode::Overhand), Vec3::ZERO);
            assert!((actual - Vec3::new(664.745239, 0., observed_z)).length() < 0.002);
        }
    }

    #[test]
    fn he_primary_matches_own_constant_vz_native_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            44 { name weapon_hegrenade attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_he_air_primary_launch());
        // Native sv_gravity=0 was read back after jump onset, leaving two
        // constant player-Vz plateaus. No unknown flight age is subtracted.
        for (grounded, player_vz, observed_z) in [
            (true, 0., 117.212517),
            (false, 223.868378, 397.048004),
            (false, 123.868378, 272.048004),
        ] {
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = grounded;
            actor.movement.velocity = Vec3::Z * player_vz;
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(ThrowMode::Overhand), Vec3::ZERO);
            assert!((actual - Vec3::new(664.745239, 0., observed_z)).length() < 0.002);
        }
    }

    #[test]
    fn flash_primary_matches_own_constant_vz_native_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            43 { name weapon_flashbang attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_flash_air_primary_launch());
        // Native sv_gravity=0 was read back after jump onset, leaving two
        // constant player-Vz plateaus. No unknown flight age is subtracted.
        for (grounded, player_vz, observed_z) in [
            (true, 0., 117.212517),
            (false, 223.868378, 397.048004),
            (false, 130.118378, 279.860504),
        ] {
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = grounded;
            actor.movement.velocity = Vec3::Z * player_vz;
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(ThrowMode::Overhand), Vec3::ZERO);
            assert!((actual - Vec3::new(664.745239, 0., observed_z)).length() < 0.002);
        }
    }

    #[test]
    fn decoy_primary_matches_own_constant_vz_native_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            47 { name weapon_decoy attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_decoy_air_primary_launch());
        // Native sv_gravity=0 was read back after jump onset, leaving two
        // constant player-Vz plateaus. No unknown flight age is subtracted.
        for (grounded, player_vz, observed_z) in [
            (true, 0., 117.212517),
            (false, 223.868378, 397.048004),
            (false, 123.868378, 272.048004),
        ] {
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = grounded;
            actor.movement.velocity = Vec3::Z * player_vz;
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(ThrowMode::Overhand), Vec3::ZERO);
            assert!((actual - Vec3::new(664.745239, 0., observed_z)).length() < 0.002);
        }
    }

    #[test]
    fn molotov_primary_matches_own_constant_vz_native_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            46 { name weapon_molotov attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_molotov_air_primary_launch());
        // Native sv_gravity=0 was read back after jump onset, leaving two
        // constant player-Vz plateaus. No unknown flight age is subtracted.
        for (grounded, player_vz, observed_z) in [
            (true, 0., 117.212517),
            (false, 223.868378, 397.048004),
            (false, 123.868378, 272.048004),
        ] {
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = grounded;
            actor.movement.velocity = Vec3::Z * player_vz;
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(ThrowMode::Overhand), Vec3::ZERO);
            assert!((actual - Vec3::new(664.745239, 0., observed_z)).length() < 0.002);
        }
    }

    #[test]
    fn incendiary_primary_matches_own_constant_vz_native_controls() {
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            48 { name weapon_incgrenade attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_incendiary_air_primary_launch());
        // Native sv_gravity=0 was read back after jump onset, leaving two
        // constant player-Vz plateaus. No unknown flight age is subtracted.
        for (grounded, player_vz, observed_z) in [
            (true, 0., 117.212517),
            (false, 223.868378, 397.048004),
            (false, 123.868378, 272.048004),
        ] {
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = grounded;
            actor.movement.velocity = Vec3::Z * player_vz;
            let actual = World::grenade_launch_velocity(world.player(id).unwrap(),
                &definition, Some(ThrowMode::Overhand), Vec3::ZERO);
            assert!((actual - Vec3::new(664.745239, 0., observed_z)).length() < 0.002);
        }
    }

    #[test]
    fn observed_air_primary_preserves_ground_buttons_air_alternates_and_no_mode() {
        for (name, prior_profile, candidate) in [
            ("weapon_smokegrenade", observed_smoke_launch(), observed_smoke_air_primary_launch()),
            ("weapon_hegrenade", observed_ground_buttons_launch("weapon_hegrenade"), observed_he_air_primary_launch()),
            ("weapon_flashbang", observed_ground_buttons_launch("weapon_flashbang"), observed_flash_air_primary_launch()),
            ("weapon_decoy", observed_ground_buttons_launch("weapon_decoy"), observed_decoy_air_primary_launch()),
            ("weapon_molotov", observed_ground_buttons_launch("weapon_molotov"), observed_molotov_air_primary_launch()),
            ("weapon_incgrenade", observed_ground_buttons_launch("weapon_incgrenade"), observed_incendiary_air_primary_launch()),
        ] {
            for rate in [64, 128] {
                for running in [false, true] {
                    for pitch in [0., -30., 45.] {
                        for mode in [ThrowMode::Overhand, ThrowMode::Underhand, ThrowMode::Combined] {
                            for jump in [false, true] {
                                if jump && mode == ThrowMode::Overhand { continue; }
                                assert_eq!(
                                    launch_utility_tick_input(name, rate, mode, Some(candidate.clone()), running, jump, pitch),
                                    launch_utility_tick_input(name, rate, mode, Some(prior_profile.clone()), running, jump, pitch),
                                    "{rate}Hz, running={running}, pitch={pitch}, mode={mode:?}, jump={jump}");
                            }
                        }
                    }
                }
            }
        }
        // No-mode callers have no ordinary button endpoint. Check that seam
        // with every independently qualified air mode populated, including
        // both horizontal axes and descending movement.
        let mut world = arena();
        let id = world.add_player("thrower", Team::Terrorist);
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            45 { name weapon_smokegrenade attributes { "primary default clip size" 1 } }
        } }"#).unwrap();
        let mut definition = catalog.weapons[0].clone();
        definition.grenade_launch = Some(observed_smoke_air_buttons_launch());
        for grounded in [false, true] {
            let player = world.player_mut(id).unwrap();
            player.movement.on_ground = grounded;
            player.movement.velocity = Vec3::new(-245., 80., -100.);
            let actor = world.player(id).unwrap();
            let actual = world.grenade_release(actor, &definition, None, Vec3::new(-30., 20., 0.));
            let mut unmatched = definition.clone();
            unmatched.grenade_launch = None;
            assert_eq!(actual, world.grenade_release(actor, &unmatched, None, Vec3::new(-30., 20., 0.)));
        }
    }

    #[test]
    fn grounded_smoke_buttons_match_native_level_negative_pitch_and_positive_checks() {
        fn release(pitch: f32, velocity: Vec3, grounded: bool, mode: Option<ThrowMode>,
            profile: Option<&weapons::GrenadeLaunch>) -> (Vec3, Vec3) {
            let mut world = arena();
            let id = world.add_player("thrower", Team::Terrorist);
            let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
                45 { name weapon_smokegrenade attributes { "primary default clip size" 1 } }
            } }"#).unwrap();
            let mut definition = catalog.weapons[0].clone();
            definition.grenade_launch = profile.cloned();
            let player = world.player_mut(id).unwrap();
            player.movement.on_ground = grounded;
            player.movement.velocity = velocity;
            world.grenade_release(world.player(id).unwrap(), &definition, mode, Vec3::new(pitch, 0., 0.)).unwrap()
        }
        let profile = observed_smoke_launch();
        let mut old_profile = profile.clone();
        old_profile.grounded_overhand_velocity = None;
        old_profile.grounded_underhand_velocity = None;
        old_profile.grounded_combined_velocity = None;
        // Smoke observations, not HE fixtures: level stationary/forward, then
        // stationary -30 and an independent +45 holdout of the fitted family.
        // At nonzero pitch +attack-only input and the held strength field (1)
        // establish primary mode. Generic zero abs-velocity is excluded.
        // Secondary/combined independently fit 0/-30, then check +45 without
        // refitting; parameters were not prospectively preregistered. Only
        // projectile-0 is admitted for each of those six stationary cases.
        for (mode, observations) in [
            (ThrowMode::Overhand, [
                (0., 664.745239, 102.212517, Some(94.712517), 0.25),
                (-30., 541.433167, 378.082062, Some(368.082062), 0.25),
                (45., 517.080017, -446.381622, None, 0.25),
            ]),
            (ThrowMode::Underhand, [
                (0., 199.423584, 10.163757, None, 0.251998098),
                (-30., 162.429962, 95.924629, None, 0.245490531),
                (45., 155.124008, -152.66449, None, 0.244251862),
            ]),
            (ThrowMode::Combined, [
                (0., 432.084351, 51.188133, None, 0.245796353),
                (-30., 351.931519, 237.003326, None, 0.24795296),
                (45., 336.101959, -304.523041, None, 0.242968991),
            ]),
        ] {
            for (pitch, native_x, received_z, later_z, age_upper_bound) in observations {
                let (pos, launch) = release(pitch, Vec3::ZERO, true, Some(mode), Some(&profile));
                assert!((launch.x - native_x).abs() < 0.002, "Smoke {mode:?} pitch {pitch}");
                assert!(launch.y.abs() < 0.002);
                for received_z in [Some(received_z), later_z].into_iter().flatten() {
                    // Gravity-only receive-time bracket, not an exact spawn tick
                    // or a fit of initial Vz to the displayed sample. New button
                    // cases use their individual release-to-later-server-dump
                    // bounds at confirmed .01 timescale, without subtracting an
                    // unknown spawn delay. Some exceed .25 seconds; no blanket 80.
                    let gravity_loss = launch.z - received_z;
                    assert!((0. ..320. * age_upper_bound).contains(&gravity_loss), "Smoke {mode:?} pitch {pitch}: {gravity_loss}");
                }
                let old = release(pitch, Vec3::ZERO, true, Some(mode), Some(&old_profile));
                assert_eq!(pos, old.0, "velocity calibration preserves the raw spawn trace");
                let moving = release(pitch, Vec3::new(245., 80., 100.), true, Some(mode), Some(&profile));
                let inherited = Vec3::new(306.25, 100., 50.);
                assert!((moving.1 - launch - inherited).length() < 0.001,
                    "grounded button horizontal response uses the separately observed coefficient; vertical stays unchanged");
                for (grounded, mode) in [
                    (false, Some(ThrowMode::Overhand)), (false, Some(ThrowMode::Underhand)),
                    (false, Some(ThrowMode::Combined)), (true, None), (false, None),
                ] {
                    let actual = release(pitch, Vec3::new(245., 80., 100.), grounded, mode, Some(&profile));
                    assert_eq!(actual, release(pitch, Vec3::new(245., 80., 100.), grounded, mode, None),
                        "Smoke air/no-mode retain the complete uncalibrated release");
                }
                let unmatched = release(pitch, Vec3::ZERO, true, Some(mode), None);
                assert_eq!(unmatched, old, "absent calibration retains the prior base vector");
            }
        }
    }

    #[test]
    fn measured_he_running_contribution_matches_both_observed_directions_without_changing_air_or_vertical() {
        fn launch(velocity: Vec3, grounded: bool, mode: ThrowMode, measured: bool) -> Vec3 {
            launch_he(velocity, grounded, mode, measured, Vec3::new(15., 0., 0.), None).velocity
        }
        let baseline = launch(Vec3::ZERO, true, ThrowMode::Overhand, true);
        // Native moving minus independently repeated stationary projectile:
        // +976.685791-670.435852 and +364.185883-670.435852 respectively.
        // Isolate movement inheritance from the separately tested base vector.
        for (speed, observed_delta) in [(244.999985, 306.249939), (-244.999985, -306.249969)] {
            let result = launch(Vec3::new(speed, 0., 0.), true, ThrowMode::Overhand, true);
            assert!((result.x - baseline.x - observed_delta).abs() < 0.001);
            assert_eq!(result.z, baseline.z);
        }
        for (grounded, mode, measured) in [
            (false, ThrowMode::Overhand, true), (true, ThrowMode::Underhand, true),
            (true, ThrowMode::Combined, true), (true, ThrowMode::Overhand, false),
        ] {
            let baseline = launch(Vec3::ZERO, grounded, mode, measured);
            let result = launch(Vec3::new(-245., 80., 100.), grounded, mode, measured);
            assert!((result - baseline - Vec3::new(-122.5, 40., 50.)).length() < 0.001);
        }
        let result = launch(Vec3::new(245., 0., 100.), true, ThrowMode::Overhand, true);
        assert!((result.z - baseline.z - 50.).abs() < 0.001, "vertical inheritance remains unchanged");
    }

    #[test]
    fn running_nonprimary_he_matches_independent_stationary_and_moving_vectors() {
        for (mode, speed, player_x, native_stationary, native_moving, native_delta) in [
            (ThrowMode::Underhand, 202.5, 245., 199.423584, 505.673584, 306.25),
            (ThrowMode::Combined, 438.75, -245., 432.084351, 125.834351, -306.25),
        ] {
            let launch = |velocity: Vec3, grounded: bool, measured: bool| launch_he_with_mode_inheritance(
                velocity, grounded, mode, true, Vec3::ZERO,
                Some(weapons::GrenadeLaunchVelocity { speed, upward_pitch_bias_degrees: 10. }),
                measured.then_some(1.25));
            let stationary = launch(Vec3::ZERO, true, true);
            let moving = launch(Vec3::new(player_x, 0., 0.), true, true);
            assert!((stationary.velocity.x - native_stationary).abs() < 0.002);
            assert!((moving.velocity.x - native_moving).abs() < 0.002);
            // Independent stationary and moving native observations, not a
            // copy of the configured inheritance coefficient.
            assert!((moving.velocity.x - stationary.velocity.x - native_delta).abs() < 0.001);
            let vertical = launch(Vec3::new(player_x, 80., 100.), true, true);
            assert!((vertical.velocity.z - stationary.velocity.z - 50.).abs() < 0.001);
            assert_eq!(moving.pos, launch(Vec3::new(player_x, 0., 0.), true, false).pos);
            assert_eq!(launch(Vec3::new(player_x, 80., 100.), false, true),
                launch(Vec3::new(player_x, 80., 100.), false, false));
            let unmatched = launch(Vec3::new(player_x, 80., 100.), true, false);
            assert!((unmatched.velocity - stationary.velocity - Vec3::new(player_x * 0.5, 40., 50.)).length() < 0.001);
        }
    }

    #[test]
    fn stationary_he_button_endpoints_match_native_with_prior_nonprimary_inheritance() {
        for (mode, speed, pitch, native_x, received_z) in [
            (ThrowMode::Underhand, 202.5, 0., 199.423584, 20.163757),
            (ThrowMode::Combined, 438.75, 0., 432.084351, 61.188133),
            // Prospective angled holdouts: unchanged .3/.65 speed ratios
            // and primary pitch family predicted these before observation.
            (ThrowMode::Underhand, 202.5, 45., 155.124008, -145.164490),
            (ThrowMode::Combined, 438.75, -30., 351.931519, 247.003326),
        ] {
            let angles = Vec3::new(pitch, 0., 0.);
            let profile = || Some(weapons::GrenadeLaunchVelocity { speed, upward_pitch_bias_degrees: 10. });
            let stationary = launch_he(Vec3::ZERO, true, mode, true, angles, profile());
            assert!((stationary.velocity.x - native_x).abs() < 0.002);
            assert!(stationary.velocity.y.abs() < 0.002);
            // Receive-time velocity is not an exact construction timestamp.
            assert!((0. ..60.).contains(&(stationary.velocity.z - received_z)));
            let old = launch_he(Vec3::ZERO, true, mode, true, angles, None);
            assert_eq!(stationary.pos, old.pos);
            let moving = launch_he(Vec3::new(245., 80., 100.), true, mode, true, angles, profile());
            assert!((moving.velocity - stationary.velocity - Vec3::new(122.5, 40., 50.)).length() < 0.001,
                "stationary mode evidence must not change unmeasured movement inheritance");
            let air = launch_he(Vec3::new(245., 80., 100.), false, mode, true, angles, profile());
            let old_air = launch_he(Vec3::new(245., 80., 100.), false, mode, true, angles, None);
            assert_eq!(air, old_air);
        }
    }

    #[test]
    fn grounded_he_velocity_matches_native_pitches_and_prospective_negative_holdout() {
        fn profile() -> Option<weapons::GrenadeLaunchVelocity> {
            Some(weapons::GrenadeLaunchVelocity { speed: 675., upward_pitch_bias_degrees: 10. })
        }
        for (pitch, native_x, received_z) in [
            (0., 664.745239, Some(102.212517)),
            (45., 517.080017, Some(-448.881622)),
            (-45., 433.881653, Some(502.080017)),
            // Preregistered before this independent native session: predicted
            // 541.4331551, observed 541.433167. No refit to this result.
            (-30., 541.433167, Some(388.082062)),
            (15., 670.435852, None),
        ] {
            let angles = Vec3::new(pitch, 0., 0.);
            let result = launch_he(Vec3::ZERO, true, ThrowMode::Overhand, true, angles, profile());
            assert!((result.velocity.x - native_x).abs() < 0.002, "pitch {pitch}");
            assert!(result.velocity.y.abs() < 0.002);
            if let Some(received_z) = received_z {
                // Public receive-time fields are later than construction.
                // Gravity must lower Z, within a conservative command-to-frame
                // bound. This is not an assertion of an exact spawn timestamp.
                let gravity_change = result.velocity.z - received_z;
                assert!((0. ..60.).contains(&gravity_change), "pitch {pitch}: {gravity_change}");
            }
            let old = launch_he(Vec3::ZERO, true, ThrowMode::Overhand, true, angles, None);
            assert_eq!(result.pos, old.pos, "velocity calibration must not move the spawn trace");
        }
        for (grounded, mode, measured) in [
            (false, ThrowMode::Overhand, true), (true, ThrowMode::Overhand, false),
        ] {
            let actual = launch_he(Vec3::new(245., 80., 100.), grounded, mode, measured, Vec3::ZERO, profile());
            let old = launch_he(Vec3::new(245., 80., 100.), grounded, mode, measured, Vec3::ZERO, None);
            assert_eq!(actual, old, "unobserved scopes retain their existing complete launch");
        }
    }

    #[test]
    fn decoy_expiry_is_once_at_deadline_and_never_round_cleanup() {
        for rate in [64, 128] {
            let mut world = arena();
            world.set_tick_rate(rate);
            let pos = Vec3::new(100., 20., 2.);
            let decoy = Field { id: 1, owner: 7, kind: Kind::Decoy, pos,
                born: 0., until: 15., decoy_weapon: None };
            world.utility_fields = vec![decoy.clone(), Field { id: 2, kind: Kind::Smoke, ..decoy.clone() }];
            world.tick = 15 * rate as u64 - 1;
            world.step_utility();
            assert!(world.events.is_empty());
            assert_eq!(world.utility_fields.len(), 2);
            // The deadline remains deterministic after restoring network state.
            let mut restored = world.clone();
            restored.restore(world.decode(&world.encode_state()).unwrap());
            for w in [&mut world, &mut restored] {
                w.tick += 1;
                w.step_utility();
                assert_eq!(w.events, vec![GameEvent::DecoyExpired { player: 7, pos }]);
                assert!(w.utility_fields.is_empty());
                w.events.clear();
                for _ in 0..rate {
                    w.tick += 1;
                    w.step_utility();
                    assert!(w.events.is_empty(), "removed decoy cannot expire repeatedly");
                }
                // Round cleanup must remain silent even for an overdue field.
                w.utility_fields.push(decoy.clone());
                w.start_round(false);
                w.step_utility();
                assert!(!w.events.iter().any(|e| matches!(e, GameEvent::DecoyExpired { .. })));
                assert!(w.utility_fields.is_empty());
            }
        }
    }

    #[test]
    fn contacts_emit_once_while_moving_but_not_at_rest_or_fire_detonation() {
        use collision::{CollisionWorld, Convex, Plane};
        use std::sync::Arc;
        for rate in [64, 128] {
            let mut collision = CollisionWorld::new();
            collision.add_convex(Convex { plane_surfaces: Vec::new(),
                mins: Vec3::new(-512., -512., -16.),
                maxs: Vec3::new(512., 512., 0.),
                planes: [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
                    .into_iter().zip([512., 512., 512., 512., 0., 16.])
                    .map(|(normal, dist)| Plane { normal, dist }).collect(),
                contents: contents::SOLID, surface: 0,
            });
            collision.finalize();
            let collision = Arc::new(collision);
            for kind in [Kind::He, Kind::Flash, Kind::Smoke, Kind::Decoy, Kind::Molotov, Kind::Incendiary] {
                let mut world = World::new(collision.clone());
                world.set_tick_rate(rate);
                world.tick = rate as u64;
                world.projectiles.push(Projectile {
                    id: 1, owner: 1, kind, pos: Vec3::Z * 4.,
                    velocity: -Vec3::Z * 400., settled: false, born: 0., decoy_weapon: None,
                });
                world.step_utility();
                if kind.fire() {
                    assert!(world.events.iter().any(|e| matches!(e, GameEvent::UtilityDetonated { .. })));
                    assert!(!world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })));
                    continue;
                }
                let contacts: Vec<_> = world.events.iter().filter_map(|e| match e {
                    GameEvent::UtilityBounced { player, kind, pos } => Some((*player, *kind, *pos)),
                    _ => None,
                }).collect();
                assert_eq!(contacts.len(), 1);
                assert_eq!((contacts[0].0, contacts[0].1), (1, kind));
                assert!((2. ..2.3).contains(&contacts[0].2.z));
                assert!(world.projectiles[0].velocity.z > 0.);
                // The final genuine soft tap must sound once even though its
                // outgoing velocity is zero. Later separation/gravity cycles
                // must remain silent at both supported tick rates.
                world.projectiles[0].velocity = -Vec3::Z * 20.;
                world.projectiles[0].settled = false;
                world.projectiles[0].pos = Vec3::Z * 2.1;
                world.projectiles[0].born = world.time();
                world.tick += 1;
                world.events.clear();
                world.step_utility();
                assert_eq!(world.events.iter().filter(|e| matches!(e,
                    GameEvent::UtilityBounced { .. })).count(), 1);
                assert_eq!(world.projectiles[0].velocity, Vec3::ZERO);
                assert!(world.projectiles[0].settled);
                for _ in 0..rate {
                    world.tick += 1;
                    world.events.clear();
                    world.step_utility();
                    assert!(!world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })));
                }
            }
        }
    }

    #[test]
    fn settled_projectiles_replay_through_contact_cycles_and_wake_when_support_disappears() {
        use collision::{CollisionWorld, Convex, Plane};
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
        struct RemovableFloor { collision: CollisionWorld, present: AtomicBool }
        impl collision::DynamicWorld for RemovableFloor {
            fn trace_box(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> collision::Trace {
                if self.present.load(Ordering::Relaxed) {
                    self.collision.trace_box(start, end, mins, maxs, mask)
                } else { collision::Trace::empty(end) }
            }
            fn point_contents(&self, _: Vec3, _: i32) -> i32 { 0 }
        }
        for rate in [64, 128] {
            for normal in [Vec3::Z, Vec3::new(0.6, 0.0, 0.8)] {
                let mut floor = CollisionWorld::new();
                floor.add_convex(Convex { plane_surfaces: Vec::new(),
                    mins: Vec3::splat(-512.), maxs: Vec3::splat(512.),
                    planes: [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, normal, -Vec3::Z]
                        .into_iter().zip([512., 512., 512., 512., 0., 512.])
                        .map(|(normal, dist)| Plane { normal, dist }).collect(),
                    contents: contents::SOLID, surface: 0,
                });
                floor.finalize();
                let floor = Arc::new(RemovableFloor { collision: floor, present: AtomicBool::new(true) });
                let mut collision = CollisionWorld::new();
                collision.dynamic = Some(floor.clone());
                let mut world = World::new(Arc::new(collision));
                world.set_tick_rate(rate);
                world.projectiles.push(Projectile {
                    id: 1, owner: 1, kind: Kind::He, pos: Vec3::Z * 6.,
                    velocity: -Vec3::Z * 100., settled: false, born: 0., decoy_weapon: None,
                });
                let mut bounced = false;
                for _ in 0..rate {
                    world.step(&[]);
                    let p = &world.projectiles[0];
                    if world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })) {
                        bounced = true;
                    }
                    if p.settled { break; }
                }
                assert!(bounced && world.projectiles[0].settled);
                let mut restored = world.clone();
                restored.restore(world.decode(&world.encode_state()).unwrap());
                let mut predicted = restored.clone();
                let mut moving_during_rest = false;
                for _ in 0..rate / 4 {
                    world.step(&[]); restored.step(&[]); predicted.step_prediction(&[]);
                    assert!(world.projectiles[0].settled, "gravity/nudge cycles must not restart tumble");
                    moving_during_rest |= world.projectiles[0].velocity != Vec3::ZERO;
                    assert_eq!(world.state(), restored.state());
                    assert_eq!(world.state(), predicted.state());
                }
                assert!(moving_during_rest, "the original ballistic/contact integration still runs");
                let previous_z = world.projectiles[0].pos.z;
                floor.present.store(false, Ordering::Relaxed);
                world.step(&[]); restored.step(&[]); predicted.step_prediction(&[]);
                assert!(!world.projectiles[0].settled, "lost support must wake on the first airborne tick");
                assert!(world.projectiles[0].pos.z < previous_z && world.projectiles[0].velocity.z < 0.);
                assert_eq!(world.state(), restored.state());
                assert_eq!(world.state(), predicted.state());
            }
        }
    }

    #[test]
    fn decoy_captures_primary_else_pistol_through_throw_death_and_snapshot_restore() {
        use crate::{TickInput, weapon::WeaponState};
        use movement::buttons;
        use std::sync::Arc;
        for rate in [64, 128] {
            for primary in [Some("weapon_ak47"), Some("weapon_awp"), None] {
                let mut catalog = weapons::Catalog::from_items_game(r#"items_game { items {
                    "7" { name weapon_ak47 baseitem 1 }
                    "9" { name weapon_awp baseitem 1 }
                    "4" { name weapon_glock baseitem 1 }
                    "47" { name weapon_decoy attributes { "primary default clip size" 1 } }
                } }"#).unwrap();
                catalog.weapons.iter_mut().find(|d| d.class_name == "weapon_decoy").unwrap()
                    .grenade_animations = Some(weapons::GrenadeAnimations {
                        pull_pin_duration: 0.25,
                        overhand: weapons::GrenadeReleaseAnimation { command_release_time: None, duration: 0.5, release_time: 0.125 },
                        underhand: weapons::GrenadeReleaseAnimation { command_release_time: None, duration: 0.5, release_time: 0.125 },
                    });
                let mut collision = collision::CollisionWorld::new();
                collision.finalize();
                let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
                world.set_tick_rate(rate);
                let owner = world.add_player("thrower", Team::Terrorist);
                let mut carried = vec![world.weapon("weapon_decoy").unwrap(), world.weapon("weapon_glock").unwrap()];
                if let Some(name) = primary { carried.push(world.weapon(name).unwrap()); }
                world.players[0].weapons = carried;
                world.players[0].active = 0;
                for _ in 0..rate / 2 {
                    world.step(&[(owner, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
                }
                assert!(world.projectiles.is_empty(), "holding the pin does not emit a decoy");
                for _ in 0..rate {
                    world.step(&[(owner, TickInput::default())]);
                    if !world.projectiles.is_empty() { break; }
                }
                let expected = world.catalog.get(primary.unwrap_or("weapon_glock")).unwrap().id.to_string();
                assert_eq!(world.projectiles.len(), 1);
                assert_eq!(world.projectiles[0].decoy_weapon.as_deref(), Some(expected.as_str()));
                // A different inventory/life after release must not rewrite the cue.
                world.players[0].weapons = vec![WeaponState::new(Arc::new(world.catalog.get("weapon_glock").unwrap().clone()))];
                world.players[0].alive = false;
                world.players[0].health = 0;
                world.players[0].respawn_at = 1000.;
                let state = world.decode(&world.encode_state()).unwrap();
                let mut restored = world.clone();
                restored.restore(state);
                let mut heard = 0;
                let mut checked_field = false;
                for _ in 0..rate * 4 {
                    world.step(&[]);
                    restored.step(&[]);
                    assert_eq!(world.state(), restored.state());
                    assert_eq!(world.events, restored.events);
                    if !checked_field && !world.utility_fields.is_empty() {
                        let state: crate::WorldState = world.decode(&world.encode_state()).unwrap();
                        assert_eq!(state.utility_fields[0].decoy_weapon.as_deref(), Some(expected.as_str()));
                        checked_field = true;
                    }
                    for event in &world.events {
                        if let GameEvent::DecoyShot { player, weapon, .. } = event {
                            assert_eq!((*player, weapon), (owner, &expected));
                            heard += 1;
                        }
                    }
                }
                assert!(checked_field && heard > 0, "flight must become a persistent audible field");
            }
        }
    }
    #[test]
    fn lethal_he_and_fire_capture_notice_without_inventing_hit_rays() {
        for rate in [64, 128] {
            for kind in [Kind::He, Kind::Molotov] {
                let mut w = arena();
                w.set_tick_rate(rate);
                let owner = w.add_player("thrower", Team::Terrorist);
                let victim = w.add_player("victim", Team::CounterTerrorist);
                w.players[0].movement.origin = Vec3::X * 1000.;
                w.players[1].movement.origin = Vec3::ZERO;
                w.blast_utility(owner, kind, Vec3::Z * 32., 140., 1000.);
                let death = w.events.iter().find_map(|e| match e {
                    GameEvent::Kill { victim: id, death, .. } if *id == victim => Some(death),
                    _ => None,
                }).unwrap();
                assert!(death.hit.is_none());
                assert_eq!(death.attacker.as_ref().unwrap().id, owner);
                assert_eq!(death.attacker.as_ref().unwrap().name, "thrower");
                assert_eq!(death.pose.origin, Vec3::ZERO);
            }
        }
    }
    fn arena() -> World {
        let mut c = collision::CollisionWorld::new();
        c.finalize();
        World::new(std::sync::Arc::new(c))
    }
    #[test]
    fn throwing_consumes_inventory_and_never_emits_bullets() {
      for rate in [64,128] {
        let mut w = arena();
        w.set_tick_rate(rate);
        let cat=weapons::Catalog::from_items_game(r#""items_game" { "items" { "43" { "name" "weapon_flashbang" "baseitem" "0" "attributes" { "primary default clip size" "1" "in game price" "200" } } } }"#).unwrap();
        assert_eq!(cat.weapons.len(), 1);
        let id = w.add_player("thrower", crate::Team::Terrorist);
        let mut def = cat.get("weapon_flashbang").unwrap().clone();
        // Installed flashbang declarations: 30fps, 30 pull frames, 23 throw
        // frames and event at 10/22 cycle. This tests our authored policy.
        def.grenade_animations=Some(weapons::GrenadeAnimations {pull_pin_duration:29./30.,
            overhand:weapons::GrenadeReleaseAnimation { command_release_time: None,duration:22./30.,release_time:(22./30.)*(10./22.)},
            underhand:weapons::GrenadeReleaseAnimation { command_release_time: None,duration:12./30.,release_time:(12./30.)*(10./12.)}});
        assert_eq!(def.clip_size, 1);
        w.player_mut(id)
            .unwrap()
            .give(crate::weapon::WeaponState::new(std::sync::Arc::new(
                def.clone(),
            )));
        w.step(&[(
            id,
            crate::TickInput {
                buttons: movement::buttons::ATTACK,
                ..Default::default()
            },
        )]);
        assert!(w.player(id).unwrap().has("weapon_flashbang"));
        assert!(w.projectiles.is_empty());
        for _ in 0..rate {
            w.step(&[(id,crate::TickInput {buttons:movement::buttons::ATTACK,..Default::default()})]);
            assert!(w.projectiles.is_empty(),"holding never launches the grenade");
        }
        assert_eq!(w.player(id).unwrap().active_weapon().unwrap().grenade_throw.unwrap().phase,ThrowPhase::Holding);
        let mut thrown=false;
        for _ in 0..rate {
            w.step(&[(id,crate::TickInput::default())]);
            thrown|=w.events.iter().any(|e|matches!(e,GameEvent::UtilityThrown {..}));
            assert!(!w.events.iter().any(|e|matches!(e,GameEvent::Fire {..}|GameEvent::Tracer {..})));
        }
        assert!(thrown);
        assert!(!w.player(id).unwrap().has("weapon_flashbang"));
        assert_eq!(w.projectiles.len(), 1);
        assert!(
            !w.events
                .iter()
                .any(|e| matches!(e, GameEvent::Fire { .. } | GameEvent::Tracer { .. }))
        );
        // Selecting a grenade that isn't carried must never create one.
        w.step(&[(
            id,
            crate::TickInput {
                utility: 43,
                buttons: movement::buttons::ATTACK,
                ..Default::default()
            },
        )]);
        assert_eq!(w.projectiles.len(), 1);
      }
    }
    #[test]
    fn flash_respects_facing_and_line_of_sight() {
        let mut w = arena();
        let a = w.add_player("looking", crate::Team::Terrorist);
        let b = w.add_player("turned", crate::Team::CounterTerrorist);
        for p in &mut w.players {
            p.movement.origin = Vec3::ZERO;
            p.view_angles = Vec3::ZERO;
        }
        w.player_mut(b).unwrap().view_angles.y = 180.0;
        w.tick = 2 * w.tick_rate as u64;
        w.projectiles.push(Projectile {
            id: 1,
            owner: a,
            kind: Kind::Flash,
            pos: Vec3::new(200.0, 0.0, 64.0),
            velocity: Vec3::ZERO,
            settled: false,
            born: 0.0,
            decoy_weapon: None,
        });
        w.step_utility();
        assert!(w.player(a).unwrap().flash_until > w.player(b).unwrap().flash_until + 2.0);
        assert_eq!(w.projectiles.len(), 0);
    }
    #[test]
    fn fire_hazard_and_actual_burn_damage_respect_the_same_solid_wall() {
        use collision::{CollisionWorld, Convex, Plane};
        let mut collision = CollisionWorld::new();
        let mins = Vec3::new(16., -128., 0.);
        let maxs = Vec3::new(24., 128., 128.);
        assert!(collision.add_convex(Convex { plane_surfaces: Vec::new(),
            mins, maxs,
            planes: [(Vec3::X, maxs.x), (-Vec3::X, -mins.x),
                (Vec3::Y, maxs.y), (-Vec3::Y, -mins.y),
                (Vec3::Z, maxs.z), (-Vec3::Z, -mins.z)].into_iter()
                .map(|(normal, dist)| Plane { normal, dist }).collect(),
            contents: contents::SOLID, surface: 0,
        }));
        collision.finalize();
        let mut w = World::new(std::sync::Arc::new(collision));
        let exposed = w.add_player("exposed", crate::Team::Terrorist);
        let sheltered = w.add_player("sheltered", crate::Team::CounterTerrorist);
        let open_point = Vec3::new(-64., 0., 0.);
        let blocked_point = Vec3::new(64., 0., 0.);
        let boundary = w.add_player("boundary", crate::Team::Terrorist);
        let elevated = w.add_player("elevated", crate::Team::CounterTerrorist);
        let boundary_point = Vec3::new(-135., 0., 0.);
        let elevated_point = Vec3::new(-125., 0., 50.);
        w.player_mut(boundary).unwrap().movement.origin = boundary_point;
        w.player_mut(elevated).unwrap().movement.origin = elevated_point;
        w.player_mut(exposed).unwrap().movement.origin = open_point;
        w.player_mut(sheltered).unwrap().movement.origin = blocked_point;
        w.utility_fields.push(Field { id: 1, owner: crate::WORLD, kind: Kind::Molotov,
            pos: Vec3::ZERO, born: 0., until: 7., decoy_weapon: None });
        // Equally near, at the same height; only the actual solid wall differs.
        assert!(w.fire_at(open_point));
        assert!(!w.fire_at(blocked_point));
        // Old 130-horizontal/65-height hazard disagreed with actual damage:
        // this ground origin burns, while the elevated origin is out of range.
        assert!(w.fire_at(boundary_point));
        assert!(!w.fire_at(elevated_point));
        w.step_utility();
        assert_eq!(w.player(exposed).unwrap().health, 92);
        assert_eq!(w.player(sheltered).unwrap().health, 100);
        assert_eq!(w.player(boundary).unwrap().health, 92);
        assert_eq!(w.player(elevated).unwrap().health, 100);
    }

    #[test]
    fn smoke_extinguishes_fire_expires_and_does_not_block_bullet_geometry() {
        let mut w = arena();
        w.tick = w.tick_rate as u64;
        w.utility_fields.push(Field {
            id: 1,
            owner: 1,
            kind: Kind::Molotov,
            pos: Vec3::ZERO,
            born: 0.0,
            until: 7.0,
            decoy_weapon: None,
        });
        w.utility_fields.push(Field {
            id: 2,
            owner: 2,
            kind: Kind::Smoke,
            pos: Vec3::ZERO,
            born: 0.0,
            until: 18.0,
            decoy_weapon: None,
        });
        assert!(w.fire_at(Vec3::ZERO));
        w.step_utility();
        assert!(!w.fire_at(Vec3::ZERO));
        let a = Vec3::new(-300.0, 0.0, 55.0);
        let b = Vec3::new(300.0, 0.0, 55.0);
        assert!(w.smoke_occludes(a, b));
        assert!(w.visible(a, b));
        w.tick = 19 * w.tick_rate as u64;
        w.step_utility();
        assert!(!w.smoke_occludes(a, b));
        assert!(w.utility_fields.is_empty());
    }
    #[test]
    fn explosive_damage_falls_off_and_armor_absorbs() {
        let mut w = arena();
        let a = w.add_player("near", crate::Team::Terrorist);
        let b = w.add_player("far", crate::Team::CounterTerrorist);
        w.player_mut(a).unwrap().movement.origin = Vec3::X * 70.0;
        w.player_mut(b).unwrap().movement.origin = Vec3::X * 300.0;
        w.player_mut(a).unwrap().armor = 0;
        w.player_mut(b).unwrap().armor = 0;
        w.blast_utility(crate::WORLD, Kind::He, Vec3::Z * 32.0, 350.0, 98.0);
        let near = w.player(a).unwrap().health;
        let far = w.player(b).unwrap().health;
        assert!(near < far && far < 100);
        w.player_mut(a).unwrap().health = 100;
        w.player_mut(a).unwrap().armor = 100;
        w.blast_utility(crate::WORLD, Kind::He, Vec3::Z * 32.0, 350.0, 98.0);
        assert!(w.player(a).unwrap().health > near);
        assert!(w.player(a).unwrap().armor < 100);
    }
    #[test]
    fn lob_hits_target_at_requested_height() {
        let from = Vec3::new(0.0, 0.0, 64.0);
        let to = Vec3::new(700.0, 120.0, 8.0);
        let angles = lob_angles(from, to).unwrap();
        let (f, _, _) = movement::angle_vectors(angles);
        let t = (to - from).truncate().length() / (750.0 * f.truncate().length());
        assert!((from + f * 750.0 * t - Vec3::Z * 160.0 * t * t - to).length() < 0.01);
        assert!(lob_angles(from, Vec3::X * 4000.0).is_none());
    }
    #[test]
    fn smoke_blocks_only_intersecting_segments() {
        assert!(crosses_smoke(
            -Vec3::X * 300.0,
            Vec3::X * 300.0,
            Vec3::ZERO,
            140.0
        ));
        assert!(!crosses_smoke(
            Vec3::X * 200.0,
            Vec3::X * 400.0,
            Vec3::ZERO,
            140.0
        ));
    }
}
