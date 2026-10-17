//! Schedule regressions using public Bevy input messages, without a native window.
//! Scoped expectations protect retained policy, not measured native zoom parity.
use super::*;
use bevy::input::mouse::{MouseButtonInput, MouseMotion};
use bevy::input::{ButtonState, InputPlugin};
use bevy::time::TimeUpdateStrategy;

#[derive(Resource, Default)]
struct TickSamples(Vec<TickInput>);

fn record_tick(
    mut local: ResMut<LocalPlayer>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    settings: Res<crate::settings::Settings>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    block: Res<InputBlockFrames>,
    menu: Res<crate::buymenu::BuyMenu>,
    mut samples: ResMut<TickSamples>,
) {
    samples.0.push(sample_tick_input(&mut local, &Controls {
        keys: &keys, mouse: &mouse, settings: &settings, enabled: true,
        blocked: block.0 != 0, captured: captured(&cursor), buy_menu: menu.open,
    }));
}

fn app() -> (App, Entity) {
    let mut simulation = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
    let id = simulation.add_player("Local", sim::Team::CounterTerrorist);
    simulation.player_mut(id).unwrap().alive = true;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin))
        .add_message::<WindowFocused>()
        .insert_resource(Time::<Fixed>::from_hz(128.))
        .insert_resource(TimeUpdateStrategy::FixedTimesteps(0))
        .insert_resource(SimWorld(simulation))
        .insert_resource(LocalPlayer::new(id, glam::Vec3::ZERO, 180.))
        .insert_resource(InputEnabled(true))
        .init_resource::<InputBlockFrames>()
        .init_resource::<crate::settings::Settings>()
        .init_resource::<crate::menu::Menu>()
        .init_resource::<crate::buymenu::BuyMenu>()
        .init_resource::<crate::tracers::InspectRequest>()
        .init_resource::<crate::interpolation::RenderPoses>()
        .init_resource::<TickSamples>()
        .add_systems(PreUpdate, focus_input.after(bevy::input::InputSystems))
        .add_systems(RunFixedMainLoop, (look, grab_cursor).chain()
            .in_set(bevy::app::RunFixedMainLoopSystems::BeforeFixedMainLoop))
        .add_systems(FixedUpdate, record_tick)
        .add_systems(Update, update_camera);
    let window = app.world_mut().spawn((PrimaryWindow, Window::default(), CursorOptions {
        grab_mode: CursorGrabMode::Locked, visible: false, ..default()
    })).id();
    for viewmodel in [false, true] {
        let mut camera = app.world_mut().spawn((PlayerCamera, Transform::default(),
            Projection::Perspective(PerspectiveProjection::default())));
        if viewmodel { camera.insert(ViewModelCamera); }
    }
    app.update(); // Initialize the real clock before advancing deterministic durations.
    (app, window)
}

fn frame(app: &mut App, counts: &[[f32; 2]], ticks: u32) {
    app.insert_resource(TimeUpdateStrategy::FixedTimesteps(ticks));
    for &[x, y] in counts {
        app.world_mut().write_message(MouseMotion { delta: Vec2::new(x, y) });
    }
    app.update();
}

fn button(app: &mut App, window: Entity, button: MouseButton, state: ButtonState) {
    app.world_mut().write_message(MouseButtonInput { button, state, window });
}

fn focus(app: &mut App, window: Entity, focused: bool) {
    app.world_mut().get_mut::<Window>(window).unwrap().focused = focused;
    app.world_mut().write_message(WindowFocused { window, focused });
}

fn assert_angles(app: &mut App, pitch: f32, yaw: f32) {
    let expected = glam::Vec3::new(pitch, yaw, 0.);
    let local = app.world().resource::<LocalPlayer>();
    assert!((local.render_angles - expected).abs().max_element() < 0.002);
    assert!((local.pitch - pitch).abs() < 0.002);
    assert!((local.yaw - yaw).abs() < 0.002);
    let forward = view_forward(pitch, yaw);
    for transform in app.world_mut().query_filtered::<&Transform, With<PlayerCamera>>().iter(app.world()) {
        assert!((transform.forward().as_vec3() - forward).length() < 0.0001);
    }
}

#[test]
fn raw_count_streams_survive_zero_and_multiple_ticks_without_replay_or_dpi_gain() {
    // Include reversals and cancellation within a frame; every partition totals
    // exactly 1000 horizontal / 200 vertical counts, below the pitch clamp.
    let stream = [[300., 60.], [-100., -20.], [800., 160.], [25., 5.], [-25., -5.]];
    for sensitivity in [1., 1.375, 2.71828] {
        for scale in [1., 1.5, 2.] {
            for partition in [1, 2, 5] {
                let (mut app, window) = app();
                app.world_mut().resource_mut::<crate::settings::Settings>().sensitivity = sensitivity;
                app.world_mut().get_mut::<Window>(window).unwrap().resolution.set_scale_factor_override(Some(scale));
                let mut total = Vec2::ZERO;
                let mut sample_count = 0;
                for (index, chunk) in stream.chunks(partition).enumerate() {
                    let ticks = [0, 3, 0, 1, 2][index];
                    total += chunk.iter().map(|&[x, y]| Vec2::new(x, y)).sum::<Vec2>();
                    frame(&mut app, chunk, ticks);
                    let expected = glam::Vec3::new(total.y * 0.022 * sensitivity,
                        180. - total.x * 0.022 * sensitivity, 0.);
                    assert_angles(&mut app, expected.x, expected.y);
                    let samples = &app.world().resource::<TickSamples>().0;
                    assert_eq!(samples.len(), sample_count + ticks as usize);
                    for sample in &samples[sample_count..] {
                        assert!((sample.view_angles - expected).abs().max_element() < 0.002);
                    }
                    sample_count = samples.len();
                }
                // A frame without device messages resets the accumulator. It
                // neither repeats the last batch nor loses look from zero-tick frames.
                frame(&mut app, &[], 3);
                assert_angles(&mut app, 4.4 * sensitivity, 180. - 22. * sensitivity);
                assert_eq!(app.world().resource::<AccumulatedMouseMotion>().delta, Vec2::ZERO);
                let last = app.world().resource::<TickSamples>().0.last().unwrap();
                assert!((last.view_angles.y - (180. - 22. * sensitivity)).abs() < 0.002);
            }
        }
    }
}

#[test]
fn retained_scoped_gain_and_noclip_use_counts_without_tick_multiplication() {
    // Independent fixture from the installed AWP schema's two FOVs. This tests
    // the existing FOV/90 policy; native zoom ratio/transition timing is UNVERIFIED.
    let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
        9 { name weapon_awp attributes { "zoom levels" 2 "zoom fov 1" 40 "zoom fov 2" 10 } }
    } }"#).unwrap();
    for (level, fov) in [(0, 90.), (1, 40.), (2, 10.)] {
        for noclip in [false, true] {
            for invert in [false, true] {
                let (mut app, _) = app();
                let mut weapon = sim::weapon::WeaponState::new(std::sync::Arc::new(catalog.get("weapon_awp").unwrap().clone()));
                weapon.zoom_level = level;
                weapon.zoom_from_fov = fov;
                app.world_mut().resource_mut::<SimWorld>().0.player_mut(1).unwrap().weapons = vec![weapon];
                app.world_mut().resource_mut::<LocalPlayer>().noclip = noclip;
                app.world_mut().resource_mut::<crate::settings::Settings>().invert_y = invert;
                frame(&mut app, &[[500., 100.]], 0);
                frame(&mut app, &[[500., 100.]], 4);
                let gain = if noclip { 1. } else { fov / 90. };
                assert_angles(&mut app, 4.4 * gain * if invert { -1. } else { 1. }, 180. - 22. * gain);
                assert_eq!(app.world().resource::<TickSamples>().0.len(), 4);
            }
        }
    }
}

#[test]
fn focus_loss_discards_pending_motion_and_capture_requires_release_to_rearm_side_buttons() {
    let (mut app, window) = app();
    {
        let mut settings = app.world_mut().resource_mut::<crate::settings::Settings>();
        settings.mouse_binds[3] = crate::settings::MouseAction::Attack;
        settings.mouse_binds[4] = crate::settings::MouseAction::Attack2;
    }
    {
        let mut local = app.world_mut().resource_mut::<LocalPlayer>();
        local.latched = buttons::JUMP;
        local.pointer_attacks = buttons::ATTACK;
        local.keyboard_attacks = buttons::ATTACK2;
        local.pending_slot = 2;
        local.pending_utility = 45;
        local.pending_drop = true;
    }
    focus(&mut app, window, false);
    button(&mut app, window, MouseButton::Back, ButtonState::Pressed);
    button(&mut app, window, MouseButton::Forward, ButtonState::Pressed);
    frame(&mut app, &[[1000., 200.]], 0);
    assert_angles(&mut app, 0., 180.);
    assert_eq!(app.world().resource::<LocalPlayer>().pending_slot, 0);
    assert_eq!(app.world().resource::<LocalPlayer>().pending_utility, 0);
    button(&mut app, window, MouseButton::Left, ButtonState::Pressed);
    frame(&mut app, &[[1000., 200.]], 2);
    assert_angles(&mut app, 0., 180.);
    assert_eq!(app.world().get::<CursorOptions>(window).unwrap().grab_mode, CursorGrabMode::None);
    for sample in &app.world().resource::<TickSamples>().0 {
        assert_eq!(sample.buttons, 0);
        assert_eq!(sample.weapon_slot, 0);
        assert!(!sample.drop);
    }
    focus(&mut app, window, true);
    frame(&mut app, &[[1000., 200.]], 0);
    assert_angles(&mut app, 0., 180.);
    button(&mut app, window, MouseButton::Left, ButtonState::Released);
    frame(&mut app, &[], 0);
    button(&mut app, window, MouseButton::Left, ButtonState::Pressed);
    frame(&mut app, &[[1000., 200.]], 1); // Capture click's motion and attacks stay in UI ownership.
    assert_angles(&mut app, 0., 180.);
    assert_eq!(app.world().get::<CursorOptions>(window).unwrap().grab_mode, CursorGrabMode::Locked);
    frame(&mut app, &[[1000., 200.]], 3);
    assert_angles(&mut app, 4.4, 158.);
    assert_eq!(app.world().resource::<TickSamples>().0.last().unwrap().buttons, 0);
    button(&mut app, window, MouseButton::Back, ButtonState::Released);
    frame(&mut app, &[], 0);
    button(&mut app, window, MouseButton::Back, ButtonState::Pressed);
    frame(&mut app, &[], 1);
    let buttons = app.world().resource::<TickSamples>().0.last().unwrap().buttons;
    assert_ne!(buttons & buttons::ATTACK, 0);
    assert_eq!(buttons & buttons::ATTACK2, 0);
}

#[test]
fn a_loss_and_regain_in_one_frame_releases_capture_and_rejects_the_ambiguous_batch() {
    let (mut app, window) = app();
    focus(&mut app, window, false);
    focus(&mut app, window, true);
    frame(&mut app, &[[1000., 200.]], 3);
    assert_angles(&mut app, 0., 180.);
    assert_eq!(app.world().get::<CursorOptions>(window).unwrap().grab_mode, CursorGrabMode::None);
    assert_eq!(app.world().resource::<AccumulatedMouseMotion>().delta, Vec2::ZERO);
}

#[test]
fn resume_blocks_discard_motion_without_replaying_it_or_extending_the_click_gate() {
    let (mut app, window) = app();
    app.world_mut().resource_mut::<InputBlockFrames>().0 = 2;
    button(&mut app, window, MouseButton::Left, ButtonState::Pressed);
    for remaining in [1, 0] {
        frame(&mut app, &[[1000., 200.]], 1);
        assert_angles(&mut app, 0., 180.);
        assert_eq!(app.world().resource::<InputBlockFrames>().0, remaining);
        assert_eq!(app.world().resource::<TickSamples>().0.last().unwrap().buttons, 0);
    }
    frame(&mut app, &[], 3);
    assert_angles(&mut app, 0., 180.);
    frame(&mut app, &[[1000., 200.]], 0);
    assert_angles(&mut app, 4.4, 158.);
    frame(&mut app, &[], 1);
    assert_angles(&mut app, 4.4, 158.);
    assert_eq!(app.world().resource::<TickSamples>().0.last().unwrap().buttons, 0);
    button(&mut app, window, MouseButton::Left, ButtonState::Released);
    frame(&mut app, &[], 0);
    button(&mut app, window, MouseButton::Left, ButtonState::Pressed);
    frame(&mut app, &[], 1);
    assert_ne!(app.world().resource::<TickSamples>().0.last().unwrap().buttons & buttons::ATTACK, 0);
}

#[test]
fn another_windows_focus_messages_do_not_steal_primary_mouse_counts() {
    let (mut app, _) = app();
    let other = app.world_mut().spawn(Window::default()).id();
    focus(&mut app, other, false);
    frame(&mut app, &[[1000., 200.]], 2);
    assert_angles(&mut app, 4.4, 158.);
}
