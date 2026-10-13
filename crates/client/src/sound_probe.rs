//! Scripted fidelity inputs, enabled only for screenshot runs. These follow
//! tools/fidelity/soundprobes.txt and use ordinary simulation input.
use bevy::prelude::*;
use movement::buttons;

#[derive(Resource, Default)]
pub struct SoundProbe {
    name: Option<String>,
    counted: Option<CountedShots>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn world(rate: u32) -> (sim::World, sim::PlayerId) {
        // Synthetic definitions and floor; no installed assets or audio device.
        let catalog = weapons::Catalog::from_items_game(r#""items_game" { "items" {
            "7" { "baseitem" "1" "name" "weapon_ak47" "attributes" {
                "cycletime" "0.1" "is full auto" "1" "primary clip size" "30"
                "primary reserve ammo max" "90" "damage" "1" } }
            "32" { "baseitem" "1" "name" "weapon_hkp2000" }
            "42" { "baseitem" "1" "name" "weapon_knife" }
        } }"#).unwrap();
        let mut collision = collision::CollisionWorld::new();
        let s = 4096.0;
        for triangle in [
            [glam::Vec3::new(-s, -s, -110.0), glam::Vec3::new(s, -s, -110.0), glam::Vec3::new(s, s, -110.0)],
            [glam::Vec3::new(-s, -s, -110.0), glam::Vec3::new(s, s, -110.0), glam::Vec3::new(-s, s, -110.0)],
        ] { collision.add_triangle(triangle, collision::contents::SOLID, 0); }
        collision.finalize();
        let mut world = sim::World::with_catalog(Arc::new(collision), Arc::new(catalog));
        world.set_tick_rate(rate);
        world.set_mode(sim::GameMode::Deathmatch, sim::RulesConfig::default());
        let id = world.add_player("counted probe", sim::Team::CounterTerrorist);
        (world, id)
    }

    #[test]
    fn actual_tick_inputs_stop_after_exact_fire_counts_at_64_and_128() {
        for rate in [64, 128] {
            for (name, requested) in [("ak47_single", 1), ("ak47_three", 3), ("ak47_five", 5)] {
                let (mut world, id) = world(rate);
                let mut probe = SoundProbe::named(Some(name.to_string()));
                probe.prepare(&mut world, id);
                let mut actual = Vec::new();
                while world.time() < probe.duration() + 1.0 {
                    let tick = world.tick;
                    let input = probe.input(world.time()).unwrap();
                    assert_eq!(input.view_angles.x, -89.0);
                    world.step(&[(id, input)]);
                    actual.extend(world.events.iter().filter_map(|event| match event {
                        sim::GameEvent::Fire { player, .. } if *player == id => Some(tick),
                        _ => None,
                    }));
                    probe.observe_step(&world, id, tick);
                    // A repeated observation cannot duplicate the same actual event.
                    probe.observe_step(&world, id, tick);
                    if actual.len() == requested {
                        assert_eq!(probe.input(world.time()).unwrap().buttons & buttons::ATTACK, 0);
                    }
                }
                let counted = probe.counted.as_ref().unwrap();
                assert!(counted.finished);
                assert_eq!(actual.len(), requested);
                assert_eq!(counted.observed.iter().map(|(tick, _)| *tick).collect::<Vec<_>>(), actual);
                assert_eq!(counted.observed.iter().map(|(_, clip)| *clip).collect::<Vec<_>>(),
                    (1..=requested).map(|n| 30 - n as i32).collect::<Vec<_>>());
                assert_eq!(world.player(id).unwrap().active_weapon().unwrap().clip, 30 - requested as i32);
                for pair in actual.windows(2) {
                    let seconds = (pair[1] - pair[0]) as f32 / rate as f32;
                    assert!(seconds >= 0.1 && seconds < 0.1 + world.dt());
                }
            }
        }
    }

    #[test]
    fn absent_probe_never_overrides_interactive_input() {
        let probe = SoundProbe::named(None);
        assert!(!probe.active());
        assert!(probe.input(2.0).is_none());
        assert!(probe.counted.is_none());
    }
}

#[derive(Default)]
struct CountedShots {
    target: usize,
    observed: Vec<(u64, i32)>,
    last_tick: Option<u64>,
    finished: bool,
}

fn target(name: Option<&str>) -> Option<usize> {
    match name {
        Some("ak47_single") => Some(1),
        Some("ak47_three") => Some(3),
        Some("ak47_five") => Some(5),
        _ => None,
    }
}

impl SoundProbe {
    pub fn from_env(screenshot: bool) -> Self {
        Self::named(screenshot.then(|| std::env::var("CSR_SOUND_PROBE").ok()).flatten())
    }

    fn named(name: Option<String>) -> Self {
        let counted = target(name.as_deref()).map(|target| CountedShots { target, ..Default::default() });
        Self { name, counted }
    }

    pub fn active(&self) -> bool {
        self.name.is_some()
    }

    pub fn duration(&self) -> f32 {
        6.5
    }

    pub fn prepare(&self, world: &mut sim::World, id: sim::PlayerId) {
        if !self.active() {
            return;
        }
        assert!(
            matches!(
                self.name.as_deref(),
                Some(
                    "ak47_fire_reload"
                        | "ak47_single"
                        | "ak47_three"
                        | "ak47_five"
                        | "deploy_each_slot"
                        | "run_gravel_knife"
                        | "walk_gravel_knife"
                        | "crouch_walk"
                        | "jump_land"
                        | "impacts_ct_spawn"
                )
            ),
            "Unknown CSR_SOUND_PROBE"
        );
        if self.counted.is_some() {
            assert_eq!(world.mode, sim::GameMode::Deathmatch, "Counted sound probes require --deathmatch");
            assert!(!world.players.iter().any(|p| p.is_bot), "Counted sound probes require --bots 0");
        }
        let weapons = ["weapon_ak47", "weapon_hkp2000", "weapon_knife"].map(|name| {
            sim::weapon::WeaponState::new(std::sync::Arc::new(
                world
                    .catalog
                    .get(name)
                    .expect("probe weapon in catalog")
                    .clone(),
            ))
        });
        let p = world.player_mut(id).unwrap();
        p.weapons = weapons.into();
        p.active = 0;
        p.movement.origin = glam::Vec3::new(382.0, 2102.0, -109.0);
        p.movement.velocity = glam::Vec3::ZERO;
        p.view_angles = glam::Vec3::new(if self.counted.is_some() { -89.0 } else { 0.0 }, 180.0, 0.0);
    }

    /// Observe the exact authoritative step, before render event batching. World
    /// increments tick after producing events, so caller retains its pre-step tick.
    pub fn observe_step(&mut self, world: &sim::World, id: sim::PlayerId, producer_tick: u64) {
        let Some(counted) = &mut self.counted else { return };
        assert_eq!(world.tick, producer_tick.checked_add(1).expect("probe tick overflow"));
        if counted.last_tick == Some(producer_tick) { return; }
        assert!(counted.last_tick.is_none_or(|last| producer_tick > last), "Counted probe observed an older step");
        counted.last_tick = Some(producer_tick);
        for event in &world.events {
            if let sim::GameEvent::Fire { player, weapon, .. } = event {
                if *player != id { continue; }
                assert_eq!(weapon, "weapon_ak47", "Unexpected counted probe weapon");
                let clip = world.player(id).and_then(|p| p.active_weapon()).expect("counted probe weapon equipped").clip;
                counted.observed.push((producer_tick, clip));
                assert!(counted.observed.len() <= counted.target, "Counted probe overshot requested shots");
                info!("sound_probe fire name {:?} owner {id} producer_tick {producer_tick} tick_rate {} sim_time {:.6} ordinal {} target {} clip_after {clip}",
                    self.name, world.tick_rate, producer_tick as f64 / world.tick_rate as f64, counted.observed.len(), counted.target);
            }
        }
        if !counted.finished && world.time() >= 6.5 {
            assert_eq!(counted.observed.len(), counted.target, "Counted probe did not reach requested shots");
            counted.finished = true;
            info!("sound_probe complete name {:?} owner {id} count {} observed {:?}", self.name, counted.target, counted.observed);
        }
    }

    pub fn input(&self, time: f32) -> Option<sim::TickInput> {
        let name = self.name.as_deref()?;
        let mut input = sim::TickInput {
            view_angles: glam::Vec3::new(0.0, 180.0, 0.0),
            ..Default::default()
        };
        let during = |start, end| (start..end).contains(&time);
        if let Some(counted) = &self.counted {
            input.view_angles.x = -89.0;
            if time >= 1.5 && time < self.duration() && counted.observed.len() < counted.target {
                input.buttons |= buttons::ATTACK;
            }
            return Some(input);
        }
        // Hold slots briefly; the sim detects each transition once.
        match name {
            "ak47_fire_reload" => {
                if during(1.5, 1.95) {
                    input.buttons |= buttons::ATTACK;
                }
                if during(2.75, 2.85) {
                    input.buttons |= buttons::RELOAD;
                }
            }
            "deploy_each_slot" => {
                input.weapon_slot = if time < 1.2 {
                    3
                } else if time < 2.4 {
                    2
                } else if time < 3.6 {
                    1
                } else {
                    0
                };
            }
            "run_gravel_knife" | "walk_gravel_knife" | "crouch_walk" => {
                if time < 0.1 {
                    input.weapon_slot = 3;
                }
                if during(0.8, 3.3) {
                    input.forward_move = 450.0;
                    if name == "walk_gravel_knife" {
                        input.buttons |= buttons::WALK;
                    }
                    if name == "crouch_walk" {
                        input.buttons |= buttons::DUCK;
                    }
                }
            }
            "jump_land" => {
                if during(0.8, 0.9) {
                    input.buttons |= buttons::JUMP;
                }
            }
            "impacts_ct_spawn" => {
                input.view_angles = if time < 1.52 {
                    glam::Vec3::new(20.0, 90.0, 0.0)
                } else {
                    glam::Vec3::new(30.0, 0.0, 0.0)
                };
                if during(0.8, 0.92) || during(1.52, 1.64) {
                    input.buttons |= buttons::ATTACK;
                }
            }
            _ => unreachable!(),
        }
        Some(input)
    }
}
