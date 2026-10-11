//! CS:GO's Scaleform UI, played from the player's own install.
//!
//! CS:GO drew its menus with Scaleform (Flash). The Legacy install still ships
//! every movie in `resource/flash/*.swf`; this crate plays them with
//! [Ruffle](https://github.com/ruffle-rs/ruffle) and stands in for the engine
//! side: the `GameInterface` / `CScaleformComponent_*` objects the
//! ActionScript calls are answered by an [`Engine`] the game implements, and
//! the Scaleform-only features Ruffle lacks (font library, `#TOKEN`
//! translation, stage extensions) are emulated. Nothing is copied out of the
//! install. See `docs/research/scaleform-spike.md`.
//!
//! The player runs on its own thread ([`Menu::spawn`]); the game sends
//! [`Input`] and reads finished frames and [`Event`]s.

mod defaults;
pub mod diff;
mod host;
mod rewrite;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A value crossing between ActionScript and the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn str(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }

    pub fn num(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            Value::String(s) => s.parse().unwrap_or(0.0),
            Value::Bool(b) => *b as u8 as f64,
            _ => 0.0,
        }
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Number(v)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Number(v as f64)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Number(v as f64)
    }
}

/// The game side of the Scaleform API. Runs on the menu thread.
pub trait Engine: Send + 'static {
    /// Answers `object.method(args)`, e.g. `("CScaleformComponent_MyPersona",
    /// "GetXuid")` or `("MainMenu", "BasePanelRunCommand")` (a `gameAPI`,
    /// named after its element). `None` falls back to the built-in answers
    /// that keep the menu in its normal offline-but-licensed state, then to
    /// `undefined`.
    fn call(&mut self, object: &str, method: &str, args: &[Value], menu: &mut Calls) -> Option<Value>;
}

/// Calls the engine can make back into ActionScript from [`Engine::call`]
/// (queued; they run after the current call returns).
#[derive(Default)]
pub struct Calls {
    pub(crate) queue: Vec<Command>,
}

impl Calls {
    /// `path.function(args)`, e.g. `("_global.MainMenuMovie",
    /// "ScaleformComponent_FriendsList_RebuildFriendsList")`.
    pub fn call(&mut self, path: &str, function: &str, args: Vec<Value>) {
        self.queue.push(Command::Call { path: path.into(), function: function.into(), args });
    }

    /// Loads an element through the root movie's `RequestElement`, as the
    /// engine did (`"MainMenu"`, `"StartSinglePlayer"`, `"OptionsMenu"`...).
    pub fn request_element(&mut self, element: &str) {
        self.queue.push(Command::RequestElement(element.into()));
    }
}

/// Messages to the menu thread.
#[derive(Clone, Debug)]
pub enum Input {
    /// Window pixels; the stage is not scaled, so they map 1:1.
    MouseMove { x: f64, y: f64 },
    MouseDown { x: f64, y: f64, button: MouseButton },
    MouseUp { x: f64, y: f64, button: MouseButton },
    /// Lines; positive scrolls up.
    MouseWheel { lines: f64 },
    MouseLeave,
    /// A key as CS:GO's `_global.KeyDownEvent` expected: the Source key name
    /// (`"KEY_ESCAPE"`, `"KEY_ENTER"`, `"KEY_A"`...) and its Windows virtual-key code.
    Key { name: String, vkey: u32, down: bool },
    Char(char),
    Resize { width: u32, height: u32 },
    Call { path: String, function: String, args: Vec<Value> },
    RequestElement(String),
    /// Advance the movies only while visible (saves CPU in a match).
    SetActive(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone)]
pub(crate) enum Command {
    Call { path: String, function: String, args: Vec<Value> },
    RequestElement(String),
}

/// A rendered frame, RGBA8 (sRGB), row-major, `width * height * 4` bytes.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Increments with every new frame.
    pub serial: u64,
}

/// Notifications from the menu thread.
#[derive(Clone, Debug)]
pub enum Event {
    /// An element finished loading (`gameAPI.OnLoadFinished`), e.g. "MainMenu".
    Loaded(String),
    /// The player failed or panicked; the game should fall back.
    Failed(String),
}

pub struct Config {
    /// The CS:GO Legacy `csgo` folder.
    pub game_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    /// `resource/csgo_<language>.txt`.
    pub language: String,
    /// Elements to request once the root movie is up, in order.
    pub elements: Vec<String>,
}

/// Handle to the menu thread.
pub struct Menu {
    input: Sender<Input>,
    events: Receiver<Event>,
    frame: Arc<Mutex<Option<Frame>>>,
}

impl Menu {
    /// Starts the player on its own thread. Loading the main menu takes a few
    /// seconds; frames arrive once something is drawn.
    pub fn spawn(config: Config, engine: Box<dyn Engine>) -> Self {
        let (input_tx, input_rx) = channel();
        let (event_tx, event_rx) = channel();
        let frame = Arc::new(Mutex::new(None));
        let shared = frame.clone();
        std::thread::Builder::new()
            .name("scaleform".into())
            .spawn(move || {
                let events = event_tx.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(config, engine, input_rx, event_tx, shared)
                }));
                let message = match result {
                    Ok(Ok(())) => return,
                    Ok(Err(e)) => e,
                    Err(panic) => panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "panic".into()),
                };
                tracing::error!("scaleform: {message}");
                let _ = events.send(Event::Failed(message));
            })
            .expect("spawning the scaleform thread");
        Self { input: input_tx, events: event_rx, frame }
    }

    pub fn send(&self, input: Input) {
        let _ = self.input.send(input);
    }

    /// The newest frame, if one arrived since the last call.
    pub fn take_frame(&self) -> Option<Frame> {
        self.frame.lock().ok()?.take()
    }

    pub fn poll_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}

fn run(
    config: Config,
    engine: Box<dyn Engine>,
    input: Receiver<Input>,
    events: Sender<Event>,
    frame: Arc<Mutex<Option<Frame>>>,
) -> Result<(), String> {
    let mut host = host::Host::new(&config, engine, events).map_err(|e| e.to_string())?;
    let mut active = true;
    let mut serial = 0u64;
    let mut requested = false;
    let mut last = Instant::now();
    loop {
        loop {
            match input.try_recv() {
                Ok(Input::SetActive(a)) => active = a,
                Ok(Input::Resize { width, height }) => host.resize(width, height),
                Ok(Input::Call { path, function, args }) => {
                    host.run_command(Command::Call { path, function, args });
                }
                Ok(Input::RequestElement(e)) => host.run_command(Command::RequestElement(e)),
                Ok(other) => host.input(other),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        if !requested && host.root_ready() {
            requested = true;
            host.define_components();
            for element in &config.elements {
                host.run_command(Command::RequestElement(element.clone()));
            }
        }
        let now = Instant::now();
        let dt = now - last;
        last = now;
        if active {
            host.step(dt);
            if host.needs_render() {
                let (width, height, rgba) = host.render().map_err(|e| e.to_string())?;
                serial += 1;
                if let Ok(mut slot) = frame.lock() {
                    *slot = Some(Frame { width, height, rgba, serial });
                }
            }
        } else {
            host.pump_loads();
        }
        // Movies run at 30 fps; 4 ms keeps input latency low without spinning.
        std::thread::sleep(Duration::from_millis(4));
    }
}
