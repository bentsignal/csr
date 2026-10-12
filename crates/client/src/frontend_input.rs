//! Balance input ownership when a native panel takes focus from the movie.
use scaleform::{Input, MouseButton};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Ownership {
    blocked: bool,
    keys: BTreeMap<String, u32>,
    buttons: Vec<MouseButton>,
}
impl Ownership {
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
            _ => {}
        }
    }
    pub fn transition(&mut self, blocked: bool, focused: bool) -> Vec<Input> {
        let mut events = Vec::new();
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
