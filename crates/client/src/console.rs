//! Local developer console. Commands mutate settings or request frontend actions;
//! gameplay still enters the simulation exclusively through TickInput.
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::{ButtonState, InputSystems, keyboard::KeyboardInput},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use std::collections::VecDeque;

#[derive(Resource)]
pub(crate) struct DeveloperConsole {
    open: bool,
    blocked_frame: bool,
    line: String,
    cursor: usize,
    history: Vec<String>,
    history_at: Option<usize>,
    draft: String,
    output: VecDeque<String>,
    pending: Vec<String>,
}
impl Default for DeveloperConsole {
    fn default() -> Self {
        Self { open: false, blocked_frame: false, line: String::new(), cursor: 0, history: Vec::new(), history_at: None, draft: String::new(), output: VecDeque::from(["Developer Console - type help. ` or Escape closes; Up/Down history; Tab completes.".into()]), pending: Vec::new() }
    }
}
impl DeveloperConsole {
    pub(crate) fn blocks_input(&self) -> bool {
        self.open || self.blocked_frame
    }
    fn print(&mut self, line: impl Into<String>) {
        let line = line.into();
        for line in line.lines() {
            self.output.push_back(line.to_owned());
        }
        while self.output.len() > 200 {
            self.output.pop_front();
        }
    }
    fn recall(&mut self, up: bool) {
        if self.history.is_empty() || (!up && self.history_at.is_none()) {
            return;
        }
        let at = if up {
            match self.history_at {
                None => {
                    self.draft = self.line.clone();
                    Some(self.history.len() - 1)
                }
                Some(at) => Some(at.saturating_sub(1)),
            }
        } else {
            self.history_at
                .and_then(|at| (at + 1 < self.history.len()).then_some(at + 1))
        };
        self.history_at = at;
        self.line = at
            .map(|at| self.history[at].clone())
            .unwrap_or_else(|| self.draft.clone());
        self.cursor = self.line.len();
    }
}
#[derive(Component)]
struct ConsoleRoot;
#[derive(Component)]
enum ConsoleText {
    Title,
    Output,
    Prompt,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<DeveloperConsole>()
        .add_systems(Startup, setup.after(crate::ui_assets::load))
        .add_systems(
            PreUpdate,
            input.after(InputSystems).before(bevy::ui::UiSystems::Focus),
        )
        .add_systems(
            Update,
            (execute.run_if(has_pending), draw)
                .chain()
                .before(crate::frontend::input),
        );
}
pub(crate) fn allows_input(console: Res<DeveloperConsole>) -> bool {
    !console.blocks_input()
}
fn setup(mut commands: Commands, assets: Res<crate::ui_assets::UiAssets>) {
    commands
        .spawn((
            crate::session::Persistent,
            ConsoleRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                top: px(12),
                width: percent(82),
                max_width: px(1200),
                height: percent(75),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(14)),
                border: UiRect::all(px(1)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.085, 0.09, 0.98)),
            BorderColor::all(Color::srgb(0.45, 0.48, 0.46)),
            GlobalZIndex(30000),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            for part in [ConsoleText::Title, ConsoleText::Output, ConsoleText::Prompt] {
                let output = matches!(part, ConsoleText::Output);
                parent.spawn((
                    crate::session::Persistent,
                    part,
                    Node {
                        flex_grow: if output { 1.0 } else { 0.0 },
                        flex_shrink: if output { 1.0 } else { 0.0 },
                        min_height: px(0),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    Text::new(""),
                    TextFont {
                        font: bevy::text::FontSource::Handle(assets.regular.clone()),
                        font_size: 18.0.into(),
                        ..default()
                    },
                    TextColor(Color::srgb(0.88, 0.9, 0.85)),
                ));
            }
        });
}
#[allow(clippy::too_many_arguments)]
fn input(
    mut console: ResMut<DeveloperConsole>,
    mut events: ResMut<Messages<KeyboardInput>>,
    mut reader: Local<bevy::ecs::message::MessageCursor<KeyboardInput>>,
    mut wheel_events: ResMut<Messages<bevy::input::mouse::MouseWheel>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut scroll: ResMut<bevy::input::mouse::AccumulatedMouseScroll>,
    enabled: Res<crate::player::InputEnabled>,
    mut local: Option<ResMut<crate::player::LocalPlayer>>,
    mut block: ResMut<crate::player::InputBlockFrames>,
    mut cursor: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    menu: Res<crate::menu::Menu>,
) {
    console.blocked_frame = false;
    if !enabled.0 {
        // Disabled screenshot/input proofs retain their existing message paths.
        reader.read(&events).for_each(drop);
        return;
    }
    let (window, cursor) = &mut *cursor;
    let received: Vec<_> = reader.read(&events).cloned().collect();
    for event in &received {
        if !window.focused || event.state != ButtonState::Pressed {
            continue;
        }
        if event.key_code == KeyCode::Backquote {
            if !event.repeat {
                console.open = !console.open;
                console.blocked_frame = true;
            }
            continue;
        }
        if !console.open {
            continue;
        }
        match event.key_code {
            KeyCode::Escape => {
                console.open = false;
                console.blocked_frame = true;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let line = console.line.trim().to_string();
                if !line.is_empty() {
                    console.print(format!("] {line}"));
                    if console.history.last() != Some(&line) {
                        console.history.push(line.clone());
                    }
                    if console.history.len() > 100 {
                        console.history.remove(0);
                    }
                    console.pending.push(line);
                }
                console.line.clear();
                console.cursor = 0;
                console.history_at = None;
                console.draft.clear();
            }
            KeyCode::ArrowUp => console.recall(true),
            KeyCode::ArrowDown => console.recall(false),
            KeyCode::ArrowLeft => {
                console.cursor = console.line[..console.cursor]
                    .char_indices()
                    .last()
                    .map_or(0, |(i, _)| i)
            }
            KeyCode::ArrowRight => {
                let width = console.line[console.cursor..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
                console.cursor += width;
            }
            KeyCode::Home => console.cursor = 0,
            KeyCode::End => console.cursor = console.line.len(),
            KeyCode::Backspace => {
                let at = console.cursor;
                if let Some((previous, _)) = console.line[..at].char_indices().last() {
                    console.line.drain(previous..at);
                    console.cursor = previous;
                }
            }
            KeyCode::Delete => {
                let at = console.cursor;
                if let Some(c) = console.line[at..].chars().next() {
                    console.line.drain(at..at + c.len_utf8());
                }
            }
            KeyCode::Tab => {
                let matches: Vec<_> = COMMANDS
                    .iter()
                    .copied()
                    .filter(|name| name.starts_with(console.line.trim()))
                    .collect();
                if matches.len() == 1 {
                    console.line = format!("{} ", matches[0]);
                    console.cursor = console.line.len();
                } else if !matches.is_empty() {
                    console.print(matches.join("  "));
                }
            }
            _ => {
                if let Some(text) = &event.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if console.line.len() < 512 {
                            let at = console.cursor;
                            console.line.insert(at, c);
                            console.cursor += c.len_utf8();
                        }
                    }
                }
            }
        }
    }
    if console.blocks_input() {
        events.clear();
        wheel_events.clear();
        keys.reset_all();
        mouse.reset_all();
        scroll.delta = Vec2::ZERO;
        if let Some(local) = local.as_mut() {
            local.latched = 0;
            local.pending_slot = 0;
            local.pending_drop = false;
            local.pending_buys.clear();
        }
        block.0 = 2;
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
        if !console.open && menu.in_match && !menu.open() {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
}
const COMMANDS: &[&str] = &[
    "help",
    "clear",
    "bind",
    "unbind",
    "bindlist",
    "volume",
    "sensitivity",
    "m_pitch",
    "cl_showfps",
    "cl_crosshaircolor",
    "cl_crosshairsize",
    "cl_crosshairgap",
    "cl_crosshairthickness",
    "cl_crosshairdot",
    "cl_radar_scale",
    "cl_radar_rotate",
    "closeonbuy",
    "disconnect",
    "connect",
    "map",
    "maps",
    "status",
    "quit",
];
const ACTION_COMMANDS: [&str; 13] = [
    "+forward",
    "+back",
    "+moveleft",
    "+moveright",
    "+jump",
    "+duck",
    "+speed",
    "+use",
    "+reload",
    "buymenu",
    "drop",
    "+showscores",
    "+lookatweapon",
];

/// One command at a time, with quoted arguments; no shell/exec/eval path.
fn words(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            ';' if !quoted => {
                return Err(
                    "Enter one command at a time; command chaining is not implemented.".into(),
                );
            }
            _ => {
                word.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("Unclosed quote.".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}
fn binding_key(name: &str) -> Option<KeyCode> {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "ctrl" => Some(KeyCode::ControlLeft),
        "shift" => Some(KeyCode::ShiftLeft),
        "alt" => Some(KeyCode::AltLeft),
        _ => crate::settings::KEYS.iter().copied().find(|key| {
            let debug = format!("{key:?}").to_ascii_lowercase();
            debug == name || debug.strip_prefix("key") == Some(name.as_str())
        }),
    }
}
fn bind(
    settings: &mut crate::settings::Settings,
    args: &[String],
    unbind: bool,
) -> Result<String, String> {
    if args.is_empty() || args.len() > 2 || (unbind && args.len() != 1) {
        return Err("Usage: bind <key|mwheelup|mwheeldown> [command]; unbind <wheel>".into());
    }
    let name = args[0].to_ascii_lowercase();
    if matches!(name.as_str(), "mwheelup" | "mwheeldown") {
        let action = if name == "mwheelup" {
            &mut settings.wheel_up
        } else {
            &mut settings.wheel_down
        };
        if unbind {
            *action = crate::settings::WheelAction::None;
        } else if let Some(command) = args.get(1) {
            *action = crate::settings::WheelAction::parse(&command.to_ascii_lowercase())
                .ok_or("Wheel commands: +jump, invprev, invnext or \"\".")?;
        }
        return Ok(format!("{name} = \"{}\"", action.command()));
    }
    if unbind {
        return Err(
            "Keyboard unbind is not implemented; use bind to move an action to another key.".into(),
        );
    }
    let key=binding_key(&name).ok_or("Unsupported key. Use letters, space, ctrl, shift, tab or arrows; bindlist shows active bindings.")?;
    if let Some(command) = args.get(1) {
        let command = command.to_ascii_lowercase();
        let action = ACTION_COMMANDS
            .iter()
            .position(|candidate| *candidate == command)
            .ok_or("Unsupported bind command. Type bindlist for supported actions.")?;
        settings.rebind(action, key);
    }
    Ok(format!(
        "{name} = {}",
        settings
            .binds
            .iter()
            .position(|candidate| *candidate == key)
            .map(|i| ACTION_COMMANDS[i])
            .unwrap_or("unbound")
    ))
}
fn has_pending(console: Res<DeveloperConsole>) -> bool {
    !console.pending.is_empty()
}
fn execute(world: &mut World) {
    let pending = std::mem::take(&mut world.resource_mut::<DeveloperConsole>().pending);
    for command in pending {
        let result = run(world, &command);
        world
            .resource_mut::<DeveloperConsole>()
            .print(result.unwrap_or_else(|error| format!("Error: {error}")));
    }
}
fn run(world: &mut World, line: &str) -> Result<String, String> {
    let tokens = words(line)?;
    let Some(command) = tokens.first() else {
        return Ok(String::new());
    };
    let name = command.to_ascii_lowercase();
    let args = &tokens[1..];
    match name.as_str() {
        "help" => Ok(format!(
            "{}\nExamples: volume 0.5 | cl_showfps 1 | bind mwheeldown +jump | bind space +jump\nSettings save immediately. Up/Down recalls commands; Tab completes command names.\nconnect accepts an IP:port (default 27015). map uses the current offline mode/team/bot selection.\nThis console does not yet implement every Source command or arbitrary command binds.",
            COMMANDS.join("  ")
        )),
        "clear" if args.is_empty() => {
            world.resource_mut::<DeveloperConsole>().output.clear();
            Ok(String::new())
        }
        "bind" | "unbind" => {
            let result = bind(
                &mut world.resource_mut::<crate::settings::Settings>(),
                args,
                name == "unbind",
            )?;
            world
                .resource::<crate::settings::Settings>()
                .save()
                .map_err(|e| format!("Binding changed but save failed: {e}"))?;
            Ok(result)
        }
        "bindlist" if args.is_empty() => {
            let settings = world.resource::<crate::settings::Settings>();
            Ok(format!(
                "{}\nmwheelup = {}\nmwheeldown = {}\nKeyboard binds swap an occupied key's previous action. Number slots, Q, mouse attacks and ` remain fixed.",
                settings
                    .binds
                    .iter()
                    .enumerate()
                    .map(|(i, key)| format!(
                        "{} = {}",
                        crate::settings::key_name(*key),
                        ACTION_COMMANDS[i]
                    ))
                    .collect::<Vec<_>>()
                    .join("\n"),
                settings.wheel_up.command(),
                settings.wheel_down.command()
            ))
        }
        "maps" if args.is_empty() => Ok(world.resource::<crate::menu::Menu>().maps.join("\n")),
        "map" if args.len() == 1 => {
            if world.resource::<crate::hosting::Hosting>().exiting() {
                return Err(
                    "The hosted server is closing. Try again after shutdown finishes.".into(),
                );
            }
            if *world.resource::<State<crate::AppState>>().get() == crate::AppState::Loading {
                return Err(
                    "A map is loading. Use disconnect to cancel it before choosing another map."
                        .into(),
                );
            }
            let menu = world.resource::<crate::menu::Menu>();
            if !menu.maps.contains(&args[0]) {
                return Err("Unknown map. Type maps for installed/classic choices.".into());
            }
            let mut options = world.resource::<crate::Options>().0.clone();
            options.map = args[0].clone();
            options.deathmatch = menu.deathmatch || options.map.starts_with("cs_");
            options.terrorist = menu.terrorist;
            options.bots = menu.bots;
            crate::remote_join::home(world);
            *world
                .resource::<crate::menu::LoadRequest>()
                .0
                .lock()
                .unwrap() = Some(options.into());
            Ok(format!(
                "Requested {} through the frontend loader.",
                args[0]
            ))
        }
        "disconnect" if args.is_empty() => {
            crate::remote_join::home(world);
            world.resource_mut::<crate::menu::Menu>().reset_match();
            world
                .resource_mut::<NextState<crate::AppState>>()
                .set(crate::AppState::Menu);
            Ok("Disconnected.".into())
        }
        "connect" if args.len() == 1 => {
            if world.resource::<crate::hosting::Hosting>().exiting() {
                return Err(
                    "The hosted server is closing. Try again after shutdown finishes.".into(),
                );
            }
            if *world.resource::<State<crate::AppState>>().get() == crate::AppState::Loading {
                return Err("A session is loading. Use disconnect to cancel it before connecting elsewhere.".into());
            }
            let address = parse_address(&args[0])?;
            *world
                .resource::<crate::remote_join::JoinRequest>()
                .0
                .lock()
                .unwrap() = Some(crate::remote_join::Connect {
                address,
                name: "Player".into(),
                team: if world.resource::<crate::menu::Menu>().terrorist {
                    sim::Team::Terrorist
                } else {
                    sim::Team::CounterTerrorist
                },
            });
            Ok(format!("Connecting to {address}..."))
        }
        "status" if args.is_empty() => {
            let state = world.resource::<State<crate::AppState>>();
            let remote = world
                .get_resource::<crate::remote::RemoteClient>()
                .map(|r| format!("{:?}", r.status()))
                .unwrap_or_else(|| "offline".into());
            Ok(format!("Frontend: {:?}; session: {remote}", state.get()))
        }
        "quit" if args.is_empty() => {
            world
                .resource_mut::<Messages<AppExit>>()
                .write(AppExit::Success);
            Ok("Exiting...".into())
        }
        name if world
            .resource::<crate::settings::Settings>()
            .convar_number(name)
            .is_some() =>
        {
            if args.len() > 1 {
                return Err(format!("Usage: {name} [number]"));
            }
            if let Some(value) = args.first() {
                let value = value
                    .parse::<f64>()
                    .map_err(|_| "Expected a finite number.")?;
                if !world
                    .resource_mut::<crate::settings::Settings>()
                    .set_convar_number(name, value)
                {
                    return Err("Unsupported value.".into());
                }
                world
                    .resource::<crate::settings::Settings>()
                    .save()
                    .map_err(|e| format!("Setting changed but save failed: {e}"))?;
            }
            Ok(format!(
                "{name} = {}",
                world
                    .resource::<crate::settings::Settings>()
                    .convar_number(name)
                    .unwrap()
            ))
        }
        _ => Err(format!(
            "Unknown command or invalid arguments: {name}. Type help."
        )),
    }
}
fn parse_address(text: &str) -> Result<std::net::SocketAddr, String> {
    text.parse()
        .or_else(|_| {
            text.parse::<std::net::IpAddr>()
                .map(|ip| std::net::SocketAddr::new(ip, 27015))
        })
        .map_err(|_| {
            "Use an IP address, optionally with :port (IPv6 with port uses [address]:port).".into()
        })
}
fn draw(
    console: Res<DeveloperConsole>,
    diagnostics: Res<DiagnosticsStore>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut roots: Query<(&mut Visibility, &mut Node), With<ConsoleRoot>>,
    mut texts: Query<(&mut Text, &ConsoleText)>,
) {
    for (mut visibility, mut node) in &mut roots {
        let target = if console.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != target {
            *visibility = target;
        }
        let width = px((window.width() - 24.0).clamp(240.0, 1200.0));
        if node.width != width {
            node.width = width;
        }
    }
    if !console.open {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let rows = ((window.height() * 0.8 - 120.0) / 23.0).clamp(2.0, 30.0) as usize;
    let lines = console
        .output
        .iter()
        .skip(console.output.len().saturating_sub(rows))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let (before, after) = console.line.split_at(console.cursor);
    for (mut text, part) in &mut texts {
        let display = match part {
            ConsoleText::Title => {
                format!("Developer Console                                      {fps:.0} fps")
            }
            ConsoleText::Output => lines.clone(),
            ConsoleText::Prompt => format!("] {before}|{after}"),
        };
        if text.0 != display {
            text.0 = display;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_bind_persists_wheel_jump_without_overwriting_other_settings() {
        let mut settings = crate::settings::Settings::default();
        settings.volume = 0.73;
        settings.radar_scale = 0.8;
        let tokens = words("bind \"mwheeldown\" \"+jump\"").unwrap();
        bind(&mut settings, &tokens[1..], false).unwrap();
        assert_eq!(settings.wheel_down, crate::settings::WheelAction::Jump);
        assert_eq!(settings.binds[4], KeyCode::Space);
        assert_eq!(
            crate::settings::Settings::parse(&settings.encode()),
            settings
        );
        assert!(words("volume 0; quit").is_err());
        assert!(words("bind \"mwheelup").is_err());
    }
    #[test]
    fn binding_history_and_addresses_reject_unsupported_commands() {
        let mut settings = crate::settings::Settings::default();
        let before = settings.clone();
        assert!(bind(&mut settings, &["mwheelup".into(), "quit".into()], false).is_err());
        assert_eq!(settings, before);
        assert_eq!(parse_address("127.0.0.1").unwrap().port(), 27015);
        assert!(parse_address("file:///tmp/foo").is_err());
        let mut console = DeveloperConsole::default();
        console.history = vec!["volume 0.1".into(), "volume 0.5".into()];
        console.line = "sens".into();
        console.recall(true);
        console.recall(true);
        assert_eq!(console.line, "volume 0.1");
        console.recall(false);
        console.recall(false);
        assert_eq!(console.line, "sens");
    }
    #[test]
    fn console_consumes_open_typing_and_escape_before_gameplay_and_movie_readers() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin))
            .init_resource::<DeveloperConsole>()
            .init_resource::<crate::menu::Menu>()
            .init_resource::<crate::player::InputBlockFrames>()
            .insert_resource(crate::player::InputEnabled(true))
            .add_systems(PreUpdate, input.after(InputSystems));
        let window = app
            .world_mut()
            .spawn((Window::default(), CursorOptions::default(), PrimaryWindow))
            .id();
        let key = |code, text: Option<&str>| KeyboardInput {
            key_code: code,
            logical_key: bevy::input::keyboard::Key::Character(text.unwrap_or("").into()),
            state: ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window,
        };
        app.world_mut()
            .resource_mut::<Messages<KeyboardInput>>()
            .write(key(KeyCode::Backquote, Some("`")));
        app.update();
        assert!(app.world().resource::<DeveloperConsole>().open);
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .get_pressed()
                .next()
                .is_none()
        );
        assert!(app.world().resource::<Messages<KeyboardInput>>().is_empty());
        app.world_mut()
            .resource_mut::<Messages<KeyboardInput>>()
            .write(key(KeyCode::KeyW, Some("w")));
        app.update();
        assert_eq!(app.world().resource::<DeveloperConsole>().line, "w");
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyW)
        );
        app.world_mut()
            .resource_mut::<Messages<KeyboardInput>>()
            .write(key(KeyCode::Escape, None));
        app.update();
        assert!(!app.world().resource::<DeveloperConsole>().open);
        assert!(app.world().resource::<DeveloperConsole>().blocked_frame);
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Escape)
        );
        app.update();
        assert!(!app.world().resource::<DeveloperConsole>().blocks_input());
    }
}
