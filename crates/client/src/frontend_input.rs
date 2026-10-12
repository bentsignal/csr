//! Balance input ownership when a native panel takes focus from the movie.
use scaleform::{Input, MouseButton};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Ownership {
    blocked: bool,
    keys: BTreeMap<String, u32>,
    buttons: Vec<MouseButton>,
    cursor: Option<(f64, f64)>,
}
impl Ownership {
    /// Physical movie coordinates: a stationary render frame is not new input.
    /// Button events remain independent of whether this returns a movement.
    pub fn mouse_move(&mut self, x: f64, y: f64) -> Option<Input> {
        if self.blocked || !x.is_finite() || !y.is_finite() || self.cursor == Some((x, y)) {
            return None;
        }
        self.cursor = Some((x, y));
        Some(Input::MouseMove { x, y })
    }
    pub fn record(&mut self, input: &Input) {
        match input {
            Input::Key {
                name,
                vkey,
                down: true,
            } => {
                self.keys.insert(name.clone(), *vkey);
            }
            Input::Key {
                name, down: false, ..
            } => {
                self.keys.remove(name);
            }
            Input::MouseDown { button, .. } => {
                if !self.buttons.contains(button) {
                    self.buttons.push(*button);
                }
            }
            Input::MouseUp { button, .. } => self.buttons.retain(|held| held != button),
            Input::Focus(_) => self.cursor = None,
            _ => {}
        }
    }
    pub fn transition(&mut self, blocked: bool, focused: bool) -> Vec<Input> {
        let mut events = Vec::new();
        if blocked != self.blocked {
            self.cursor = None;
        }
        if blocked && !self.blocked {
            // Releasing outside the stage cannot activate an obscured button.
            events.push(Input::MouseMove { x: -1., y: -1. });
            for button in self.buttons.drain(..) {
                events.push(Input::MouseUp {
                    x: -1.,
                    y: -1.,
                    button,
                });
            }
            for (name, vkey) in std::mem::take(&mut self.keys) {
                events.push(Input::Key {
                    name,
                    vkey,
                    down: false,
                });
            }
            events.push(Input::MouseLeave);
            events.push(Input::Focus(false));
        } else if !blocked && self.blocked {
            events.push(Input::Focus(focused));
        }
        self.blocked = blocked;
        events
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stationary_frames_coalesce_but_physical_position_and_scale_changes_send() {
        let mut owner = Ownership::default();
        assert!(matches!(
            owner.mouse_move(20., 30.),
            Some(Input::MouseMove { x: 20., y: 30. })
        ));
        for _ in 0..10_000 {
            assert!(owner.mouse_move(20., 30.).is_none());
        }
        assert!(matches!(
            owner.mouse_move(21., 30.),
            Some(Input::MouseMove { x: 21., y: 30. })
        ));
        // Same logical cursor after a scale-factor change is new physical input.
        assert!(matches!(
            owner.mouse_move(21. * 2., 30. * 2.),
            Some(Input::MouseMove { x: 42., y: 60. })
        ));
        assert!(owner.mouse_move(42., 60.).is_none());
    }
    #[test]
    fn ownership_and_focus_restore_resend_stationary_hover_once() {
        let mut owner = Ownership::default();
        assert!(owner.mouse_move(20., 30.).is_some());
        owner.transition(true, false);
        assert!(owner.mouse_move(20., 30.).is_none());
        owner.transition(false, true);
        assert!(owner.mouse_move(20., 30.).is_some());
        assert!(owner.mouse_move(20., 30.).is_none());
        owner.record(&Input::Focus(true));
        assert!(owner.mouse_move(20., 30.).is_some());
        assert!(owner.mouse_move(20., 30.).is_none());
    }
    #[test]
    fn movement_coalescing_preserves_held_buttons_and_balanced_releases() {
        let mut owner = Ownership::default();
        owner.mouse_move(20., 30.);
        owner.record(&Input::MouseDown {
            x: 20.,
            y: 30.,
            button: MouseButton::Left,
        });
        for _ in 0..100 {
            assert!(owner.mouse_move(20., 30.).is_none());
        }
        assert_eq!(owner.buttons, vec![MouseButton::Left]);
        let release = owner.transition(true, true);
        assert_eq!(
            release
                .iter()
                .filter(|input| matches!(
                    input,
                    Input::MouseUp {
                        button: MouseButton::Left,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(owner.buttons.is_empty());
        owner.transition(false, true);
        assert!(owner.mouse_move(20., 30.).is_some());
        owner.record(&Input::MouseDown {
            x: 20.,
            y: 30.,
            button: MouseButton::Right,
        });
        assert!(owner.mouse_move(20., 30.).is_none());
        owner.record(&Input::MouseUp {
            x: 20.,
            y: 30.,
            button: MouseButton::Right,
        });
        assert!(
            !owner
                .transition(true, true)
                .iter()
                .any(|input| matches!(input, Input::MouseUp { .. }))
        );
    }
    #[test]
    fn overlay_releases_held_inputs_once_without_replaying_on_close() {
        let mut owner = Ownership::default();
        owner.record(&Input::Key {
            name: "KEY_RIGHT".into(),
            vkey: 39,
            down: true,
        });
        owner.record(&Input::MouseDown {
            x: 20.,
            y: 30.,
            button: MouseButton::Left,
        });
        let release = owner.transition(true, true);
        assert!(matches!(release[0], Input::MouseMove { x: -1., y: -1. }));
        assert!(matches!(
            release[1],
            Input::MouseUp {
                x: -1.,
                y: -1.,
                button: MouseButton::Left
            }
        ));
        assert!(
            matches!(&release[2], Input::Key { name, vkey: 39, down: false } if name == "KEY_RIGHT")
        );
        assert!(matches!(release[4], Input::Focus(false)));
        assert!(owner.transition(true, false).is_empty());
        assert!(matches!(
            owner.transition(false, true).as_slice(),
            [Input::Focus(true)]
        ));
        assert!(owner.transition(false, true).is_empty());
        assert!(owner.keys.is_empty() && owner.buttons.is_empty());
    }
    #[test]
    fn already_released_inputs_do_not_emit_duplicate_releases() {
        let mut owner = Ownership::default();
        for down in [true, false] {
            owner.record(&Input::Key {
                name: "KEY_A".into(),
                vkey: 65,
                down,
            });
        }
        owner.record(&Input::MouseDown {
            x: 1.,
            y: 1.,
            button: MouseButton::Right,
        });
        owner.record(&Input::MouseUp {
            x: 1.,
            y: 1.,
            button: MouseButton::Right,
        });
        assert!(
            !owner
                .transition(true, false)
                .iter()
                .any(|event| matches!(event, Input::MouseUp { .. } | Input::Key { .. }))
        );
        assert!(matches!(
            owner.transition(false, false).as_slice(),
            [Input::Focus(false)]
        ));
    }
}
