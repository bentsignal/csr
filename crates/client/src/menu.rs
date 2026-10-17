//! Local CS:GO-style front end. All presented game modes are playable offline.
use crate::{
    player::{InputEnabled, LocalPlayer},
    settings::{ACTIONS, RESOLUTIONS, Settings},
    ui_assets::UiAssets,
};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Inventory,
    Cases,
    Play,
    Pause,
    Settings,
    Graphics,
    Audio,
    Controls,
    Crosshair,
}
#[derive(Resource)]
pub struct Menu {
    pub page: Option<Page>,
    pub in_match: bool,
    pub maps: Vec<String>,
    pub map: usize,
    pub deathmatch: bool,
    pub terrorist: bool,
    pub bots: u32,
    pub capture: Option<usize>,
    /// The working classic menu explicitly hands keyboard/mouse ownership to
    /// this shared controls panel. Other fallback pages stay disabled.
    pub(crate) classic_controls: bool,
    pub(crate) controls_just_closed: bool,
    pub status: String,
    dirty: bool,
    scroll: f32,
    size: Vec2,
    root: Option<Entity>,
}
impl Default for Menu {
    fn default() -> Self {
        Self {
            page: None,
            in_match: true,
            maps: Vec::new(),
            map: 0,
            deathmatch: false,
            terrorist: false,
            bots: 5,
            capture: None,
            classic_controls: false,
            controls_just_closed: false,
            status: String::new(),
            dirty: true,
            scroll: 0.0,
            size: Vec2::ZERO,
            root: None,
        }
    }
}
impl Menu {
    pub fn open(&self) -> bool {
        self.page.is_some()
    }
    /// Return true when the movie's owned child must cancel instead of the
    /// host closing Pause. One physical Escape must have exactly one owner.
    fn escape(&mut self, movie_options: bool) -> bool {
        if self.in_match && self.open() && movie_options { return true; }
        self.page = match self.page {
            None => Some(Page::Pause),
            Some(Page::Pause) => None,
            Some(Page::Main) => Some(Page::Main),
            _ => Some(if self.in_match { Page::Pause } else { Page::Main }),
        };
        false
    }
    pub(crate) fn reset_match(&mut self) {
        self.root = None;
        self.in_match = false;
        self.page = Some(Page::Main);
        self.dirty = true;
        self.capture = None;
        self.classic_controls = false;
        self.controls_just_closed = false;
    }
    pub(crate) fn open_classic_controls(&mut self) {
        self.classic_controls = true;
        self.controls_just_closed = false;
        self.page = Some(Page::Controls);
        self.capture = None;
        self.scroll = 0.0;
        self.status.clear();
        self.dirty = true;
    }
    fn close_classic_controls(&mut self) {
        self.classic_controls = false;
        self.controls_just_closed = true;
        self.page = Some(if self.in_match { Page::Pause } else { Page::Main });
        self.capture = None;
        self.status.clear();
        self.dirty = true;
    }
}
/// A catalog selection retains its immutable archive identity through loading.
#[derive(Clone)]
pub struct MapRequest {
    pub options: crate::ClientOptions,
    pub pack: Option<(packs::Installed, packs::CatalogEntry, content::ContentId)>,
}
impl From<crate::ClientOptions> for MapRequest {
    fn from(options: crate::ClientOptions) -> Self { Self { options, pack: None } }
}
impl MapRequest {
    pub(crate) fn label(&self) -> String {
        self.pack.as_ref().and_then(|(_, entry, id)| entry.manifest().maps.iter().find(|map| map.id == id.to_string()))
            .map(|map| crate::pack_flow::caption(&map.name))
            .unwrap_or_else(|| workshop::find(&self.options.map)
                .map(|map| map.title.to_owned()).unwrap_or_else(|| self.options.map.clone()))
    }
}
#[derive(Resource, Clone, Default)]
pub struct LoadRequest(pub Arc<Mutex<Option<MapRequest>>>);
#[derive(Component, Clone, Copy)]
pub enum Action {
    Page(Page),
    Resume,
    Disconnect,
    Quit,
    Start,
    SelectMap(usize),
    SelectWeapon(usize),
    Filter(usize),
    Equip(bool),
    Rotate(f32),
    Mode,
    Team,
    Bots(i32),
    Adjust(u8, f32),
    Bind(usize),
    ClearBind(usize),
    Save,
    Defaults,
    ControlsDefaults,
    ControlsBack,
}

pub(crate) fn setup(
    mut menu: ResMut<Menu>,
    opts: Res<crate::Options>,
    level: Res<crate::GameData>,
    mut clock: ResMut<Time<Virtual>>,
) {
    let mut maps: Vec<String> = level::maps::list(&level.game_dir, &level.vfs)
        .into_iter()
        .filter(|map| map.old.is_none() || crate::frontend_game::menu_archive(&map.name))
        .map(|map| map.name)
        .collect();
    if maps.is_empty() {
        maps.push(opts.0.map.clone());
    }
    menu.map = maps.iter().position(|p| p == &opts.0.map).unwrap_or(0);
    menu.maps = maps;
    menu.deathmatch = opts.0.deathmatch || menu.maps[menu.map].starts_with("cs_");
    menu.terrorist = opts.0.terrorist;
    menu.bots = opts.0.bots;
    let test = std::env::var("CSR_TEST_MENU").ok();
    if (!opts.0.direct && opts.0.screenshot.is_none()) || test.is_some() {
        menu.page = Some(match test.as_deref() {
            Some("pause") => Page::Pause,
            Some("graphics") => Page::Graphics,
            Some("audio") => Page::Audio,
            Some("controls") => Page::Controls,
            Some("crosshair") => Page::Crosshair,
            Some("play") => Page::Play,
            Some("inventory") => Page::Inventory,
            Some("cases") => Page::Cases,
            _ => Page::Main,
        });
        menu.in_match = test.as_deref().is_some_and(|v| v == "pause") || (test.is_some() && std::env::var_os("CSR_TEST_PAUSED").is_some());
        clock.pause();
    }
}

pub(crate) fn home_controls(state: Res<State<crate::AppState>>, menu: Res<Menu>) -> bool {
    *state.get() == crate::AppState::Menu && !menu.in_match
        // Retain the close pass: input clears the handoff flag, while draw
        // removes its former root before the movie becomes visible again.
        && (menu.classic_controls || menu.controls_just_closed || menu.root.is_some())
}

/// Unlike match entities, a Home editor must survive without match ownership
/// until its explicit retirement pass, even if reset_match forgets its handle.
#[derive(Component)]
pub(crate) struct ClassicControlsRoot(crate::AppState);

// Both Home and Match register draw. Order by an explicit shared set rather
// than its ambiguous SystemTypeSet, even though their run conditions differ.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ControlsDraw;

pub(crate) fn install_controls_retirement(app: &mut App) {
    app.add_systems(Update, retire_classic_controls
        .after(crate::frontend::cancel_loading).before(ControlsDraw));
}

pub(crate) fn retire_classic_controls(
    mut commands: Commands,
    mut menu: ResMut<Menu>,
    state: Res<State<crate::AppState>>,
    roots: Query<(Entity, &ClassicControlsRoot)>,
) {
    let scene = if menu.in_match { crate::AppState::Match } else { crate::AppState::Menu };
    for (root, owner) in &roots {
        let tracked = menu.root == Some(root);
        if tracked && menu.classic_controls && owner.0 == scene && owner.0 == *state.get() {
            continue;
        }
        commands.entity(root).despawn();
        // A forgotten root must never clear a replacement's handle. Draw runs
        // after these commands are applied and cannot despawn this root twice.
        if tracked {
            menu.root = None;
            menu.dirty = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn input(
    mut commands: Commands,
    mut menu: ResMut<Menu>,
    mut settings: ResMut<Settings>,
    keys: Res<ButtonInput<KeyCode>>,
    actions: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut exit: MessageWriter<AppExit>,
    (load_request, opts, remote, front): (Res<LoadRequest>, Res<crate::Options>, Option<Res<crate::remote::RemoteClient>>, Option<Res<crate::frontend::Frontend>>),
    mut local: Option<ResMut<LocalPlayer>>,
    mut buy: ResMut<crate::buymenu::BuyMenu>,
    mut clock: ResMut<Time<Virtual>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    enabled: Res<InputEnabled>,
    mut inventory: ResMut<crate::inventory::Inventory>,
    collection: Res<crate::inventory::Collection>,
    mut previous_open: Local<bool>,
) {
    if !enabled.0 {
        return;
    }
    let mut save_controls = false;
    menu.controls_just_closed = false;
    // Native movie callbacks may close the menu between input passes.
    let was_open = menu.open() || *previous_open;
    if let Some(index) = menu.capture {
        if keys.just_pressed(KeyCode::Escape) {
            menu.capture = None;
            menu.dirty = true;
        } else if let Some(key) = keys
            .get_just_pressed()
            .find(|k| crate::settings::KEYS.contains(k))
        {
            if index == ACTIONS.len() { settings.set_console_key(*key); }
            else { settings.rebind(index, *key); }
            save_controls = menu.classic_controls;
            menu.capture = None;
            menu.dirty = true;
        }
    } else if keys.just_pressed(KeyCode::Escape) && menu.classic_controls {
        menu.close_classic_controls();
    } else if keys.just_pressed(KeyCode::Escape) && !buy.open {
        let movie_options = front.as_ref().and_then(|front| front.options_receiver());
        if menu.escape(movie_options.is_some()) {
            // Options is a child of Pause, although the host page remains
            // Pause. Cancel only that owned child and keep the match paused.
            front.as_ref().unwrap().cancel_options(movie_options.unwrap());
        }
        menu.dirty = true;
    }
    if menu.open() && menu.capture.is_none() {
        for (interaction, action) in &actions {
            if *interaction != Interaction::Pressed {
                continue;
            }
            menu.dirty = true;
            menu.status.clear();
            match *action {
                Action::Page(p) => {
                    menu.page = Some(p);
                    menu.scroll = 0.0;
                }
                Action::Resume => {
                    if menu.in_match {
                        menu.page = None;
                    }
                }
                Action::Disconnect => {
                    menu.in_match = false;
                    menu.page = Some(Page::Main);
                }
                Action::Quit => {
                    exit.write(AppExit::Success);
                }
                Action::SelectMap(index) => {
                    if index < menu.maps.len() {
                        menu.map = index;
                        if menu.maps[index].starts_with("cs_") {
                            menu.deathmatch = true;
                        }
                    }
                }
                Action::SelectWeapon(index) => {
                    if let Some(def) = collection.0.weapons.get(index) {
                        inventory.selected = def.class_name.clone();
                        inventory.yaw = 0.0;
                    }
                }
                Action::Filter(filter) => {
                    inventory.filter = filter;
                    menu.scroll = 0.0;
                }
                Action::Rotate(delta) => {
                    inventory.yaw += delta;
                    menu.dirty = false;
                }
                Action::Equip(ct) => {
                    if inventory.equip(&collection.0, ct) {
                        menu.status = match inventory.save() {
                            Ok(()) => {
                                "Loadout saved. Applied on your next Deathmatch spawn.".into()
                            }
                            Err(e) => format!("Could not save loadout: {e}"),
                        };
                    } else {
                        menu.status =
                            "This weapon cannot fill a primary or pistol slot for that team."
                                .into();
                    }
                }
                Action::Mode => {
                    if menu.maps[menu.map].starts_with("de_") {
                        menu.deathmatch = !menu.deathmatch;
                    } else {
                        menu.status = "Hostage maps currently support Deathmatch.".into();
                    }
                }
                Action::Team => menu.terrorist = !menu.terrorist,
                Action::Bots(d) => menu.bots = (menu.bots as i32 + d).clamp(0, 10) as u32,
                Action::Bind(i) => {
                    menu.capture = Some(i);
                    menu.status =
                        "Press a keyboard key. Esc cancels. Existing bindings swap.".into();
                }
                Action::ClearBind(i) => {
                    if i == ACTIONS.len() { settings.console_key = None; }
                    else if let Some(binding) = settings.binds.get_mut(i) { *binding = None; }
                    save_controls |= menu.classic_controls;
                }
                Action::ControlsBack => menu.close_classic_controls(),
                Action::ControlsDefaults => {
                    settings.restore_controls_defaults();
                    menu.status = "Control defaults restored. Apply to save.".into();
                }
                Action::Defaults => {
                    *settings = Settings::default();
                    menu.status = "Defaults restored. Apply to save.".into();
                }
                Action::Save => {
                    menu.status = match settings.save() {
                        Ok(()) => "Settings saved.".into(),
                        Err(e) => format!("Could not save settings: {e}"),
                    };
                }
                Action::Start => match settings.save() {
                    Err(e) => menu.status = format!("Could not save settings: {e}"),
                    Ok(()) => {
                        let mut next = opts.0.clone();
                        next.map = menu.maps[menu.map].clone();
                        next.deathmatch = menu.deathmatch;
                        next.terrorist = menu.terrorist;
                        next.bots = menu.bots;
                        next.direct = true;
                        next.screenshot = None;
                        next.camera = None;
                        *load_request.0.lock().unwrap() = Some(next.into());
                    }
                },
                Action::Adjust(id, delta) => {
                    save_controls |= menu.classic_controls && matches!(id, 1 | 9 | 25..=31);
                    match id {
                    0 => settings.volume = (settings.volume + delta).clamp(0.0, 1.0),
                    1 => {
                        settings.sensitivity = ((settings.sensitivity + delta) * 1000.0)
                            .round()
                            .clamp(100.0, 10000.0)
                            / 1000.0
                    }
                    2 => settings.exposure = (settings.exposure + delta).clamp(0.5, 4.0),
                    3 => settings.vsync = !settings.vsync,
                    4 => settings.fullscreen = !settings.fullscreen,
                    5 => {
                        settings.resolution =
                            (settings.resolution as i32 + delta as i32).rem_euclid(4) as usize
                    }
                    6 => settings.msaa = !settings.msaa,
                    7 => settings.blood = !settings.blood,
                    8 => settings.hitmarker = !settings.hitmarker,
                    9 => settings.invert_y = !settings.invert_y,
                    10 => {
                        settings.cross_color =
                            (settings.cross_color as i32 + delta as i32).rem_euclid(5) as usize
                    }
                    11 => settings.cross_size = (settings.cross_size + delta).clamp(2.0, 20.0),
                    12 => settings.cross_gap = (settings.cross_gap + delta).clamp(0.0, 16.0),
                    13 => {
                        settings.cross_thickness =
                            (settings.cross_thickness + delta).clamp(1.0, 6.0)
                    }
                    14 => settings.cross_dot = !settings.cross_dot,
                    24 => settings.menu_blur=!settings.menu_blur,
                    25..=29 => settings.cycle_mouse_binding((id - 25) as usize, delta.signum() as i32),
                    30..=31 => settings.cycle_wheel_binding(id == 30, delta.signum() as i32),
                    _ => {}
                    }
                },
            }
        }
    }
    if save_controls {
        // Accepted edits already affect live input. Persist the same state now,
        // so Back, Escape or closing the window cannot silently discard them.
        // Restore Defaults remains an explicit Apply operation.
        menu.status = match settings.save() {
            Ok(()) => "Controls saved.".into(),
            Err(error) => format!("Could not save controls: {error}"),
        };
        menu.dirty = true;
    }
    if menu.open() {
        buy.open = false;
        buy.just_closed = false;
        if let Some(local) = local.as_mut() {
            local.latched = 0;
            local.pointer_attacks = 0;
            local.keyboard_attacks = 0;
            local.pending_drop = false;
            local.pending_slot = 0;
            local.clear_utility_requests();
            local.pending_buys.clear();
        }
        if remote.is_some() { clock.unpause(); } else { clock.pause(); }
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if was_open {
        clock.unpause();
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
        // A UI click cannot also fire in the resumed match.
        commands.insert_resource(crate::player::InputBlockFrames(2));
    }
    *previous_open = menu.open();
}

pub(crate) fn label(
    commands: &mut Commands,
    parent: Entity,
    text: impl Into<String>,
    size: f32,
    color: Color,
) -> Entity {
    let e = commands
        .spawn((
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
        ))
        .id();
    commands.entity(parent).add_children(&[e]);
    e
}
fn button(
    commands: &mut Commands,
    parent: Entity,
    text: impl Into<String>,
    action: Action,
    width: Val,
) -> Entity {
    let e = commands
        .spawn((
            Button,
            action,
            Node {
                width,
                min_height: px(34.0),
                padding: UiRect::axes(px(10.0), px(7.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.30, 0.36, 0.40, 0.68)),
        ))
        .id();
    commands.entity(parent).add_children(&[e]);
    label(commands, e, text, 16.0, Color::srgb(0.93, 0.95, 0.96));
    e
}
fn column(commands: &mut Commands, parent: Entity, node: Node) -> Entity {
    let e = commands.spawn(node).id();
    commands.entity(parent).add_children(&[e]);
    e
}
fn row(commands: &mut Commands, parent: Entity, name: &str, value: String, id: u8, step: f32) {
    let r = column(
        commands,
        parent,
        Node {
            width: percent(100),
            height: px(38.0),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            column_gap: px(10.0),
            ..default()
        },
    );
    let left = column(
        commands,
        r,
        Node {
            width: percent(48),
            ..default()
        },
    );
    label(commands, left, name, 18.0, Color::srgb(0.80, 0.85, 0.89));
    button(commands, r, "−", Action::Adjust(id, -step), px(38.0));
    let middle = column(
        commands,
        r,
        Node {
            width: px(160.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
    );
    label(commands, middle, value, 18.0, Color::WHITE);
    button(commands, r, "+", Action::Adjust(id, step), px(38.0));
}

pub fn draw(
    mut commands: Commands,
    mut menu: ResMut<Menu>,
    settings: Res<Settings>,
    art: Res<UiAssets>,
    inventory: Res<crate::inventory::Inventory>,
    collection: Res<crate::inventory::Collection>,
    preview: Res<crate::menu_scene::Preview>,
    loc: Res<crate::localize::Localization>,
    window: Single<&Window, With<PrimaryWindow>>,
    front: Option<Res<crate::frontend::Frontend>>,
) {
    // Cleanup must also run on the frame the explicit controls route closes;
    // gating the whole draw system would strand its former root on screen.
    if front.as_ref().is_some_and(|front| front.has_classic_menu()) && !menu.classic_controls {
        if let Some(root) = menu.root.take() { commands.entity(root).despawn(); }
        menu.dirty = true;
        return;
    }
    let size = Vec2::new(window.width(), window.height());
    if menu.size != size {
        menu.size = size;
        menu.dirty = true;
    }
    if !menu.dirty && !settings.is_changed() {
        return;
    }
    menu.dirty = false;
    if let Some(root) = menu.root.take() {
        commands.entity(root).despawn();
    }
    let Some(page) = menu.page else {
        return;
    };
    if menu.classic_controls {
        let root = draw_classic_controls(&mut commands, &menu, &settings);
        if let Some(front) = &front { commands.entity(root).insert(UiTargetCamera(front.camera())); }
        menu.root = Some(root);
        return;
    }
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(if menu.in_match {Color::srgba(0.02,0.03,0.04,0.36)} else {Color::srgb(0.035, 0.055, 0.072)}),
            GlobalZIndex(100),
        ))
        .id();
    menu.root = Some(root);
    if !menu.in_match && matches!(page, Page::Main | Page::Play | Page::Inventory | Page::Cases) {
        if let Some(img) = art.image("menu-background") {
            let backdrop = commands
                .spawn((
                    ImageNode {
                        color: Color::srgb(0.60, 0.65, 0.69),
                        ..ImageNode::new(img)
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                ))
                .id();
            commands.entity(root).add_children(&[backdrop]);
        }
    }
    let rail = column(
        &mut commands,
        root,
        Node {
            width: px(116.0),
            flex_shrink: 0.0,
            height: percent(100),
            padding: UiRect::axes(px(8.0), px(26.0)),
            flex_direction: FlexDirection::Column,
            row_gap: px(12.0),
            ..default()
        },
    );
    commands
        .entity(rail)
        .insert(BackgroundColor(Color::srgba(0.025, 0.04, 0.052, 0.90)));
    if let Some(logo) = art.image("logo") {
        let e = commands
            .spawn((
                ImageNode::new(logo),
                Node {
                    width: px(100.0),
                    height: px(45.0),
                    margin: UiRect::bottom(px(26.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(rail).add_children(&[e]);
    }

    for (name, p) in [
        ("HOME", Page::Main),
        ("PLAY", Page::Play),
        ("INVENTORY", Page::Inventory),
        ("CASES", Page::Cases),
        ("SETTINGS", Page::Settings),
    ] {
        button(&mut commands, rail, name, Action::Page(p), percent(100));
    }
    if menu.in_match {
        button(&mut commands, rail, "RESUME", Action::Resume, percent(100));
        button(
            &mut commands,
            rail,
            "LEAVE",
            Action::Disconnect,
            percent(100),
        );
    }
    column(
        &mut commands,
        rail,
        Node {
            flex_grow: 1.0,
            ..default()
        },
    );
    button(&mut commands, rail, "QUIT", Action::Quit, percent(100));
    let content = column(
        &mut commands,
        root,
        Node {
            flex_grow: 1.0,
            height: percent(100),
            padding: UiRect::axes(px(26.0), px(22.0)),
            min_width: px(0.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(12.0),
            ..default()
        },
    );
    label(
        &mut commands,
        content,
        match page {
            Page::Main => "COUNTER-STRIKE: GLOBAL OFFENSIVE",
            Page::Inventory => "INVENTORY",
            Page::Cases => "INVENTORY / CASES & SKINS",
            Page::Play => "PLAY  /  PRACTICE WITH BOTS",
            Page::Pause => "MATCH PAUSED",
            Page::Settings => "GAME SETTINGS",
            Page::Graphics => "VIDEO SETTINGS",
            Page::Audio => "AUDIO SETTINGS",
            Page::Controls => "KEYBOARD / MOUSE",
            Page::Crosshair => "CROSSHAIR",
        },
        30.0,
        Color::WHITE,
    );
    let settings_page = matches!(
        page,
        Page::Settings | Page::Graphics | Page::Audio | Page::Controls | Page::Crosshair
    );
    if settings_page {
        let tabs = column(
            &mut commands,
            content,
            Node {
                column_gap: px(8.0),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
        );
        for (name, p) in [
            ("GAME", Page::Settings),
            ("VIDEO", Page::Graphics),
            ("AUDIO", Page::Audio),
            ("KEYBOARD / MOUSE", Page::Controls),
            ("CROSSHAIR", Page::Crosshair),
        ] {
            button(&mut commands, tabs, name, Action::Page(p), Val::Auto);
        }
    }
    let body = column(
        &mut commands,
        content,
        Node {
            flex_grow: 1.0,
            flex_basis: px(0.0),
            min_height: px(0.0),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(if page == Page::Controls { 2.0 } else { 5.0 }),
            ..default()
        },
    );
    let on = |v: bool| if v { "Enabled" } else { "Disabled" }.to_string();
    match page {
        Page::Cases => {}
        Page::Main => {
            let scene = column(
                &mut commands,
                body,
                Node {
                    flex_grow: 1.0,
                    width: percent(100),
                    ..default()
                },
            );
            let model = commands
                .spawn((
                    ImageNode::new(preview.image.clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        right: px(100.0),
                        bottom: px(0.0),
                        height: percent(100),
                        aspect_ratio: Some(0.9),
                        ..default()
                    },
                ))
                .id();
            commands.entity(scene).add_children(&[model]);
            let news = column(
                &mut commands,
                scene,
                Node {
                    width: px(350.0),
                    padding: UiRect::all(px(20.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(18.0),
                    align_self: AlignSelf::FlexStart,
                    margin: UiRect::top(px(26.0)),
                    ..default()
                },
            );
            commands
                .entity(news)
                .insert(BackgroundColor(Color::srgba(0.04, 0.065, 0.085, 0.84)));
            label(
                &mut commands,
                news,
                "PLAY COUNTER-STRIKE",
                28.0,
                Color::WHITE,
            );
            if let Some(img) = art.image("map-de_dust2") {
                picture(&mut commands, news, img, percent(100), px(150.0));
            }
            label(
                &mut commands,
                news,
                "LOCAL PLAY",
                21.0,
                Color::WHITE,
            );
            label(
                &mut commands,
                news,
                "Competitive or Deathmatch against bots. Choose a map to begin.",
                19.0,
                Color::srgb(0.72, 0.78, 0.82),
            );
            button(
                &mut commands,
                news,
                "PLAY",
                Action::Page(Page::Play),
                percent(100),
            );
            button(
                &mut commands,
                news,
                "OPEN INVENTORY",
                Action::Page(Page::Inventory),
                percent(100),
            );
            let profile = column(
                &mut commands,
                scene,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(0.0),
                    top: px(0.0),
                    width: px(150.0),
                    height: percent(100),
                    padding: UiRect::all(px(16.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(15.0),
                    ..default()
                },
            );
            commands
                .entity(profile)
                .insert(BackgroundColor(Color::srgba(0.025, 0.04, 0.055, 0.78)));
            if let Some(img) = art.image("ct") {
                picture(&mut commands, profile, img, px(64.0), px(64.0));
            }
            label(&mut commands, profile, "YOU", 24.0, Color::WHITE);
            label(
                &mut commands,
                profile,
                "LOCAL PROFILE",
                14.0,
                Color::srgb(0.70, 0.78, 0.84),
            );
            label(
                &mut commands,
                profile,
                "Offline practice",
                18.0,
                Color::srgb(0.72, 0.82, 0.62),
            );
            column(
                &mut commands,
                profile,
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            );
            label(
                &mut commands,
                profile,
                "CS:GO LEGACY",
                15.0,
                Color::srgb(0.7, 0.76, 0.8),
            );
            label(
                &mut commands,
                body,
                "LOCAL PLAY  /  BASE WEAPON COLLECTION  /  PRACTICE LOADOUTS",
                16.0,
                Color::srgb(0.83, 0.86, 0.88),
            );
        }
        Page::Inventory => {
            button(&mut commands,body,"CASES & OWNED SKINS",Action::Page(Page::Cases),px(240.));
            let tabs = column(
                &mut commands,
                body,
                Node {
                    column_gap: px(5.0),
                    ..default()
                },
            );
            for (i, name) in crate::inventory::FILTERS.iter().enumerate() {
                button(
                    &mut commands,
                    tabs,
                    if inventory.filter == i {
                        format!("[{name}]")
                    } else {
                        name.to_string()
                    },
                    Action::Filter(i),
                    Val::Auto,
                );
            }
            let area = column(
                &mut commands,
                body,
                Node {
                    flex_grow: 1.0,
                    flex_basis: px(0.0),
                    min_height: px(0.0),
                    column_gap: px(18.0),
                    ..default()
                },
            );
            let grid = scroll_grid(&mut commands, area, menu.scroll);
            let width = ((size.x - 600.0) / 4.0 - 12.0).max(112.0);
            for (i, def) in collection
                .0
                .weapons
                .iter()
                .enumerate()
                .filter(|(_, d)| crate::inventory::visible(d, inventory.filter))
            {
                let selected = def.class_name == inventory.selected;
                let title = loc.weapon_name(def);
                let card = card(
                    &mut commands,
                    grid,
                    &title,
                    Action::SelectWeapon(i),
                    selected,
                    width,
                    110.0,
                );
                if let Some(img) = art.image(&format!(
                    "item-{}",
                    def.class_name.trim_start_matches("weapon_")
                )) {
                    card_image(&mut commands, card, img, 68.0);
                }
            }
            let detail = column(
                &mut commands,
                area,
                Node {
                    width: px(360.0),
                    flex_shrink: 0.0,
                    padding: UiRect::all(px(18.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6.0),
                    ..default()
                },
            );
            commands
                .entity(detail)
                .insert(BackgroundColor(Color::srgba(0.045, 0.065, 0.082, 0.92)));
            if let Some(def) = collection.0.get(&inventory.selected) {
                label(
                    &mut commands,
                    detail,
                    loc.weapon_name(def),
                    30.0,
                    Color::WHITE,
                );
                label(
                    &mut commands,
                    detail,
                    "BASE ITEM  •  LOCAL COLLECTION",
                    14.0,
                    Color::srgb(0.68, 0.74, 0.79),
                );
                picture(
                    &mut commands,
                    detail,
                    preview.image.clone(),
                    percent(100),
                    px((size.y - 580.0).clamp(130.0, 310.0)),
                );
                let turn = column(
                    &mut commands,
                    detail,
                    Node {
                        column_gap: px(8.0),
                        ..default()
                    },
                );
                button(
                    &mut commands,
                    turn,
                    "ROTATE LEFT",
                    Action::Rotate(-0.3),
                    percent(50),
                );
                button(
                    &mut commands,
                    turn,
                    "ROTATE RIGHT",
                    Action::Rotate(0.3),
                    percent(50),
                );
                label(
                    &mut commands,
                    detail,
                    format!(
                        "Damage {}   •   {} RPM   •   {} rounds",
                        def.damage as i32,
                        def.rpm() as i32,
                        def.clip_size.max(0)
                    ),
                    16.0,
                    Color::srgb(0.77, 0.82, 0.86),
                );
                let equip = column(
                    &mut commands,
                    detail,
                    Node {
                        column_gap: px(6.0),
                        ..default()
                    },
                );
                for (ct, name) in [(false, "EQUIP FOR T"), (true, "EQUIP FOR CT")] {
                    if crate::inventory::allowed(def, ct) {
                        button(&mut commands, equip, name, Action::Equip(ct), Val::Auto);
                    }
                }
            }
            label(
                &mut commands,
                detail,
                "DEATHMATCH LOADOUTS",
                17.0,
                Color::WHITE,
            );
            for (i, pair) in inventory.loadouts.chunks(2).enumerate() {
                let display = pair
                    .iter()
                    .map(|name| {
                        collection
                            .0
                            .get(name)
                            .map(|d| loc.weapon_name(d))
                            .unwrap_or_else(|| name.clone())
                    })
                    .collect::<Vec<_>>()
                    .join(" / ");
                label(
                    &mut commands,
                    detail,
                    format!("{}  {}", ["T", "CT"][i], display),
                    15.0,
                    Color::srgb(0.7, 0.76, 0.8),
                );
            }
            label(
                &mut commands,
                body,
                "Equip applies on your next Deathmatch spawn. Competitive retains its buy economy.",
                16.0,
                Color::srgb(0.83, 0.86, 0.88),
            );
        }
        Page::Pause => {
            label(
                &mut commands,
                body,
                "Local match paused",
                30.0,
                Color::WHITE,
            );
            label(
                &mut commands,
                body,
                "Resume your match or adjust your settings.",
                21.0,
                Color::srgb(0.7, 0.76, 0.8),
            );
            button(
                &mut commands,
                body,
                "RESUME GAME",
                Action::Resume,
                px(280.0),
            );
        }
        Page::Play => {
            let options = column(
                &mut commands,
                body,
                Node {
                    column_gap: px(10.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
            );
            button(
                &mut commands,
                options,
                format!(
                    "MODE  •  {}",
                    if menu.deathmatch {
                        "Deathmatch"
                    } else {
                        "Competitive"
                    }
                ),
                Action::Mode,
                px(260.0),
            );
            button(
                &mut commands,
                options,
                format!(
                    "TEAM  •  {}",
                    if menu.terrorist {
                        "Terrorists"
                    } else {
                        "Counter-Terrorists"
                    }
                ),
                Action::Team,
                px(260.0),
            );
            button(&mut commands, options, "−", Action::Bots(-1), px(34.0));
            label(
                &mut commands,
                options,
                format!(
                    "{} enemies / {} allies",
                    menu.bots,
                    menu.bots.saturating_sub(1)
                ),
                17.0,
                Color::WHITE,
            );
            button(&mut commands, options, "+", Action::Bots(1), px(34.0));
            let grid = scroll_grid(&mut commands, body, menu.scroll);
            let cols = if size.x >= 1200.0 { 6.0 } else { 5.0 };
            let width = ((size.x - 168.0 - (cols - 1.0) * 12.0 - 20.0) / cols).floor();
            let card_height = if size.y < 800.0 { 88.0 } else { 118.0 };
            for (i, name) in menu.maps.iter().enumerate() {
                let title = name
                    .trim_start_matches("de_")
                    .trim_start_matches("cs_")
                    .replace('_', " ")
                    .to_uppercase();
                let c = card(
                    &mut commands,
                    grid,
                    &title,
                    Action::SelectMap(i),
                    menu.map == i,
                    width,
                    card_height,
                );
                if let Some(img) = art.image(&format!("map-{name}")) {
                    // Crop the installed 640×360 image to fill the short map card.
                    let crop = 640.0 * (card_height - 32.0) / (width - 4.0);
                    let banner = commands
                        .spawn((
                            ImageNode {
                                image: img,
                                rect: Some(Rect::new(
                                    0.0,
                                    (360.0 - crop) * 0.5,
                                    640.0,
                                    (360.0 + crop) * 0.5,
                                )),
                                image_mode: bevy::ui::widget::NodeImageMode::Stretch,
                                ..default()
                            },
                            Node {
                                position_type: PositionType::Absolute,
                                top: px(0.0),
                                left: px(0.0),
                                width: percent(100),
                                height: px(card_height - 32.0),
                                ..default()
                            },
                        ))
                        .id();
                    commands.entity(c).add_children(&[banner]);
                }
            }
            let footer = column(
                &mut commands,
                body,
                Node {
                    column_gap: px(20.0),
                    align_items: AlignItems::Center,
                    margin: UiRect::top(px(8.0)),
                    ..default()
                },
            );
            let selected = menu.maps.get(menu.map).map(String::as_str).unwrap_or("");
            label(
                &mut commands,
                footer,
                format!("SELECTED  •  {selected}"),
                21.0,
                Color::WHITE,
            );
            column(
                &mut commands,
                footer,
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            );
            let go = button(&mut commands, footer, "GO", Action::Start, px(180.0));
            commands
                .entity(go)
                .insert(BackgroundColor(Color::srgb(0.28, 0.43, 0.22)));
        }
        Page::Settings => {
            row(
                &mut commands,
                body,
                "Blood effects",
                on(settings.blood),
                7,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Optional hit confirmation",
                on(settings.hitmarker),
                8,
                1.0,
            );
            label(
                &mut commands,
                body,
                "Hurt sounds and directional damage feedback are always active.",
                18.0,
                Color::srgb(0.65, 0.73, 0.8),
            );
        }
        Page::Graphics => {
            row(
                &mut commands,
                body,
                "Display mode",
                if settings.fullscreen {
                    "Borderless fullscreen"
                } else {
                    "Windowed"
                }
                .into(),
                4,
                1.0,
            );
            let (w, h) = RESOLUTIONS[settings.resolution];
            row(
                &mut commands,
                body,
                "Resolution",
                format!("{w} × {h}"),
                5,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Wait for vertical sync",
                on(settings.vsync),
                3,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Multisampling anti-aliasing",
                if settings.msaa { "4× MSAA" } else { "None" }.into(),
                6,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Brightness",
                format!("{:.1}", settings.exposure),
                2,
                0.1,
            );
        }
        Page::Audio => {
            row(
                &mut commands,
                body,
                "Master volume",
                format!("{}%", (settings.volume * 100.0).round()),
                0,
                0.05,
            );
            label(
                &mut commands,
                body,
                "Stereo positional audio • weapon, impact and player sounds",
                19.0,
                Color::srgb(0.7, 0.76, 0.8),
            );
        }
        Page::Controls => {
            controls_scroll(&mut commands, body, menu.scroll);
            controls_content(&mut commands, body, &menu, &settings);
        }
        Page::Crosshair => {
            row(
                &mut commands,
                body,
                "Colour",
                ["Green", "Blue", "Yellow", "Light Blue", "Red"][settings.cross_color].into(),
                10,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Length",
                format!("{:.0}", settings.cross_size),
                11,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Gap",
                format!("{:.0}", settings.cross_gap),
                12,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Thickness",
                format!("{:.0}", settings.cross_thickness),
                13,
                1.0,
            );
            row(
                &mut commands,
                body,
                "Centre dot",
                on(settings.cross_dot),
                14,
                1.0,
            );
            let preview = column(
                &mut commands,
                body,
                Node {
                    height: px(160.0),
                    width: px(460.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            );
            commands
                .entity(preview)
                .insert(BackgroundColor(Color::srgb(0.12, 0.16, 0.19)));
            let pivot = column(
                &mut commands,
                preview,
                Node {
                    width: px(0.0),
                    height: px(0.0),
                    ..default()
                },
            );
            for i in 0..5 {
                if i == 4 && !settings.cross_dot {
                    continue;
                }
                let (x, y, w, h) = cross_bar(&settings, i);
                let e = commands
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(x),
                            top: px(y),
                            width: px(w),
                            height: px(h),
                            ..default()
                        },
                        BackgroundColor(settings.crosshair_color()),
                    ))
                    .id();
                commands.entity(pivot).add_children(&[e]);
            }
        }
    }
    label(
        &mut commands,
        content,
        if menu.status.is_empty() {
            ""
        } else {
            &menu.status
        },
        17.0,
        Color::srgb(0.83, 0.83, 0.63),
    );
    if settings_page {
        let footer = column(
            &mut commands,
            content,
            Node {
                column_gap: px(12.0),
                flex_shrink: 0.0,
                ..default()
            },
        );
        button(
            &mut commands,
            footer,
            "APPLY / SAVE",
            Action::Save,
            px(200.0),
        );
        button(
            &mut commands,
            footer,
            "RESTORE DEFAULTS",
            if page == Page::Controls { Action::ControlsDefaults } else { Action::Defaults },
            px(220.0),
        );
    }
}
#[derive(Component)]
pub(crate) struct ControlsScroll;

fn controls_scroll(commands: &mut Commands, body: Entity, scroll: f32) {
    commands.entity(body).insert((ControlsScroll, RelativeCursorPosition::default(),
        ScrollPosition(Vec2::new(0.0, scroll))));
    // Keep the same flex sizing as the common body, with dedicated vertical
    // scrolling. CardGrid wraps cards and is deliberately not reused here.
    commands.entity(body).insert(Node {
        flex_grow: 1.0, flex_basis: px(0.0), min_height: px(0.0), min_width: px(0.0),
        width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(5.0),
        overflow: Overflow::scroll_y(), padding: UiRect::all(px(10.0)), ..default()
    });
}

fn controls_content(commands: &mut Commands, body: Entity, menu: &Menu, settings: &Settings) {
    let on = |v: bool| if v { "Enabled" } else { "Disabled" }.to_string();
    if !settings.controls_source.is_empty() {
        label(commands, body, &settings.controls_source, 16.0, Color::srgb(0.7, 0.76, 0.8));
    }
    row(
        commands,
        body,
        "Mouse sensitivity",
        format!("{:.3}", settings.sensitivity),
        1,
        0.1,
    );
    row(
        commands,
        body,
        "Invert mouse",
        on(settings.invert_y),
        9,
        1.0,
    );
    for (index, action) in settings.mouse_binds.iter().enumerate() {
        row(commands, body, &format!("Mouse button {}", index + 1), action.label().into(), 25 + index as u8, 1.0);
    }
    row(commands, body, "Mouse wheel up", settings.wheel_up.label().into(), 30, 1.0);
    row(commands, body, "Mouse wheel down", settings.wheel_down.label().into(), 31, 1.0);
    label(commands, body, "Keyboard bindings", 18.0, Color::srgb(0.8, 0.85, 0.89));
    for (i, name) in ACTIONS.iter().chain(std::iter::once(&"Toggle developer console")).enumerate() {
        let r = column(
            commands,
            body,
            Node {
                min_height: px(36.0),
                flex_shrink: 0.0,
                column_gap: px(20.0),
                align_items: AlignItems::Center,
                ..default()
            },
        );
        let l = column(
            commands,
            r,
            Node {
                width: px(260.0),
                ..default()
            },
        );
        label(commands, l, *name, 17.0, Color::srgb(0.8, 0.85, 0.89));
        let binding = button(
            commands,
            r,
            if menu.capture == Some(i) {
                "Press a key…".into()
            } else {
                (if i == ACTIONS.len() { settings.console_key } else { settings.binds[i] }).map(crate::settings::key_name).unwrap_or_else(|| "Unbound".into())
            },
            Action::Bind(i),
            px(180.0),
        );
        commands.entity(binding).insert(Node {
            width: px(180.0),
            min_height: px(26.0),
            padding: UiRect::axes(px(12.0), px(2.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        });
        button(commands, r, "Clear", Action::ClearBind(i), px(74.0));
    }
}

/// Functional controls route shared with the fallback settings page. Its
/// appearance still requires reference work; this does not claim Scaleform parity.
fn draw_classic_controls(commands: &mut Commands, menu: &Menu, settings: &Settings) -> Entity {
    let root = commands.spawn((Node {
        position_type: PositionType::Absolute, width: percent(100), height: percent(100),
        padding: UiRect::axes(px(32.0), px(24.0)), flex_direction: FlexDirection::Column,
        row_gap: px(12.0), ..default()
    }, BackgroundColor(Color::srgb(0.035, 0.055, 0.072)), GlobalZIndex(100),
        ClassicControlsRoot(if menu.in_match { crate::AppState::Match } else { crate::AppState::Menu }))).id();
    label(commands, root, "KEYBOARD / MOUSE", 30.0, Color::WHITE);
    label(commands, root, "Scroll to see all controls. Select a key to change it; Escape cancels capture.",
        17.0, Color::srgb(0.8, 0.85, 0.89));
    let body = column(commands, root, Node::default());
    controls_scroll(commands, body, menu.scroll);
    controls_content(commands, body, menu, settings);
    label(commands, root, &menu.status, 17.0, Color::srgb(0.83, 0.83, 0.63));
    let footer = column(commands, root, Node {
        column_gap: px(12.0), flex_shrink: 0.0, flex_wrap: FlexWrap::Wrap, row_gap: px(8.0), ..default()
    });
    button(commands, footer, "APPLY / SAVE", Action::Save, px(180.0));
    button(commands, footer, "RESTORE DEFAULTS", Action::ControlsDefaults, px(200.0));
    button(commands, footer, "BACK", Action::ControlsBack, px(120.0));
    root
}

pub(crate) fn scroll_controls(
    mut areas: Query<(&RelativeCursorPosition, &ComputedNode, &mut ScrollPosition), With<ControlsScroll>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<Menu>,
) {
    let dy = wheel.read().map(|event| event.y *
        if event.unit == bevy::input::mouse::MouseScrollUnit::Line { 42.0 } else { 1.0 }).sum::<f32>();
    for (cursor, node, mut scroll) in &mut areas {
        // A redraw has not reached PostUpdate layout yet. Retain its saved
        // offset until there is a measured viewport/content size to clamp to.
        if node.size().y <= 0.0 { continue; }
        let max = ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
        let page = node.size().y * node.inverse_scale_factor() * 0.8;
        if cursor.cursor_over { scroll.y -= dy; }
        if menu.capture.is_none() {
            if keys.just_pressed(KeyCode::PageDown) { scroll.y += page; }
            if keys.just_pressed(KeyCode::PageUp) { scroll.y -= page; }
            if keys.just_pressed(KeyCode::Home) { scroll.y = 0.0; }
            if keys.just_pressed(KeyCode::End) { scroll.y = max; }
        }
        scroll.y = scroll.y.clamp(0.0, max);
        menu.scroll = scroll.y;
    }
}

pub fn cross_bar(s: &Settings, i: u8) -> (f32, f32, f32, f32) {
    let (l, t, g) = (s.cross_size, s.cross_thickness, s.cross_gap);
    match i {
        0 => (-g - l, -t / 2.0, l, t),
        1 => (g, -t / 2.0, l, t),
        2 => (-t / 2.0, -g - l, t, l),
        3 => (-t / 2.0, g, t, l),
        _ => (-t / 2.0, -t / 2.0, t, t),
    }
}

#[derive(Component)]
pub struct Card {
    selected: bool,
}
#[derive(Component)]
pub(crate) struct CardGrid;
fn picture(commands: &mut Commands, parent: Entity, image: Handle<Image>, width: Val, height: Val) {
    let e = commands
        .spawn((
            ImageNode::new(image),
            Node {
                width,
                height,
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_children(&[e]);
}
fn scroll_grid(commands: &mut Commands, parent: Entity, scroll: f32) -> Entity {
    let grid = column(
        commands,
        parent,
        Node {
            flex_grow: 1.0,
            flex_basis: px(0.0),
            min_height: px(0.0),
            min_width: px(0.0),
            width: percent(100),
            flex_wrap: FlexWrap::Wrap,
            align_content: AlignContent::FlexStart,
            column_gap: px(12.0),
            row_gap: px(12.0),
            padding: UiRect::all(px(10.0)),
            overflow: Overflow::scroll_y(),
            ..default()
        },
    );
    commands.entity(grid).insert((
        CardGrid,
        RelativeCursorPosition::default(),
        ScrollPosition(Vec2::new(0.0, scroll)),
        BackgroundColor(Color::srgba(0.035, 0.055, 0.073, 0.73)),
    ));
    grid
}
fn card(
    commands: &mut Commands,
    parent: Entity,
    title: &str,
    action: Action,
    selected: bool,
    width: f32,
    height: f32,
) -> Entity {
    let e = commands
        .spawn((
            Button,
            action,
            Card { selected },
            RelativeCursorPosition::default(),
            UiTransform::IDENTITY,
            ZIndex(0),
            Node {
                width: px(width),
                height: px(height),
                flex_shrink: 0.0,
                border: UiRect::all(px(2.0)),
                ..default()
            },
            BorderColor::all(if selected {
                Color::srgb(0.78, 0.84, 0.54)
            } else {
                Color::srgba(0.5, 0.6, 0.65, 0.18)
            }),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.35),
                px(0.0),
                px(2.0),
                px(0.0),
                px(3.0),
            ),
            BackgroundColor(Color::srgb(0.11, 0.15, 0.18)),
        ))
        .id();
    commands.entity(parent).add_children(&[e]);
    let text = label(commands, e, title, 16.0, Color::WHITE);
    commands.entity(text).insert(Node {
        position_type: PositionType::Absolute,
        bottom: px(4.0),
        left: px(8.0),
        ..default()
    });
    e
}
fn card_image(commands: &mut Commands, parent: Entity, image: Handle<Image>, height: f32) {
    let e = commands
        .spawn((
            ImageNode::new(image),
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(100),
                height: px(height - 4.0),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_children(&[e]);
}
pub(crate) fn cards(
    mut cards: Query<(
        &Card,
        &Action,
        &Interaction,
        &RelativeCursorPosition,
        &mut UiTransform,
        &mut BorderColor,
        &mut BoxShadow,
        &mut ZIndex,
    )>,
    mut grids: Query<(&RelativeCursorPosition, &ComputedNode, &mut ScrollPosition), With<CardGrid>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    time: Res<Time<Real>>,
    mut menu: ResMut<Menu>,
    opts: Res<crate::Options>,
) {
    let dt = 1.0 - (-18.0 * time.delta_secs()).exp();
    for (card, action, interaction, cursor, mut transform, mut border, mut shadow, mut z) in
        &mut cards
    {
        let fixture = opts.0.screenshot.is_some()
            && std::env::var_os("CSR_TEST_HOVER").is_some()
            && matches!(action,Action::SelectMap(i) if *i==(menu.map+1)%menu.maps.len().max(1));
        let hover = *interaction != Interaction::None || fixture;
        let target = if hover { 1.035 } else { 1.0 };
        transform.scale = transform.scale.lerp(Vec2::splat(target), dt);
        let tilt = if hover {
            cursor.normalized.map_or(0.4, |p| p.x - 0.5) * 0.035
        } else {
            0.0
        };
        transform.rotation = Rot2::radians(tilt);
        transform.translation = Val2 {
            x: px(0.0),
            y: px(if hover { -4.0 } else { 0.0 }),
        };
        *border = BorderColor::all(if card.selected {
            Color::srgb(0.78, 0.84, 0.54)
        } else if hover {
            Color::srgb(0.9, 0.94, 0.96)
        } else {
            Color::srgba(0.5, 0.6, 0.65, 0.18)
        });
        *shadow = BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, if hover { 0.72 } else { 0.35 }),
            px(0.0),
            px(if hover { 9.0 } else { 2.0 }),
            px(0.0),
            px(if hover { 12.0 } else { 3.0 }),
        );
        z.0 = if hover { 2 } else { 0 };
    }
    let dy = wheel
        .read()
        .map(|e| {
            e.y * if e.unit == bevy::input::mouse::MouseScrollUnit::Line {
                42.0
            } else {
                1.0
            }
        })
        .sum::<f32>();
    if dy != 0.0 {
        for (cursor, node, mut scroll) in &mut grids {
            if cursor.cursor_over {
                let max = ((node.content_size().y - node.size().y) * node.inverse_scale_factor())
                    .max(0.0);
                scroll.y = (scroll.y - dy).clamp(0.0, max);
                menu.scroll = scroll.y;
            }
        }
    }
}

pub fn hover(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (With<Action>, Without<Card>, Changed<Interaction>),
    >,
) {
    for (i, mut c) in &mut buttons {
        c.0 = match i {
            Interaction::Hovered => Color::srgba(0.42, 0.50, 0.55, 0.9),
            Interaction::Pressed => Color::srgb(0.55, 0.62, 0.45),
            _ => Color::srgba(0.30, 0.36, 0.40, 0.68),
        };
    }
}

pub(crate) fn apply(
    settings: Res<Settings>,
    opts: Res<crate::Options>,
    mut local: Option<ResMut<LocalPlayer>>,
    mut sounds: ResMut<crate::audio::SoundLibrary>,
    mut materials: ResMut<Assets<crate::material::WorldMaterial>>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut display: Local<crate::frontend_display::DisplayState>,
    mut cameras: Query<&mut Msaa, With<crate::player::PlayerCamera>>,
    mut sinks: Query<(&crate::audio::SoundGain, &mut bevy::audio::AudioSink)>,
    mut spatial: Query<(&crate::audio::SoundGain, &mut bevy::audio::SpatialAudioSink)>,
) {
    let configuration_changed = settings.is_changed() || local.as_ref().is_some_and(|local| local.is_added());
    let volume = settings.audio_options.effective_volume(settings.volume, window.focused);
    if !configuration_changed && sounds.master_volume == volume {
        return;
    }
    if let Some(local) = local.as_mut() { local.sensitivity = settings.sensitivity; }
    sounds.master_volume = volume;
    use bevy::audio::AudioSinkPlayback;
    for (gain, mut sink) in &mut sinks {
        sink.set_volume(bevy::audio::Volume::Linear(volume * gain.0));
    }
    for (gain, mut sink) in &mut spatial {
        sink.set_volume(bevy::audio::Volume::Linear(volume * gain.0));
    }
    // A focus-only change updates playback, not display/material settings.
    if !configuration_changed || opts.0.screenshot.is_some() {
        return;
    }
    display.apply(&mut window, &settings, opts.0.size_explicit.then_some((opts.0.width, opts.0.height)));
    for mut aa in &mut cameras {
        *aa = if settings.msaa {
            Msaa::Sample4
        } else {
            Msaa::Off
        };
    }
    for (_, material) in materials.iter_mut() {
        material.params.exposure = settings.exposure;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_retirement_schedule_accepts_both_actual_draw_registrations() {
        let mut app = App::new();
        install_controls_retirement(&mut app);
        app.add_systems(Update, draw.in_set(ControlsDraw).run_if(home_controls))
            .add_systems(Update, draw.in_set(ControlsDraw).run_if(crate::remote_join::ready));
        app.world_mut().schedule_scope(Update, |world, schedule| {
            // Build actual system metadata and ordering without executing draw
            // or requiring a renderer. Before(draw) used to fail initialization
            // because these two instances share one SystemTypeSet.
            schedule.initialize(world).unwrap();
        });
    }

    // Build the actual controls hierarchy without layout, assets, a movie,
    // renderer or map loading. Console dispatch and request_load are real;
    // keyboard-to-console delivery and production draw scheduling are outside
    // this entity-lifetime fixture.
    fn controls_lifetime_app() -> App {
        let mut a = harness();
        a.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<crate::AppState>()
            .init_resource::<crate::session::Loader>()
            .init_resource::<crate::hosting::Hosting>()
            .add_observer(crate::session::own_entity::<Node>)
            .add_systems(Update, crate::session::request_load)
            .add_systems(Update, retire_classic_controls.after(input).after(crate::session::request_load));
        {
            let mut menu = a.world_mut().resource_mut::<Menu>();
            menu.in_match = false;
            menu.page = Some(Page::Main);
            menu.maps = vec!["de_dust2".into()];
        }
        a.update(); // Consume initial state entry before dispatching requests.
        a
    }

    fn spawn_controls_tree(world: &mut World) -> Vec<Entity> {
        world.resource_mut::<Menu>().open_classic_controls();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let root = draw_classic_controls(&mut Commands::new(&mut queue, world),
            world.resource::<Menu>(), world.resource::<Settings>());
        queue.apply(world);
        world.resource_mut::<Menu>().root = Some(root);
        let mut tree = vec![root];
        let mut at = 0;
        while at < tree.len() {
            if let Some(children) = world.get::<Children>(tree[at]) {
                tree.extend(children.iter());
            }
            at += 1;
        }
        assert!(tree.len() > ACTIONS.len(), "fixture must contain the real controls widgets");
        tree
    }

    fn assert_controls_tree_gone(world: &World, tree: &[Entity]) {
        for entity in tree {
            assert!(world.get_entity(*entity).is_err(), "old controls entity {entity:?} survived");
        }
    }

    #[test]
    fn home_console_disconnect_retires_the_forgotten_controls_tree() {
        let mut a = controls_lifetime_app();
        let old = spawn_controls_tree(a.world_mut());
        assert!(a.world().get::<crate::session::MatchEntity>(old[0]).is_none());
        assert_eq!(crate::console::command_fixture(a.world_mut(), "disconnect").unwrap(), "Disconnected.");
        assert!(a.world().resource::<Menu>().root.is_none(), "exercise the lost-handle reset");
        assert!(!a.world().resource::<Menu>().classic_controls);
        assert!(!a.world().resource::<Menu>().controls_just_closed);
        assert!(a.world().get_entity(old[0]).is_ok(), "reset alone cannot retire a Home entity");
        a.update();
        assert_controls_tree_gone(a.world(), &old);
        let reopened = spawn_controls_tree(a.world_mut());
        a.update();
        assert_eq!(a.world().resource::<Menu>().root, Some(reopened[0]));
        assert!(reopened.iter().all(|entity| a.world().get_entity(*entity).is_ok()));
    }

    #[test]
    fn home_console_map_retires_controls_on_loading_even_before_flag_cleanup() {
        let mut a = controls_lifetime_app();
        let old = spawn_controls_tree(a.world_mut());
        crate::console::command_fixture(a.world_mut(), "map de_dust2").unwrap();
        assert!(a.world().resource::<LoadRequest>().0.lock().unwrap().is_some());
        a.update(); // The supported loader request queues the state transition.
        assert_eq!(*a.world().resource::<State<crate::AppState>>().get(), crate::AppState::Menu);
        assert!(matches!(a.world().resource::<NextState<crate::AppState>>(),
            NextState::Pending(crate::AppState::Loading)));
        a.update(); // No begin_load is installed: no game data or worker needed.
        assert_eq!(*a.world().resource::<State<crate::AppState>>().get(), crate::AppState::Loading);
        assert!(a.world().resource::<Menu>().classic_controls,
            "scene retirement must work independently of frontend flag cleanup");
        assert!(a.world().resource::<Menu>().root.is_none());
        assert_controls_tree_gone(a.world(), &old);
    }

    #[test]
    fn classic_controls_back_retires_children_and_keeps_the_reopened_tree() {
        for in_match in [false, true] {
            let mut a = controls_lifetime_app();
            a.insert_state(if in_match { crate::AppState::Match } else { crate::AppState::Menu });
            a.world_mut().resource_mut::<Menu>().in_match = in_match;
            let old = spawn_controls_tree(a.world_mut());
            let back = old.iter().copied().find(|entity|
                matches!(a.world().get::<Action>(*entity), Some(Action::ControlsBack))).unwrap();
            a.world_mut().entity_mut(back).insert(Interaction::Pressed);
            a.update(); // Real ControlsBack handling, then unconditional retirement.
            assert_eq!(a.world().resource::<Menu>().page,
                Some(if in_match { Page::Pause } else { Page::Main }));
            assert!(a.world().resource::<Menu>().root.is_none());
            assert_controls_tree_gone(a.world(), &old);
            let reopened = spawn_controls_tree(a.world_mut());
            a.update();
            a.update();
            assert_eq!(a.world().resource::<Menu>().root, Some(reopened[0]));
            assert!(reopened.iter().all(|entity| a.world().get_entity(*entity).is_ok()));
        }
    }

    #[test]
    fn retiring_a_forgotten_controls_tree_preserves_its_replacement_handle() {
        let mut a = controls_lifetime_app();
        let old = spawn_controls_tree(a.world_mut());
        crate::console::command_fixture(a.world_mut(), "disconnect").unwrap();
        // Reopening before the retirement pass leaves both marked roots live.
        let replacement = spawn_controls_tree(a.world_mut());
        a.update();
        assert_controls_tree_gone(a.world(), &old);
        assert_eq!(a.world().resource::<Menu>().root, Some(replacement[0]));
        assert!(replacement.iter().all(|entity| a.world().get_entity(*entity).is_ok()));
        assert_eq!(a.world_mut().query_filtered::<Entity, With<ClassicControlsRoot>>()
            .iter(a.world()).count(), 1);
    }

    #[test]
    fn home_controls_accept_public_key_edges_before_a_player_exists() {
        const CHILD: &str = "CSR_HOME_KEYS_AUTOSAVE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let data = std::env::temp_dir().join(format!("csr-home-keys-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            std::fs::create_dir_all(&data).unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "menu::tests::home_controls_accept_public_key_edges_before_a_player_exists", "--nocapture"])
                .env(CHILD, "1").current_dir(data).output().unwrap();
            assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
            return;
        }
        use bevy::input::{ButtonState, keyboard::{Key, KeyboardInput}};
        let mut a = App::new();
        a.add_plugins((MinimalPlugins, bevy::input::InputPlugin))
            .insert_resource(State::new(crate::AppState::Menu))
            .init_resource::<Menu>()
            .init_resource::<Settings>()
            .init_resource::<LoadRequest>()
            .init_resource::<crate::inventory::Inventory>()
            .init_resource::<crate::inventory::Collection>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .insert_resource(crate::Options(crate::ClientOptions::default()))
            .insert_resource(InputEnabled(true))
            .add_systems(RunFixedMainLoop, input.run_if(home_controls)
                .in_set(bevy::app::RunFixedMainLoopSystems::BeforeFixedMainLoop));
        let window = a.world_mut().spawn((PrimaryWindow, CursorOptions::default())).id();
        assert!(!a.world().contains_resource::<LocalPlayer>());
        {
            let mut menu = a.world_mut().resource_mut::<Menu>();
            menu.in_match = false;
            menu.page = Some(Page::Main);
            menu.open_classic_controls();
            // Button/callback and rendered-layout delivery are covered by the
            // opt-in acceptance case; this fixture isolates schedule/key input.
            menu.capture = Some(0);
        }
        let mut edge = |code, state| {
            a.world_mut().write_message(KeyboardInput { key_code: code,
                logical_key: if code == KeyCode::Escape { Key::Escape } else { Key::F11 },
                state, text: None, repeat: false, window });
            a.update();
        };
        edge(KeyCode::F11, ButtonState::Pressed);
        edge(KeyCode::F11, ButtonState::Released);
        assert_eq!(a.world().resource::<Settings>().binds[0], Some(KeyCode::F11));
        assert_eq!(Settings::load().binds[0], Some(KeyCode::F11));
        assert!(a.world().resource::<Menu>().classic_controls);
        a.world_mut().write_message(KeyboardInput { key_code: KeyCode::Escape,
            logical_key: Key::Escape, state: ButtonState::Pressed, text: None, repeat: false, window });
        a.update();
        assert_eq!(a.world().resource::<Menu>().page, Some(Page::Main));
        assert!(a.world().resource::<Menu>().controls_just_closed);
        a.world_mut().write_message(KeyboardInput { key_code: KeyCode::Escape,
            logical_key: Key::Escape, state: ButtonState::Released, text: None, repeat: false, window });
        a.update();
        assert!(!a.world().resource::<Menu>().controls_just_closed);
        a.world_mut().write_message(KeyboardInput { key_code: KeyCode::Escape,
            logical_key: Key::Escape, state: ButtonState::Pressed, text: None, repeat: false, window });
        a.update();
        assert_eq!(a.world().resource::<Menu>().page, Some(Page::Main), "closed Home editor no longer owns Escape");
    }

    #[test]
    fn controls_edits_survive_a_separate_process_without_apply() {
        const CHILD: &str = "CSR_CONTROLS_RESTART_PHASE";
        let phase = std::env::var(CHILD).unwrap_or_default();
        if phase.is_empty() {
            let base = std::env::temp_dir().join(format!("csr-controls-restart-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            for in_match in [false, true] {
                let data = base.join(if in_match { "match" } else { "home" });
                std::fs::create_dir_all(&data).unwrap();
                for stage in ["write", "read"] {
                    let result = std::process::Command::new(std::env::current_exe().unwrap())
                        .args(["--exact", "menu::tests::controls_edits_survive_a_separate_process_without_apply", "--nocapture"])
                        .env(CHILD, stage).env("CSR_CONTROLS_RESTART_MATCH", if in_match { "1" } else { "0" })
                        .current_dir(&data).output().unwrap();
                    assert!(result.status.success(), "{stage}: {}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
                }
            }
            return;
        }
        if phase == "read" {
            let loaded = Settings::load();
            assert_eq!(loaded.binds[23], Some(KeyCode::KeyK));
            assert_eq!(loaded.binds[24], Some(KeyCode::KeyV));
            assert_eq!(loaded.mouse_binds[3], crate::settings::MouseAction::Action(0));
            assert_eq!(loaded.wheel_up, crate::settings::WheelAction::Jump);
            assert_eq!(loaded.volume, 0.13);
            assert_eq!(loaded.selection_binding_label(2), "K");
            assert_eq!(loaded.selection_binding_label(3), "V");
            return;
        }
        let mut a = harness();
        a.world_mut().resource_mut::<Settings>().volume = 0.13;
        {
            let mut menu = a.world_mut().resource_mut::<Menu>();
            menu.in_match = std::env::var("CSR_CONTROLS_RESTART_MATCH").unwrap() == "1";
            menu.open_classic_controls();
        }
        for (index, key) in [(23, KeyCode::KeyK), (24, KeyCode::KeyV)] {
            a.world_mut().resource_mut::<Menu>().capture = Some(index);
            a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
            a.update();
            a.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        }
        let action = a.world_mut().spawn((Interaction::Pressed, Action::Adjust(28, 1.0))).id();
        a.update();
        a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::Adjust(30, -1.0)));
        a.update();
        a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::ControlsBack));
        a.update();
        assert!(!a.world().resource::<Menu>().classic_controls);
        // This process exits here. The parent starts a new process for reading;
        // no Apply click or in-memory Settings clone crosses that boundary.
    }
    #[test]
    fn classic_controls_apply_reopen_capture_and_back_use_private_preferences() {
        const CHILD: &str = "CSR_CONTROLS_MENU_FIXTURE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let data = std::env::temp_dir().join(format!("csr-menu-controls-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            std::fs::create_dir_all(&data).unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "menu::tests::classic_controls_apply_reopen_capture_and_back_use_private_preferences", "--nocapture"])
                .env(CHILD, "1").current_dir(data).output().unwrap();
            assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
            return;
        }
        for in_match in [false, true] {
            let mut a = harness();
            a.world_mut().resource_mut::<Menu>().in_match = in_match;
            a.world_mut().resource_mut::<Menu>().open_classic_controls();
            let action = a.world_mut().spawn((Interaction::Pressed, Action::Bind(ACTIONS.len()))).id();
            a.update();
            assert_eq!(a.world().resource::<Menu>().capture, Some(ACTIONS.len()));
            a.world_mut().entity_mut(action).insert(Interaction::None);
            a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Backslash);
            a.update();
            assert_eq!(a.world().resource::<Settings>().console_key, Some(KeyCode::Backslash));
            a.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::ClearBind(4)));
            a.update();
            a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::Adjust(25, 1.0)));
            a.update();
            a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::Adjust(30, 1.0)));
            a.update();
            a.world_mut().resource_mut::<Settings>().sensitivity = 1.375;
            a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::Save));
            a.update();
            assert_eq!(a.world().resource::<Menu>().status, "Settings saved.");
            let expected = a.world().resource::<Settings>().clone();
            assert_eq!(Settings::load(), expected);
            a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::ControlsBack));
            a.update();
            assert!(!a.world().resource::<Menu>().classic_controls);
            assert!(a.world().resource::<Menu>().controls_just_closed);
            assert_eq!(a.world().resource::<Menu>().page, Some(if in_match { Page::Pause } else { Page::Main }));
            a.world_mut().entity_mut(action).insert(Interaction::None);
            a.world_mut().resource_mut::<Menu>().open_classic_controls();
            assert_eq!(*a.world().resource::<Settings>(), expected);
            a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
            a.update();
            assert!(!a.world().resource::<Menu>().classic_controls);
            assert_eq!(a.world().resource::<Menu>().page, Some(if in_match { Page::Pause } else { Page::Main }));
            // Return keeps the menu/cursor open; the same Escape does not resume.
            assert!(a.world().resource::<Menu>().open());
        }
    }

    #[test]
    fn controls_restore_apply_reload_preserves_unrelated_preferences() {
        const CHILD: &str = "CSR_CONTROLS_DEFAULTS_FIXTURE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let data = std::env::temp_dir().join(format!("csr-controls-defaults-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            std::fs::create_dir_all(&data).unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "menu::tests::controls_restore_apply_reload_preserves_unrelated_preferences", "--nocapture"])
                .env(CHILD, "1").current_dir(&data).output().unwrap();
            std::fs::remove_dir_all(&data).unwrap();
            assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
            return;
        }
        // Every unrelated persisted field differs from its default. Equality
        // after real UI actions and disk reload detects a whole-settings reset.
        let sentinel = Settings::parse(concat!(
            "volume=0.73\nsnd_mute_losefocus=0\nsnd_menumusic_volume=0.27\n",
            "cl_showfps=1\nmenu_blur=false\nexposure=3.25\nvsync=true\nfullscreen=true\n",
            "resolution=3\nmsaa=false\nblood=false\nhitmarker=true\n",
            "cross_color=3\ncross_size=13\ncross_gap=7\ncross_thickness=4\ncross_dot=true\n",
            "radar_scale=0.8\nradar_rotate=false\ncloseonbuy=1\n"
        ));
        for in_match in [false, true] {
            let mut a = harness();
            let mut changed = sentinel.clone();
            changed.controls_source = "Fixture import".into();
            changed.binds.fill(None);
            changed.console_key = Some(KeyCode::Backslash);
            changed.mouse_binds.fill(crate::settings::MouseAction::Action(4));
            changed.wheel_up = crate::settings::WheelAction::Jump;
            changed.wheel_down = crate::settings::WheelAction::None;
            changed.sensitivity = 3.375;
            changed.invert_y = true;
            changed.save().unwrap();
            a.insert_resource(changed.clone());
            a.world_mut().resource_mut::<Menu>().in_match = in_match;
            a.world_mut().resource_mut::<Menu>().open_classic_controls();
            let action = a.world_mut().spawn((Interaction::Pressed, Action::ControlsDefaults)).id();
            a.update();
            let defaults = Settings::default();
            let mut expected = sentinel.clone();
            expected.binds = defaults.binds;
            expected.console_key = defaults.console_key;
            expected.mouse_binds = defaults.mouse_binds;
            expected.wheel_up = defaults.wheel_up;
            expected.wheel_down = defaults.wheel_down;
            expected.sensitivity = defaults.sensitivity;
            expected.invert_y = defaults.invert_y;
            expected.controls_source = defaults.controls_source;
            assert_eq!(*a.world().resource::<Settings>(), expected);
            assert_eq!(Settings::load(), changed, "Restore waits for Apply before saving");
            a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::Save));
            a.update();
            assert_eq!(a.world().resource::<Menu>().status, "Settings saved.");
            let reloaded = Settings::load();
            assert_eq!(reloaded, expected);
            a.world_mut().entity_mut(action).insert((Interaction::Pressed, Action::ControlsBack));
            a.update();
            assert_eq!(a.world().resource::<Menu>().page, Some(if in_match { Page::Pause } else { Page::Main }));
            a.insert_resource(reloaded);
            a.world_mut().resource_mut::<Menu>().open_classic_controls();
            assert_eq!(*a.world().resource::<Settings>(), expected);
        }
    }

    #[test]
    fn controls_body_scrolls_independently_of_footer_and_keeps_every_binding_row() {
        let mut a = App::new();
        a.init_resource::<Menu>().init_resource::<Settings>().add_systems(Update,
            |mut commands: Commands, menu: Res<Menu>, settings: Res<Settings>| {
                draw_classic_controls(&mut commands, &menu, &settings);
            });
        a.update();
        let world = a.world_mut();
        let mut query = world.query_filtered::<(Entity, &ChildOf, &Node), With<ControlsScroll>>();
        let (body, root) = {
            let (body, parent, node) = query.single(world).unwrap();
            assert_eq!(node.overflow, Overflow::scroll_y());
            assert_eq!(node.min_height, px(0.0));
            (body, parent.parent())
        };
        let buttons: Vec<_> = world.query::<(&Action, &ChildOf)>().iter(world)
            .map(|(action, parent)| (*action, parent.parent())).collect();
        assert_eq!(buttons.iter().filter(|(action, _)| matches!(action, Action::Bind(_))).count(), ACTIONS.len() + 1);
        assert_eq!(buttons.iter().filter(|(action, _)| matches!(action, Action::ControlsDefaults)).count(), 1);
        assert!(!buttons.iter().any(|(action, _)| matches!(action, Action::Defaults)));
        for (_, parent) in buttons.iter().filter(|(action, _)| matches!(action, Action::Save | Action::ControlsBack)) {
            assert_ne!(*parent, body);
            assert_eq!(world.get::<ChildOf>(*parent).unwrap().parent(), root);
            assert_eq!(world.get::<Node>(*parent).unwrap().flex_shrink, 0.0);
        }
    }

    #[test]
    fn options_escape_returns_to_pause_then_each_press_toggles_once() {
        let mut menu = Menu::default();
        assert!(!menu.escape(false));
        assert_eq!(menu.page, Some(Page::Pause));
        // Native Options does not change the host's Page::Pause.
        assert!(menu.escape(true));
        assert_eq!(menu.page, Some(Page::Pause));
        // The owned Options callback closes the child; the next press resumes.
        assert!(!menu.escape(false));
        assert_eq!(menu.page, None);
        for _ in 0..4 {
            assert!(!menu.escape(false));
            assert_eq!(menu.page, Some(Page::Pause));
            assert!(!menu.escape(false));
            assert_eq!(menu.page, None);
        }
    }
    fn harness() -> App {
        let mut a = App::new();
        a.init_resource::<Menu>()
            .init_resource::<Settings>()
            .init_resource::<LoadRequest>()
            .init_resource::<crate::inventory::Inventory>()
            .init_resource::<crate::inventory::Collection>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Time<Virtual>>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .insert_resource(crate::Options(crate::ClientOptions::default()))
            .insert_resource(LocalPlayer::new(1, glam::Vec3::ZERO, 0.0))
            .insert_resource(InputEnabled(true))
            .add_message::<bevy::input::keyboard::KeyboardInput>()
            .add_systems(Update, (input, crate::player::grab_cursor).chain());
        a.world_mut().spawn((
            PrimaryWindow,
            CursorOptions {
                grab_mode: CursorGrabMode::Locked,
                visible: false,
                ..default()
            },
        ));
        a
    }
    #[test]
    fn map_cards_select_directly_and_ignore_invalid_indices() {
        let mut a = harness();
        {
            let mut m = a.world_mut().resource_mut::<Menu>();
            m.page = Some(Page::Play);
            m.maps = (0..27).map(|i| format!("de_test{i}")).collect();
        }
        let e = a
            .world_mut()
            .spawn((Interaction::Pressed, Action::SelectMap(26)))
            .id();
        a.update();
        assert_eq!(a.world().resource::<Menu>().map, 26);
        a.world_mut()
            .entity_mut(e)
            .insert((Interaction::Pressed, Action::SelectMap(99)));
        a.update();
        assert_eq!(a.world().resource::<Menu>().map, 26);
    }
    #[test]
    fn remote_pause_releases_controls_without_freezing_presentation() {
        let mut a = harness();
        a.insert_resource(crate::remote::RemoteClient::discover(
            "127.0.0.1:1".parse().unwrap(), "Test".into(), sim::Team::CounterTerrorist,
        ).unwrap());
        a.world_mut().resource_mut::<LocalPlayer>().latched = movement::buttons::JUMP;
        a.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        a.update();
        assert!(a.world().resource::<Menu>().open());
        assert!(!a.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(a.world().resource::<LocalPlayer>().latched, 0);
        let mut cursor = a.world_mut().query::<&CursorOptions>();
        assert_eq!(cursor.single(a.world()).unwrap().grab_mode, CursorGrabMode::None);
    }

    #[test]
    fn escape_pause_resume_clears_actions_and_recaptures_cursor() {
        let mut a = harness();
        {
            let mut l = a.world_mut().resource_mut::<LocalPlayer>();
            l.latched = 1;
            l.pending_drop = true;
            l.pending_slot = 2;
        }
        a.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        a.update();
        assert_eq!(a.world().resource::<Menu>().page, Some(Page::Pause));
        assert!(a.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(a.world().resource::<LocalPlayer>().latched, 0);
        assert!(!a.world().resource::<LocalPlayer>().pending_drop);
        a.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        a.update();
        a.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        a.update();
        assert!(!a.world().resource::<Menu>().open());
        assert!(!a.world().resource::<Time<Virtual>>().is_paused());
        assert!(a.world().resource::<crate::player::InputBlockFrames>().0 > 0);
        let mut q = a.world_mut().query::<&CursorOptions>();
        let cursor = q.single(a.world()).unwrap();
        assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
        assert!(!cursor.visible);
    }
    #[test]
    fn a_movie_callback_closing_pause_resumes_clock_and_blocks_its_click() {
        let mut a = harness();
        a.world_mut().resource_mut::<Menu>().page = Some(Page::Pause);
        a.update();
        assert!(a.world().resource::<Time<Virtual>>().is_paused());
        // The movie queues this change during Update, after the input pass.
        a.world_mut().resource_mut::<Menu>().page = None;
        a.update();
        assert!(!a.world().resource::<Time<Virtual>>().is_paused());
        assert!(a.world().resource::<crate::player::InputBlockFrames>().0 > 0);
        let mut q = a.world_mut().query::<&CursorOptions>();
        let cursor = q.single(a.world()).unwrap();
        assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
        assert!(!cursor.visible);
    }
    #[test]
    fn hostage_map_cannot_select_unimplemented_bomb_rules() {
        let mut a = harness();
        {
            let mut m = a.world_mut().resource_mut::<Menu>();
            m.page = Some(Page::Play);
            m.maps = vec!["de_dust2".into(), "cs_office".into()];
        }
        let e = a
            .world_mut()
            .spawn((Interaction::Pressed, Action::SelectMap(1)))
            .id();
        a.update();
        assert_eq!(a.world().resource::<Menu>().map, 1);
        assert!(a.world().resource::<Menu>().deathmatch);
        a.world_mut().entity_mut(e).insert(Action::Mode);
        a.world_mut().entity_mut(e).insert(Interaction::Pressed);
        a.update();
        assert!(a.world().resource::<Menu>().deathmatch);
    }
    #[test]
    fn clicking_resume_leaves_mouse_gated_until_release() {
        let mut a = harness();
        a.world_mut().resource_mut::<Menu>().page = Some(Page::Pause);
        a.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        a.world_mut().spawn((Interaction::Pressed, Action::Resume));
        a.update();
        assert!(!a.world().resource::<Menu>().open());
        assert!(a.world().resource::<crate::player::InputBlockFrames>().0 > 0);
    }
}
