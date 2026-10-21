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
    respawn: bool,
    page: Page,
    offset: usize,
    root: Option<Entity>,
    dirty: bool,
}
#[derive(Component, Clone)]
enum Action { Toggle, Page(Page), Weapon(u32), Bots(u32), Respawn, Map(String), Previous, Next }

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Panel>()
        .add_systems(OnEnter(AppState::Match), enter)
        .add_systems(PreUpdate, input.after(bevy::input::InputSystems).after(bevy::ui::UiSystems::Focus)
            .run_if(in_state(AppState::Match)))
        .add_systems(Update, draw.after(crate::frontend::update));
}
fn enter(mut panel: ResMut<Panel>, opts: Res<crate::Options>) {
    panel.open = opts.0.playtest;
    panel.dirty = true;
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
            Action::Bots(count) => { panel.bots = Some(*count); panel.respawn = true; panel.open = false; }
            Action::Respawn => { panel.respawn = true; panel.open = false; }
            Action::Map(map) => {
                let mut options = opts.0.clone();
                options.map = map.clone();
                options.direct = true;
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
fn label(name: &str) -> String {
    name.trim_start_matches("weapon_").replace('_', " ")
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
    opts: Res<crate::Options>, front: Res<crate::frontend::Frontend>,
    art: Res<crate::ui_assets::UiAssets>, menu: Res<crate::menu::Menu>,
    world: Option<Res<crate::player::SimWorld>>,
) {
    let active = opts.0.playtest && *state.get() == AppState::Match;
    if !active {
        if let Some(root) = panel.root.take() { commands.entity(root).despawn(); }
        return;
    }
    if !panel.dirty && panel.root.is_some() { return; }
    panel.dirty = false;
    if let Some(root) = panel.root.take() { commands.entity(root).despawn(); }
    let root = commands.spawn((Persistent, UiTargetCamera(front.camera()), GlobalZIndex(25000), Node {
        position_type: PositionType::Absolute,
        left: if panel.open { percent(8) } else { px(16) },
        top: if panel.open { percent(10) } else { px(16) },
        width: if panel.open { percent(84) } else { auto() },
        max_width: px(1100), padding: UiRect::all(px(20)),
        flex_direction: FlexDirection::Column, row_gap: px(14), ..default()
    }, BackgroundColor(Color::srgba(0.035, 0.055, 0.07, if panel.open { 0.97 } else { 0.75 })))).id();
    panel.root = Some(root);
    if !panel.open {
        button(&mut commands, root, "F8  Playtest controls", Action::Toggle, art.regular.clone());
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
        }).map(|d| (label(&d.class_name), Action::Weapon(d.def_index))).collect()).unwrap_or_default(),
        Page::Maps => menu.maps.iter().map(|name| (
            workshop::find(name).map(|m| m.title.to_owned()).unwrap_or_else(|| name.clone()), Action::Map(name.clone())
        )).collect(),
        Page::Match => vec![("Solo / explore".into(), Action::Bots(0)), ("Ten-player match".into(), Action::Bots(9)),
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
