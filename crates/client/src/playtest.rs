//! Opt-in offline controls: selections enter the same fixed-tick armory as purchases.
use bevy::{prelude::*, window::{CursorGrabMode, CursorOptions, PrimaryWindow}};
use crate::{session::{AppState, Persistent}, player::LocalPlayer};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Page { #[default] Weapons, Knives, Match, Maps }
#[derive(Resource, Default)]
pub(crate) struct Panel {
    pub(crate) open: bool,
    pub(crate) weapon: Option<u32>,
    bots: Option<u32>,
    selected_bots: Option<u32>,
    offline_playtest: bool,
    bots_paused: bool,
    respawn: bool,
    page: Page,
    offset: usize,
    root: Option<Entity>,
    dirty: bool,
}
#[derive(Component, Clone)]
enum Action { Toggle, Page(Page), Weapon(u32), Bots(u32), PauseBots, Respawn, Map(String), Previous, Next }

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Panel>()
        .add_systems(OnEnter(AppState::Match), enter)
        .add_systems(PreUpdate, input.after(bevy::input::InputSystems).after(bevy::ui::UiSystems::Focus)
            .run_if(in_state(AppState::Match)))
        .add_systems(Update, draw.after(crate::frontend::update));
}
fn enter(mut panel: ResMut<Panel>, opts: Res<crate::Options>, remote: Option<Res<crate::remote::RemoteClient>>) {
    panel.enter_match(opts.0.playtest, remote.is_some());
}
impl Panel {
    fn enter_match(&mut self, playtest: bool, remote: bool) {
        self.offline_playtest = playtest && !remote;
        self.bots_paused = false;
        self.open = self.offline_playtest;
        self.dirty = true;
    }
}
/// Append ordinary idle inputs at the offline fixed-tick authority seam.
/// Explicit player IDs replace AI inputs in World::step; physics, timers and
/// already committed weapon/utility actions still advance normally.
pub(crate) fn append_bot_inputs(
    panel: &Panel, world: &sim::World, local: sim::PlayerId,
    inputs: &mut Vec<(sim::PlayerId, sim::TickInput)>,
) {
    if !panel.offline_playtest || !panel.bots_paused { return; }
    for bot in world.players.iter().filter(|p| p.is_bot && p.id != local) {
        inputs.push((bot.id, sim::TickInput {
            view_angles: bot.view_angles,
            buttons: if bot.movement.ducked { movement::buttons::DUCK }
                else { bot.last_input.buttons & movement::buttons::DUCK },
            ..default()
        }));
    }
}
/// Called only by the offline fixed-tick owner, never by a UI/render system.
pub(crate) fn configure(panel: &mut Panel, world: &mut sim::World, id: sim::PlayerId) {
    if let Some(count) = panel.bots.take() {
        let old: Vec<_> = world.players.iter().filter(|p| p.is_bot).map(|p| p.id).collect();
        for bot in old { world.remove_player(bot); }
        let team = world.player(id).map(|p| p.team).unwrap_or(sim::Team::CounterTerrorist);
        for i in 0..count {
            let other = if team == sim::Team::Terrorist { sim::Team::CounterTerrorist } else { sim::Team::Terrorist };
            world.add_bot(&format!("Bot {}", i + 1), if i % 2 == 0 { other } else { team });
        }
    }
    if std::mem::take(&mut panel.respawn) || (panel.weapon.is_some() && world.player(id).is_some_and(|p| !p.alive)) { world.respawn(id); }
}
fn input(
    mut panel: ResMut<Panel>, opts: Res<crate::Options>,
    mut keys: ResMut<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut local: ResMut<LocalPlayer>, settings: Res<crate::settings::Settings>,
    mut menu: ResMut<crate::menu::Menu>, request: Res<crate::menu::LoadRequest>,
    mut cursor: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    if !opts.0.playtest || remote.is_some() { return; }
    let was_open = panel.open;
    if keys.just_pressed(KeyCode::F8) || (panel.open && keys.just_pressed(KeyCode::Escape)) {
        panel.open = !panel.open;
        keys.clear_just_pressed(KeyCode::Escape);
        panel.dirty = true;
    }
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed || !mouse.just_pressed(MouseButton::Left) { continue; }
        match action {
            Action::Toggle => panel.open = !panel.open,
            Action::Page(page) => { panel.page = *page; panel.offset = 0; }
            Action::Previous => panel.offset = panel.offset.saturating_sub(12),
            Action::Next => panel.offset += 12,
            Action::Weapon(index) => { panel.weapon = Some(*index); panel.open = false; }
            Action::Bots(count) => { panel.selected_bots = Some(*count); panel.bots = Some(*count); panel.respawn = true; panel.open = false; }
            Action::PauseBots => { panel.bots_paused = !panel.bots_paused; panel.open = false; }
            Action::Respawn => { panel.respawn = true; panel.open = false; }
            Action::Map(map) => {
                let mut options = opts.0.clone();
                options.map = map.clone();
                if let Some(count) = panel.selected_bots {
                    options.bots = count.div_ceil(2);
                    options.bot_difficulty = if count == 0 { Some(0) } else { None };
                }
                options.direct = true;
                options.deathmatch = true;
                options.grenade_practice = true;
                *request.0.lock().unwrap() = Some(options.into());
                panel.open = false;
            }
        }
        panel.dirty = true;
    }
    if panel.open {
        menu.page = None;
        local.latched = 0;
        local.pointer_attacks = 0;
        local.keyboard_attacks = 0;
        local.pending_slot = 0;
        local.pending_drop = false;
        local.pending_buys.clear();
        local.pending_utility = 0;
        cursor.1.grab_mode = CursorGrabMode::None;
        cursor.1.visible = true;
    } else if was_open {
        local.capture_held_mouse(&mouse, &settings);
        if cursor.0.focused {
            cursor.1.grab_mode = CursorGrabMode::Locked;
            cursor.1.visible = false;
        }
    }
}
fn button(commands: &mut Commands, parent: Entity, text: &str, action: Action, font: Handle<Font>) {
    let id = commands.spawn((Persistent, Button, action, Node {
        min_height: px(38), padding: UiRect::axes(px(14), px(8)),
        justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default()
    }, BackgroundColor(Color::srgb(0.16, 0.23, 0.28))))
    .with_children(|p| { p.spawn((Persistent, Text::new(text), TextFont {
        font: font.into(), font_size: FontSize::Px(18.), ..default()
    }, TextColor(Color::WHITE))); }).id();
    commands.entity(parent).add_children(&[id]);
}
fn draw(
    mut commands: Commands, mut panel: ResMut<Panel>, state: Res<State<AppState>>,
    opts: Res<crate::Options>,
    art: Res<crate::ui_assets::UiAssets>, loc: Res<crate::localize::Localization>, menu: Res<crate::menu::Menu>,
    world: Option<Res<crate::player::SimWorld>>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    let active = remote.is_none() && opts.0.playtest && *state.get() == AppState::Match;
    if !active {
        if let Some(root) = panel.root.take() { commands.entity(root).despawn(); }
        return;
    }
    if !panel.dirty && panel.root.is_some() { return; }
    panel.dirty = false;
    if let Some(root) = panel.root.take() { commands.entity(root).despawn(); }
    let root = commands.spawn((Persistent, GlobalZIndex(25000), Node {
        position_type: PositionType::Absolute,
        left: if panel.open { percent(8) } else { percent(43) },
        top: if panel.open { percent(10) } else { auto() },
        bottom: if panel.open { auto() } else { px(12) },
        width: if panel.open { percent(84) } else { auto() },
        max_width: px(1100), padding: UiRect::all(px(if panel.open { 20 } else { 6 })),
        flex_direction: FlexDirection::Column, row_gap: px(14), ..default()
    }, BackgroundColor(Color::srgba(0.035, 0.055, 0.07, if panel.open { 0.97 } else { 0.75 })))).id();
    panel.root = Some(root);
    if !panel.open {
        let hint = commands.spawn((Persistent, Text::new(if panel.bots_paused {
            "F8  Playtest controls  |  Bots paused"
        } else { "F8  Playtest controls" }), TextFont {
            font: art.regular.clone().into(), font_size: FontSize::Px(14.), ..default()
        }, TextColor(Color::WHITE))).id();
        commands.entity(root).add_children(&[hint]);
        return;
    }
    let title = commands.spawn((Persistent, Text::new("PLAYTEST  |  F8 or Escape to resume"), TextFont {
        font: art.bold.clone().into(), font_size: FontSize::Px(26.), ..default()
    }, TextColor(Color::WHITE))).id();
    commands.entity(root).add_children(&[title]);
    let tabs = commands.spawn((Persistent, Node { column_gap: px(10), flex_wrap: FlexWrap::Wrap, ..default() })).id();
    commands.entity(root).add_children(&[tabs]);
    for (text, page) in [("Weapons", Page::Weapons), ("Classic knives", Page::Knives), ("Match", Page::Match), ("Maps", Page::Maps)] {
        button(&mut commands, tabs, text, Action::Page(page), art.regular.clone());
    }
    let body = commands.spawn((Persistent, Node { flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() })).id();
    commands.entity(root).add_children(&[body]);
    let entries: Vec<(String, Action)> = match panel.page {
        Page::Weapons | Page::Knives => world.as_ref().map(|world| world.0.catalog.weapons.iter().filter(|d| {
            use weapons::WeaponClass::*;
            if panel.page == Page::Knives { d.class == Knife && crate::classic_cosmetics::item_available(&d.class_name, 0) }
            else { matches!(d.class, Pistol|Smg|Rifle|Sniper|Shotgun|MachineGun|Grenade) }
        }).map(|d| (loc.weapon_name(d), Action::Weapon(d.def_index))).collect()).unwrap_or_default(),
        Page::Maps => menu.maps.iter().map(|name| (
            workshop::find(name).map(|m| m.title.to_owned()).unwrap_or_else(|| name.clone()), Action::Map(name.clone())
        )).collect(),
        Page::Match => vec![("Solo / explore".into(), Action::Bots(0)), ("Ten-player match".into(), Action::Bots(9)),
            ((if panel.bots_paused { "Resume bots" } else { "Pause bots" }).into(), Action::PauseBots),
            ("Respawn and refill".into(), Action::Respawn)],
    };
    panel.offset = panel.offset.min(entries.len().saturating_sub(1) / 12 * 12);
    for (name, action) in entries.iter().skip(panel.offset).take(12) {
        button(&mut commands, body, name, action.clone(), art.regular.clone());
    }
    if entries.len() > 12 {
        let row = commands.spawn((Persistent, Node { column_gap: px(10), ..default() })).id();
        commands.entity(root).add_children(&[row]);
        if panel.offset > 0 { button(&mut commands, row, "Previous", Action::Previous, art.regular.clone()); }
        if panel.offset + 12 < entries.len() { button(&mut commands, row, "Next", Action::Next, art.regular.clone()); }
    }
    button(&mut commands, root, "Resume match", Action::Toggle, art.regular.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pause_world() -> (sim::World, sim::PlayerId, sim::PlayerId) {
        let v = glam::Vec3::new;
        let mut floor = collision::CollisionWorld::new();
        for triangle in [[v(-4096., -4096., 0.), v(4096., -4096., 0.), v(4096., 4096., 0.)],
            [v(-4096., -4096., 0.), v(4096., 4096., 0.), v(-4096., 4096., 0.)]] {
            floor.add_triangle(triangle, collision::contents::SOLID, 0);
        }
        floor.finalize();
        // Synthetic weapon stats only; no installed content or renderer needed.
        let catalog = weapons::Catalog::from_items_game(r#""items_game" { "items" {
            "7" { "name" "weapon_ak47" "baseitem" "1" "attributes" {
                "damage" "1" "primary clip size" "30" "primary reserve ammo max" "60"
                "is full auto" "1" "cycletime" "0.1"
            } }
        } }"#).unwrap();
        let mut world = sim::World::with_catalog(std::sync::Arc::new(floor), std::sync::Arc::new(catalog));
        for (x, yaw, team) in [(0., 0., sim::Team::CounterTerrorist), (256., 180., sim::Team::Terrorist)] {
            world.spawns.push(sim::SpawnPoint { origin: v(x, 0., 1.), yaw, team });
        }
        let local = world.add_player("local", sim::Team::CounterTerrorist);
        let bot = world.add_bot("target", sim::Team::Terrorist);
        (world, local, bot)
    }
    #[test]
    fn pause_button_and_draw_only_change_panel_until_fixed_tick() {
        let (world, local, _) = pause_world();
        let before = world.state_hash();
        let mut panel = Panel { page: Page::Match, ..default() };
        panel.enter_match(true, false);
        let mut app = App::new();
        app.insert_resource(crate::Options(crate::ClientOptions { playtest: true, ..default() }))
            .insert_resource(panel)
            .insert_resource(crate::player::SimWorld(world))
            .insert_resource(LocalPlayer::new(local, glam::Vec3::ZERO, 0.))
            .insert_resource(State::new(AppState::Match))
            .insert_resource(crate::ui_assets::UiAssets { regular: default(), bold: default(), images: default() })
            .init_resource::<crate::localize::Localization>()
            .init_resource::<crate::settings::Settings>()
            .init_resource::<crate::menu::Menu>()
            .init_resource::<crate::menu::LoadRequest>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_systems(Update, (input, draw).chain());
        app.world_mut().spawn((Window::default(), CursorOptions::default(), PrimaryWindow));
        app.update();
        let mut actions = app.world_mut().query::<(Entity, &Action)>();
        let button = actions.iter(app.world()).find(|(_, action)| matches!(action, Action::PauseBots)).unwrap().0;
        app.world_mut().entity_mut(button).insert(Interaction::Pressed);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        let panel = app.world().resource::<Panel>();
        assert!(panel.bots_paused);
        assert!(!panel.open);
        assert!(panel.bots.is_none() && panel.weapon.is_none() && !panel.respawn);
        assert_eq!(app.world().resource::<crate::player::SimWorld>().0.state_hash(), before);
        assert!(app.world().resource::<crate::menu::LoadRequest>().0.lock().unwrap().is_none());
        // Reopen the same Match page, then use its Resume bots button.
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().reset_all();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::F8);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        let button = actions.iter(app.world()).find(|(_, action)| matches!(action, Action::PauseBots)).unwrap().0;
        app.world_mut().entity_mut(button).insert(Interaction::Pressed);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert!(!app.world().resource::<Panel>().bots_paused);
        assert_eq!(app.world().resource::<crate::player::SimWorld>().0.state_hash(), before);
    }
    #[test]
    fn pause_is_offline_opt_in_and_new_match_resets_it() {
        let (world, local, _) = pause_world();
        let mut panel = Panel::default();
        for (playtest, remote) in [(false, false), (true, true)] {
            panel.enter_match(playtest, remote);
            panel.bots_paused = true; // Even a stale flag cannot override these flows.
            let mut inputs = vec![(local, sim::TickInput::default())];
            append_bot_inputs(&panel, &world, local, &mut inputs);
            assert_eq!(inputs.len(), 1);
        }
        panel.enter_match(true, false);
        assert!(!panel.bots_paused);
        panel.bots_paused = true;
        let mut inputs = vec![(local, sim::TickInput::default())];
        append_bot_inputs(&panel, &world, local, &mut inputs);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[1].1.buttons, 0, "standing bots must not start crouching");
        // OnEnter(Match) runs again after loading another map.
        panel.enter_match(true, false);
        assert!(!panel.bots_paused);
        let mut inputs = vec![];
        append_bot_inputs(&panel, &world, local, &mut inputs);
        assert!(inputs.is_empty());
    }
    #[test]
    fn paused_tick_inputs_stop_bot_actions_but_keep_local_controls_and_physics_64_128() {
        use movement::buttons;
        for rate in [64, 128] {
            let (mut world, local, bot) = pause_world();
            world.set_tick_rate(rate);
            world.step(&[(local, default()), (bot, sim::TickInput {
                view_angles: glam::Vec3::new(0., 180., 0.), forward_move: 450.,
                buttons: buttons::ATTACK | buttons::JUMP | buttons::DUCK | buttons::USE, ..default()
            })]);
            let target = world.player(bot).unwrap();
            assert!(target.shots > 0 && !target.movement.on_ground);
            let (shots, angles, velocity) = (target.shots, target.view_angles, target.movement.velocity);
            let clip = target.active_weapon().unwrap().clip;
            let start = world.tick;
            let mut panel = Panel::default();
            panel.enter_match(true, false);
            panel.bots_paused = true;
            let local_input = sim::TickInput { view_angles: glam::Vec3::new(0., 90., 0.),
                forward_move: 200., buttons: buttons::WALK, ..default() };
            let local_start = world.player(local).unwrap().movement.origin;
            for n in 0..rate * 2 {
                let mut inputs = vec![(local, local_input)];
                append_bot_inputs(&panel, &world, local, &mut inputs);
                assert_eq!(inputs[0], (local, local_input));
                assert_eq!(inputs.len(), 2);
                world.step(&inputs);
                let target = world.player(bot).unwrap();
                assert_eq!(target.last_input, sim::TickInput { view_angles: angles, buttons: buttons::DUCK, ..default() });
                assert_eq!(target.shots, shots);
                assert_eq!(target.active_weapon().unwrap().clip, clip);
                assert_eq!(world.player(local).unwrap().last_input, local_input);
                if n == 0 { assert!(target.movement.velocity.z < velocity.z, "gravity still applies"); }
            }
            assert_eq!(world.tick, start + u64::from(rate * 2));
            assert!(world.player(bot).unwrap().movement.on_ground, "airborne bots still land");
            assert!(world.player(bot).unwrap().movement.ducked);
            assert_ne!(world.player(local).unwrap().movement.origin, local_start);
            panel.bots_paused = false;
            let mut inputs = vec![(local, local_input)];
            append_bot_inputs(&panel, &world, local, &mut inputs);
            assert_eq!(inputs, vec![(local, local_input)]);
            world.step(&inputs);
            assert_ne!(world.player(bot).unwrap().last_input,
                sim::TickInput { view_angles: angles, buttons: buttons::DUCK, ..default() }, "ordinary AI resumes");
        }
    }
    #[test]
    fn panel_click_queues_weapon_without_reloading_map_and_consumes_escape() {
        let mut app = App::new();
        app.insert_resource(crate::Options(crate::ClientOptions { playtest: true, ..default() }))
            .insert_resource(Panel { open: true, ..default() })
            .insert_resource(LocalPlayer::new(0, glam::Vec3::ZERO, 0.))
            .init_resource::<crate::settings::Settings>()
            .init_resource::<crate::menu::Menu>()
            .init_resource::<crate::menu::LoadRequest>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_systems(Update, input);
        app.world_mut().spawn((Window::default(), CursorOptions::default(), PrimaryWindow));
        let button = app.world_mut().spawn((Interaction::Pressed, Action::Weapon(515))).id();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        let panel = app.world().resource::<Panel>();
        assert!(!panel.open);
        assert_eq!(panel.weapon, Some(515));
        assert!(!panel.respawn, "changing a knife must preserve the current position");
        assert!(app.world().resource::<crate::menu::LoadRequest>().0.lock().unwrap().is_none());
        app.world_mut().despawn(button);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().reset_all();
        app.world_mut().resource_mut::<Panel>().open = true;
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<Panel>().open);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Escape));
    }
    #[test]
    #[ignore = "requires read-only CS:GO Legacy installed weapon definitions"]
    fn installed_playtest_arsenal_fires_and_reloads_through_tick_input() {
        let data = crate::GameData::load().expect("Legacy install");
        let guns: Vec<_> = data.catalog.weapons.iter().filter(|d| matches!(d.class,
            weapons::WeaponClass::Pistol | weapons::WeaponClass::Smg | weapons::WeaponClass::Rifle
            | weapons::WeaponClass::Sniper | weapons::WeaponClass::Shotgun | weapons::WeaponClass::MachineGun)).collect();
        assert!(guns.len() >= 30);
        let mut floor = collision::CollisionWorld::new();
        let v = glam::Vec3::new;
        for triangle in [[v(-8192., -8192., 0.), v(8192., -8192., 0.), v(8192., 8192., 0.)],
            [v(-8192., -8192., 0.), v(8192., 8192., 0.), v(-8192., 8192., 0.)]] {
            floor.add_triangle(triangle, collision::contents::SOLID, 0);
        }
        floor.finalize();
        let floor = std::sync::Arc::new(floor);
        for rate in [64, 128] {
            for gun in &guns {
                let mut world = sim::World::with_catalog(floor.clone(), data.catalog.clone());
                world.set_tick_rate(rate);
                world.rules.grenade_practice = true;
                let id = world.add_player("arsenal", sim::Team::CounterTerrorist);
                world.step(&[(id, sim::TickInput { buy: sim::Purchase::Weapon(gun.def_index), ..default() })]);
                assert_eq!(world.player(id).unwrap().active_weapon().unwrap().def.id, gun.id);
                // Let ordinary deployment finish with the trigger released.
                for _ in 0..world.ticks(3.) { world.step(&[(id, sim::TickInput::default())]); }
                let mut fired = false;
                for _ in 0..world.ticks(1.) {
                    world.step(&[(id, sim::TickInput { buttons: movement::buttons::ATTACK, ..default() })]);
                    fired |= world.events.iter().any(|e| matches!(e, sim::GameEvent::Fire { player, weapon, .. }
                        if *player == id && weapon == &gun.class_name));
                }
                assert!(fired, "{} at {} Hz", gun.class_name, rate);
                let clip = world.player(id).unwrap().active_weapon().unwrap().clip;
                assert!(clip < gun.clip_size, "{} must spend ammunition", gun.class_name);
                // Release and allow slow bolt/shot recovery before one reload press.
                for _ in 0..world.ticks(3.) { world.step(&[(id, sim::TickInput::default())]); }
                world.step(&[(id, sim::TickInput { buttons: movement::buttons::RELOAD, ..default() })]);
                for _ in 0..world.ticks(8.) { world.step(&[(id, sim::TickInput::default())]); }
                let held = world.player(id).unwrap().active_weapon().unwrap();
                assert_eq!(held.clip, gun.clip_size, "{} reload at {} Hz", gun.class_name, rate);
                assert!(held.reload_end.is_none());
            }
        }
    }

}
