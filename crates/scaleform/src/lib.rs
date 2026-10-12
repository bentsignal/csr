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

/// Reads public export/import names without interpreting movie instructions.
/// The frontend uses these names to offer cards present in the installed UI.
pub fn library_symbols(data: &[u8]) -> Result<std::collections::HashSet<String>, String> {
    rewrite::symbol_names(data).map_err(|error| error.to_string())
}

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
    /// Engine-generated image URLs, resolved from the game's local data.
    /// Returning encoded image bytes keeps runtime art outside the repository.
    fn image(&mut self, _url: &str) -> Option<Vec<u8>> { None }

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

    /// Sets a public movie property without replacing its authored display object.
    pub fn set(&mut self, path: &str, property: &str, value: Value) {
        self.queue.push(Command::Set { path: path.into(), property: property.into(), value });
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
    /// Native text-field navigation/editing, separate from Flash Key events.
    Edit { name: String, control: bool, shift: bool },
    Focus(bool),
    Char(char),
    Resize { width: u32, height: u32 },
    Call { path: String, function: String, args: Vec<Value> },
    Set { path: String, property: String, value: Value },
    RequestElement(String),
    /// Read-only names/types of a runtime object's public properties.
    Inspect(String),
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
    Set { path: String, property: String, value: Value },
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
    Inspected { path: String, properties: Vec<String> },
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

const INPUT_TURN_BUDGET: usize = 256;
const WORKER_TRACE_LIMIT: usize = 65_536;

/// A progress opportunity follows every finite input batch, including batches
/// whose producer keeps the channel nonempty. Input order is never changed.
enum InputTurn<T> {
    Input(T),
    Progress {
        count: usize,
        budget_exhausted: bool,
    },
}
fn service_input_turn<T, E>(
    input: &Receiver<T>,
    budget: usize,
    mut service: impl FnMut(InputTurn<T>) -> Result<(), E>,
) -> Result<bool, E> {
    assert!(budget > 0);
    let mut count = 0;
    while count < budget {
        match input.try_recv() {
            Ok(message) => {
                service(InputTurn::Input(message))?;
                count += 1;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => return Ok(false),
        }
    }
    service(InputTurn::Progress {
        count,
        budget_exhausted: count == budget,
    })?;
    Ok(true)
}

struct WorkerTrace {
    remaining: usize,
}
impl WorkerTrace {
    fn from_env() -> Self {
        Self {
            remaining: if std::env::var("CSR_UI_WORKER_TRACE").as_deref() == Ok("1") {
                WORKER_TRACE_LIMIT
            } else {
                0
            },
        }
    }
    fn event(
        &mut self,
        phase: &'static str,
        active: bool,
        serial: u64,
        count: usize,
        exhausted: bool,
    ) {
        if self.remaining == 0 {
            return;
        }
        self.remaining -= 1;
        tracing::info!(target: "scaleform::worker", phase, active, serial, input_count = count, budget_exhausted = exhausted, "UI worker");
        if self.remaining == 0 {
            tracing::info!(target: "scaleform::worker", "UI worker trace limit reached; tracing stopped");
        }
    }
}

fn run(
    config: Config,
    engine: Box<dyn Engine>,
    input: Receiver<Input>,
    events: Sender<Event>,
    frame: Arc<Mutex<Option<Frame>>>,
) -> Result<(), String> {
    let mut trace = WorkerTrace::from_env();
    trace.event("host_begin", true, 0, 0, false);
    let mut host = host::Host::new(&config, engine, events).map_err(|e| e.to_string())?;
    trace.event("host_end", true, 0, 0, false);
    let mut active = true;
    let mut serial = 0u64;
    let mut requested = false;
    let mut last = Instant::now();
    loop {
        trace.event("input_begin", active, serial, 0, false);
        let connected =
            service_input_turn(&input, INPUT_TURN_BUDGET, |turn| -> Result<(), String> {
                match turn {
                    InputTurn::Input(message) => match message {
                        Input::SetActive(value) => {
                            active = value;
                            trace.event("set_active", active, serial, 0, false);
                        }
                        Input::Resize { width, height } => host.resize(width, height),
                        Input::Inspect(path) => host.inspect(&path),
                        Input::Call {
                            path,
                            function,
                            args,
                        } => {
                            host.run_command(Command::Call {
                                path,
                                function,
                                args,
                            });
                        }
                        Input::Set {
                            path,
                            property,
                            value,
                        } => {
                            host.run_command(Command::Set {
                                path,
                                property,
                                value,
                            });
                        }
                        Input::RequestElement(element) => {
                            trace.event("request_begin", active, serial, 0, false);
                            host.run_command(Command::RequestElement(element));
                            trace.event("request_end", active, serial, 0, false);
                        }
                        other => host.input(other),
                    },
                    InputTurn::Progress {
                        count,
                        budget_exhausted,
                    } => {
                        trace.event("input_end", active, serial, count, budget_exhausted);
                        if !requested && host.root_ready() {
                            requested = true;
                            host.define_components();
                            for element in &config.elements {
                                trace.event(
                                    "request_begin",
                                    active,
                                    serial,
                                    count,
                                    budget_exhausted,
                                );
                                host.run_command(Command::RequestElement(element.clone()));
                                trace.event("request_end", active, serial, count, budget_exhausted);
                            }
                        }
                        let now = Instant::now();
                        let dt = now - last;
                        last = now;
                        if active {
                            // Host::step pumps the executor, advances the player and drains
                            // engine commands. Separate render spans identify GPU readback waits.
                            trace.event("step_begin", active, serial, count, budget_exhausted);
                            host.step(dt);
                            trace.event("step_end", active, serial, count, budget_exhausted);
                            if host.needs_render() {
                                trace.event(
                                    "render_begin",
                                    active,
                                    serial,
                                    count,
                                    budget_exhausted,
                                );
                                let (width, height, rgba) =
                                    host.render().map_err(|e| e.to_string())?;
                                serial += 1;
                                trace.event("render_end", active, serial, count, budget_exhausted);
                                if let Ok(mut slot) = frame.lock() {
                                    *slot = Some(Frame {
                                        width,
                                        height,
                                        rgba,
                                        serial,
                                    });
                                }
                            }
                        } else {
                            trace.event("executor_begin", active, serial, count, budget_exhausted);
                            host.pump_loads();
                            trace.event("executor_end", active, serial, count, budget_exhausted);
                        }
                    }
                }
                Ok(())
            })?;
        if !connected {
            trace.event("disconnected", active, serial, 0, false);
            return Ok(());
        }
        // Movies run at 30 fps; 4 ms keeps input latency low without spinning.
        std::thread::sleep(Duration::from_millis(4));
    }
}

#[cfg(test)]
mod worker_tests {
    use super::{INPUT_TURN_BUDGET, InputTurn, service_input_turn};
    use std::sync::mpsc::channel;

    #[test]
    fn excess_input_stays_fifo_and_progress_runs_after_budget() {
        let (sender, receiver) = channel();
        for n in 0..INPUT_TURN_BUDGET + 3 {
            sender.send(n).unwrap();
        }
        let mut delivered = Vec::new();
        let mut ticks = 0;
        let connected = service_input_turn(&receiver, INPUT_TURN_BUDGET, |turn| {
            match turn {
                InputTurn::Input(n) => delivered.push(n),
                InputTurn::Progress {
                    count,
                    budget_exhausted,
                } => {
                    assert_eq!(count, INPUT_TURN_BUDGET);
                    assert!(budget_exhausted);
                    assert_eq!(delivered.len(), INPUT_TURN_BUDGET);
                    ticks += 1;
                }
            }
            Ok::<_, ()>(())
        })
        .unwrap();
        assert!(connected);
        assert_eq!(ticks, 1);
        assert_eq!(delivered, (0..INPUT_TURN_BUDGET).collect::<Vec<_>>());
        assert_eq!(
            receiver.try_iter().collect::<Vec<_>>(),
            (INPUT_TURN_BUDGET..INPUT_TURN_BUDGET + 3).collect::<Vec<_>>()
        );
    }

    #[test]
    fn refilling_producer_cannot_prevent_a_progress_opportunity() {
        let (sender, receiver) = channel();
        sender.send(0).unwrap();
        let mut delivered = Vec::new();
        let mut ticks = 0;
        service_input_turn(&receiver, INPUT_TURN_BUDGET, |turn| {
            match turn {
                InputTurn::Input(n) => {
                    delivered.push(n);
                    sender.send(n + 1).unwrap();
                }
                InputTurn::Progress {
                    count,
                    budget_exhausted,
                } => {
                    assert_eq!(count, INPUT_TURN_BUDGET);
                    assert!(budget_exhausted);
                    ticks += 1;
                }
            }
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(ticks, 1);
        assert_eq!(delivered, (0..INPUT_TURN_BUDGET).collect::<Vec<_>>());
        assert_eq!(receiver.try_recv().unwrap(), INPUT_TURN_BUDGET);
    }

    #[test]
    fn disconnect_after_queued_inputs_does_not_add_a_tick_or_drop_messages() {
        let (sender, receiver) = channel();
        for n in 0..3 {
            sender.send(n).unwrap();
        }
        drop(sender);
        let mut delivered = Vec::new();
        let connected = service_input_turn(&receiver, INPUT_TURN_BUDGET, |turn| {
            match turn {
                InputTurn::Input(n) => delivered.push(n),
                InputTurn::Progress { .. } => panic!("disconnected worker must return"),
            }
            Ok::<_, ()>(())
        })
        .unwrap();
        assert!(!connected);
        assert_eq!(delivered, vec![0, 1, 2]);
    }
}
