//! Root ownership for the native browser view and local discovery backend.
use crate::{
    frontend_servers as view, frontend_servers_data as ui, remote_servers as net, session::AppState,
};
use bevy::{
    input::{
        keyboard::KeyboardInput,
        mouse::{MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    window::PrimaryWindow,
};
use std::{path::PathBuf, sync::Mutex};

#[derive(Resource, Default)]
pub(crate) struct Browser {
    backend: Mutex<Option<net::Backend>>,
    definition: Option<ui::Definition>,
    fonts: view::Fonts,
    model: ui::Model,
    view: Option<view::View>,
    paint: String,
    was_open: bool,
    refresh_pending: bool,
    error: Option<String>,
    exit: Option<AppExit>,
}

impl Browser {
    pub(crate) fn model(&self) -> &ui::Model {
        &self.model
    }
    pub(crate) fn snapshot(&self) -> Option<ui::Snapshot> {
        self.backend.lock().unwrap().as_ref().map(snapshot)
    }
}

fn data_root() -> Result<PathBuf, String> {
    let current = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = std::env::var_os("CSR_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| current.clone());
    Ok(if root.is_absolute() {
        root
    } else {
        current.join(root)
    })
}

/// Installed UI definitions and fonts are read once alongside front-end setup.
/// Favorites, DNS and network operations remain in the owned backend worker.
pub(crate) fn setup(
    mut browser: ResMut<Browser>,
    data: Res<crate::GameData>,
    mut fonts: ResMut<Assets<Font>>,
) {
    let result = (|| {
        let backend = net::Backend::new(data_root()?)?;
        let parent = data
            .game_dir
            .parent()
            .ok_or("CS:GO Legacy platform folder is unavailable")?;
        let mut platform = vfs::Vfs::new();
        platform.push_back("legacy-platform", vfs::Mount::Dir(parent.join("platform")));
        let definition = ui::Definition::load(&data.vfs, &platform)?;
        let windows =
            PathBuf::from(std::env::var_os("WINDIR").ok_or("Windows font folder is unavailable")?);
        let normal = std::fs::read(windows.join("Fonts/tahoma.ttf"))
            .map_err(|e| format!("Could not load Tahoma: {e}"))?;
        let bold = std::fs::read(windows.join("Fonts/tahomabd.ttf"))
            .map_err(|e| format!("Could not load bold Tahoma: {e}"))?;
        Ok::<_, String>((backend, definition, normal, bold))
    })();
    match result {
        Ok((backend, definition, normal, bold)) => {
            browser
                .fonts
                .insert("Default", fonts.add(Font::from_bytes(normal)));
            browser
                .fonts
                .insert("UiBold", fonts.add(Font::from_bytes(bold)));
            browser.definition = Some(definition);
            *browser.backend.get_mut().unwrap() = Some(backend);
        }
        Err(error) => browser.error = Some(error),
    }
}

fn snapshot(backend: &net::Backend) -> ui::Snapshot {
    let source = backend.snapshot();
    ui::Snapshot {
        generation: source.generation,
        rows: source
            .rows
            .into_iter()
            .map(|row| ui::ServerRow {
                key: ui::ServerKey(row.key.0),
                endpoint: row.endpoint,
                name: row.name,
                map: row.map,
                players: row.players,
                ping_ms: row.ping_ms,
                favorite: row.favorite,
                connectable: row.connectable,
            })
            .collect(),
        busy: source.busy,
        error: source.error,
        status: Some(source.status),
    }
}

fn tab(tab: ui::Tab) -> net::Tab {
    match tab {
        ui::Tab::Internet => net::Tab::Internet,
        ui::Tab::Favorites => net::Tab::Favorites,
    }
}
fn intent(intent: ui::Intent) -> net::Intent {
    match intent {
        ui::Intent::Refresh(value) => net::Intent::Refresh(tab(value)),
        ui::Intent::ProbeAddress(value) => net::Intent::ProbeAddress(value),
        ui::Intent::AddFavorite(value) => net::Intent::AddFavorite(value),
        ui::Intent::RemoveFavorite { key, generation } => net::Intent::RemoveFavorite {
            key: net::ServerKey(key.0),
            generation,
        },
        ui::Intent::Connect { key, generation } => net::Intent::Connect {
            key: net::ServerKey(key.0),
            generation,
        },
        ui::Intent::CancelAddress => net::Intent::CancelAddress,
        ui::Intent::Close => net::Intent::Close,
    }
}

fn cancel_superseded(intents: &mut Vec<ui::Intent>) {
    if intents.contains(&ui::Intent::CancelAddress) {
        intents.retain(|intent| {
            !matches!(
                intent,
                ui::Intent::ProbeAddress(_) | ui::Intent::AddFavorite(_)
            )
        });
    }
}

pub(crate) fn update(
    mut browser: ResMut<Browser>,
    state: Res<State<AppState>>,
    mut menu: ResMut<crate::menu::Menu>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
) {
    let browser = &mut *browser;
    let open = *state.get() == AppState::Browser && browser.exit.is_none();
    if let Some(backend) = browser.backend.get_mut().unwrap() {
        backend.poll();
        if open && !browser.was_open {
            browser.refresh_pending = true;
        } else if !open && browser.was_open {
            backend.cancel();
            browser.refresh_pending = false;
        }
        if open && browser.refresh_pending && !backend.busy() {
            let _ = backend.apply(net::Intent::Refresh(tab(browser.model.tab)));
            browser.refresh_pending = false;
        }
    }
    if !open {
        browser.model.blur();
        if let Some(view) = browser.view.take() {
            view.despawn(&mut commands);
        }
        browser.paint.clear();
    } else if let Some(error) = &browser.error {
        menu.status = error.clone();
        next.set(AppState::Menu);
    }
    browser.was_open = open;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn input(
    mut browser: ResMut<Browser>,
    state: Res<State<AppState>>,
    window: Single<(Entity, &Window), With<PrimaryWindow>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut wheel: MessageReader<MouseWheel>,
    buttons: Query<(&Interaction, &view::UiAction)>,
    request: Res<crate::remote_join::JoinRequest>,
    options: Res<crate::Options>,
    mut next: ResMut<NextState<AppState>>,
) {
    let browser = &mut *browser;
    let (primary, window) = *window;
    let owns = *state.get() == AppState::Browser && window.focused && browser.exit.is_none();
    if !owns {
        browser.model.blur();
        keyboard.clear();
        wheel.clear();
        return;
    }
    let Some(backend) = browser.backend.get_mut().unwrap() else {
        keyboard.clear();
        return;
    };
    let current = snapshot(backend);
    browser.model.reconcile(&current);
    let mut intents = Vec::new();
    for (interaction, action) in &buttons {
        if *interaction == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) {
            if let Some(intent) = view::interact(action, owns, &mut browser.model, &current) {
                intents.push(intent);
            }
        }
        if *interaction == Interaction::Hovered && mouse.just_pressed(MouseButton::Right) {
            view::context(action, owns, &mut browser.model, &current);
        }
    }
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let Some(definition) = &browser.definition else {
        keyboard.clear();
        wheel.clear();
        return;
    };
    for event in wheel
        .read()
        .filter(|event| event.window == primary && event.y.is_finite())
    {
        let delta = match event.unit {
            MouseScrollUnit::Line => -event.y * 3.0,
            MouseScrollUnit::Pixel => -event.y / 16.0,
        };
        view::scroll(
            delta.round() as i32,
            owns,
            &mut browser.model,
            &current,
            definition,
        );
    }
    for event in keyboard.read().filter(|event| event.window == primary) {
        let (_, intent) = view::keyboard_with_layout(
            event,
            control,
            owns,
            &mut browser.model,
            &current,
            definition,
        );
        if let Some(intent) = intent {
            intents.push(intent);
        }
    }
    // Close wins over a connect captured earlier in this input batch.
    if intents.contains(&ui::Intent::Close) {
        *request.0.lock().unwrap() = None;
        let _ = backend.apply(net::Intent::Close);
        next.set(AppState::Menu);
        return;
    }
    let captured_connect = intents
        .iter()
        .find(|value| matches!(value, ui::Intent::Connect { .. }))
        .cloned();
    intents.retain(|value| !matches!(value, ui::Intent::Connect { .. }));
    cancel_superseded(&mut intents);
    for selected in intents {
        let _ = backend.apply(intent(selected));
    }
    let current = snapshot(backend);
    if let Some(selected) = captured_connect
        .filter(|selected| browser.model.connect(&current).as_ref() == Some(selected))
    {
        match backend.apply(intent(selected)) {
            Ok(Some(selected)) => {
                *request.0.lock().unwrap() = Some(crate::remote_join::Connect {
                    address: selected.address,
                    name: "You".into(),
                    team: if options.0.terrorist {
                        sim::Team::Terrorist
                    } else {
                        sim::Team::CounterTerrorist
                    },
                });
            }
            _ => (), // The backend exposes a bounded, current error in Snapshot.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_close_prevents_an_undispatched_favorite_commit() {
        let mut intents = vec![
            ui::Intent::AddFavorite("localhost:27015".into()),
            ui::Intent::ProbeAddress("localhost:27015".into()),
            ui::Intent::Refresh(ui::Tab::Favorites),
            ui::Intent::CancelAddress,
        ];
        cancel_superseded(&mut intents);
        assert_eq!(
            intents,
            vec![
                ui::Intent::Refresh(ui::Tab::Favorites),
                ui::Intent::CancelAddress
            ]
        );
    }

    #[test]
    fn unavailable_native_browser_returns_home_with_its_error() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Browser)
            .init_resource::<Browser>()
            .init_resource::<crate::menu::Menu>()
            .add_systems(Update, update);
        app.world_mut().resource_mut::<Browser>().error =
            Some("Missing installed browser layout".into());
        app.update();
        assert!(matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::Menu)
        ));
        assert_eq!(
            app.world().resource::<crate::menu::Menu>().status,
            "Missing installed browser layout"
        );
        assert!(app.world().resource::<Browser>().view.is_none());
    }

    #[test]
    fn downstream_exit_retention_does_not_reopen_either_owner() {
        fn hold(mut exits: ResMut<Messages<AppExit>>) {
            exits.drain().for_each(drop);
        }
        let mut app = App::new();
        app.add_message::<AppExit>()
            .init_resource::<Browser>()
            .init_resource::<crate::pack_ui::PackUi>()
            .add_systems(Last, (gate_exit, crate::pack_ui::gate_exit, hold).chain());
        app.world_mut()
            .resource_mut::<Messages<AppExit>>()
            .write(AppExit::from_code(7));
        app.update();
        assert!(app.should_exit().is_none());
        assert_eq!(
            app.world().resource::<Browser>().exit,
            Some(AppExit::from_code(7))
        );
        assert!(
            app.world()
                .resource::<crate::pack_ui::PackUi>()
                .blocks_movie()
        );
        app.update();
        assert!(app.should_exit().is_none());
        assert_eq!(
            app.world().resource::<Browser>().exit,
            Some(AppExit::from_code(7))
        );
        assert!(
            app.world()
                .resource::<crate::pack_ui::PackUi>()
                .blocks_movie()
        );
    }
}

pub(crate) fn draw(
    mut browser: ResMut<Browser>,
    state: Res<State<AppState>>,
    front: Res<crate::frontend::Frontend>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let browser = &mut *browser;
    if *state.get() != AppState::Browser || browser.exit.is_some() {
        return;
    }
    let Some(backend) = browser.backend.get_mut().unwrap() else {
        return;
    };
    let current = snapshot(backend);
    browser.model.reconcile(&current);
    let paint = format!(
        "{current:?}|{:?}|{}x{}",
        browser.model,
        window.width(),
        window.height()
    );
    if paint == browser.paint {
        return;
    }
    browser.paint = paint;
    if let Some(view) = browser.view.take() {
        view.despawn(&mut commands);
    }
    let Some(definition) = &browser.definition else {
        return;
    };
    match view::spawn(
        &mut commands,
        front.camera(),
        Vec2::new(window.width(), window.height()),
        definition,
        &browser.fonts,
        &browser.model,
        &current,
    ) {
        Ok(view) => {
            commands
                .entity(view.root)
                .insert(crate::session::Persistent);
            browser.view = Some(view);
        }
        Err(error) => browser.error = Some(error),
    }
}

/// Retain one cancelled worker until cleanup completes; no replacement can
/// detach an outstanding DNS operation. Preserve the first error exit code.
pub(crate) fn gate_exit(mut browser: ResMut<Browser>, mut messages: ResMut<Messages<AppExit>>) {
    let browser = &mut *browser;
    let first = browser.exit.is_none();
    for exit in messages.drain() {
        if browser.exit.as_ref().is_none_or(|old| !old.is_error()) {
            browser.exit = Some(exit);
        }
    }
    let Some(exit) = browser.exit.clone() else {
        return;
    };
    if let Some(backend) = browser.backend.get_mut().unwrap() {
        if backend.busy() {
            if first {
                backend.cancel();
            }
            return;
        }
    }
    messages.write(exit);
    // The pack gate may still retain the message. Never reopen input/jobs.
}
