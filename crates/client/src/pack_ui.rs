//! Private packs use an explicit local manager alongside the authored menu.
//! Installation never mutates the mounted level; Play binds a catalog snapshot.
use crate::{
    frontend::Frontend,
    pack_flow::{Flow, caption},
    pack_picker::{PackPicker, PickerOutcome},
    session::{AppState, Persistent},
};
use bevy::{
    app::AppExit,
    prelude::*,
    window::{FileDragAndDrop, PrimaryWindow, WindowCloseRequested, WindowFocused},
};
use std::sync::Mutex;

struct Backend {
    flow: Flow,
    picker: Option<PackPicker>,
}
#[derive(Resource)]
pub(crate) struct PackUi {
    backend: Mutex<Backend>,
    open: bool,
    root: Option<Entity>,
    page: usize,
    drawn: String,
    blocked: u8,
    hover: bool,
    was_menu: bool,
    exit: Option<AppExit>,
    feedback: f64,
}
impl Default for PackUi {
    fn default() -> Self {
        Self {
            backend: Mutex::new(Backend {
                flow: Flow::new(packs::Installed::current()),
                picker: None,
            }),
            open: false,
            root: None,
            page: 0,
            drawn: String::new(),
            blocked: 0,
            hover: false,
            was_menu: false,
            exit: None,
            feedback: 0.,
        }
    }
}
impl PackUi {
    pub(crate) fn blocks_movie(&self) -> bool {
        self.open || self.blocked > 0 || self.hover
    }
}
#[derive(Component, Clone)]
pub(crate) enum Action {
    Open,
    Close,
    Import,
    Cancel,
    Refresh,
    Previous,
    Next,
    Play(packs::CatalogEntry, content::ContentId),
}
#[derive(Component)]
pub(crate) struct MapArea;

pub(crate) fn scroll(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    ui: Res<PackUi>,
    mut areas: Query<&mut ScrollPosition, With<MapArea>>,
) {
    let delta: f32 = wheel
        .read()
        .map(|event| match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y * 28.,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y,
        })
        .sum();
    if ui.open {
        for mut position in &mut areas {
            position.0.y = (position.0.y - delta).max(0.);
        }
    }
}

pub(crate) fn update(
    mut ui: ResMut<PackUi>,
    state: Res<State<AppState>>,
    window: Single<Entity, With<PrimaryWindow>>,
    mut drops: MessageReader<FileDragAndDrop>,
    mut focus: MessageReader<WindowFocused>,
    mut closes: MessageReader<WindowCloseRequested>,
    mut exit: MessageWriter<AppExit>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    all_buttons: Query<&Interaction, With<Action>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    opts: Res<crate::Options>,
    request: Res<crate::menu::LoadRequest>,
    time: Res<Time<Real>>,
) {
    let ui = &mut *ui;
    let menu = *state.get() == AppState::Menu;
    ui.blocked = ui.blocked.saturating_sub(1);
    ui.feedback = (ui.feedback - time.delta_secs_f64()).max(0.);
    ui.hover = menu && all_buttons.iter().any(|value| *value != Interaction::None);
    let backend = ui.backend.get_mut().unwrap();
    if menu && !ui.was_menu {
        backend.flow.request_refresh();
    }
    ui.was_menu = menu;
    for event in focus.read() {
        if menu && event.window == *window && event.focused {
            backend.flow.request_refresh();
        }
    }
    for event in drops.read() {
        if let FileDragAndDrop::DroppedFile {
            window: target,
            path_buf,
        } = event
        {
            if *target == *window && ui.exit.is_none() {
                if let Err(error) = backend.flow.install(path_buf.clone()) {
                    backend.flow.notice = error;
                }
                ui.open = menu;
                ui.feedback = 10.;
            }
        }
    }
    if closes.read().any(|event| event.window == *window) {
        exit.write(AppExit::Success);
    }
    if let Some(picker) = &mut backend.picker {
        if let Some(outcome) = picker.poll() {
            backend.picker = None;
            match outcome {
                PickerOutcome::Selected(path) => {
                    if let Err(error) = backend.flow.install(path) {
                        backend.flow.notice = error;
                    }
                    ui.feedback = 10.;
                }
                PickerOutcome::Failed(error) => backend.flow.notice = caption(&error),
                PickerOutcome::Cancelled => {}
            }
        }
    }
    if menu && ui.exit.is_none() {
        for (interaction, action) in &buttons {
            if *interaction != Interaction::Pressed || !mouse.just_pressed(MouseButton::Left) {
                continue;
            }
            ui.blocked = 2;
            match action {
                Action::Open => {
                    ui.open = true;
                    backend.flow.request_refresh();
                }
                Action::Close => ui.open = false,
                Action::Import if backend.picker.is_none() => {
                    backend.picker = Some(PackPicker::start())
                }
                Action::Cancel => {
                    backend.picker = None;
                    backend.flow.cancel();
                }
                Action::Refresh => backend.flow.request_refresh(),
                Action::Previous => ui.page = ui.page.saturating_sub(1),
                Action::Next => ui.page = ui.page.saturating_add(1),
                Action::Play(entry, id) if !backend.flow.busy() => {
                    let mut options = opts.0.clone();
                    options.map = id.to_string();
                    options.direct = true;
                    options.deathmatch = true;
                    *request.0.lock().unwrap() = Some(crate::menu::MapRequest {
                        options,
                        pack: Some((backend.flow.store(), entry.clone(), id.clone())),
                    });
                    ui.open = false;
                }
                _ => {}
            }
        }
        if ui.open && keys.just_pressed(KeyCode::Escape) {
            ui.open = false;
            ui.blocked = 2;
        }
    }
    backend.flow.poll();
    if ui.feedback > 0. && backend.flow.busy() {
        ui.feedback = 10.;
    }
    if !menu {
        ui.open = false;
    }
}

fn button(
    commands: &mut Commands,
    parent: Entity,
    label: &str,
    action: Action,
    font: Handle<Font>,
) {
    let entity = commands
        .spawn((
            Persistent,
            Button,
            action,
            Node {
                min_height: px(32),
                padding: UiRect::axes(px(12), px(6)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.2, 0.25, 0.28, 0.95)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Persistent,
                Text::new(label),
                TextFont {
                    font: font.into(),
                    font_size: FontSize::Px(17.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        })
        .id();
    commands.entity(parent).add_children(&[entity]);
}
fn text(
    commands: &mut Commands,
    parent: Entity,
    label: impl Into<String>,
    font: Handle<Font>,
    size: f32,
) {
    let entity = commands
        .spawn((
            Persistent,
            Text::new(label),
            TextFont {
                font: font.into(),
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(parent).add_children(&[entity]);
}

pub(crate) fn draw(
    mut commands: Commands,
    mut ui: ResMut<PackUi>,
    state: Res<State<AppState>>,
    front: Res<Frontend>,
    art: Res<crate::ui_assets::UiAssets>,
    movie_menu: Res<crate::menu::Menu>,
) {
    let ui = &mut *ui;
    let menu = *state.get() == AppState::Menu;
    let backend = ui.backend.get_mut().unwrap();
    let count = backend
        .flow
        .catalog
        .entries
        .iter()
        .map(|entry| entry.manifest().maps.len())
        .sum::<usize>();
    let pages = count.div_ceil(8).max(1);
    ui.page = ui.page.min(pages - 1);
    let status = if ui.exit.is_some() {
        "Finishing cancellation before closing…".into()
    } else if backend.picker.is_some() {
        "Choose a content pack in the file window…".into()
    } else {
        backend.flow.status()
    };
    let feedback = !menu && ui.feedback > 0.;
    let front_camera = menu || *state.get() == AppState::Loading || movie_menu.open();
    let key = format!(
        "{:?}:{front_camera}:{}:{}:{}:{status}:{}:{feedback}",
        state.get(),
        ui.open,
        ui.page,
        backend.flow.revision,
        backend.flow.busy()
    );
    if key == ui.drawn {
        return;
    }
    ui.drawn = key;
    if let Some(root) = ui.root.take() {
        commands.entity(root).despawn();
    }
    if !menu && !feedback {
        return;
    }
    let root = commands
        .spawn((
            Persistent,
            Node {
                position_type: PositionType::Absolute,
                right: px(24),
                bottom: px(20),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                width: if ui.open || feedback {
                    percent(90)
                } else {
                    px(180)
                },
                max_width: px(700),
                padding: UiRect::all(px(14)),
                max_height: percent(85),
                ..default()
            },
            GlobalZIndex(110),
            BackgroundColor(Color::srgba(0.035, 0.055, 0.065, 0.98)),
        ))
        .id();
    if front_camera {
        commands.entity(root).insert(UiTargetCamera(front.camera()));
    }
    ui.root = Some(root);
    let font = art.regular.clone();
    if feedback {
        text(&mut commands, root, status, font, 17.0);
        return;
    }
    if !ui.open {
        button(&mut commands, root, "Content packs", Action::Open, font);
        return;
    }
    text(&mut commands, root, "CONTENT PACKS", font.clone(), 24.0);
    text(
        &mut commands,
        root,
        "Drop a .csrpack file onto the game, or choose Install pack. Maps launch in deathmatch.",
        font.clone(),
        17.0,
    );
    let controls = commands
        .spawn((
            Persistent,
            Node {
                column_gap: px(8),
                row_gap: px(8),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
        ))
        .id();
    commands.entity(root).add_children(&[controls]);
    for (label, action) in [
        ("Install pack", Action::Import),
        ("Refresh", Action::Refresh),
        ("Cancel", Action::Cancel),
        ("Close", Action::Close),
    ] {
        button(&mut commands, controls, label, action, font.clone());
    }
    if !status.is_empty() {
        text(&mut commands, root, status, font.clone(), 17.0);
    }
    if let Some(error) = backend.flow.catalog.errors.first() {
        text(
            &mut commands,
            root,
            format!("A pack could not be read: {}", caption(error)),
            font.clone(),
            16.0,
        );
    }
    text(
        &mut commands,
        root,
        format!(
            "{} installed packs · {count} maps",
            backend.flow.catalog.entries.len()
        ),
        font.clone(),
        17.0,
    );
    if count == 0 {
        text(
            &mut commands,
            root,
            "No installed maps. Packs containing other resources are kept for future use.",
            font.clone(),
            17.0,
        );
    }
    let maps = commands
        .spawn((
            Persistent,
            MapArea,
            ScrollPosition::default(),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                max_height: px(300),
                min_height: px(0),
                flex_shrink: 1.,
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    commands.entity(root).add_children(&[maps]);
    for (entry, map) in backend
        .flow
        .catalog
        .entries
        .iter()
        .flat_map(|entry| entry.manifest().maps.iter().map(move |map| (entry, map)))
        .skip(ui.page * 8)
        .take(8)
    {
        let Ok(id) = map.id.parse::<content::ContentId>() else {
            continue;
        };
        let label = format!(
            "{} · {}",
            caption(&map.name).chars().take(64).collect::<String>(),
            caption(&entry.manifest().name)
                .chars()
                .take(64)
                .collect::<String>()
        );
        button(
            &mut commands,
            maps,
            &label,
            Action::Play(entry.clone(), id),
            font.clone(),
        );
    }
    if pages > 1 {
        let row = commands
            .spawn((
                Persistent,
                Node {
                    column_gap: px(12),
                    ..default()
                },
            ))
            .id();
        commands.entity(root).add_children(&[row]);
        button(
            &mut commands,
            row,
            "Previous",
            Action::Previous,
            font.clone(),
        );
        text(
            &mut commands,
            row,
            format!("{} / {pages}", ui.page + 1),
            font.clone(),
            17.0,
        );
        button(&mut commands, row, "Next", Action::Next, font);
    }
}

/// App::should_exit reads the message buffer after Last. Hold exit until the
/// owned pack worker has finished its cleanup; the window stays alive meanwhile.
pub(crate) fn gate_exit(mut ui: ResMut<PackUi>, mut messages: ResMut<Messages<AppExit>>) {
    let ui = &mut *ui;
    for exit in messages.drain() {
        if ui.exit.as_ref().is_none_or(|old| !old.is_error()) {
            ui.exit = Some(exit);
        }
    }
    let Some(exit) = ui.exit.clone() else {
        return;
    };
    let backend = ui.backend.get_mut().unwrap();
    backend.picker = None;
    if backend.flow.busy() {
        backend.flow.cancel();
        ui.open = true;
    } else {
        messages.write(exit);
        ui.exit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_waits_for_owned_catalog_cleanup_and_preserves_error_status() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppExit>()
            .init_resource::<PackUi>()
            .add_systems(Update, |mut ui: ResMut<PackUi>| {
                ui.backend.get_mut().unwrap().flow.poll();
            })
            .add_systems(Last, gate_exit);
        {
            let mut ui = app.world_mut().resource_mut::<PackUi>();
            ui.backend.get_mut().unwrap().flow = Flow::new(packs::Installed::new(
                &std::env::temp_dir().join(format!("csr-exit-catalog-{}", std::process::id())),
            ));
        }
        app.world_mut()
            .resource_mut::<Messages<AppExit>>()
            .write(AppExit::Success);
        app.world_mut()
            .resource_mut::<Messages<AppExit>>()
            .write(AppExit::from_code(7));
        app.update();
        assert!(
            app.should_exit().is_none(),
            "exit bypassed the owned worker"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                assert_eq!(exit, AppExit::from_code(7));
                assert!(
                    !app.world()
                        .resource::<PackUi>()
                        .backend
                        .lock()
                        .unwrap()
                        .flow
                        .busy()
                );
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

pub(crate) fn proof_enabled() -> bool {
    std::env::var_os("CSR_TEST_PACK_FLOW").is_some()
}
pub(crate) struct Proof {
    start: std::time::Instant,
    entered: std::time::Instant,
    phase: u8,
    hash: String,
    id: String,
}
impl Default for Proof {
    fn default() -> Self {
        Self {
            start: std::time::Instant::now(),
            entered: std::time::Instant::now(),
            phase: 0,
            hash: String::new(),
            id: String::new(),
        }
    }
}
impl Proof {
    fn next(&mut self) {
        self.phase += 1;
        self.entered = std::time::Instant::now();
        info!("pack flow: phase {}", self.phase);
    }
}
/// Opt-in engine-input proof, with private fixtures and captures outside the repo.
/// This exercises Bevy's pointer hit testing; it does not claim an OS dialog test.
pub(crate) fn proof(
    mut proof: Local<Proof>,
    mut ui: ResMut<PackUi>,
    front: Option<Res<Frontend>>,
    state: Res<State<AppState>>,
    mut menu: ResMut<crate::menu::Menu>,
    opts: Res<crate::Options>,
    level: Option<Res<crate::LoadedLevel>>,
    sim: Option<Res<crate::player::SimWorld>>,
    entities: Query<(), With<crate::session::MatchEntity>>,
    mut window: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    buttons: Query<(&Action, &ComputedNode, &bevy::ui::UiGlobalTransform)>,
    mut drops: MessageWriter<FileDragAndDrop>,
    mut mouse: MessageWriter<bevy::input::mouse::MouseButtonInput>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(front) = front else {
        return;
    };
    if proof.start.elapsed().as_secs() > 180
        || front.failed.is_some()
        || opts.0.screenshot.is_none()
    {
        error!(
            "pack flow failed at phase {}: timeout, movie failure or missing screenshot",
            proof.phase
        );
        exit.write(AppExit::error());
        return;
    }
    let capture = |commands: &mut Commands, name: &str| {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(
                opts.0.screenshot.as_ref().unwrap().with_file_name(name),
            ));
    };
    let window_id = window.0;
    match proof.phase {
        0 if front.ready && *state.get() == AppState::Menu && proof.start.elapsed().as_secs() >= 5 => {
            let source = std::path::PathBuf::from(std::env::var_os("CSR_TEST_PACK_FLOW").unwrap());
            drops.write(FileDragAndDrop::DroppedFile { window: window_id, path_buf: source }); proof.next();
        },
        1 => {
            let backend = ui.backend.get_mut().unwrap();
            if !backend.flow.busy() {
                if let Some(entry) = backend.flow.catalog.entries.first() {
                    if let Some(map) = entry.manifest().maps.first() { proof.hash = entry.archive_hash().into(); proof.id = map.id.clone(); proof.next(); }
                    else { error!("pack flow requires a map fixture"); exit.write(AppExit::error()); }
                } else { error!("pack flow install failed: {}", backend.flow.status()); exit.write(AppExit::error()); }
            }
        },
        2 if proof.entered.elapsed().as_millis() > 500 => {
            if let Some((_, node, transform)) = buttons.iter().find(|(action, _, _)| matches!(action, Action::Play(_, id) if id.to_string() == proof.id)) {
                if node.size().min_element() <= 0. { return; }
                capture(&mut commands, "pack-installed.png");
                let position = transform.affine().translation;
                info!("pack flow: pointer on map at {position:?}");
                window.1.set_physical_cursor_position(Some(position.as_dvec2())); proof.next();
            }
        },
        3 => { mouse.write(bevy::input::mouse::MouseButtonInput { window: window_id, button: MouseButton::Left, state: bevy::input::ButtonState::Pressed }); proof.next(); },
        4 => { mouse.write(bevy::input::mouse::MouseButtonInput { window: window_id, button: MouseButton::Left, state: bevy::input::ButtonState::Released }); proof.next(); },
        5 if *state.get() == AppState::Match && sim.as_ref().is_some_and(|sim| sim.0.tick > 300) => {
            let level = level.unwrap();
            if level.0.id.to_string() != proof.id || level.0.pack_archive_hash() != Some(proof.hash.as_str()) { error!("pack flow mounted a different selection"); exit.write(AppExit::error()); return; }
            capture(&mut commands, "pack-match.png"); proof.next();
        },
        6 if proof.entered.elapsed().as_secs_f64() > 1. => { menu.in_match = false; menu.page = Some(crate::menu::Page::Main); proof.next(); },
        7 if *state.get() == AppState::Menu && level.is_none() && sim.is_none() && entities.is_empty() && proof.entered.elapsed().as_secs() >= 3 => {
            capture(&mut commands, "pack-returned-home.png");
            let mut broken = std::path::PathBuf::from(std::env::var_os("CSR_TEST_PACK_FLOW").unwrap()); broken.set_file_name("Broken content.csrpack");
            drops.write(FileDragAndDrop::DroppedFile { window: window_id, path_buf: broken }); proof.next();
        },
        8 if proof.entered.elapsed().as_secs_f64() > 1. => {
            let backend = ui.backend.get_mut().unwrap();
            if !backend.flow.busy() {
                if !backend.flow.notice.starts_with("Could not install") || !backend.flow.catalog.entries.iter().any(|entry| entry.archive_hash() == proof.hash) { error!("pack flow did not retain the previous pack after failure"); exit.write(AppExit::error()); return; }
                capture(&mut commands, "pack-rejected.png"); proof.next();
            }
        },
        9 if proof.entered.elapsed().as_secs() >= 1 => { info!("pack flow: install -> catalog -> pointer Play -> bound match -> Home -> malformed rollback; failures: 0"); exit.write(AppExit::Success); proof.next(); },
        _ => {},
    }
}
