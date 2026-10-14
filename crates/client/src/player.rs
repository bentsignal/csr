//! Local player: input → [`sim::World`] at the world's tick rate (128 Hz by
//! default) → interpolated camera. The sim runs the bots and applies
//! loadouts itself; this module only turns devices into [`TickInput`]s.
//! While dead, the camera follows a living teammate (CS:GO's first-person
//! spectating); left click switches to the next one.

use crate::coords::{to_bevy, view_forward};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use movement::buttons;
use sim::{PlayerId, Purchase, TickInput};

/// CS:GO's `m_yaw` / `m_pitch`: degrees per mouse count.
const M_YAW: f32 = 0.022;

#[derive(Resource)]
pub struct SimWorld(pub sim::World);

#[derive(Resource)]
pub struct LocalPlayer {
    pub id: PlayerId,
    pub pitch: f32,
    pub yaw: f32,
    /// CS:GO `sensitivity`.
    pub sensitivity: f32,
    pub noclip: bool,
    pub noclip_pos: glam::Vec3,
    /// Eye position before and after the latest tick (Source axes).
    pub prev_eye: glam::Vec3,
    pub curr_eye: glam::Vec3,
    /// The pose shared by all cameras and the weapon this render frame.
    pub render_eye: glam::Vec3,
    pub render_angles: glam::Vec3,
    /// Buttons pressed since the last tick, so short taps are not lost.
    pub latched: u32,
    /// Inventory slot requested since the last tick (0 = none).
    pub pending_slot: u8,
    /// Purchases waiting to be sent, one per tick.
    pub pending_buys: Vec<Purchase>,
    pub pending_drop: bool,
    /// The teammate being watched while dead.
    pub spectating: Option<PlayerId>,
    was_alive: bool,
    remote_life: u64,
    /// Mouse buttons held when entering pointer capture, until their release.
    capture_attacks: u32,
}

impl LocalPlayer {
    pub fn new(id: PlayerId, eye: glam::Vec3, yaw: f32) -> Self {
        Self {
            id,
            pitch: 0.0,
            yaw,
            sensitivity: 2.0,
            noclip: false,
            noclip_pos: eye,
            prev_eye: eye,
            curr_eye: eye,
            render_eye: eye,
            render_angles: glam::Vec3::new(0.0, yaw, 0.0),
            latched: 0,
            pending_slot: 0,
            pending_buys: Vec::new(),
            pending_drop: false,
            spectating: None,
            was_alive: true,
            remote_life: 0,
            capture_attacks: 0,
        }
    }

    /// Whose eyes the camera is in (the local player unless spectating).
    pub fn view_target(&self) -> PlayerId {
        self.spectating.unwrap_or(self.id)
    }

    /// Discard camera life and unsent controls when an owned session stops
    /// being ready; actor ids and life epochs may repeat after reconnection.
    pub(crate) fn clear_remote_lifecycle(&mut self) {
        self.remote_life = 0;
        self.was_alive = false;
        self.spectating = None;
        self.latched = 0;
        self.pending_slot = 0;
        self.pending_drop = false;
        self.pending_buys.clear();
        self.capture_attacks = 0;
    }

    fn release_capture_attacks(&mut self, mouse: &ButtonInput<MouseButton>) {
        for (button, attack) in [(MouseButton::Left, buttons::ATTACK), (MouseButton::Right, buttons::ATTACK2)] {
            if !mouse.pressed(button) || mouse.just_released(button) {
                self.capture_attacks &= !attack;
            }
        }
    }

    /// Camera lifecycle comes from authority, including lost death snapshots.
    pub fn sync_remote(&mut self, world: &sim::World, id: PlayerId) {
        if self.id != id { self.id = id; self.remote_life = 0; }
        self.noclip = false;
        let Some(me) = world.player(id) else { return; };
        if me.alive {
            if self.remote_life != me.life_epoch || !self.was_alive {
                self.pitch = me.view_angles.x;
                self.yaw = me.view_angles.y;
                self.prev_eye = me.eye();
            } else { self.prev_eye = self.curr_eye; }
            self.curr_eye = me.eye();
            self.spectating = None;
        } else {
            if !self.spectating.and_then(|s| world.player(s)).is_some_and(|p| p.alive) {
                self.spectating = spectate_candidates(world, me).first().copied();
            }
            if let Some(target) = self.spectating.and_then(|s| world.player(s)) {
                self.prev_eye = self.curr_eye;
                self.curr_eye = target.eye();
            }
        }
        self.remote_life = me.life_epoch;
        self.was_alive = me.alive;
    }
}

#[derive(Component)]
pub struct PlayerCamera;

#[derive(Component)]
pub struct ViewModelCamera;

/// False in automated runs (smoke tests) so stray input cannot move the camera.
#[derive(Resource)]
pub struct InputEnabled(pub bool);
#[derive(Resource,Default)]
pub struct InputBlockFrames(pub u8);

fn captured(cursor: &CursorOptions) -> bool {
    cursor.grab_mode != CursorGrabMode::None
}

pub fn grab_cursor(
    mut local: ResMut<LocalPlayer>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    enabled: Res<InputEnabled>,
    menu: Res<crate::buymenu::BuyMenu>,
    front: Res<crate::menu::Menu>,
) {
    local.release_capture_attacks(&mouse);
    if !enabled.0 {
        return;
    }
    if menu.open || front.open() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
        return;
    }
    if (mouse.just_pressed(MouseButton::Left) || menu.just_closed) && cursor.grab_mode == CursorGrabMode::None {
        for (button, attack) in [(MouseButton::Left, buttons::ATTACK), (MouseButton::Right, buttons::ATTACK2)] {
            if mouse.pressed(button) { local.capture_attacks |= attack; }
        }
        local.latched &= !local.capture_attacks;
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

/// Living players the local player may watch: teammates, or anyone once
/// the whole team is dead.
fn spectate_candidates(world: &sim::World, me: &sim::Player) -> Vec<PlayerId> {
    let team: Vec<PlayerId> = world.players.iter().filter(|p| p.alive && p.team == me.team && p.id != me.id).map(|p| p.id).collect();
    if !team.is_empty() {
        return team;
    }
    world.players.iter().filter(|p| p.alive && p.id != me.id).map(|p| p.id).collect()
}

#[allow(clippy::too_many_arguments)]
pub fn look(
    mut local: ResMut<LocalPlayer>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    enabled: Res<InputEnabled>,
    world: Res<SimWorld>,
    menu: Res<crate::buymenu::BuyMenu>,
    front: Res<crate::menu::Menu>,
    settings: Res<crate::settings::Settings>,
    mut block: ResMut<InputBlockFrames>,
    mut inspect: ResMut<crate::tracers::InspectRequest>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    local.release_capture_attacks(&mouse);
    if front.open() {return;}
    if remote.as_ref().is_some_and(|r| r.status() != &netcode::remote_session::Status::Ready) { return; }
    if block.0 > 0 { if !mouse.pressed(MouseButton::Left) && !mouse.pressed(MouseButton::Right) {block.0=0;} return; }
    if !enabled.0 {
        return;
    }
    let grabbed = captured(&cursor) && !menu.open;
    if grabbed {
        let d = motion.delta;
        let zoom = if local.noclip { 1.0 } else {
            world.0.player(local.id).filter(|p| p.alive).and_then(|p| p.active_weapon()).map_or(1.0, |w| w.rendered_fov(world.0.time()) / 90.0)
        };
        local.yaw -= d.x * local.sensitivity * M_YAW * zoom;
        local.pitch = (local.pitch + d.y * (if settings.invert_y {-1.0} else {1.0}) * local.sensitivity * M_YAW * zoom).clamp(-89.0, 89.0);
        local.yaw = local.yaw.rem_euclid(360.0);
    }
    if remote.is_some() { local.noclip = false; }
    if remote.is_none() && !menu.open && keys.just_pressed(KeyCode::KeyV) {
        local.noclip = !local.noclip;
        local.noclip_pos = local.curr_eye;
    }
    let me = world.0.player(local.id);
    let alive = me.is_some_and(|p| p.alive);
    if alive && grabbed && keys.just_pressed(settings.binds[12]) {inspect.0=inspect.0.wrapping_add(1);}

    // Spectating: left click cycles through living teammates.
    if !alive && grabbed && mouse.just_pressed(MouseButton::Left) {
        if let Some(me) = me {
            let list = spectate_candidates(&world.0, me);
            if !list.is_empty() {
                let at = local.spectating.and_then(|s| list.iter().position(|&id| id == s)).map_or(0, |i| (i + 1) % list.len());
                local.spectating = Some(list[at]);
            }
        }
    }

    if menu.open {
        return;
    }

    let mut b = 0;
    if keys.pressed(settings.binds[4]) {
        b |= buttons::JUMP;
    }
    if keys.pressed(settings.binds[5]) {
        b |= buttons::DUCK;
    }
    if keys.pressed(settings.binds[6]) {
        b |= buttons::WALK;
    }
    if keys.just_pressed(settings.binds[8]) {
        b |= buttons::RELOAD;
    }
    // The shared sampler also guards held attacks after grab_cursor captures.
    if grabbed && alive && mouse.just_pressed(MouseButton::Left) {
        b |= buttons::ATTACK;
    }
    if grabbed && alive && mouse.just_pressed(MouseButton::Right) {
        b |= buttons::ATTACK2;
    }
    local.latched |= b;

    if !menu.open {
        for (key, slot) in [
            (KeyCode::Digit1, 1u8),
            (KeyCode::Digit2, 2),
            (KeyCode::Digit3, 3),
            (KeyCode::Digit4, 4),
            (KeyCode::Digit5, 5),
            (KeyCode::KeyQ, sim::SLOT_LAST),
        ] {
            if keys.just_pressed(key) {
                local.pending_slot = slot;
            }
        }
    }
    if keys.just_pressed(settings.binds[10]) {
        local.pending_drop = true;
    }
    if grabbed && scroll.delta.y != 0.0 && me.is_some_and(|p| !p.weapons.is_empty()) {
        // Installed defaults bind MWHEELUP invprev and MWHEELDOWN invnext.
        // Authority chooses the weapon; reducing this to a slot loses reverse
        // direction among grenades and at slot boundaries.
        local.pending_slot = if scroll.delta.y > 0.0 { sim::SLOT_PREVIOUS } else { sim::SLOT_NEXT };
    }
}

fn move_axes(keys: &ButtonInput<KeyCode>, settings: &crate::settings::Settings) -> (f32, f32) {
    let mut forward = 0.0;
    let mut side = 0.0;
    if keys.pressed(settings.binds[0]) {
        forward += 450.0;
    }
    if keys.pressed(settings.binds[1]) {
        forward -= 450.0;
    }
    if keys.pressed(settings.binds[3]) {
        side += 450.0;
    }
    if keys.pressed(settings.binds[2]) {
        side -= 450.0;
    }
    (forward, side)
}

pub struct Controls<'a> {
    pub keys: &'a ButtonInput<KeyCode>,
    pub mouse: &'a ButtonInput<MouseButton>,
    pub settings: &'a crate::settings::Settings,
    pub enabled: bool,
    pub blocked: bool,
    pub captured: bool,
    pub buy_menu: bool,
}

/// Complete device input; one-shot requests are consumed only for a sim tick.
pub fn sample_tick_input(local: &mut LocalPlayer, controls: &Controls<'_>) -> TickInput {
    let empty = ButtonInput::<KeyCode>::default();
    let keys = if controls.enabled && !controls.blocked && !controls.buy_menu {
        controls.keys
    } else { &empty };
    let (forward_move, side_move) = move_axes(keys, controls.settings);
    let mut held = 0;
    for (binding, button) in [(4, buttons::JUMP), (5, buttons::DUCK), (6, buttons::WALK), (7, buttons::USE)] {
        if keys.pressed(controls.settings.binds[binding]) { held |= button; }
    }
    if controls.enabled && !controls.blocked && controls.captured && !controls.buy_menu {
        if controls.mouse.pressed(MouseButton::Left) { held |= buttons::ATTACK; }
        if controls.mouse.pressed(MouseButton::Right) { held |= buttons::ATTACK2; }
    }
    TickInput {
        view_angles: glam::Vec3::new(local.pitch, local.yaw, 0.),
        forward_move, side_move,
        buttons: (held | std::mem::take(&mut local.latched)) & !local.capture_attacks,
        weapon_slot: std::mem::take(&mut local.pending_slot),
        buy: if local.pending_buys.is_empty() { Purchase::None } else { local.pending_buys.remove(0) },
        drop: std::mem::take(&mut local.pending_drop),
        ..Default::default()
    }
}

/// Runs in `FixedUpdate` at the sim tick rate.
#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut world: ResMut<SimWorld>,
    mut pending: ResMut<crate::effects::PendingEvents>,
    mut local: ResMut<LocalPlayer>,
    real_keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    enabled: Res<InputEnabled>,
    menu: Res<crate::buymenu::BuyMenu>,
    front: Res<crate::menu::Menu>,
    settings: Res<crate::settings::Settings>,
    block: Res<InputBlockFrames>,
    mut poses: ResMut<crate::interpolation::RenderPoses>,
    mut probe: ResMut<crate::sound_probe::SoundProbe>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    if remote.is_some() || front.open() {return;}
    let no_keys = ButtonInput::<KeyCode>::default();
    let keys: &ButtonInput<KeyCode> = if enabled.0 && !menu.open && block.0==0 { &real_keys } else { &no_keys };
    let mut input = sample_tick_input(&mut local, &Controls {
        keys: &real_keys, mouse: &mouse, settings: &settings, enabled: enabled.0,
        blocked: block.0 != 0, captured: captured(&cursor), buy_menu: menu.open,
    });
    // Smoke tests can hold the trigger to exercise the fire path.
    if !enabled.0 && std::env::var_os("CSR_AUTOFIRE").is_some() {
        input.buttons |= buttons::ATTACK;
    }
    if !enabled.0 && std::env::var_os("CSR_ATTACK2").is_some() { input.buttons |= buttons::ATTACK2; }
    // Screenshot-only phase proof through real TickInput. Each five-second
    // cycle holds from 0.5 to 2.5 seconds, then releases all attack buttons.
    if !enabled.0 {
        let mode=std::env::var("CSR_TEST_GRENADE_INPUT").ok();
        let bits=match mode.as_deref() {
            Some("left")=>Some(buttons::ATTACK),Some("right")=>Some(buttons::ATTACK2),
            Some("both")=>Some(buttons::ATTACK|buttons::ATTACK2),_=>None,
        };
        if let Some(bits)=bits {
            input.buttons&=!(buttons::ATTACK|buttons::ATTACK2);
            if (0.5..2.5).contains(&world.0.time().rem_euclid(5.)) {input.buttons|=bits;}
        }
    }
    if !enabled.0 && std::env::var_os("CSR_TEST_SWITCH").is_some() {
        if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("suppressor") {
            // Screenshot-only fixed input schedule, not gameplay timers. The
            // imported transition/deploy durations still decide readiness.
            input.buttons &= !(buttons::ATTACK | buttons::ATTACK2);
            let at = |seconds| world.0.tick == world.0.ticks(seconds);
            if at(0.5) || at(5.75) { input.buttons |= buttons::ATTACK; }
            if at(0.75) || at(7.5) { input.buttons |= buttons::ATTACK2; }
            if at(6.0) { input.weapon_slot = 3; }
            if at(6.25) {
                input.weapon_slot = std::env::var("CSR_TEST_WEAPON").ok()
                    .and_then(|name| world.0.catalog.get(&name).map(|def| sim::weapon::slot(def)))
                    .unwrap_or(1);
            }
            if input.buttons != 0 || input.weapon_slot != 0 {
                let weapon = world.0.player(local.id).and_then(|p| p.active_weapon());
                info!("suppressor fixture: tick={} time={:.5} buttons={} slot={} weapon={:?} attached={:?} transition={:?} next_attack={:?}",
                    world.0.tick, world.0.time(), input.buttons, input.weapon_slot,
                    weapon.map(|w| w.def.class_name.as_str()), weapon.map(|w| w.suppressor_attached),
                    weapon.and_then(|w| w.suppressor_transition), weapon.map(|w| w.next_attack));
            }
        } else if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("inventory-previous") {
            // Keep the ordinary practice inventory intact. Select Flash by its
            // installed item id, then ask authority for the previous weapon.
            input.buttons &= !(buttons::ATTACK | buttons::ATTACK2);
            if world.0.tick == world.0.ticks(0.5) {
                input.utility = world.0.catalog.get("weapon_flashbang").map_or(0, |def| def.def_index);
            }
            if world.0.tick == world.0.ticks(1.5) { input.weapon_slot = sim::SLOT_PREVIOUS; }
            if input.utility != 0 || input.weapon_slot != 0 {
                let player = world.0.player(local.id);
                info!("inventory wheel fixture: tick={} time={:.5} request_slot={} utility={} before={:?} inventory={:?}",
                    world.0.tick, world.0.time(), input.weapon_slot, input.utility,
                    player.and_then(|p| p.active_weapon()).map(|w| w.def.class_name.as_str()),
                    player.map(|p| p.weapons.iter().map(|w| w.def.class_name.as_str()).collect::<Vec<_>>()));
            }
        } else {
            match world.0.tick {
                64 => input.buttons |= buttons::ATTACK,
                80 => input.weapon_slot = 3,
                144 => input.weapon_slot = 1,
                _ => {}
            }
        }
    }

    let mut inputs = Vec::new();

    if local.noclip {
        let dt = world.0.dt();
        let (f, r, _) = movement::angle_vectors(glam::Vec3::new(local.pitch, local.yaw, 0.0));
        let speed = if keys.pressed(settings.binds[6]) { 1600.0 } else { 600.0 };
        let wish = f * (input.forward_move / 450.0) + r * (input.side_move / 450.0);
        local.prev_eye = local.noclip_pos;
        local.noclip_pos += wish * speed * dt;
        local.curr_eye = local.noclip_pos;
    } else {
        let input = probe.input(world.0.time()).unwrap_or(input);
        if probe.active() {
            local.pitch = input.view_angles.x;
            local.yaw = input.view_angles.y;
        }
        inputs.push((
            local.id,
            input,
        ));
    }
    let id = local.id;
    let producer_tick = world.0.tick;
    world.0.step(&inputs);
    probe.observe_step(&world.0, id, producer_tick);
    poses.capture(&world.0);
    pending.0.extend(world.0.events.iter().cloned());
    if local.noclip {
        return;
    }
    // First-person spectator smoke test, isolated from interactive controls.
    if !enabled.0 {
        if let Ok(index) = std::env::var("CSR_SPECTATE_BOT").and_then(|s| s.parse::<usize>().map_err(|_| std::env::VarError::NotPresent)) {
            if let Some(target) = world.0.players.iter().filter(|p| p.is_bot && p.alive).nth(index) {
                local.spectating = Some(target.id);
                local.prev_eye = local.curr_eye;
                local.curr_eye = target.eye();
                return;
            }
        }
    }
    let Some(me) = world.0.player(id) else { return };
    if me.alive {
        // Fresh spawn: face the way the spawn point faces.
        if !local.was_alive || world.0.events.iter().any(|e| matches!(e, sim::GameEvent::RoundStart { .. })) {
            local.yaw = me.view_angles.y;
            local.pitch = 0.0;
            local.prev_eye = me.eye();
        } else {
            local.prev_eye = local.curr_eye;
        }
        local.curr_eye = me.eye();
        local.spectating = None;
        local.was_alive = true;
        return;
    }
    local.was_alive = false;
    // Dead: keep watching someone alive.
    let valid = local.spectating.and_then(|s| world.0.player(s)).is_some_and(|p| p.alive);
    if !valid {
        let list = spectate_candidates(&world.0, me);
        local.spectating = list.first().copied();
        if let Some(t) = local.spectating.and_then(|s| world.0.player(s)) {
            local.curr_eye = t.eye();
        }
    }
    if let Some(t) = local.spectating.and_then(|s| world.0.player(s)) {
        local.prev_eye = local.curr_eye;
        local.curr_eye = t.eye();
    }
}

pub fn update_camera(
    mut local: ResMut<LocalPlayer>,
    world: Res<SimWorld>,
    time: Res<Time<Fixed>>,
    poses: Res<crate::interpolation::RenderPoses>,
    mut cams: Query<(&mut Transform, &mut Projection, Option<&ViewModelCamera>), With<PlayerCamera>>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    if remote.as_ref().is_some_and(|r| r.status() != &netcode::remote_session::Status::Ready) { return; }
    let alpha = time.overstep_fraction();
    let target = world.0.player(local.view_target()).map(|p| poses.sample(p, alpha));
    let eye = if local.noclip { local.prev_eye.lerp(local.curr_eye, alpha) } else { target.map_or(local.curr_eye, |p| p.eye) };
    let mut angles = if local.spectating.is_some() && !local.noclip {
        target.map_or(glam::Vec3::ZERO, |p| p.angles)
    } else {
        // Mouse look is sampled once per render frame before fixed ticks.
        // Keep local aiming current even when no tick runs; only the body/eye
        // and simulated recoil use the interpolated pose. TickInput remains
        // the sole way these view angles reach authoritative gameplay.
        glam::Vec3::new(local.pitch, local.yaw, 0.0)
    };
    if !local.noclip {
        if let Some(p) = target { angles.x += p.punch.x; angles.y += p.punch.y; }
    }
    local.render_eye = eye;
    local.render_angles = angles;
    let fwd = view_forward(angles.x, angles.y);
    let fov = if local.noclip { 90.0 } else {
        world.0.player(local.view_target()).filter(|p| p.alive).and_then(|p| p.active_weapon())
            .map_or(90.0, |w| w.rendered_fov(world.0.time() + alpha * world.0.dt()))
    };
    for (mut cam, mut projection, viewmodel) in &mut cams {
        if viewmodel.is_none() {
            if let Projection::Perspective(p) = &mut *projection { p.fov = crate::viewmodel::vertical_fov(fov); }
        }
        cam.translation = to_bevy(eye);
        cam.look_to(fwd, Vec3::Y);
    }
}

#[cfg(test)]
mod remote_input_tests {
    use super::*;
    #[derive(Resource, Default)]
    struct Sampled(TickInput);

    fn sample_after_capture(
        mut local: ResMut<LocalPlayer>,
        keys: Res<ButtonInput<KeyCode>>,
        mouse: Res<ButtonInput<MouseButton>>,
        settings: Res<crate::settings::Settings>,
        cursor: Single<&CursorOptions, With<PrimaryWindow>>,
        mut result: ResMut<Sampled>,
    ) {
        result.0 = sample_tick_input(&mut local, &Controls {
            keys: &keys, mouse: &mouse, settings: &settings, enabled: true,
            blocked: false, captured: captured(&cursor), buy_menu: false,
        });
    }

    fn capture_app() -> App {
        let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        let id = world.add_player("Local", sim::Team::CounterTerrorist);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(SimWorld(world))
            .insert_resource(LocalPlayer::new(id, glam::Vec3::ZERO, 0.))
            .insert_resource(InputEnabled(true))
            .init_resource::<InputBlockFrames>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<crate::settings::Settings>()
            .init_resource::<crate::menu::Menu>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .init_resource::<crate::tracers::InspectRequest>()
            .init_resource::<Sampled>()
            .add_systems(Update, (look, grab_cursor, sample_after_capture).chain());
        app.world_mut().spawn((PrimaryWindow, CursorOptions {
            grab_mode: CursorGrabMode::None, visible: true,
            ..Default::default()
        }));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
        app
    }

    #[test]
    fn mouse_aim_reaches_both_cameras_without_a_sim_tick_or_pose_interpolation() {
        let mut app = capture_app();
        app.init_resource::<crate::interpolation::RenderPoses>()
            .add_systems(Update, update_camera.after(sample_after_capture));
        for viewmodel in [false, true] {
            let entity = app.world_mut().spawn((PlayerCamera, Transform::default(),
                Projection::Perspective(PerspectiveProjection::default()))).id();
            if viewmodel { app.world_mut().entity_mut(entity).insert(ViewModelCamera); }
        }
        app.world_mut().query::<&mut CursorOptions>().single_mut(app.world_mut()).unwrap().grab_mode = CursorGrabMode::Locked;
        let before = app.world().resource::<SimWorld>().0.snapshot();
        let old_angles = app.world().resource::<SimWorld>().0.player(1).unwrap().view_angles;
        app.world_mut().resource_mut::<AccumulatedMouseMotion>().delta = Vec2::new(20., 10.);
        // Run just the production input/camera systems with alpha zero and
        // no sim step; the previous pose must not delay the latest look.
        app.world_mut().run_schedule(Update);
        let expected = glam::Vec3::new(0.44, 359.12, 0.);
        let local = app.world().resource::<LocalPlayer>();
        assert!((local.render_angles - expected).abs().max_element() < 0.0001);
        assert_eq!(app.world().resource::<Sampled>().0.view_angles, local.render_angles);
        let fwd = view_forward(local.render_angles.x, local.render_angles.y);
        for transform in app.world_mut().query_filtered::<&Transform, With<PlayerCamera>>().iter(app.world()) {
            assert!((transform.forward().as_vec3() - fwd).length() < 0.0001);
        }
        assert_eq!(app.world().resource::<SimWorld>().0.player(1).unwrap().view_angles, old_angles);
        assert_eq!(app.world().resource::<SimWorld>().0.snapshot(), before);
    }

    #[test]
    fn capture_press_stays_suppressed_while_held_without_blocking_movement() {
        let mut app = capture_app();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
        assert_eq!(app.world().resource::<Sampled>().0.forward_move, 450.);
        assert_eq!(app.world_mut().query::<&CursorOptions>().single(app.world()).unwrap().grab_mode, CursorGrabMode::Locked);
        for _ in 0..8 {
            app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
            // Even a previously queued mouse attack cannot bypass capture.
            app.world_mut().resource_mut::<LocalPlayer>().latched |= buttons::ATTACK;
            app.update();
            assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
            assert_eq!(app.world().resource::<Sampled>().0.forward_move, 450.);
        }
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
    }

    #[test]
    fn capture_guards_each_already_held_button_until_its_own_release() {
        let mut app = capture_app();
        for button in [MouseButton::Left, MouseButton::Right] {
            app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(button);
        }
        app.update();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & (buttons::ATTACK | buttons::ATTACK2), 0);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Right);
        // Bevy retains just_released alongside a new press in the same frame.
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
        app.update();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK2, 0);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.update();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK2, 0);
    }

    #[test]
    fn sampler_keeps_held_controls_and_consumes_complete_one_shot_requests_once() {
        let settings = crate::settings::Settings::default();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
        keys.press(KeyCode::KeyE);
        let mut mouse = ButtonInput::<MouseButton>::default();
        mouse.press(MouseButton::Left);
        let mut local = LocalPlayer::new(7, glam::Vec3::ZERO, 45.);
        local.pitch = 12.;
        local.latched = buttons::RELOAD;
        local.pending_slot = 3;
        local.pending_buys = vec![Purchase::Kevlar, Purchase::DefuseKit];
        local.pending_drop = true;
        let controls = Controls { keys: &keys, mouse: &mouse, settings: &settings,
            enabled: true, blocked: false, captured: true, buy_menu: false };
        let first = sample_tick_input(&mut local, &controls);
        assert_eq!(first.view_angles, glam::Vec3::new(12., 45., 0.));
        assert_eq!((first.forward_move, first.side_move), (450., 450.));
        assert_eq!(first.buttons, buttons::RELOAD | buttons::USE | buttons::ATTACK);
        assert_eq!(first.weapon_slot, 3);
        assert_eq!(first.buy, Purchase::Kevlar);
        assert!(first.drop);
        let second = sample_tick_input(&mut local, &controls);
        assert_eq!(second.buttons, buttons::USE | buttons::ATTACK);
        assert_eq!(second.weapon_slot, 0);
        assert_eq!(second.buy, Purchase::DefuseKit);
        assert!(!second.drop);
        assert!(local.pending_buys.is_empty());
        local.pending_buys.push(Purchase::KevlarHelmet);
        let during_buy = sample_tick_input(&mut local, &Controls { buy_menu: true, ..controls });
        assert_eq!(during_buy.forward_move, 0.);
        assert_eq!(during_buy.buttons, 0);
        assert_eq!(during_buy.buy, Purchase::KevlarHelmet);
    }
}
