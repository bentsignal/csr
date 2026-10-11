//! Scripted fidelity inputs, enabled only for screenshot runs. These follow
//! tools/fidelity/soundprobes.txt and use ordinary simulation input.
use bevy::prelude::*;
use movement::buttons;

#[derive(Resource, Default)]
pub struct SoundProbe {
    name: Option<String>,
}

impl SoundProbe {
    pub fn from_env(screenshot: bool) -> Self {
        Self {
            name: screenshot
                .then(|| std::env::var("CSR_SOUND_PROBE").ok())
                .flatten(),
        }
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
        p.view_angles = glam::Vec3::new(0.0, 180.0, 0.0);
    }

    pub fn input(&self, time: f32) -> Option<sim::TickInput> {
        let name = self.name.as_deref()?;
        let mut input = sim::TickInput {
            view_angles: glam::Vec3::new(0.0, 180.0, 0.0),
            ..Default::default()
        };
        let during = |start, end| (start..end).contains(&time);
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
