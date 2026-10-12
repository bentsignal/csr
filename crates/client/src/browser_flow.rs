//! Opt-in browser-to-remote engine proof. Inputs, captures and userdata are owned
//! fixtures, not an operating-system mouse/keyboard interaction claim.
//! The installed main browser RES has no Close control. Final closing exercises
//! the implemented Escape handler; native window-chrome geometry is unmeasured.
use crate::{
    frontend::Frontend,
    frontend_servers::UiAction,
    frontend_servers_data::{Model, ServerKey, Snapshot, Tab},
    remote::RemoteClient,
    server_browser::Browser,
    session::{AppState, MatchEntity},
};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    ui::UiGlobalTransform,
    window::PrimaryWindow,
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

pub(crate) fn enabled() -> bool {
    std::env::var_os("CSR_TEST_BROWSER").is_some()
}

fn target(input: &str) -> Result<String, &'static str> {
    let port = input
        .strip_prefix("localhost:")
        .ok_or("proof requires localhost:<port>")?;
    let port: u16 = port.parse().map_err(|_| "invalid localhost port")?;
    if port == 0 {
        return Err("proof requires a nonzero localhost port");
    }
    Ok(format!("localhost:{port}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Start,
    Play,
    Community,
    AwaitBrowser,
    Favorites,
    AwaitFavorites,
    OpenAdd,
    AwaitAdd,
    Type,
    Submit,
    AwaitFavorite,
    DismissAdd,
    AwaitDismissed,
    CaptureFavorite,
    Select,
    Connect,
    AwaitReady,
    CaptureReady,
    Home,
    AwaitHome,
    CaptureHome,
    ReopenPlay,
    ReopenCommunity,
    AwaitReopen,
    CaptureReopen,
    Context,
    AwaitContext,
    CaptureContext,
    Remove,
    AwaitRemoved,
    CaptureRemoved,
    Close,
    AwaitClosed,
    CaptureClosed,
    Finish,
    Done,
}
pub(crate) struct Proof {
    start: Instant,
    entered: Instant,
    ready_at: Option<Instant>,
    phase: Phase,
    step: usize,
    pointer: Option<(Entity, Vec2, u64, Instant)>,
    pointer_diagnostic: Duration,
    endpoint: String,
    key: Option<ServerKey>,
    map: String,
    tick: u64,
}
impl Default for Proof {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            entered: Instant::now(),
            ready_at: None,
            phase: Phase::Start,
            step: 0,
            pointer: None,
            pointer_diagnostic: Duration::ZERO,
            endpoint: String::new(),
            key: None,
            map: String::new(),
            tick: 0,
        }
    }
}
impl Proof {
    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.step = 0;
        self.pointer = None;
        self.pointer_diagnostic = Duration::ZERO;
        self.entered = Instant::now();
        info!("browser flow: entered {phase:?}");
    }
    fn settled(&self) -> bool {
        self.entered.elapsed() >= Duration::from_millis(500)
    }
}

#[derive(Resource, Default)]
struct Captures {
    requested: BTreeSet<&'static str>,
    saved: BTreeSet<&'static str>,
    failed: Option<String>,
}
fn capture(world: &mut World, name: &'static str) {
    if !world.contains_resource::<Captures>() {
        world.init_resource::<Captures>();
    }
    if !world.resource_mut::<Captures>().requested.insert(name) {
        return;
    }
    let destination = world
        .resource::<crate::Options>()
        .0
        .screenshot
        .as_ref()
        .unwrap()
        .with_file_name(name);
    let started = SystemTime::now();
    let checked_path = destination.clone();
    let mut save = save_to_disk(destination);
    world.spawn(Screenshot::primary_window()).observe(
        move |event: On<ScreenshotCaptured>, mut captured: ResMut<Captures>| {
            save(event); // Desktop save_to_disk completes synchronously inside this observer.
            match std::fs::metadata(&checked_path) {
                Ok(metadata)
                    if metadata.len() > 0
                        && metadata
                            .modified()
                            .is_ok_and(|modified| modified >= started) =>
                {
                    captured.saved.insert(name);
                    info!(
                        "browser flow: captured {name} at {}",
                        checked_path.display()
                    );
                }
                _ => {
                    captured.failed = Some(format!(
                        "capture was not freshly saved: {}",
                        checked_path.display()
                    ))
                }
            }
        },
    );
}
fn capture_then(world: &mut World, proof: &mut Proof, name: &'static str, next: Phase) {
    if proof.step == 0 {
        capture(world, name);
        proof.step = 1;
    }
    // No subsequent input or scene transition before this exact GPU readback/save.
    if world.resource::<Captures>().saved.contains(name) {
        proof.enter(next);
    }
}
fn failed(world: &mut World, proof: &mut Proof, message: &str) {
    error!(
        "browser flow: phase {:?}: {message}; failures: 1",
        proof.phase
    );
    proof.enter(Phase::Done);
    world.write_message(AppExit::Error(std::num::NonZeroU8::new(7).unwrap()));
}
fn browser(world: &World) -> Option<(Model, Snapshot)> {
    let browser = world.get_resource::<Browser>()?;
    Some((browser.model().clone(), browser.snapshot()?))
}
fn window(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<(Entity, &mut Window), With<PrimaryWindow>>();
    let (id, mut window) = query.single_mut(world).ok()?;
    // Screenshot windows may have no OS focus. This is explicit engine-fixture
    // ownership; no native desktop focus claim is made.
    window.focused = true;
    Some(id)
}
fn mouse(world: &mut World, id: Entity, button: MouseButton, state: ButtonState) {
    world.write_message(MouseButtonInput {
        window: id,
        button,
        state,
    });
}
fn point(world: &mut World, id: Entity, position: Vec2) {
    world
        .get_mut::<Window>(id)
        .unwrap()
        .set_physical_cursor_position(Some(position.as_dvec2()));
}
fn click_ui(
    world: &mut World,
    proof: &mut Proof,
    id: Entity,
    button: MouseButton,
    matches: impl Fn(&UiAction) -> bool,
) -> bool {
    if proof.step == 2 {
        let input = world.resource::<ButtonInput<MouseButton>>();
        let observed = proof
            .pointer
            .and_then(|(entity, _, _, _)| world.get::<Interaction>(entity).copied());
        let model = browser(world).map(|(model, snapshot)| {
            (
                model.tab,
                model.add_open,
                model.context_row,
                snapshot.generation,
                snapshot.busy,
            )
        });
        info!(
            "browser flow: {:?} releasing {button:?}: observed interaction {observed:?}, pressed {}, just_pressed {}, model {model:?}",
            proof.phase,
            input.pressed(button),
            input.just_pressed(button)
        );
        mouse(world, id, button, ButtonState::Released);
        return true;
    }
    // A backend publication can replace the entire view. Tabs are unstamped,
    // whereas commands/rows are rejected when their captured generation is old.
    // Wait for a laid-out, current view and its actual Bevy hit before pressing.
    if !proof.settled() {
        return false;
    }
    let Some((_, snapshot)) = browser(world) else {
        return false;
    };
    if snapshot.busy {
        proof.step = 0;
        proof.pointer = None;
        return false;
    }
    let candidate = world
        .query_filtered::<(
            Entity,
            &UiAction,
            &ComputedNode,
            &UiGlobalTransform,
            &Interaction,
        ), With<Button>>()
        .iter(world)
        .find_map(|(entity, action, node, transform, interaction)| {
            (matches(action)
                && current_action(action, snapshot.generation)
                && node.size().min_element() > 0.0)
                .then_some((
                    entity,
                    transform.affine().translation,
                    *interaction,
                    action.clone(),
                ))
        });
    let Some((entity, position, interaction, action)) = candidate else {
        proof.step = 0;
        proof.pointer = None;
        return false;
    };
    let window = world.get::<Window>(id).unwrap();
    if !position.is_finite()
        || position.x < 0.0
        || position.y < 0.0
        || position.x >= window.physical_width() as f32
        || position.y >= window.physical_height() as f32
    {
        return false;
    }
    let same_target = proof.pointer.is_some_and(|(old, point, generation, _)| {
        old == entity && point.distance(position) < 0.5 && generation == snapshot.generation
    });
    if !same_target {
        proof.step = 0;
    }
    match proof.step {
        0 => {
            info!(
                "browser flow: {:?} pointer {position:?}, {button:?}, entity {entity:?}, action {action:?}, generation {}",
                proof.phase, snapshot.generation
            );
            point(world, id, position);
            proof.pointer = Some((entity, position, snapshot.generation, Instant::now()));
            proof.step = 1;
            false
        }
        1 => {
            if interaction != Interaction::Hovered {
                if proof.entered.elapsed() >= proof.pointer_diagnostic {
                    let hovered: Vec<_> = world
                        .query::<(&Interaction, &UiAction)>()
                        .iter(world)
                        .filter(|(hit, _)| **hit != Interaction::None)
                        .map(|(hit, action)| format!("{hit:?} {action:?}"))
                        .collect();
                    let cursor = world.get::<Window>(id).unwrap().physical_cursor_position();
                    warn!(
                        "browser flow: {:?} waiting for target hover: {entity:?} {action:?} {interaction:?}; physical cursor {cursor:?}; actual hits {hovered:?}",
                        proof.phase
                    );
                    proof.pointer_diagnostic = proof.entered.elapsed() + Duration::from_secs(2);
                }
                return false;
            }
            if proof.pointer.unwrap().3.elapsed() < Duration::from_millis(100) {
                return false;
            }
            info!(
                "browser flow: {:?} pressing hovered {entity:?} {action:?}, generation {}",
                proof.phase, snapshot.generation
            );
            mouse(world, id, button, ButtonState::Pressed);
            proof.step = 2;
            false
        }
        _ => unreachable!(),
    }
}
fn current_action(action: &UiAction, generation: u64) -> bool {
    match action {
        UiAction::Command(_, captured)
        | UiAction::Select(_, captured)
        | UiAction::Connect(_, captured)
        | UiAction::RemoveFavorite(_, captured) => *captured == generation,
        UiAction::Tab(_) | UiAction::Address => true,
    }
}
fn click_movie(world: &mut World, proof: &mut Proof, id: Entity, x: f64, y: f64) -> bool {
    let window = world.get::<Window>(id).unwrap();
    let x = x * window.physical_width() as f64 / 1920.0;
    let y = y * window.physical_height() as f64 / 1080.0;
    let front = world.resource::<Frontend>();
    match proof.step {
        0 => {
            front.send(scaleform::Input::MouseMove { x, y });
            proof.step = 1;
            false
        }
        1 => {
            front.send(scaleform::Input::MouseDown {
                x,
                y,
                button: scaleform::MouseButton::Left,
            });
            proof.step = 2;
            false
        }
        _ => {
            front.send(scaleform::Input::MouseUp {
                x,
                y,
                button: scaleform::MouseButton::Left,
            });
            true
        }
    }
}
fn character(c: char) -> Option<KeyCode> {
    Some(match c {
        'l' => KeyCode::KeyL,
        'o' => KeyCode::KeyO,
        'c' => KeyCode::KeyC,
        'a' => KeyCode::KeyA,
        'h' => KeyCode::KeyH,
        's' => KeyCode::KeyS,
        't' => KeyCode::KeyT,
        ':' => KeyCode::Semicolon,
        '0' => KeyCode::Digit0,
        '1' => KeyCode::Digit1,
        '2' => KeyCode::Digit2,
        '3' => KeyCode::Digit3,
        '4' => KeyCode::Digit4,
        '5' => KeyCode::Digit5,
        '6' => KeyCode::Digit6,
        '7' => KeyCode::Digit7,
        '8' => KeyCode::Digit8,
        '9' => KeyCode::Digit9,
        _ => return None,
    })
}
fn keyboard(world: &mut World, id: Entity, key: KeyCode, text: Option<char>, state: ButtonState) {
    let text = text.map(|c| c.to_string());
    world.write_message(KeyboardInput {
        window: id,
        key_code: key,
        logical_key: text
            .as_ref()
            .map_or(Key::Escape, |text| Key::Character(text.as_str().into())),
        text: if state == ButtonState::Pressed {
            text.map(Into::into)
        } else {
            None
        },
        state,
        repeat: false,
    });
}
fn escape(world: &mut World, proof: &mut Proof, id: Entity) -> bool {
    if proof.step == 0 {
        keyboard(world, id, KeyCode::Escape, None, ButtonState::Pressed);
        proof.step = 1;
        false
    } else {
        keyboard(world, id, KeyCode::Escape, None, ButtonState::Released);
        true
    }
}
fn clean_home(world: &mut World) -> bool {
    !world.contains_resource::<RemoteClient>()
        && !world.contains_resource::<crate::LoadedLevel>()
        && !world.contains_resource::<crate::player::SimWorld>()
        && !world.contains_resource::<crate::remote_join::Admission>()
        && world
            .query_filtered::<Entity, With<MatchEntity>>()
            .iter(world)
            .next()
            .is_none()
}

pub(crate) fn update(world: &mut World, mut proof: Local<Proof>) {
    if proof.phase == Phase::Done {
        return;
    }
    if proof.start.elapsed() > Duration::from_secs(180) {
        failed(world, &mut proof, "180-second deadline exceeded");
        return;
    }
    if let Some(error) = world
        .get_resource::<Captures>()
        .and_then(|captures| captures.failed.clone())
    {
        failed(world, &mut proof, &error);
        return;
    }
    let Some(front) = world.get_resource::<Frontend>() else {
        return;
    };
    if front.failed.is_some() {
        failed(world, &mut proof, "authored movie failed");
        return;
    }
    let movie_ready = front.ready;
    let Some(id) = window(world) else {
        failed(world, &mut proof, "primary window unavailable");
        return;
    };
    let state = *world.resource::<State<AppState>>().get();
    match proof.phase {
        Phase::Start => {
            let endpoint = match std::env::var("CSR_TEST_BROWSER")
                .ok()
                .as_deref()
                .map(target)
            {
                Some(Ok(endpoint)) => endpoint,
                _ => {
                    failed(
                        world,
                        &mut proof,
                        "CSR_TEST_BROWSER must be localhost:<nonzero port>",
                    );
                    return;
                }
            };
            let data = std::env::var_os("CSR_DATA_DIR").map(PathBuf::from);
            let shot = world.resource::<crate::Options>().0.screenshot.as_ref();
            if data.as_ref().is_none_or(|data| !data.is_absolute())
                || shot.is_none_or(|shot| {
                    !shot.is_absolute()
                        || shot
                            .extension()
                            .is_none_or(|extension| !extension.eq_ignore_ascii_case("png"))
                })
            {
                failed(
                    world,
                    &mut proof,
                    "owned absolute CSR_DATA_DIR and PNG screenshot are required",
                );
                return;
            }
            if !movie_ready || state != AppState::Menu {
                return;
            }
            if proof.ready_at.get_or_insert_with(Instant::now).elapsed() < Duration::from_secs(5) {
                return;
            }
            info!(
                "browser flow: engine-input fixture; target {endpoint}; pid {}; userdata {}; screenshots {}; no OS interaction claim",
                std::process::id(),
                data.unwrap().display(),
                shot.unwrap().parent().unwrap().display()
            );
            proof.endpoint = endpoint;
            proof.enter(Phase::Play);
        }
        Phase::Play | Phase::ReopenPlay => {
            if state == AppState::Menu && click_movie(world, &mut proof, id, 486.0, 26.0) {
                let next = if proof.phase == Phase::Play {
                    Phase::Community
                } else {
                    Phase::ReopenCommunity
                };
                proof.enter(next);
            }
        }
        Phase::Community | Phase::ReopenCommunity => {
            if proof.settled()
                && state == AppState::Menu
                && click_movie(world, &mut proof, id, 503.0, 222.0)
            {
                let next = if proof.phase == Phase::Community {
                    Phase::AwaitBrowser
                } else {
                    Phase::AwaitReopen
                };
                proof.enter(next);
            }
        }
        Phase::AwaitBrowser if state == AppState::Browser => proof.enter(Phase::Favorites),
        Phase::Favorites => {
            let Some((model, snapshot)) = browser(world) else {
                return;
            };
            if snapshot.busy {
                return;
            }
            if model.tab == Tab::Favorites {
                proof.enter(Phase::AwaitFavorites);
            } else if click_ui(world, &mut proof, id, MouseButton::Left, |action| {
                matches!(action, UiAction::Tab(Tab::Favorites))
            }) {
                proof.enter(Phase::AwaitFavorites);
            }
        }
        Phase::AwaitFavorites => {
            let Some((model, snapshot)) = browser(world) else {
                return;
            };
            if snapshot.busy || model.tab != Tab::Favorites {
                return;
            }
            if snapshot.error.is_some() || !snapshot.rows.is_empty() {
                failed(
                    world,
                    &mut proof,
                    "Favorites fixture is not empty/valid; refusing to modify preexisting favorites",
                );
                return;
            }
            proof.enter(Phase::OpenAdd);
        }
        Phase::OpenAdd => {
            if click_ui(
                world,
                &mut proof,
                id,
                MouseButton::Left,
                |action| matches!(action, UiAction::Command(command, _) if command == "AddServerByName"),
            ) {
                proof.enter(Phase::AwaitAdd);
            }
        }
        Phase::AwaitAdd => {
            let Some((model, snapshot)) = browser(world) else {
                return;
            };
            if model.add_open && model.address_focused && !snapshot.busy {
                if !model.address.text().is_empty() {
                    failed(
                        world,
                        &mut proof,
                        "Add dialog retained an unexpected address",
                    );
                    return;
                }
                proof.enter(Phase::Type);
            }
        }
        Phase::Type => {
            let offset = proof.step / 2;
            let Some(c) = proof.endpoint.chars().nth(offset) else {
                let Some((model, _)) = browser(world) else {
                    return;
                };
                if model.address.text() == proof.endpoint {
                    proof.enter(Phase::Submit);
                } else {
                    failed(
                        world,
                        &mut proof,
                        "typed hostname does not match the actual Add dialog",
                    );
                }
                return;
            };
            let key = character(c).unwrap();
            keyboard(
                world,
                id,
                key,
                Some(c),
                if proof.step % 2 == 0 {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
            );
            proof.step += 1;
        }
        Phase::Submit => {
            if click_ui(
                world,
                &mut proof,
                id,
                MouseButton::Left,
                |action| matches!(action, UiAction::Command(command, _) if command == "OK"),
            ) {
                proof.enter(Phase::AwaitFavorite);
            }
        }
        Phase::AwaitFavorite => {
            let Some((_, snapshot)) = browser(world) else {
                return;
            };
            if snapshot.busy {
                return;
            }
            if snapshot.error.is_some() {
                failed(world, &mut proof, "favorite persistence/discovery failed");
                return;
            }
            if let Some(row) = snapshot
                .rows
                .iter()
                .find(|row| row.endpoint == proof.endpoint && row.favorite && row.connectable)
            {
                proof.key = Some(row.key);
                proof.map = row.map.clone().unwrap_or_default();
                if row.name.is_some() || row.players.is_some() || row.ping_ms.is_some() {
                    failed(world, &mut proof, "discovery invented unsupported metadata");
                    return;
                }
                info!(
                    "browser flow: discovered favorite {} key {:?}, generation {}, advertised map {:?}; name/count/ping unknown",
                    row.endpoint, row.key, snapshot.generation, row.map
                );
                proof.enter(Phase::DismissAdd);
            }
        }
        Phase::DismissAdd => {
            if escape(world, &mut proof, id) {
                proof.enter(Phase::AwaitDismissed);
            }
        }
        Phase::AwaitDismissed => {
            if browser(world).is_some_and(|(model, _)| !model.add_open) {
                proof.enter(Phase::CaptureFavorite);
            }
        }
        Phase::CaptureFavorite => {
            capture_then(world, &mut proof, "browser-favorite.png", Phase::Select)
        }
        Phase::Select => {
            let key = proof.key.unwrap();
            if click_ui(
                world,
                &mut proof,
                id,
                MouseButton::Left,
                |action| matches!(action, UiAction::Select(found, _) if *found == key),
            ) {
                proof.enter(Phase::Connect);
            }
        }
        Phase::Connect => {
            let key = proof.key.unwrap();
            if browser(world).is_none_or(|(model, _)| model.selected != Some(key)) {
                failed(
                    world,
                    &mut proof,
                    "actual row pointer did not select the favorite",
                );
                return;
            }
            if click_ui(
                world,
                &mut proof,
                id,
                MouseButton::Left,
                |action| matches!(action, UiAction::Connect(found, _) if *found == key),
            ) {
                proof.enter(Phase::AwaitReady);
            }
        }
        Phase::AwaitReady => {
            if state != AppState::Match {
                return;
            }
            let Some(client) = world.get_resource::<RemoteClient>() else {
                return;
            };
            if client.status() != &netcode::remote_session::Status::Ready {
                return;
            }
            let Some(local) = world.get_resource::<crate::player::LocalPlayer>() else {
                return;
            };
            let Some(level) = world.get_resource::<crate::LoadedLevel>() else {
                return;
            };
            let Some(sim) = world.get_resource::<crate::player::SimWorld>() else {
                return;
            };
            let Some(info) = client.server_info() else {
                return;
            };
            if local.id == 0
                || client.frame().local.is_none()
                || info.content.map_name != proof.map
                || crate::remote_map::manifest_for_level(&level.0).as_ref() != Ok(&info.content)
            {
                failed(
                    world,
                    &mut proof,
                    "rendered authority/local content did not match the selected server",
                );
                return;
            }
            proof.tick = sim.0.tick;
            info!(
                "browser flow: Ready, local actor {}, authoritative {} Hz, {} actors, tick {}, map {}",
                local.id,
                sim.0.tick_rate,
                sim.0.players.len(),
                sim.0.tick,
                level.0.id
            );
            proof.enter(Phase::CaptureReady);
        }
        Phase::CaptureReady => {
            if state != AppState::Match
                || world
                    .get_resource::<RemoteClient>()
                    .is_none_or(|client| client.status() != &netcode::remote_session::Status::Ready)
            {
                failed(
                    world,
                    &mut proof,
                    "server left Ready before its capture completed",
                );
                return;
            }
            if proof.settled() && world.resource::<crate::player::SimWorld>().0.tick > proof.tick {
                capture_then(world, &mut proof, "browser-ready.png", Phase::Home);
            }
        }
        Phase::Home => {
            let mut menu = world.resource_mut::<crate::menu::Menu>();
            menu.in_match = false;
            menu.page = Some(crate::menu::Page::Main);
            info!("browser flow: owned engine Home transition");
            proof.enter(Phase::AwaitHome);
        }
        Phase::AwaitHome if state == AppState::Menu && proof.settled() => {
            if !clean_home(world) {
                failed(world, &mut proof, "Home retained remote/match owners");
                return;
            }
            proof.enter(Phase::CaptureHome);
        }
        Phase::CaptureHome => {
            capture_then(world, &mut proof, "browser-home.png", Phase::ReopenPlay)
        }
        Phase::AwaitReopen if state == AppState::Browser => {
            let Some((model, snapshot)) = browser(world) else {
                return;
            };
            if snapshot.busy {
                return;
            }
            if model.tab != Tab::Favorites
                || snapshot.error.is_some()
                || !snapshot.rows.iter().any(|row| {
                    row.endpoint == proof.endpoint
                        && row.key == proof.key.unwrap()
                        && row.favorite
                        && row.connectable
                })
            {
                failed(
                    world,
                    &mut proof,
                    "re-entry did not refresh/preserve the owned favorite identity",
                );
                return;
            }
            proof.enter(Phase::CaptureReopen);
        }
        Phase::CaptureReopen => {
            capture_then(world, &mut proof, "browser-reopened.png", Phase::Context)
        }
        Phase::Context => {
            let key = proof.key.unwrap();
            if click_ui(
                world,
                &mut proof,
                id,
                MouseButton::Right,
                |action| matches!(action, UiAction::Select(found, _) if *found == key),
            ) {
                proof.enter(Phase::AwaitContext);
            }
        }
        Phase::AwaitContext => {
            if browser(world).is_some_and(|(model, _)| model.context_row == proof.key) {
                proof.enter(Phase::CaptureContext);
            }
        }
        Phase::CaptureContext => {
            capture_then(world, &mut proof, "browser-context.png", Phase::Remove)
        }
        Phase::Remove => {
            let key = proof.key.unwrap();
            if click_ui(
                world,
                &mut proof,
                id,
                MouseButton::Left,
                |action| matches!(action, UiAction::RemoveFavorite(found, _) if *found == key),
            ) {
                proof.enter(Phase::AwaitRemoved);
            }
        }
        Phase::AwaitRemoved => {
            let Some((model, snapshot)) = browser(world) else {
                return;
            };
            if snapshot.busy {
                return;
            }
            if snapshot.error.is_some()
                || model.context_row.is_some()
                || snapshot
                    .rows
                    .iter()
                    .any(|row| row.key == proof.key.unwrap())
            {
                failed(
                    world,
                    &mut proof,
                    "owned context removal did not clear the favorite",
                );
                return;
            }
            proof.enter(Phase::CaptureRemoved);
        }
        Phase::CaptureRemoved => {
            capture_then(world, &mut proof, "browser-removed.png", Phase::Close)
        }
        Phase::Close => {
            if escape(world, &mut proof, id) {
                proof.enter(Phase::AwaitClosed);
            }
        }
        Phase::AwaitClosed if state == AppState::Menu && proof.settled() => {
            if !clean_home(world) {
                failed(
                    world,
                    &mut proof,
                    "closing Browser retained match/remote owners",
                );
                return;
            }
            proof.enter(Phase::CaptureClosed);
        }
        Phase::CaptureClosed => {
            capture_then(world, &mut proof, "browser-closed.png", Phase::Finish)
        }
        Phase::Finish => {
            let captures = world.resource::<Captures>();
            if captures.saved.len() != 7 || captures.saved != captures.requested {
                failed(world, &mut proof, "not all requested captures completed");
                return;
            }
            info!(
                "browser flow: authored Play -> Community -> Favorites -> actual Add/typed hostname/OK -> persisted discovery -> row Connect -> rendered Ready -> engine Home -> authored reopen -> right-click context removal -> Escape Close; seven ScreenshotCaptured saves; failures: 0"
            );
            proof.enter(Phase::Done);
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_targets_reject_stale_rendered_generation() {
        let key = ServerKey(7);
        for action in [
            UiAction::Command("AddServerByName".into(), 11),
            UiAction::Select(key, 11),
            UiAction::Connect(key, 11),
            UiAction::RemoveFavorite(key, 11),
        ] {
            assert!(current_action(&action, 11));
            assert!(!current_action(&action, 12));
        }
        assert!(current_action(&UiAction::Tab(Tab::Favorites), 12));
        assert!(current_action(&UiAction::Address, 12));
    }
    #[test]
    fn proof_target_accepts_only_the_owned_localhost_fixture() {
        assert_eq!(target("localhost:027015"), Ok("localhost:27015".into()));
        for value in [
            "localhost:0",
            "localhost:65536",
            "localhost",
            "example.com:27015",
            "127.0.0.1:27015",
            "http://localhost:27015",
            "localhost:27015\n",
        ] {
            assert!(target(value).is_err(), "accepted {value:?}");
        }
    }
    #[test]
    fn every_character_in_a_normalized_hostname_has_a_literal_keyboard_event() {
        for port in [1, 27015, 65535] {
            for c in target(&format!("localhost:{port}")).unwrap().chars() {
                assert!(character(c).is_some());
            }
        }
        assert!(character('\n').is_none());
    }
}
