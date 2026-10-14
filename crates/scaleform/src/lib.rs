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
mod capture;
pub mod diff;
mod host;
mod rewrite;

/// Reads public export/import names without interpreting movie instructions.
/// The frontend uses these names to offer cards present in the installed UI.
pub fn library_symbols(data: &[u8]) -> Result<std::collections::HashSet<String>, String> {
    rewrite::symbol_names(data).map_err(|error| error.to_string())
}

use std::collections::VecDeque;
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
    /// One display transition. No movie tick or frame publication can split
    /// these commands; resulting frames carry this presentation revision.
    Present { commands: Vec<Command>, revision: u64 },
    /// Passive owned visibility bookkeeping. It observes the latest pointer
    /// state; unlike an authored call it does not create a motion boundary.
    ObservePauseSurface { args: Vec<Value> },
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
pub enum Command {
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
    /// Most recent completed display transition (zero before any transition).
    pub presentation_revision: u64,
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
    input: InputSender,
    events: Receiver<Event>,
    frame: Arc<Mutex<Option<Frame>>>,
}

impl Menu {
    /// Starts the player on its own thread. Loading the main menu takes a few
    /// seconds; frames arrive once something is drawn.
    pub fn spawn(config: Config, engine: Box<dyn Engine>) -> Self {
        let (input_tx, input_rx) = input_queue();
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
        self.input.send(input);
    }

    /// The newest frame, if one arrived since the last call.
    pub fn take_frame(&self) -> Option<Frame> {
        self.frame.lock().ok()?.take()
    }

    pub fn poll_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}

// A moving cursor must not retain obsolete positions while the worker renders.
// Only the immediately adjacent motion run is replaceable. Every other input
// forms a FIFO barrier, including release, focus and engine lifecycle commands.
struct InputQueue {
    pending: VecDeque<(Input, Instant)>,
    trace_remaining: usize,
    sender_alive: bool,
    receiver_alive: bool,
}
struct InputSender(Arc<Mutex<InputQueue>>);
struct InputReceiver(Arc<Mutex<InputQueue>>);
impl InputReceiver {
    fn has_pending(&self) -> bool {
        !self.0.lock().unwrap().pending.is_empty()
    }
}
fn input_queue() -> (InputSender, InputReceiver) {
    let shared = Arc::new(Mutex::new(InputQueue {
        pending: VecDeque::new(),
        trace_remaining: if std::env::var("CSR_UI_WORKER_TRACE").as_deref() == Ok("1") { WORKER_TRACE_LIMIT } else { 0 },
        sender_alive: true,
        receiver_alive: true,
    }));
    (InputSender(shared.clone()), InputReceiver(shared))
}
impl InputSender {
    fn send(&self, input: Input) {
        let mut queue = self.0.lock().unwrap();
        if !queue.receiver_alive {
            return;
        }
        if matches!(input, Input::MouseMove { .. }) {
            // Only our passive visibility sampler may sit between replaceable
            // positions. A click, key, focus, lifecycle or authored call still
            // freezes the preceding position and remains strictly FIFO.
            for (pending, queued) in queue.pending.iter_mut().rev() {
                match pending {
                    Input::ObservePauseSurface { .. } => continue,
                    Input::MouseMove { .. } => {
                        *pending = input;
                        *queued = Instant::now();
                        return;
                    }
                    _ => break,
                }
            }
        }
        queue.pending.push_back((input, Instant::now()));
    }
}
impl Drop for InputSender {
    fn drop(&mut self) {
        self.0.lock().unwrap().sender_alive = false;
    }
}
impl Drop for InputReceiver {
    fn drop(&mut self) {
        let mut queue = self.0.lock().unwrap();
        queue.receiver_alive = false;
        queue.pending.clear();
    }
}
trait InputSource<T> {
    fn try_input(&self) -> Result<T, TryRecvError>;
    /// Take at most one FIFO boundary that already exists at this checkpoint.
    /// Sources without inspectable adjacency retain the ordinary finite batch.
    fn take_boundary(&self, _remaining: usize) -> Vec<T> { Vec::new() }
}
impl InputSource<Input> for InputReceiver {
    fn try_input(&self) -> Result<Input, TryRecvError> {
        let mut queue = self.0.lock().unwrap();
        if let Some((input, queued)) = queue.pending.pop_front() {
            queue.trace_motion(&input, queued);
            return Ok(input);
        }
        Err(if queue.sender_alive {
            TryRecvError::Empty
        } else {
            TryRecvError::Disconnected
        })
    }
    fn take_boundary(&self, remaining: usize) -> Vec<Input> {
        let mut queue = self.0.lock().unwrap();
        let count = match queue.pending.front() {
            Some((Input::MouseMove { .. }, _)) => match queue.pending.get(1) {
                Some((input, _)) if !matches!(input, Input::MouseMove { .. }) => 2,
                _ => return Vec::new(),
            },
            Some(_) => 1,
            None => return Vec::new(),
        };
        if count > remaining { return Vec::new(); }
        // Snapshot under one lock before running callbacks. Producer work
        // appended while these events execute cannot extend this completion.
        let entries: Vec<_> = queue.pending.drain(..count).collect();
        entries.into_iter().map(|(input, queued)| {
            queue.trace_motion(&input, queued);
            input
        }).collect()
    }
}
impl InputQueue {
    fn trace_motion(&mut self, input: &Input, queued: Instant) {
        if self.trace_remaining == 0 { return; }
        if let Input::MouseMove { x, y } = input {
            self.trace_remaining -= 1;
            tracing::info!(target: "scaleform::queue", age_ms = queued.elapsed().as_secs_f64() * 1000., pending = self.pending.len(), x, y, "UI pointer consumed");
        }
    }
}
impl<T> InputSource<T> for Receiver<T> {
    fn try_input(&self) -> Result<T, TryRecvError> {
        self.try_recv()
    }
}

const INPUT_TURN_BUDGET: usize = 256;
const INPUT_TURN_TIME: Duration = Duration::from_millis(8);
const WORKER_TRACE_LIMIT: usize = 65_536;

/// A progress opportunity follows every finite input batch, including batches
/// whose producer keeps the queue nonempty. Barriers are never reordered.
/// After elapsed expiry, at most one existing FIFO boundary completes before
/// progress: a non-motion event, with its preceding motion if necessary. This
/// prevents one slow motion from repeatedly deferring a queued click/key while
/// keeping pure-motion turns unchanged. All calls still count toward the cap.
/// The elapsed budget cannot preempt a slow movie action or GPU readback.
enum InputTurn<T> {
    Input(T),
    Progress {
        count: usize,
        budget_exhausted: bool,
    },
}
fn service_input_turn<T, E>(
    input: &impl InputSource<T>,
    budget: usize,
    service: impl FnMut(InputTurn<T>) -> Result<(), E>,
) -> Result<bool, E> {
    let started = Instant::now();
    service_input_turn_until(
        input,
        budget,
        || started.elapsed() >= INPUT_TURN_TIME,
        service,
    )
}
fn service_input_turn_until<T, E>(
    input: &impl InputSource<T>,
    budget: usize,
    mut expired: impl FnMut() -> bool,
    mut service: impl FnMut(InputTurn<T>) -> Result<(), E>,
) -> Result<bool, E> {
    assert!(budget > 0);
    let mut count = 0;
    let mut time_exhausted = false;
    while count < budget {
        match input.try_input() {
            Ok(message) => {
                service(InputTurn::Input(message))?;
                count += 1;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => return Ok(false),
        }
        if expired() {
            time_exhausted = true;
            for message in input.take_boundary(budget - count) {
                service(InputTurn::Input(message))?;
                count += 1;
            }
            break;
        }
    }
    service(InputTurn::Progress {
        count,
        budget_exhausted: count == budget || time_exhausted,
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
    input: InputReceiver,
    events: Sender<Event>,
    frame: Arc<Mutex<Option<Frame>>>,
) -> Result<(), String> {
    let mut trace = WorkerTrace::from_env();
    trace.event("host_begin", true, 0, 0, false);
    let mut host = host::Host::new(&config, engine, events).map_err(|e| e.to_string())?;
    trace.event("host_end", true, 0, 0, false);
    let mut active = true;
    let mut serial = 0u64;
    let mut presentation_revision = 0u64;
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
                        Input::Present { commands, revision } => {
                            for command in commands {
                                host.run_command(command);
                            }
                            presentation_revision = revision;
                        }
                        Input::ObservePauseSurface { args } => host.run_command(Command::Call {
                            path: "_global.csrBindings".into(),
                            function: "samplePauseSurface".into(),
                            args,
                        }),
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
                                        presentation_revision,
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
        // Do not add an idle wait behind inputs that arrived during a movie
        // action/readback. Every turn still gives movies a progress opportunity;
        // an empty queue retains the throttle rather than spinning.
        if !input.has_pending() {
            std::thread::sleep(Duration::from_millis(4));
        }
    }
}

#[cfg(test)]
mod worker_tests {
    use super::{INPUT_TURN_BUDGET, Input, InputSource, InputTurn, MouseButton, Value, input_queue, service_input_turn_until};
    use std::sync::mpsc::channel;

    #[test]
    fn passive_surface_sample_keeps_latest_pointer_but_never_crosses_click_or_call() {
        let (sender, receiver) = input_queue();
        sender.send(motion(1));
        sender.send(Input::ObservePauseSurface { args: vec!["first".into()] });
        sender.send(motion(2));
        sender.send(Input::MouseDown { x: 2., y: -2., button: MouseButton::Left });
        sender.send(motion(3));
        sender.send(Input::ObservePauseSurface { args: vec!["second".into()] });
        sender.send(motion(4));
        sender.send(Input::Call { path: "button".into(), function: "activate".into(), args: vec![] });
        sender.send(motion(5));
        assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 2., .. })));
        assert!(matches!(receiver.try_input(), Ok(Input::ObservePauseSurface { .. })));
        assert!(matches!(receiver.try_input(), Ok(Input::MouseDown { x: 2., .. })));
        assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 4., .. })));
        assert!(matches!(receiver.try_input(), Ok(Input::ObservePauseSurface { .. })));
        assert!(matches!(receiver.try_input(), Ok(Input::Call { .. })));
        assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 5., .. })));
        assert!(matches!(receiver.try_input(), Err(std::sync::mpsc::TryRecvError::Empty)));
    }

    #[test]
    fn excess_input_stays_fifo_and_progress_runs_after_budget() {
        let (sender, receiver) = channel();
        for n in 0..INPUT_TURN_BUDGET + 3 {
            sender.send(n).unwrap();
        }
        let mut delivered = Vec::new();
        let mut ticks = 0;
        let connected = service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || false, |turn| {
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
        service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || false, |turn| {
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
        let connected = service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || false, |turn| {
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

    fn motion(n: usize) -> Input {
        Input::MouseMove {
            x: n as f64,
            y: -(n as f64),
        }
    }

    #[test]
    fn high_rate_motion_preserves_every_barrier_and_its_latest_position() {
        let barriers = vec![
            Input::MouseDown {
                x: 31.,
                y: -31.,
                button: MouseButton::Left,
            },
            Input::MouseUp {
                x: 31.,
                y: -31.,
                button: MouseButton::Left,
            },
            Input::MouseDown {
                x: 31.,
                y: -31.,
                button: MouseButton::Right,
            },
            Input::MouseUp {
                x: 31.,
                y: -31.,
                button: MouseButton::Middle,
            },
            Input::MouseWheel { lines: -2. },
            Input::MouseLeave,
            Input::Key {
                name: "KEY_A".into(),
                vkey: 65,
                down: true,
            },
            Input::Key {
                name: "KEY_A".into(),
                vkey: 65,
                down: false,
            },
            Input::Edit {
                name: "KEY_LEFT".into(),
                control: true,
                shift: false,
            },
            Input::Focus(false),
            Input::Focus(true),
            Input::Char('猫'),
            Input::Resize {
                width: 1280,
                height: 720,
            },
            Input::Call {
                path: "root".into(),
                function: "close".into(),
                args: vec![Value::Bool(true)],
            },
            Input::Set {
                path: "root".into(),
                property: "_visible".into(),
                value: Value::Bool(false),
            },
            Input::RequestElement("OptionsMenu".into()),
            Input::Inspect("root".into()),
            Input::SetActive(false),
            Input::SetActive(true),
        ];
        let (sender, receiver) = input_queue();
        let mut expected = Vec::new();
        for repetition in 0..100 {
            for barrier in &barriers {
                for n in 0..32 {
                    sender.send(motion(repetition * 32 + n));
                }
                expected.push(format!("{:?}", motion(repetition * 32 + 31)));
                sender.send(barrier.clone());
                expected.push(format!("{barrier:?}"));
            }
        }
        assert_eq!(receiver.0.lock().unwrap().pending.len(), expected.len());
        let mut actual = Vec::new();
        while let Ok(input) = receiver.try_input() {
            actual.push(format!("{input:?}"));
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn concurrent_producer_keeps_final_motion_before_each_click_barrier() {
        let (sender, receiver) = input_queue();
        let producer = std::thread::spawn(move || {
            for index in 0..1000 {
                for n in 0..50 { sender.send(motion(index * 50 + n)); }
                let coordinate = (index * 50 + 49) as f64;
                sender.send(Input::MouseDown { x: coordinate, y: -coordinate, button: MouseButton::Left });
                sender.send(Input::MouseUp { x: coordinate, y: -coordinate, button: MouseButton::Left });
            }
        });
        let mut last_motion = None;
        let mut clicks = 0;
        let mut held = false;
        loop {
            match receiver.try_input() {
                Ok(Input::MouseMove { x, .. }) => last_motion = Some(x),
                Ok(Input::MouseDown { x, .. }) => {
                    assert!(!held); held = true;
                    assert_eq!(x, (clicks * 50 + 49) as f64);
                    assert_eq!(last_motion, Some(x));
                }
                Ok(Input::MouseUp { x, .. }) => {
                    assert!(held); held = false;
                    assert_eq!(x, (clicks * 50 + 49) as f64); clicks += 1;
                }
                Ok(other) => panic!("unexpected generated input: {other:?}"),
                Err(std::sync::mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        producer.join().unwrap();
        assert_eq!(clicks, 1000); assert!(!held);
    }

    #[test]
    fn latest_motion_flushes_before_disconnect_and_closed_receiver_rejects_more_work() {
        let (sender, receiver) = input_queue();
        for n in 0..10_000 {
            sender.send(motion(n));
        }
        assert_eq!(receiver.0.lock().unwrap().pending.len(), 1);
        drop(sender);
        assert!(matches!(
            receiver.try_input(),
            Ok(Input::MouseMove {
                x: 9999.,
                y: -9999.
            })
        ));
        assert!(matches!(
            receiver.try_input(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        ));
        let (sender, receiver) = input_queue();
        sender.send(motion(0));
        drop(receiver);
        sender.send(Input::Focus(false));
        assert!(sender.0.lock().unwrap().pending.is_empty());
    }

    #[test]
    fn empty_live_queue_does_not_disconnect_and_consumed_motion_is_not_replayed() {
        let (sender, receiver) = input_queue();
        assert!(matches!(
            receiver.try_input(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        sender.send(motion(1));
        assert!(matches!(
            receiver.try_input(),
            Ok(Input::MouseMove { x: 1., .. })
        ));
        sender.send(motion(2));
        sender.send(motion(3));
        assert!(matches!(
            receiver.try_input(),
            Ok(Input::MouseMove { x: 3., .. })
        ));
        assert!(matches!(
            receiver.try_input(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn elapsed_budget_yields_progress_with_pending_fifo_work_and_live_producer() {
        let (sender, receiver) = channel();
        sender.send(0).unwrap();
        let mut checks = 0;
        let mut delivered = Vec::new();
        let connected = service_input_turn_until(
            &receiver,
            INPUT_TURN_BUDGET,
            || {
                checks += 1;
                checks == 3
            },
            |turn| {
                match turn {
                    InputTurn::Input(n) => {
                        delivered.push(n);
                        sender.send(n + 1).unwrap();
                    }
                    InputTurn::Progress {
                        count,
                        budget_exhausted,
                    } => {
                        assert_eq!(count, 3);
                        assert!(budget_exhausted);
                    }
                }
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        assert!(connected);
        assert_eq!(delivered, vec![0, 1, 2]);
        assert_eq!(receiver.try_recv().unwrap(), 3);
    }

    #[test]
    fn expired_motion_completes_queued_focus_loss_before_progress() {
        let (sender, receiver) = input_queue();
        sender.send(motion(1));
        sender.send(Input::Focus(false));
        let mut delivered = Vec::new();
        let mut focused = true;
        service_input_turn_until(
            &receiver,
            INPUT_TURN_BUDGET,
            || true,
            |turn| {
                match turn {
                    InputTurn::Input(input) => {
                        if let Input::Focus(value) = input { focused = value; }
                        delivered.push(format!("{input:?}"));
                    }
                    InputTurn::Progress {
                        count,
                        budget_exhausted,
                    } => {
                        assert_eq!(count, 2);
                        assert!(budget_exhausted);
                        assert!(!focused);
                    }
                }
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        assert_eq!(delivered, vec![format!("{:?}", motion(1)), "Focus(false)".into()]);
        assert!(matches!(receiver.try_input(), Err(std::sync::mpsc::TryRecvError::Empty)));
    }

    #[test]
    fn expired_pure_motion_keeps_progress_and_latest_pending_position() {
        let (sender, receiver) = input_queue();
        sender.send(motion(0));
        let mut progress = 0;
        service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || true, |turn| {
            match turn {
                InputTurn::Input(input) => {
                    assert!(matches!(input, Input::MouseMove { x: 0., .. }));
                    for n in 1..1000 { sender.send(motion(n)); }
                }
                InputTurn::Progress { count, budget_exhausted } => {
                    assert_eq!(count, 1); assert!(budget_exhausted); progress += 1;
                }
            }
            Ok::<_, ()>(())
        }).unwrap();
        assert_eq!(progress, 1);
        assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 999., .. })));
    }

    #[test]
    fn slow_motion_finishes_one_existing_click_key_or_command_boundary_in_order() {
        let barriers = vec![
            Input::MouseDown { x: 99., y: -99., button: MouseButton::Left },
            Input::MouseUp { x: 99., y: -99., button: MouseButton::Left },
            Input::Key { name: "KEY_A".into(), vkey: 65, down: true },
            Input::Inspect("_global.csrPerformanceProbe.1".into()),
            Input::Resize { width: 2560, height: 1440 },
            Input::SetActive(false),
        ];
        for barrier in barriers {
            let (sender, receiver) = input_queue();
            sender.send(motion(0));
            let mut delivered = Vec::new();
            let mut progress = 0;
            service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || true, |turn| {
                match turn {
                    InputTurn::Input(input) => {
                        delivered.push(format!("{input:?}"));
                        if delivered.len() == 1 {
                            // Arrives while the first indivisible motion runs.
                            for n in 1..100 { sender.send(motion(n)); }
                            sender.send(barrier.clone());
                            sender.send(motion(100));
                        }
                    }
                    InputTurn::Progress { count, budget_exhausted } => {
                        assert_eq!(count, 3); assert!(budget_exhausted); progress += 1;
                    }
                }
                Ok::<_, ()>(())
            }).unwrap();
            assert_eq!(delivered, vec![format!("{:?}", motion(0)), format!("{:?}", motion(99)), format!("{barrier:?}")]);
            assert_eq!(progress, 1);
            assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 100., .. })));
        }
    }

    #[test]
    fn insufficient_remaining_count_leaves_motion_and_its_boundary_queued() {
        let (sender, receiver) = input_queue();
        sender.send(motion(0));
        let key = Input::Key { name: "KEY_ENTER".into(), vkey: 13, down: true };
        service_input_turn_until(&receiver, 2, || true, |turn| {
            match turn {
                InputTurn::Input(input) => {
                    assert!(matches!(input, Input::MouseMove { x: 0., .. }));
                    sender.send(motion(1)); sender.send(key.clone());
                }
                InputTurn::Progress { count, .. } => assert_eq!(count, 1),
            }
            Ok::<_, ()>(())
        }).unwrap();
        assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 1., .. })));
        assert_eq!(format!("{:?}", receiver.try_input().unwrap()), format!("{key:?}"));
        // The same two-event snapshot fits exactly, never exceeding the cap.
        sender.send(motion(2)); sender.send(key.clone());
        assert_eq!(receiver.take_boundary(2).len(), 2);
        assert!(matches!(receiver.try_input(), Err(std::sync::mpsc::TryRecvError::Empty)));
    }

    #[test]
    fn boundary_snapshot_does_not_chase_producer_or_coalesce_through_reserved_key() {
        let (sender, receiver) = input_queue();
        sender.send(motion(0));
        let key = Input::Key { name: "KEY_A".into(), vkey: 65, down: true };
        let release = Input::MouseUp { x: 199., y: -199., button: MouseButton::Left };
        let mut delivered = Vec::new();
        service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || true, |turn| {
            match turn {
                InputTurn::Input(input) => {
                    delivered.push(format!("{input:?}"));
                    if delivered.len() == 1 {
                        for n in 1..100 { sender.send(motion(n)); }
                        sender.send(key.clone());
                    } else {
                        // More barriers arrive during completion. Only the
                        // already reserved boundary may finish this turn.
                        for n in 100..200 { sender.send(motion(n)); }
                        sender.send(release.clone());
                    }
                }
                InputTurn::Progress { count, .. } => assert_eq!(count, 3),
            }
            Ok::<_, ()>(())
        }).unwrap();
        assert_eq!(delivered, vec![format!("{:?}", motion(0)), format!("{:?}", motion(99)), format!("{key:?}")]);
        assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 199., .. })));
        assert_eq!(format!("{:?}", receiver.try_input().unwrap()), format!("{release:?}"));
        assert!(matches!(receiver.try_input(), Ok(Input::MouseMove { x: 199., .. })));
        assert_eq!(format!("{:?}", receiver.try_input().unwrap()), format!("{release:?}"));
    }

    #[test]
    fn shutdown_flushes_boundary_then_disconnects_without_repeating_progress() {
        let (sender, receiver) = input_queue();
        sender.send(motion(0)); sender.send(Input::SetActive(false));
        drop(sender);
        let mut active = true;
        let mut progress = 0;
        assert!(service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || true, |turn| {
            match turn {
                InputTurn::Input(Input::SetActive(value)) => active = value,
                InputTurn::Input(_) => (),
                InputTurn::Progress { count, .. } => {
                    assert_eq!(count, 2); assert!(!active); progress += 1;
                }
            }
            Ok::<_, ()>(())
        }).unwrap());
        assert!(!service_input_turn_until(&receiver, INPUT_TURN_BUDGET, || true, |_| -> Result<(), ()> {
            panic!("disconnected queue cannot deliver or tick again")
        }).unwrap());
        assert_eq!(progress, 1);
    }
}
