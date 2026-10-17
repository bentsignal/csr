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
    /// Fresh pointer edges retain their own capture admission when two physical
    /// buttons share an attack command.
    pub(crate) pointer_attacks: u32,
    /// Keyboard attack taps are independent of pointer-capture suppression.
    pub(crate) keyboard_attacks: u32,
    /// Inventory slot requested since the last tick (0 = none).
    pub pending_slot: u8,
    /// Device edges and explicit console actions have separate capture admission.
    pub(crate) pending_utility: u32,
    pub(crate) console_utility: u32,
    /// Purchases waiting to be sent, one per tick.
    pub pending_buys: Vec<Purchase>,
    pub pending_drop: bool,
    /// The teammate being watched while dead.
    pub spectating: Option<PlayerId>,
    was_alive: bool,
    remote_life: u64,
    /// Mouse buttons held when entering pointer capture, until their release.
    pub(crate) capture_mouse: u8,
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
            pointer_attacks: 0,
            keyboard_attacks: 0,
            pending_slot: 0,
            pending_utility: 0,
            console_utility: 0,
            pending_buys: Vec::new(),
            pending_drop: false,
            spectating: None,
            was_alive: true,
            remote_life: 0,
            capture_mouse: 0,
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
        self.pointer_attacks = 0;
        self.keyboard_attacks = 0;
        self.pending_slot = 0;
        self.clear_utility_requests();
        self.pending_drop = false;
        self.pending_buys.clear();
        self.capture_mouse = 0;
    }

    pub(crate) fn clear_utility_requests(&mut self) {
        self.pending_utility = 0;
        self.console_utility = 0;
    }

    fn release_capture_attacks(&mut self, mouse: &ButtonInput<MouseButton>) {
        for (index, button) in crate::settings::MOUSE_BUTTONS.iter().enumerate() {
            if !mouse.pressed(*button) || mouse.just_released(*button) {
                self.capture_mouse &= !(1 << index);
            }
        }
    }
    fn capture_held_mouse(&mut self, mouse: &ButtonInput<MouseButton>, settings: &crate::settings::Settings) {
        for (index, button) in crate::settings::MOUSE_BUTTONS.iter().enumerate() {
            if mouse.pressed(*button) { self.capture_mouse |= 1 << index; }
        }
        self.latched &= !self.capture_tick_mask(settings);
        self.pointer_attacks &= !self.capture_attack_mask(settings);
    }
    fn capture_attack_mask(&self, settings: &crate::settings::Settings) -> u32 {
        settings.mouse_binds.iter().enumerate().filter(|(index, _)| self.capture_mouse & (1 << index) != 0)
            .fold(0, |mask, (_, action)| mask | match action {
                crate::settings::MouseAction::Attack => buttons::ATTACK,
                crate::settings::MouseAction::Attack2 => buttons::ATTACK2,
                _ => 0,
            })
    }
    fn capture_tick_mask(&self, settings: &crate::settings::Settings) -> u32 {
        settings.mouse_binds.iter().enumerate().filter(|(index, _)| self.capture_mouse & (1 << index) != 0)
            .fold(self.capture_attack_mask(settings), |mask, (_, action)| mask | match action {
                crate::settings::MouseAction::Action(4) => buttons::JUMP,
                crate::settings::MouseAction::Action(5) => buttons::DUCK,
                crate::settings::MouseAction::Action(6) => buttons::WALK,
                crate::settings::MouseAction::Action(7) => buttons::USE,
                crate::settings::MouseAction::Action(8) => buttons::RELOAD,
                _ => 0,
            })
    }

    /// Camera lifecycle comes from authority, including lost death snapshots.
    pub fn sync_remote(&mut self, world: &sim::World, id: PlayerId) {
        if self.id != id { self.id = id; self.remote_life = 0; }
        self.noclip = false;
        let Some(me) = world.player(id) else { return; };
        if me.alive {
            if self.remote_life != me.life_epoch || !self.was_alive {
                self.clear_utility_requests();
                self.pitch = me.view_angles.x;
                self.yaw = me.view_angles.y;
                self.prev_eye = me.eye();
            } else { self.prev_eye = self.curr_eye; }
            self.curr_eye = me.eye();
            self.spectating = None;
        } else {
            self.clear_utility_requests();
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

/// Resolve names against the selected catalog and actual carried ownership.
/// The wire already carries this definition index; no client grants inventory.
pub(crate) fn carried_utility(world: &sim::World, id: PlayerId, name: &str) -> Option<u32> {
    let player = world.player(id).filter(|p| p.alive)?;
    let def = world.catalog.get(name)?;
    (def.class == weapons::WeaponClass::Grenade && def.def_index != 0
        && player.weapons.iter().any(|w| w.def.id == def.id && w.def.class == weapons::WeaponClass::Grenade))
        .then_some(def.def_index)
}

#[derive(Component)]
pub struct PlayerCamera;

/// Main playable-world view, excluding the sky and viewmodel cameras.
/// Metadata only: camera updates/rendering continue to use their existing roles.
#[derive(Component)]
pub struct WorldPlayerCamera;

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
    settings: Res<crate::settings::Settings>,
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
        local.capture_held_mouse(&mouse, &settings);
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
    if block.0 > 0 {
        // Resume/console close may have already locked the cursor. Move held
        // UI presses into the existing release-to-rearm guard even in that
        // path, then expire this finite transition block independently of them.
        local.capture_held_mouse(&mouse, &settings);
        block.0 = if !mouse.pressed(MouseButton::Left) && !mouse.pressed(MouseButton::Right) {
            0
        } else { block.0 - 1 };
        return;
    }
    if !enabled.0 {
        return;
    }
    let grabbed = captured(&cursor) && !menu.open;
    let blocked_mouse = if grabbed { local.capture_mouse } else { u8::MAX };
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
    if alive && grabbed && settings.action_just_pressed(12, &keys, &mouse, blocked_mouse) {inspect.start();}

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
    if settings.action_just_pressed(4, &keys, &mouse, blocked_mouse) {
        b |= buttons::JUMP;
    }
    if settings.action_just_pressed(5, &keys, &mouse, blocked_mouse) {
        b |= buttons::DUCK;
    }
    if settings.action_just_pressed(6, &keys, &mouse, blocked_mouse) {
        b |= buttons::WALK;
    }
    if settings.action_just_pressed(7, &keys, &mouse, blocked_mouse) {
        b |= buttons::USE;
    }
    if settings.action_just_pressed(8, &keys, &mouse, blocked_mouse) {
        b |= buttons::RELOAD;
        if me.and_then(|p| p.active_weapon()).is_some_and(|w| w.def.class == weapons::WeaponClass::Knife) {
            inspect.cancel();
        }
    }
    // The shared sampler also guards held attacks after grab_cursor captures.
    if grabbed && alive && settings.mouse_just_pressed(crate::settings::MouseAction::Attack, &mouse, blocked_mouse) {
        local.pointer_attacks |= buttons::ATTACK;
    }
    if grabbed && alive && settings.mouse_just_pressed(crate::settings::MouseAction::Attack2, &mouse, blocked_mouse) {
        local.pointer_attacks |= buttons::ATTACK2;
    }
    if grabbed && alive {
        for (index, name) in crate::settings::GRENADE_NAMES.iter().enumerate() {
            if settings.action_just_pressed(crate::settings::KEY_GRENADE_FIRST + index, &keys, &mouse, blocked_mouse) {
                local.pending_utility = carried_utility(&world.0, local.id, name).unwrap_or(0);
            }
        }
        for (binding, button) in [(crate::settings::KEY_ATTACK, buttons::ATTACK), (crate::settings::KEY_ATTACK2, buttons::ATTACK2)] {
            if settings.binds[binding].is_some_and(|key| keys.just_pressed(key)) {
                local.keyboard_attacks |= button;
            }
        }
    }
    local.latched |= b;

    if grabbed && alive {
        for (index, slot) in crate::settings::SELECTION_SLOTS.iter().copied().enumerate() {
            if settings.action_just_pressed(crate::settings::KEY_SELECTION_FIRST + index, &keys, &mouse, blocked_mouse) {
                local.pending_slot = slot;
            }
        }
    }
    if settings.action_just_pressed(10, &keys, &mouse, blocked_mouse) {
        local.pending_drop = true;
    }
    if grabbed && alive {
        for (action, slot) in [
            (crate::settings::MouseAction::Previous, sim::SLOT_PREVIOUS),
            (crate::settings::MouseAction::Next, sim::SLOT_NEXT),
            (crate::settings::MouseAction::Last, sim::SLOT_LAST),
        ] {
            if settings.mouse_just_pressed(action, &mouse, blocked_mouse) { local.pending_slot = slot; }
        }
    }
    if grabbed && scroll.delta.y != 0.0 {
        // Installed defaults bind MWHEELUP invprev and MWHEELDOWN invnext.
        // Authority chooses the weapon; reducing this to a slot loses reverse
        // direction among grenades and at slot boundaries.
        let action = if scroll.delta.y > 0.0 { settings.wheel_up } else { settings.wheel_down };
        match action {
            crate::settings::WheelAction::Jump => local.latched |= buttons::JUMP,
            crate::settings::WheelAction::Previous => local.pending_slot = sim::SLOT_PREVIOUS,
            crate::settings::WheelAction::Next => local.pending_slot = sim::SLOT_NEXT,
            crate::settings::WheelAction::None => {}
        }
    }
}

fn move_axes(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>, settings: &crate::settings::Settings, blocked: u8) -> (f32, f32) {
    let mut forward = 0.0;
    let mut side = 0.0;
    if settings.action_pressed(0, keys, mouse, blocked) {
        forward += 450.0;
    }
    if settings.action_pressed(1, keys, mouse, blocked) {
        forward -= 450.0;
    }
    if settings.action_pressed(3, keys, mouse, blocked) {
        side += 450.0;
    }
    if settings.action_pressed(2, keys, mouse, blocked) {
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
    let attack_allowed = controls.enabled && !controls.blocked && controls.captured && !controls.buy_menu;
    let blocked_mouse = if attack_allowed { local.capture_mouse } else { u8::MAX };
    let (forward_move, side_move) = move_axes(keys, controls.mouse, controls.settings, blocked_mouse);
    let mut held = 0;
    for (binding, button) in [(4, buttons::JUMP), (5, buttons::DUCK), (6, buttons::WALK), (7, buttons::USE)] {
        if controls.settings.action_pressed(binding, keys, controls.mouse, blocked_mouse) { held |= button; }
    }
    if attack_allowed {
        if controls.settings.mouse_pressed(crate::settings::MouseAction::Attack, controls.mouse, blocked_mouse) { held |= buttons::ATTACK; }
        if controls.settings.mouse_pressed(crate::settings::MouseAction::Attack2, controls.mouse, blocked_mouse) { held |= buttons::ATTACK2; }
        for (binding, button) in [(crate::settings::KEY_ATTACK, buttons::ATTACK), (crate::settings::KEY_ATTACK2, buttons::ATTACK2)] {
            if controls.settings.binds[binding].is_some_and(|key| keys.pressed(key)) { held |= button; }
        }
    }
    let keyboard_attacks = std::mem::take(&mut local.keyboard_attacks);
    let pointer_attacks = std::mem::take(&mut local.pointer_attacks);
    let device_utility = std::mem::take(&mut local.pending_utility);
    let console_utility = std::mem::take(&mut local.console_utility);
    TickInput {
        view_angles: glam::Vec3::new(local.pitch, local.yaw, 0.),
        forward_move, side_move,
        buttons: held | (if attack_allowed { pointer_attacks | keyboard_attacks } else { 0 })
            | (std::mem::take(&mut local.latched) & !local.capture_attack_mask(controls.settings)),
        weapon_slot: std::mem::take(&mut local.pending_slot),
        // An explicit console command is not a device edge. It can select
        // while the console owns focus, but still travels through authority.
        utility: if controls.enabled && !controls.buy_menu && console_utility != 0 {
            console_utility
        } else if attack_allowed { device_utility } else { 0 },
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
    let _frame_scope = crate::input_latency::frame_scope("simulation");
    if remote.is_some() || front.open() {return;}
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
        } else if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("burst-release") {
            // Screenshot-only ordinary inputs: empty the declared magazine,
            // hold the final burst's trigger, then release to exercise idle
            // reload. Never mutate the weapon state to manufacture a result.
            input.buttons &= !(buttons::ATTACK | buttons::ATTACK2);
            if world.0.tick == world.0.ticks(0.5) { input.buttons |= buttons::ATTACK2; }
            if let Some(player) = world.0.player(local.id) {
                if let Some(gun) = player.active_weapon().filter(|gun| gun.burst_profile().is_some()) {
                    let now = world.0.time();
                    let last = 1.5 + ((gun.def.clip_size - 1) / 3) as f32 * 0.85;
                    if (1.5..last).contains(&now) && (now - 1.5).rem_euclid(0.85) < 0.2
                        || (last..last + 4.5).contains(&now) { input.buttons |= buttons::ATTACK; }
                    if input.buttons != player.last_input.buttons {
                        info!("burst release fixture: tick={} time={now:.5} buttons={} weapon={} mode={} clip={} reserve={} shots={} reload={:?}",
                            world.0.tick, input.buttons, gun.def.class_name, gun.burst_mode, gun.clip, gun.reserve, player.shots, gun.reload_end);
                    }
                }
            }
        } else if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("empty-trigger") {
            // Screenshot-only ordinary rifle inputs: exhaust declared ammo
            // with released-trigger reloads, then short/held depleted presses.
            // No direct ammo/state writes manufacture the empty condition.
            input.buttons &= !(buttons::ATTACK | buttons::ATTACK2);
            let now = world.0.time();
            if let Some(gun) = world.0.player(local.id).and_then(|p| p.active_weapon())
                .filter(|gun| gun.def.full_auto && gun.def.clip_size > 0) {
                let magazines = 1 + (gun.def.reserve_ammo.max(0) as u32)
                    .div_ceil(gun.def.clip_size as u32);
                let end = 1.5 + (magazines - 1) as f32 * 6. + 3.;
                if ((1.5..end).contains(&now) && (now - 1.5).rem_euclid(6.) < 3.)
                    || (end + 1.0..end + 1.15).contains(&now)
                    || (end + 1.7..end + 3.2).contains(&now) {
                    input.buttons |= buttons::ATTACK;
                }
            }
            if let Some(player) = world.0.player(local.id) {
                if input.buttons != player.last_input.buttons {
                    if let Some(gun) = player.active_weapon() {
                        info!("empty trigger fixture: tick={} time={now:.5} buttons={} weapon={} clip={} reserve={} shots={} reload={:?}",
                            world.0.tick, input.buttons, gun.def.class_name, gun.clip, gun.reserve, player.shots, gun.reload_end);
                    }
                }
            }
        } else if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("automatic-release") {
            // Screenshot-only normal magazine: keep the trigger held after
            // its declared shots, then release. The sim alone owns ammo,
            // firing deadlines and the subsequent automatic reload.
            input.buttons &= !(buttons::ATTACK | buttons::ATTACK2);
            if let Some(player) = world.0.player(local.id) {
                if let Some(gun) = player.active_weapon().filter(|gun| gun.def.full_auto && gun.def.clip_size > 0) {
                    let now = world.0.time();
                    let release = 1.5 + (gun.def.clip_size - 1) as f32 * gun.def.cycle_time + 4.0;
                    if (1.5..release).contains(&now) { input.buttons |= buttons::ATTACK; }
                    if input.buttons != player.last_input.buttons {
                        info!("automatic release fixture: tick={} time={now:.5} buttons={} weapon={} mode={} clip={} reserve={} shots={} reload={:?}",
                            world.0.tick, input.buttons, gun.def.class_name, gun.burst_mode, gun.clip, gun.reserve, player.shots, gun.reload_end);
                    }
                }
            }
        } else if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("viewmodel-settling") {
            // Finite screenshot-only ordinary movement. Authority owns the
            // real ground velocity; no pose or movement-state fabrication.
            input.buttons = 0; input.side_move = 0.; input.forward_move = 0.;
            local.pitch = 0.; local.yaw = 0.; input.view_angles = glam::Vec3::ZERO;
            let case = std::env::var("CSR_VIEWMODEL_CASE").unwrap_or_default();
            if case != "baseline" && (1.5..3.5).contains(&world.0.time()) {
                input.forward_move = 450.;
                if case == "walk" { input.buttons |= buttons::WALK; }
            }
        } else if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("viewmodel-look") {
            // Finite screenshot fixture: ordinary TickInput and the same local
            // camera angles, with no ammo/weapon/animation-state fabrication.
            input.buttons = 0; input.forward_move = 0.; input.side_move = 0.;
            let changed = (1.5..3.).contains(&world.0.time());
            let angles = match std::env::var("CSR_VIEWMODEL_CASE").as_deref() {
                Ok("yaw10") if changed => glam::Vec3::new(0., 10., 0.),
                Ok("yaw5") if changed => glam::Vec3::new(0., 5., 0.),
                Ok("pitch10") if changed => glam::Vec3::new(10., 0., 0.),
                _ => glam::Vec3::ZERO,
            };
            local.pitch = angles.x; local.yaw = angles.y; input.view_angles = angles;
        } else if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("buy-close") {
            // Screenshot-only purchase through normal simulation validation.
            // Competitive spawn money, armor and buy-zone checks stay intact.
            if world.0.tick == world.0.ticks(0.5) {
                input.buy = Purchase::Kevlar;
                let player = world.0.player(local.id);
                info!("buy-close fixture: tick={} time={:.5} buy={:?} menu_open={} money={:?} armor={:?} buy_zone={:?}",
                    world.0.tick, world.0.time(), input.buy, menu.open,
                    player.map(|p| p.money), player.map(|p| p.armor), player.map(|p| p.in_buy_zone));
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
        let speed = if input.buttons & buttons::WALK != 0 { 1600.0 } else { 600.0 };
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
    local.clear_utility_requests();
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
        block: Res<InputBlockFrames>,
        mut result: ResMut<Sampled>,
    ) {
        result.0 = sample_tick_input(&mut local, &Controls {
            keys: &keys, mouse: &mouse, settings: &settings, enabled: true,
            blocked: block.0 != 0, captured: captured(&cursor), buy_menu: false,
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
    fn persisted_weapon_selection_replaces_fixed_keys_and_preserves_short_taps() {
        use crate::settings::{Settings, KEY_SELECTION_FIRST};
        let mut settings = Settings::parse("volume=0.4\nbind13=KeyR\n");
        assert_eq!(settings.binds[KEY_SELECTION_FIRST], Some(KeyCode::KeyQ));
        assert_eq!(settings.binds[KEY_SELECTION_FIRST + 1], Some(KeyCode::Digit1));
        settings.rebind(KEY_SELECTION_FIRST, KeyCode::KeyX);
        settings.unbind_key(KeyCode::Digit1);
        settings.rebind(KEY_SELECTION_FIRST + 1, KeyCode::KeyZ);
        settings.rebind(KEY_SELECTION_FIRST + 6, KeyCode::KeyC);
        settings.rebind(KEY_SELECTION_FIRST + 7, KeyCode::Digit2);
        let settings = Settings::parse(&settings.encode());
        assert_eq!(settings.binds[crate::settings::KEY_ATTACK], Some(KeyCode::KeyR));
        assert_eq!(settings.volume, 0.4);
        let mut app = capture_app();
        app.insert_resource(settings.clone());
        app.world_mut().query::<&mut CursorOptions>().single_mut(app.world_mut()).unwrap().grab_mode = CursorGrabMode::Locked;
        for key in [KeyCode::KeyQ, KeyCode::Digit1] {
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
            app.update();
            assert_eq!(app.world().resource::<Sampled>().0.weapon_slot, 0,
                "old keys cannot fall through after unbinding or moving their actions");
        }
        for (key, slot) in [
            (KeyCode::KeyX, sim::SLOT_LAST), (KeyCode::KeyZ, 1),
            (KeyCode::KeyC, sim::SLOT_NEXT), (KeyCode::Digit2, sim::SLOT_PREVIOUS),
            (KeyCode::Digit4, 4), (KeyCode::Digit5, 5),
        ] {
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
            app.world_mut().run_system_cached(look).unwrap();
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(key);
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
            app.world_mut().run_system_cached(sample_after_capture).unwrap();
            assert_eq!(app.world().resource::<Sampled>().0.weapon_slot, slot);
            app.world_mut().run_system_cached(sample_after_capture).unwrap();
            assert_eq!(app.world().resource::<Sampled>().0.weapon_slot, 0, "one request per tap");
        }
        for (enabled, blocked, captured, buy_menu) in [
            (false, false, true, false), (true, true, true, false),
            (true, false, false, false), (true, false, true, true),
        ] {
            app.world_mut().resource_mut::<InputEnabled>().0 = enabled;
            app.world_mut().resource_mut::<InputBlockFrames>().0 = if blocked { 2 } else { 0 };
            app.world_mut().resource_mut::<crate::buymenu::BuyMenu>().open = buy_menu;
            app.world_mut().query::<&mut CursorOptions>().single_mut(app.world_mut()).unwrap().grab_mode =
                if captured { CursorGrabMode::Locked } else { CursorGrabMode::None };
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Digit4);
            app.world_mut().run_system_cached(look).unwrap();
            app.world_mut().run_system_cached(sample_after_capture).unwrap();
            assert_eq!(app.world().resource::<Sampled>().0.weapon_slot, 0,
                "blocked device edge never enters the one-shot selection queue");
        }
    }

    #[test]
    fn named_grenade_key_tap_uses_catalog_identity_and_independent_capture_guards() {
        let catalog = std::sync::Arc::new(weapons::Catalog::from_items_game(r#"
            items_game { items {
                7342 { name weapon_smokegrenade attributes { "primary default clip size" 1 } }
                7343 { name weapon_flashbang attributes { "primary default clip size" 1 } }
            } }"#).unwrap());
        let mut simulation = sim::World::with_catalog(
            std::sync::Arc::new(collision::CollisionWorld::new()), catalog.clone());
        let id = simulation.add_player("Local", sim::Team::CounterTerrorist);
        let player = simulation.player_mut(id).unwrap();
        player.alive = true;
        player.weapons = ["weapon_flashbang", "weapon_smokegrenade"].map(|name|
            sim::weapon::WeaponState::new(std::sync::Arc::new(catalog.get(name).unwrap().clone()))).into();
        player.active = 0;
        let mut app = capture_app();
        app.insert_resource(SimWorld(simulation));
        let smoke = crate::settings::KEY_GRENADE_FIRST + 2;
        let mut settings = crate::settings::Settings::default();
        settings.rebind(smoke, KeyCode::KeyX);
        settings.mouse_binds[0] = crate::settings::MouseAction::Action(smoke);
        app.insert_resource(crate::settings::Settings::parse(&settings.encode()));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Sampled>().0.utility, 0, "capture click must not select");
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyX);
        app.world_mut().run_system_cached(look).unwrap();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyX);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        let input = app.world().resource::<Sampled>().0;
        assert_eq!(input.utility, 7342, "catalog item index, not the stock smoke index");
        let mut simulation = app.world_mut().resource_mut::<SimWorld>();
        assert_eq!(simulation.0.player(id).unwrap().active, 0, "sampling cannot select directly");
        simulation.0.step(&[(id, input)]);
        assert_eq!(simulation.0.player(id).unwrap().active_weapon().unwrap().def.class_name, "weapon_smokegrenade");
        drop(simulation);
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        assert_eq!(app.world().resource::<Sampled>().0.utility, 0, "short tap consumed exactly once");
        let keys = ButtonInput::default();
        let mouse = ButtonInput::default();
        let settings = crate::settings::Settings::default();
        let mut local = app.world_mut().resource_mut::<LocalPlayer>();
        for (enabled, blocked, captured, buy_menu) in [
            (false, false, true, false), (true, true, true, false),
            (true, false, false, false), (true, false, true, true),
        ] {
            local.pending_utility = 7342;
            assert_eq!(sample_tick_input(&mut local, &Controls {
                keys: &keys, mouse: &mouse, settings: &settings, enabled, blocked, captured, buy_menu,
            }).utility, 0);
            assert_eq!(local.pending_utility, 0, "blocked device requests do not leak later");
        }
    }

    #[test]
    fn persisted_keyboard_attacks_keep_taps_independent_of_capture_mouse_and_ui_guards() {
        use crate::settings::{Settings, KEY_ATTACK, KEY_ATTACK2};
        let mut settings = Settings::parse("volume=0.4\nbind0=KeyW\n");
        assert_eq!(settings.binds[KEY_ATTACK], None, "old configs keep mouse-only defaults");
        settings.rebind(KEY_ATTACK, KeyCode::KeyX);
        settings.rebind(KEY_ATTACK2, KeyCode::KeyC);
        let settings = Settings::parse(&settings.encode());
        let mut app = capture_app();
        app.insert_resource(settings.clone());
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update(); // Mouse1 remains held but cannot shoot after capturing.
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyX);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyC);
        app.world_mut().run_system_cached(look).unwrap();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyX);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        let attacks = buttons::ATTACK | buttons::ATTACK2;
        assert_eq!(app.world().resource::<Sampled>().0.buttons & attacks, attacks);
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & attacks, buttons::ATTACK2,
            "the X tap is consumed once while held C continues secondary attack");

        let keys = app.world().resource::<ButtonInput<KeyCode>>().clone();
        let mouse = app.world().resource::<ButtonInput<MouseButton>>().clone();
        for (enabled, blocked, captured, buy_menu) in [
            (false, false, true, false), (true, true, true, false),
            (true, false, false, false), (true, false, true, true),
        ] {
            let input = sample_tick_input(&mut app.world_mut().resource_mut::<LocalPlayer>(), &Controls {
                keys: &keys, mouse: &mouse, settings: &settings,
                enabled, blocked, captured, buy_menu,
            });
            assert_eq!(input.buttons & attacks, 0, "attack obeys ordinary gameplay/UI admission");
        }
        let mut settings = settings;
        settings.unbind_key(KeyCode::KeyX);
        settings.unbind_key(KeyCode::KeyC);
        let restored = Settings::parse(&settings.encode());
        assert_eq!(restored.binds[KEY_ATTACK..=KEY_ATTACK2], [None, None]);
        assert_eq!(restored.mouse_binds, Settings::default().mouse_binds);
        assert_eq!(restored.volume, 0.4);
    }

    #[test]
    fn fresh_shared_attack_button_is_independent_of_another_capture_held_button() {
        let mut app = capture_app();
        app.world_mut().resource_mut::<crate::settings::Settings>().mouse_binds[3] = crate::settings::MouseAction::Attack;
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update(); // Pointer capture suppresses this held left button.
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Back);
        app.world_mut().run_system_cached(look).unwrap();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Back);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0,
            "the fresh side-button tap must survive while capture-left remains held");
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
    }

    #[test]
    fn rebound_capture_click_does_not_jump_until_released_and_pressed_again() {
        let mut app = capture_app();
        app.world_mut().resource_mut::<crate::settings::Settings>().mouse_binds[0] = crate::settings::MouseAction::Action(4);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & (buttons::JUMP | buttons::ATTACK), 0);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.update();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::JUMP, 0);
        // A keyboard jump remains independent of a suppressed capture click.
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Space);
        app.update();
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::JUMP, 0);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Space);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::JUMP, 0);
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
    }

    #[test]
    fn rebound_side_buttons_keep_short_taps_and_held_secondary_attack_on_tick_input() {
        let mut app = capture_app();
        let mut settings = app.world_mut().resource_mut::<crate::settings::Settings>();
        settings.mouse_binds[1] = crate::settings::MouseAction::None;
        settings.mouse_binds[3] = crate::settings::MouseAction::Attack2;
        settings.mouse_binds[4] = crate::settings::MouseAction::Action(4);
        app.world_mut().query::<&mut CursorOptions>().single_mut(app.world_mut()).unwrap().grab_mode = CursorGrabMode::Locked;
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Back);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Forward);
        // Render input runs before the short jump button is released; the sim
        // must retain that edge even if no fixed tick ran while it was down.
        app.world_mut().run_system_cached(look).unwrap();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Forward);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & (buttons::JUMP | buttons::ATTACK2), buttons::JUMP | buttons::ATTACK2);
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & (buttons::JUMP | buttons::ATTACK2), buttons::ATTACK2);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Back);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Right);
        app.world_mut().run_system_cached(sample_after_capture).unwrap();
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK2, 0);
    }

    #[test]
    fn wheel_jump_is_consumed_once_and_does_not_select_a_weapon() {
        let mut app = capture_app();
        app.world_mut().resource_mut::<crate::settings::Settings>().wheel_down = crate::settings::WheelAction::Jump;
        app.world_mut().query::<&mut CursorOptions>().single_mut(app.world_mut()).unwrap().grab_mode = CursorGrabMode::Locked;
        app.world_mut().resource_mut::<AccumulatedMouseScroll>().delta = Vec2::new(0., -1.);
        app.world_mut().run_schedule(Update);
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::JUMP, 0);
        assert_eq!(app.world().resource::<Sampled>().0.weapon_slot, 0);
        app.world_mut().resource_mut::<AccumulatedMouseScroll>().delta = Vec2::ZERO;
        app.world_mut().run_schedule(Update);
        assert_eq!(app.world().resource::<Sampled>().0.buttons & buttons::JUMP, 0);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Space);
        app.world_mut().run_schedule(Update);
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::JUMP, 0);
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
        // Pause and console close have already locked the pointer before look.
        app.world_mut().query::<&mut CursorOptions>().single_mut(app.world_mut()).unwrap().grab_mode = CursorGrabMode::Locked;
        app.world_mut().resource_mut::<InputBlockFrames>().0 = 2;
        for button in [MouseButton::Left, MouseButton::Right] {
            app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(button);
        }
        {
            let mut local = app.world_mut().resource_mut::<LocalPlayer>();
            local.latched = buttons::ATTACK | buttons::ATTACK2;
            local.pointer_attacks = buttons::ATTACK | buttons::ATTACK2;
        }
        for remaining in [1, 0] {
            app.world_mut().resource_mut::<AccumulatedMouseMotion>().delta = Vec2::new(20., 10.);
            app.update();
            assert_eq!(app.world().resource::<InputBlockFrames>().0, remaining,
                "a held UI click cannot extend the finite transition block");
            assert_eq!(app.world().resource::<Sampled>().0.buttons & (buttons::ATTACK | buttons::ATTACK2), 0);
            assert_eq!(app.world().resource::<Sampled>().0.forward_move, if remaining == 0 {450.} else {0.});
            assert_eq!(app.world().resource::<LocalPlayer>().yaw, 0.);
            app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        }
        app.update(); // Both mouse buttons are still held, with fresh frame motion.
        let local = app.world().resource::<LocalPlayer>();
        assert!((local.yaw - 359.12).abs() < 0.0001);
        assert!((local.pitch - 0.44).abs() < 0.0001);
        assert_eq!(app.world().resource::<Sampled>().0.forward_move, 450.);
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
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_ne!(app.world().resource::<Sampled>().0.buttons & buttons::ATTACK, 0);
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
