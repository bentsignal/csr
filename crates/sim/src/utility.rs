//! Deterministic grenades. Inventory, collision and effects are shared by humans and bots.
use crate::{GameEvent, PlayerId, World};
use collision::contents;
use glam::Vec3;
use serde::{Deserialize, Serialize};

mod throw_state;
pub use throw_state::{ThrowMode,ThrowPhase,ThrowState};

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
    /// Presentation rests after the existing soft floor stop, despite the
    /// solver's small gravity/contact cycles. Never skips ballistic integration.
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
    pub fn fire_at(&self, point: Vec3) -> bool {
        self.utility_fields.iter().any(|f| {
            f.kind.fire()
                && f.until > self.time()
                && (point.z - f.pos.z).abs() < 65.0
                && point.truncate().distance(f.pos.truncate()) < 130.0
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
    fn spawn_grenade(&mut self, i: usize) -> bool {
        let time = self.time();
        let p = &self.players[i];
        let Some(kind) = p
            .active_weapon()
            .and_then(|w| Kind::from_name(&w.def.class_name))
        else {
            return false;
        };
        let (from, angles) = p.aim_origin_angles();
        let (forward, _, _) = movement::angle_vectors(angles);
        let tr = self.collision.trace_box(
            from,
            from + forward * 18.0,
            Vec3::splat(-2.0),
            Vec3::splat(2.0),
            contents::MASK_SOLID,
        );
        if tr.start_solid {
            return false;
        }
        let pos = from + forward * 18.0 * tr.fraction;
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
        let strength=p.active_weapon().and_then(|w|w.grenade_throw).map_or(1.,|s|s.mode.strength());
        // Read the release tick's actual movement, not the pull-pin tick. This
        // includes running/backpedalling/jumping; crouch uses the current eye.
        // The inherited 0.5 coefficient remains an uncalibrated approximation.
        let velocity = forward * (750.0*strength) + p.movement.velocity * 0.5;
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
            let was_moving = p.velocity != Vec3::ZERO;
            p.velocity.z -= 320.0 * dt;
            let end = p.pos + p.velocity * dt;
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
                p.settled = support.hit() && !support.start_solid && support.normal.z > FLOOR_NORMAL_Z;
            }
            let age = time - p.born;
            let detonate = if p.kind.fire() {
                (floor && age > 0.15) || age > 3.5
            } else if matches!(p.kind, Kind::Smoke | Kind::Decoy) {
                age > 1.5 && (p.velocity.length() < 20.0 || age > 3.0)
            } else {
                age > 1.5
            };
            // The existing solver settles soft floor contacts to ZERO. Such
            // contacts are silent: its 0.1-unit separation nudge otherwise
            // produces another gravity-driven "bounce" every few ticks.
            // No new speed threshold or velocity-to-loudness curve is added.
            if was_moving && tr.hit() && !tr.start_solid && p.velocity != Vec3::ZERO
                && !(p.kind.fire() && detonate)
            {
                self.events.push(GameEvent::UtilityBounced {
                    player: p.owner, kind: p.kind, pos: p.pos,
                });
            }
            if detonate {
                exploded.push(p.clone());
            }
        }
        self.projectiles
            .retain(|p| !exploded.iter().any(|e| e.id == p.id));
        for p in exploded {
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
                            15.0
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
        // Decoys fire twice a second; fire burns about five times a second.
        if self.tick % (self.tick_rate as u64 / 2).max(1) == 0 {
            for f in self.utility_fields.iter().filter(|f| f.kind == Kind::Decoy) {
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
                self.blast_utility(f.owner, f.kind, f.pos + Vec3::Z * 12.0, 140.0, 8.0);
            }
        }
    }
    fn blast_utility(&mut self, owner: PlayerId, kind: Kind, pos: Vec3, radius: f32, base: f32) {
        let attacker = self.players.iter().position(|p| p.id == owner);
        for i in 0..self.players.len() {
            let p = &self.players[i];
            let chest = p.movement.origin + Vec3::Z * 32.0;
            let dist = chest.distance(pos);
            if !p.alive || dist >= radius || !self.visible(pos, chest) {
                continue;
            }
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
            collision.add_convex(Convex {
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
                // Exercise repeated gravity/nudge contacts, not just one ZERO tick.
                world.projectiles[0].velocity = Vec3::ZERO;
                world.projectiles[0].pos = Vec3::Z * 2.1;
                world.projectiles[0].born = world.time();
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
                floor.add_convex(Convex {
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
                        assert!(!p.settled, "a genuine bounce must keep tumbling");
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
