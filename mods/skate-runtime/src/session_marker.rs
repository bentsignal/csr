// SPDX-License-Identifier: GPL-3.0-only
//! Original marker eligibility, geometry validator, UI hold and actor reset.
//! The headless host supplies explicit controller reports and simulation time.
#[path = "../upstream/session_marker/state.rs"]
mod state;
#[path = "../upstream/session_marker/validation.rs"]
mod validation;
use skate_core::physics::{board::BodyId, skeleton_animation_record::AnimationPartTransform};
#[derive(Clone, Copy)]
struct Marker {
    transform: AnimationPartTransform,
    on_board: bool,
    foot_forward: bool,
}
pub(crate) struct SessionMarker {
    pub ui:mechanics_link::MarkerUi,
    /// Refused Place Marker presses so far (the front-end error sound).
    pub refused: u64,
    saved: Option<Marker>,
    hold: state::Hold,
    validation: validation::Validation,
    previous: u16,
    ui_time: f64,
}
impl SessionMarker {
    pub fn load(
        root: &std::path::Path,
        saved: Option<mechanics_link::Checkpoint>,
    ) -> Result<Self, String> {
        Ok(Self {
            ui:Default::default(),
            saved: saved.map(|m| Marker {
                transform: m.transform,
                on_board: m.on_board,
                foot_forward: m.foot_forward,
            }),
            hold: Default::default(),
            validation: validation::Validation::load(root)?,
            previous: 0,
            refused: 0,
            ui_time: 0.,
        })
    }
    pub fn checkpoint(&self) -> Option<mechanics_link::Checkpoint> {
        self.saved.map(|m| mechanics_link::Checkpoint {
            transform: m.transform,
            on_board: m.on_board,
            foot_forward: m.foot_forward,
        })
    }
    pub fn update(
        &mut self,
        buttons: u16,
        dt: f32,
        physics: &crate::physics::GamePhysics,
        skater: &mut crate::physics::SkaterRuntime,
    ) {
        let modifier = buttons & 0x100 != 0;
        if modifier && !self.ui.visible {self.ui.event+=1;self.ui.action=1;}
        self.ui.visible=modifier;
        let set = modifier && buttons & 2 != 0 && self.previous & 2 == 0;
        let held = modifier && buttons & 1 != 0;
        self.previous = buttons;
        let p = &skater.player_input.physical;
        let on_board = p.state.category_12 != 500;
        let deck = physics.board.part_transforms()[BodyId::Deck.index()];
        let mut transform = skater.animated_skeleton.roots.animation_to_world;
        if on_board {
            for i in 0..3 {
                transform[i][..3].copy_from_slice(&deck.basis.columns[i]);
            }
            transform[3] = [
                deck.translation.x,
                deck.translation.y + 0.2,
                deck.translation.z,
                0.,
            ];
        }
        let processed = &skater.player_input.processed;
        transform = crate::physics::facing_from_visual(
            transform,
            processed.flags_2468,
            processed.flags_2476,
            on_board,
        );
        let state = p.state.state_16;
        let allowed = (p.state.category_12 == 100
            && p.collision.wheel_count_0 >= 2
            && state != 104
            && deck.basis.columns[1][1] > 0.71)
            || (p.state.category_12 == 500 && state == 500);
        if set && std::env::var_os("CSR_SKATE_DIAGNOSTICS").is_some() {
            eprintln!(
                "SKATE_MARKER state={state} category={} wheels={} deck_up={} surface={} eligible={allowed} geometry={} position={:?}",
                p.state.category_12,
                p.collision.wheel_count_0,
                deck.basis.columns[1][1],
                p.surface_default_mode,
                self.validation.check(physics.world(), transform[3]),
                transform[3]
            );
        }
        self.ui.can_place=allowed && p.surface_default_mode != 8 && self.validation.check(physics.world(),transform[3]);
        if set
            && allowed
            && p.surface_default_mode != 8
            && self.validation.check(physics.world(), transform[3])
        {
            self.saved = Some(Marker {
                transform,
                on_board,
                foot_forward: skater.animation.foot_forward(),
            });
            self.ui.event+=1;self.ui.action=2;
        } else if set {
            // Place Marker refused here: retail's marker error record (audio only).
            self.refused += 1;
        }
        let usable = self
            .saved
            .is_some_and(|m| self.validation.check(physics.world(), m.transform[3]))
            && !matches!(state, 104 | 502);
        let ready = p.state.flag_69 == 0
            && state != 702
            && !(physics.board_wiping_out && p.skeleton.teleport_pending_604 != 0)
            && skater.player_input.pending_teleport().is_none();
        self.ui.can_return=usable;
        let distance = self.saved.map_or(0., |m| {
            bevy::math::Vec3::from_slice(&skater.animated_skeleton.roots.animation_to_world[3][..3])
                .distance(bevy::math::Vec3::from_slice(&m.transform[3][..3]))
        });
        self.ui_time += f64::from(dt);
        while self.ui_time + 1e-10 >= 1. / 60. {
            self.ui_time -= 1. / 60.;
            let step=self.hold.update(held, usable, distance, ready);
            self.ui.progress=step.progress;
            if step.relocate {
                if let Some(target) = self.saved {
                    if skater
                        .player_input
                        .request_teleport(target.transform)
                        .is_ok()
                    {
                        skater.animation.restore_foot_forward(target.foot_forward);
                        self.ui.event+=1;self.ui.action=3;
                        skater
                            .teleport_state
                            .request_manual(target.transform, target.on_board);
                    } else {
                        self.hold.cancel();
                    }
                }
            }
        }
    }
}
