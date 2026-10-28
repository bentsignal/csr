//! Xbox 360/One/Series through Windows' standard XInput API. Skate receives
//! unmodified reports: its own stock deadzones and gesture recognizers run in
//! the provider, not a keyboard emulation layer.
use mechanics::link::Controller;
#[cfg(windows)]
pub fn poll() -> Controller {
    use windows_sys::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};
    for slot in 0..4 {
        let mut state = XINPUT_STATE::default();
        // XInput writes a fixed-size initialized output; failure is disconnected.
        if unsafe { XInputGetState(slot, &mut state) } == 0 {
            let g = state.Gamepad;
            return Controller {
                connected: true,
                buttons: g.wButtons,
                triggers: [g.bLeftTrigger, g.bRightTrigger],
                left: [g.sThumbLX, g.sThumbLY],
                right: [g.sThumbRX, g.sThumbRY],
            };
        }
    }
    Controller::default()
}
#[cfg(not(windows))]
pub fn poll() -> Controller {
    Controller::default()
}
pub const VIEW: u16 = 0x20;
pub fn apply(
    input: &mut sim::TickInput,
    report: Controller,
    previous: u16,
    skating: bool,
    dt: f32,
) {
    input.controller = report;
    input.toggle_mode |= report.connected && report.buttons & VIEW != 0 && previous & VIEW == 0;
    if !report.connected {
        return;
    }
    if skating {
        // D-pad weapon layer leaves sticks, face buttons, triggers and bumpers
        // with the stock Skate mappings. LB keeps D-pad commands in that layer.
        if report.buttons & 0x100 == 0 {
            if report.buttons & 8 != 0 {
                input.buttons |= movement::buttons::ATTACK;
            }
            if report.buttons & 4 != 0 {
                input.buttons |= movement::buttons::RELOAD;
            }
            if report.buttons & 1 != 0 && previous & 1 == 0 {
                input.weapon_slot = sim::SLOT_NEXT;
            }
            if report.buttons & 2 != 0 && previous & 2 == 0 {
                input.weapon_slot = sim::SLOT_PREVIOUS;
            }
        }
        return;
    }
    let axis = |v: i16| {
        let v = v as f32 / 32768.;
        if v.abs() < 0.23 {
            0.
        } else {
            v.signum() * (v.abs() - 0.23) / 0.77
        }
    };
    input.forward_move = (input.forward_move + 450. * axis(report.left[1])).clamp(-450., 450.);
    input.side_move = (input.side_move + 450. * axis(report.left[0])).clamp(-450., 450.);
    input.view_angles.y =
        (input.view_angles.y - axis(report.right[0]) * 180. * dt).rem_euclid(360.);
    input.view_angles.x =
        (input.view_angles.x - axis(report.right[1]) * 140. * dt).clamp(-89., 89.);
    for (mask, button) in [
        (0x1000, movement::buttons::JUMP),
        (0x2000, movement::buttons::DUCK),
        (0x4000, movement::buttons::RELOAD),
        (0x8000, movement::buttons::USE),
    ] {
        if report.buttons & mask != 0 {
            input.buttons |= button;
        }
    }
    if report.triggers[1] > 30 {
        input.buttons |= movement::buttons::ATTACK;
    }
    if report.triggers[0] > 30 {
        input.buttons |= movement::buttons::ATTACK2;
    }
    if report.buttons & 0x200 != 0 && previous & 0x200 == 0 {
        input.weapon_slot = sim::SLOT_NEXT;
    }
    if report.buttons & 0x100 != 0 && previous & 0x100 == 0 {
        input.weapon_slot = sim::SLOT_PREVIOUS;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn riding_combat_and_marker_modifier_share_dpad_without_stealing_tricks(){
        let report=Controller{connected:true,buttons:9,triggers:[200,255],right:[32767,-32768],..Default::default()};
        let mut input=sim::TickInput::default();apply(&mut input,report,0,true,1./128.);
        assert_ne!(input.buttons&movement::buttons::ATTACK,0);assert_eq!(input.weapon_slot,sim::SLOT_NEXT);assert_eq!(input.controller,report);
        let mut held=sim::TickInput::default();apply(&mut held,report,report.buttons,true,1./128.);assert_eq!(held.weapon_slot,0);
        let mut marker=sim::TickInput::default();apply(&mut marker,Controller{buttons:report.buttons|0x100,..report},0,true,1./128.);
        assert_eq!(marker.buttons,0);assert_eq!(marker.weapon_slot,0);
    }
    #[test]
    fn skate_reports_keep_flicks_triggers_and_face_buttons() {
        let report = Controller {
            connected: true,
            buttons: 0x1020,
            left: [10000, -10000],
            right: [-32768, 32767],
            triggers: [255, 64],
        };
        let mut input = sim::TickInput::default();
        apply(&mut input, report, 0, true, 1. / 128.);
        assert_eq!(input.controller, report);
        assert!(input.toggle_mode);
        assert_eq!(input.buttons, 0);
        assert_eq!(input.view_angles, glam::Vec3::ZERO);
        input.toggle_mode = false;
        apply(&mut input, report, report.buttons, true, 1. / 128.);
        assert!(!input.toggle_mode);
    }
    #[test]
    fn cs_and_disconnect_are_useful_without_changing_skate_controls() {
        let mut input = sim::TickInput::default();
        apply(
            &mut input,
            Controller {
                connected: true,
                left: [32767, 32767],
                right: [10000, 10000],
                triggers: [0, 255],
                ..Default::default()
            },
            0,
            false,
            1. / 128.,
        );
        assert!(input.forward_move > 440. && input.side_move > 440.);
        assert_ne!(input.buttons & movement::buttons::ATTACK, 0);
        let mut empty = sim::TickInput::default();
        apply(&mut empty, Controller::default(), 0, true, 1. / 128.);
        assert_eq!(empty, sim::TickInput::default());
    }
}
