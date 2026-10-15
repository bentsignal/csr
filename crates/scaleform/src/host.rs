//! Ruffle with CS:GO's movies: an in-memory navigator serving the install's
//! `resource/flash` folder (plus our bridge shim and the virtual font
//! library), an ExternalInterface provider wired to the [`Engine`], and an
//! offscreen wgpu target read back for the game.

use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver as MapReceiver, Sender as EventSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender};
use ruffle_core::backend::navigator::{
    ErrorResponse, NavigationMethod, NavigatorBackend, NullExecutor, NullSpawner, OwnedFuture,
    Request, SuccessResponse, create_fetch_error,
};
use ruffle_core::swf::Twips;
use ruffle_core::events::{KeyDescriptor, KeyLocation, LogicalKey, NamedKey, PhysicalKey, MouseButton as RuffleButton, MouseWheelDelta, PlayerEvent, TextControlCode};
use ruffle_core::external::{ExternalInterfaceProvider, Value as ExtValue};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error as LoadError;
use ruffle_core::socket::{ConnectionState, SocketAction, SocketHandle};
use ruffle_core::{FloatDuration, Player, PlayerBuilder, StageAlign, StageScaleMode, ViewportDimensions};
use ruffle_render_wgpu::backend::{WgpuRenderBackend, create_wgpu_instance, request_adapter_and_device};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::TextureTarget;
use ruffle_render_wgpu::utils::BufferDimensions;
use ruffle_render_wgpu::wgpu;
use url::Url;

use crate::rewrite::{self, Localization, RewriteStats};
use crate::{Calls, Command, Config, Engine, Event, Input, MouseButton, Value};

const BASE_URL: &str = "file:///csgo/resource/flash/";
const SHIM_NAME: &str = "csr_bridge.swf";
const ROOT_MOVIE: &str = "mainuirootmovie.swf";
/// The root movie normally sits at level 0; our shim takes level 0, so the
/// root goes to a level no element uses.
const ROOT_LEVEL: u32 = 9000;

type Error = String;

/// Files the movies can load: `resource/flash/...` from the game's file system
/// (loose files, then `pak01_dir.vpk`), plus generated movies and images the
/// engine provides (`img://...`). Keyed by lower-case path relative to
/// `resource/flash/`.
struct Files {
    vfs: vfs::Vfs,
    engine: Rc<RefCell<Box<dyn Engine>>>,
    loc: Rc<Localization>,
    cache: RefCell<HashMap<String, Rc<Vec<u8>>>>,
    stats: RefCell<RewriteStats>,
}

impl Files {
    fn get(&self, url: &str) -> Option<Rc<Vec<u8>>> {
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let lower = path.to_ascii_lowercase();
        const PREFIX: &str = "/csgo/resource/flash/";
        let rel = match lower.find(PREFIX) {
            Some(i) => lower[i + PREFIX.len()..].to_string(),
            None => lower,
        };
        if let Some(data) = self.cache.borrow().get(&rel) {
            return Some(data.clone());
        }
        if let Some(request) = crate::loading_image::Request::parse(&rel) {
            let data = Rc::new(request.read(&self.vfs)?);
            const MAX_LOADING_CACHE: usize = 64 * 1024 * 1024;
            if data.len() <= MAX_LOADING_CACHE {
                let mut cache = self.cache.borrow_mut();
                let used: usize = cache.iter().filter(|(key, _)| crate::loading_image::Request::parse(key).is_some()).map(|(_, value)| value.len()).sum();
                if used + data.len() > MAX_LOADING_CACHE {
                    cache.retain(|key, _| crate::loading_image::Request::parse(key).is_none());
                }
                cache.insert(rel, data.clone());
            }
            return Some(data);
        }
        // Steam avatars came from the engine's image cache; until the game
        // supplies pictures, every avatar is a neutral tile.
        if rel.starts_with("img://avatar_") {
            return Some(Rc::new(solid_png(64, 64, [58, 64, 72])));
        }
        if rel.starts_with("img://") {
            // Native inventory images are addressed by opaque item ids, not
            // installation paths. The engine resolves only ids it owns.
            return self.engine.try_borrow_mut().ok()?.image(&rel).map(Rc::new);
        }
        if rel.split(['/', '\\']).any(|part| part == "..") || rel.contains(':') || rel.starts_with('/') {
            return None;
        }
        let Some(raw) = self.vfs.read(&format!("resource/flash/{rel}")) else {
            tracing::debug!("scaleform: no file {rel}");
            return None;
        };
        // This movie embeds only the eleven glyphs in its placeholder token.
        // The missing localized characters match that subset's omissions.
        // A Scaleform library fallback is inferred, not yet independently
        // established. Complete from the installed family while preserving
        // original glyphs; exact donor typography remains a measured gap.
        let raw = if rel == "message-box.swf" {
            match self.vfs.read("resource/flash/fontlib.swf")
                .ok_or_else(|| anyhow::anyhow!("fontlib.swf missing"))
                .and_then(|library| crate::messagebox_font::complete(&raw, &library)) {
                Ok(data) => data,
                Err(error) => { tracing::warn!("scaleform: MessageBox font completion: {error}"); raw }
            }
        } else { raw };
        let data = if rel.ends_with(".swf") {
            match rewrite::translate_movie(&raw, &self.loc, &mut self.stats.borrow_mut()) {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!("scaleform: rewriting {rel}: {e}");
                    raw
                }
            }
        } else {
            raw
        };
        let data = if rel == "single-player.swf" {
            let result = self.vfs.read("resource/flash/sharedlib.swf")
                .ok_or_else(|| anyhow::anyhow!("sharedlib.swf missing"))
                .and_then(|library| rewrite::symbol_names(&library))
                .and_then(|names| {
                    let mut maps: Vec<_> = names.iter().filter(|name| name.starts_with("map-") && name.ends_with("-overall")).map(String::as_str).collect();
                    maps.sort_unstable();
                    rewrite::import_symbols(&data, "sharedlib.swf", &maps)
                });
            match result { Ok(data) => data, Err(error) => { tracing::warn!("scaleform: shared map symbols: {error}"); data } }
        } else if rel == "optionsmenu.swf" {
            match rewrite::import_symbols(&data, "sharedlib.swf", &["slide-control_number", "btn-main-master"]) {
                Ok(data) => data,
                Err(error) => { tracing::warn!("scaleform: shared option symbols: {error}"); data }
            }
        } else { data };
        let data = Rc::new(data);
        self.cache.borrow_mut().insert(rel, data.clone());
        Some(data)
    }
}

pub(crate) fn solid_png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let tile = image::RgbaImage::from_pixel(width, height, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut png = Vec::new();
    tile.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encoding a PNG in memory");
    png
}

struct MemResponse {
    url: String,
    data: Option<Rc<Vec<u8>>>,
}

impl SuccessResponse for MemResponse {
    fn url(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.url)
    }
    fn set_url(&mut self, url: String) {
        self.url = url;
    }
    fn body(self: Box<Self>) -> OwnedFuture<Vec<u8>, LoadError> {
        let data = self.data.map(|d| (*d).clone()).unwrap_or_default();
        Box::pin(async move { Ok(data) })
    }
    fn text_encoding(&self) -> Option<&'static encoding_rs::Encoding> {
        None
    }
    fn status(&self) -> u16 {
        200
    }
    fn redirected(&self) -> bool {
        false
    }
    fn next_chunk(&mut self) -> OwnedFuture<Option<Vec<u8>>, LoadError> {
        let chunk = self.data.take().map(|d| (*d).clone());
        Box::pin(async move { Ok(chunk) })
    }
    fn expected_length(&self) -> Result<Option<u64>, LoadError> {
        Ok(self.data.as_ref().map(|d| d.len() as u64))
    }
}

struct InstallNavigator {
    files: Rc<Files>,
    spawner: NullSpawner,
    base: Url,
}

impl NavigatorBackend for InstallNavigator {
    fn navigate_to_url(&self, url: &str, target: &str, _vars: Option<(NavigationMethod, IndexMap<String, String>)>) {
        tracing::info!("scaleform: navigate_to_url {url} ({target})");
    }

    fn fetch(&self, request: Request) -> OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse> {
        let result = (|| {
            // Ruffle parses SuccessResponse.url without a base when constructing
            // loaded movies/images. Keep its identity absolute, including query
            // parameters, even when the request used an install-relative path.
            let url = self.resolve_url(request.url())
                .map_err(|error| create_fetch_error(request.url(), error))?;
            if !matches!(url.scheme(), "file" | "img") {
                return Err(create_fetch_error(request.url(), "unsupported install URL scheme"));
            }
            // Preserve the original lookup restrictions: resolving first must
            // not turn a rejected parent path into an admitted local request.
            let data = self.files.get(request.url())
                .ok_or_else(|| create_fetch_error(request.url(), "not in the install"))?;
            Ok(Box::new(MemResponse { url: url.to_string(), data: Some(data) }) as Box<dyn SuccessResponse>)
        })();
        Box::pin(async move { result })
    }

    fn resolve_url(&self, url: &str) -> Result<Url, url::ParseError> {
        self.base.join(url)
    }

    fn spawn_future(&mut self, future: OwnedFuture<(), LoadError>) {
        self.spawner.spawn_local(future);
    }

    fn pre_process_url(&self, url: Url) -> Url {
        url
    }

    fn connect_socket(
        &mut self,
        _host: String,
        _port: u16,
        _timeout: Duration,
        handle: SocketHandle,
        _receiver: Receiver<Vec<u8>>,
        sender: Sender<SocketAction>,
    ) {
        let _ = sender.try_send(SocketAction::Connect(handle, ConnectionState::Failed));
    }
}

/// Routes every proxied call: bookkeeping, the game's [`Engine`], then the
/// built-in defaults.
struct Dispatcher {
    engine: Rc<RefCell<Box<dyn Engine>>>,
    loc: Rc<Localization>,
    calls: Calls,
    events: EventSender<Event>,
    root_ready: bool,
    slider_axis_pending: Option<f64>,
}

impl Dispatcher {
    fn call(&mut self, name: &str, args: &[Value]) -> Value {
        let (object, method) = name.split_once('.').unwrap_or(("", name));
        match (object, method) {
            ("csr", "RootReady") => {
                self.root_ready = true;
                // A bare-root HUD has no requested menu element to announce
                // readiness. Its engine queues the owned HUD loader here.
                let _ = self.engine.borrow_mut().call(object, method, args, &mut self.calls);
                return Value::Undefined;
            }
            ("csr", "RegisterSliderDragAxis") => {
                let [Value::String(handle), Value::Number(axis)] = args else { return Value::Bool(false) };
                let Some(path) = handle.strip_prefix("mc:") else { return Value::Bool(false) };
                if !path.starts_with("_level") || !path.ends_with(".Widget.ScrollPip")
                    || !axis.is_finite() || axis.abs() > 16_384. {
                    return Value::Bool(false);
                }
                self.slider_axis_pending = Some(*axis);
                return Value::Bool(true);
            }
            (_, "OnLoadFinished") if !object.starts_with("CScaleformComponent_") => {
                if object == "OptionsMenu" { self.slider_axis_pending = None; }
                let _ = self.events.send(Event::Loaded(object.to_string()));
            }
            _ => {}
        }
        if let Some(v) = self.engine.borrow_mut().call(object, method, args, &mut self.calls) {
            return v;
        }
        if method == "Translate" {
            let token = args.first().map(Value::str).unwrap_or_default();
            return Value::String(self.loc.translate_whole(&token).unwrap_or(&token).to_string());
        }
        crate::defaults::call(object, method, args).unwrap_or(Value::Undefined)
    }
}

struct Provider(Rc<RefCell<Dispatcher>>);

impl ExternalInterfaceProvider for Provider {
    fn call_method(&self, _context: &mut ruffle_core::context::UpdateContext<'_>, name: &str, args: &[ExtValue]) -> ExtValue {
        let args: Vec<Value> = args.iter().map(from_ext).collect();
        to_ext(self.0.borrow_mut().call(name, &args))
    }
    fn on_callback_available(&self, _name: &str) {}
    fn get_id(&self) -> Option<String> {
        None
    }
}

fn from_ext(v: &ExtValue) -> Value {
    match v {
        ExtValue::Undefined => Value::Undefined,
        ExtValue::Null => Value::Null,
        ExtValue::Bool(b) => Value::Bool(*b),
        ExtValue::Number(n) => Value::Number(*n),
        ExtValue::String(s) => Value::String(s.clone()),
        ExtValue::List(l) => Value::List(l.iter().map(from_ext).collect()),
        ExtValue::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), from_ext(v))).collect()),
    }
}

fn to_ext(v: Value) -> ExtValue {
    match v {
        Value::Undefined => ExtValue::Undefined,
        Value::Null => ExtValue::Null,
        Value::Bool(b) => ExtValue::Bool(b),
        Value::Number(n) => ExtValue::Number(n),
        Value::String(s) => ExtValue::String(s),
        Value::List(l) => ExtValue::List(l.into_iter().map(to_ext).collect()),
        Value::Object(o) => ExtValue::Object(o.into_iter().map(|(k, v)| (k, to_ext(v))).collect()),
    }
}

struct PendingReadback {
    buffer: wgpu::Buffer,
    device: wgpu::Device,
    dimensions: BufferDimensions,
    width: u32,
    height: u32,
    presentation_revision: u64,
    viewport_generation: u64,
    completion: MapReceiver<Result<(), wgpu::BufferAsyncError>>,
    started: Option<Instant>,
    submitted: Instant,
}

impl Drop for PendingReadback {
    fn drop(&mut self) {
        // Also cancels a still-pending mapping on shutdown or failure. Mapped
        // views live only inside poll_render and are dropped before this guard.
        self.buffer.unmap();
    }
}

pub(crate) struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub presentation_revision: u64,
}

pub(crate) struct Host {
    player: Arc<Mutex<Player>>,
    dispatcher: Rc<RefCell<Dispatcher>>,
    executor: NullExecutor,
    mouse: (f64, f64),
    held_keys: HashMap<String, u32>,
    held_buttons: HashSet<MouseButton>,
    render_trace_remaining: usize,
    work_trace_remaining: usize,
    pending_readback: Option<PendingReadback>,
    viewport_generation: u64,
    overlay: bool,
}

impl Host {
    pub fn new(config: &Config, engine: Box<dyn Engine>, events: EventSender<Event>) -> Result<Self, Error> {
        Self::new_surface(config, engine, events, false)
    }

    pub fn overlay(config: &Config, engine: Box<dyn Engine>, events: EventSender<Event>) -> Result<Self, Error> {
        Self::new_surface(config, engine, events, true)
    }

    fn new_surface(config: &Config, engine: Box<dyn Engine>, events: EventSender<Event>, overlay: bool) -> Result<Self, Error> {
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&config.game_dir).map_err(|e| format!("mounting {}: {e}", config.game_dir.display()))?;
        let loc_path = format!("resource/csgo_{}.txt", config.language);
        let loc = Rc::new(
            fs.read(&loc_path)
                .ok_or_else(|| format!("{loc_path} not found"))
                .and_then(|b| Localization::from_bytes(&b).map_err(|e| e.to_string()))?,
        );
        let fontlib = fs.read("resource/flash/fontlib.swf").ok_or("resource/flash/fontlib.swf not found")?;
        tracing::info!("scaleform: {} localization tokens", loc.len());
        let mapping = fs.read_string("resource/flash/fontmapping.cfg").ok_or("fontmapping.cfg not found")?;
        let mut cache = HashMap::new();
        cache.insert(SHIM_NAME.to_string(), Rc::new(compile_shim()?));
        let gfxfontlib = rewrite::build_gfxfontlib(&fontlib, &mapping).map_err(|e| e.to_string())?;
        cache.insert("gfxfontlib.swf".to_string(), Rc::new(gfxfontlib));
        let engine = Rc::new(RefCell::new(engine));
        let files = Rc::new(Files { vfs: fs, engine: engine.clone(), loc: loc.clone(), cache: RefCell::new(cache), stats: Default::default() });

        let dispatcher = Rc::new(RefCell::new(Dispatcher {
            engine,
            loc,
            calls: Calls::default(),
            events,
            root_ready: false,
            slider_axis_pending: None,
        }));
        let executor = NullExecutor::new();
        let navigator = InstallNavigator {
            files,
            spawner: executor.spawner(),
            base: Url::parse(BASE_URL).map_err(|e| e.to_string())?,
        };

        let instance = create_wgpu_instance(wgpu::Backends::PRIMARY, wgpu::BackendOptions::default(), None);
        let (adapter, device, queue) = futures::executor::block_on(request_adapter_and_device(
            wgpu::Backends::PRIMARY,
            &instance,
            None,
            wgpu::PowerPreference::HighPerformance,
        ))
        .map_err(|e| format!("scaleform GPU device: {e}"))?;
        let descriptors = Arc::new(Descriptors::new(instance, adapter, device, queue));
        let (width, height) = (config.width.max(1), config.height.max(1));
        let target = TextureTarget::new(&descriptors.device, (width, height)).map_err(|e| e.to_string())?;
        let renderer = WgpuRenderBackend::new(descriptors, target).map_err(|e| e.to_string())?;

        let player = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_navigator(navigator)
            .with_external_interface(Box::new(Provider(dispatcher.clone())))
            .with_viewport_dimensions(width, height, 1.0)
            // The root movie sets Stage.scaleMode = "noScale", align "TL";
            // Lib.ResizeManager does the scaling itself.
            .with_scale_mode(StageScaleMode::NoScale, true)
            .with_align(StageAlign::TOP | StageAlign::LEFT, true)
            .with_autoplay(true)
            .with_max_execution_duration(Duration::from_secs(15))
            .build();
        // The selected player's authored background paints Home; Pause and the
        // dedicated HUD surface preserve the world beneath authored alpha.
        player.lock().unwrap().set_window_mode("transparent");
        player.lock().unwrap().fetch_root_movie(
            format!("{BASE_URL}{SHIM_NAME}"),
            vec![
                ("csrRootMovie".to_string(), ROOT_MOVIE.to_string()),
                ("csrRootLevel".to_string(), ROOT_LEVEL.to_string()),
            ],
            Box::new(|_| {}),
        );
        Ok(Self { player, dispatcher, executor, mouse: (0.0, 0.0), held_keys: HashMap::new(), held_buttons: HashSet::new(),
            pending_readback: None, viewport_generation: 0, overlay,
            work_trace_remaining: if std::env::var("CSR_UI_WORKER_TRACE").as_deref() == Ok("1") { 2048 } else { 0 },
            render_trace_remaining: if std::env::var("CSR_UI_RENDER_TRACE").as_deref() == Ok("1") { 1024 } else { 0 },
        })
    }

    pub fn root_ready(&self) -> bool {
        self.dispatcher.borrow().root_ready
    }

    pub fn inspect(&mut self, path: &str) {
        let value = self.call_as("csrInspect", vec![ExtValue::String(path.into())]);
        let properties = match value {
            ExtValue::List(values) => values.into_iter().filter_map(|v| match v { ExtValue::String(s) => Some(s), _ => None }).collect(),
            _ => vec![],
        };
        let _ = self.dispatcher.borrow().events.send(Event::Inspected { path: path.into(), properties });
    }

    /// Defines `_global.CScaleformComponent_*` for every component the movies use.
    pub fn define_components(&mut self) {
        let names = crate::defaults::COMPONENTS
            .iter()
            .map(|c| ExtValue::String(format!("CScaleformComponent_{c}")))
            .collect();
        self.call_as("csrDefineComponents", vec![ExtValue::List(names)]);
    }

    /// Runs pending loads without advancing the movies.
    pub fn pump_loads(&mut self) {
        self.executor.run();
    }

    fn work_started(&self) -> Option<Instant> {
        (self.work_trace_remaining > 0).then(Instant::now)
    }

    fn work_finished(&mut self, phase: &'static str, started: Option<Instant>) {
        let Some(started) = started else { return };
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_millis(5) && self.work_trace_remaining > 0 {
            self.work_trace_remaining -= 1;
            tracing::info!(target: "scaleform::worker", phase,
                elapsed_ms = elapsed.as_secs_f64() * 1000.0, "Slow UI work");
        }
    }

    pub fn step(&mut self, dt: Duration) {
        let started = self.work_started();
        self.executor.run();
        self.work_finished("load_executor", started);
        let started = self.work_started();
        self.player.lock().unwrap().tick(FloatDuration::from_secs(dt.as_secs_f64()));
        self.work_finished("movie_tick", started);
        let started = self.work_started();
        self.executor.run();
        self.work_finished("load_executor", started);
        let started = self.work_started();
        self.drain_calls();
        self.work_finished("movie_callbacks", started);
    }

    fn drain_calls(&mut self) {
        // Engine calls may queue more calls; bound the loop.
        for _ in 0..8 {
            let queue = std::mem::take(&mut self.dispatcher.borrow_mut().calls.queue);
            if queue.is_empty() {
                return;
            }
            for command in queue {
                self.run_command_now(command);
            }
        }
    }

    pub fn run_command(&mut self, command: Command) {
        let phase = match &command {
            Command::Call { function, .. } if function == "samplePauseSurface" => "pause_visibility",
            Command::Call { .. } => "movie_call",
            Command::Set { .. } => "movie_set",
            Command::RequestElement(_) => "movie_request",
        };
        let started = self.work_started();
        self.run_command_now(command);
        self.drain_calls();
        self.work_finished(phase, started);
    }

    fn run_command_now(&mut self, command: Command) {
        match command {
            Command::Call { path, function, args } => {
                let args = args.into_iter().map(to_ext).collect();
                let r = self.call_as(
                    "csrCall",
                    vec![ExtValue::String(path.clone()), ExtValue::String(function.clone()), ExtValue::List(args)],
                );
                if let ExtValue::String(s) = &r {
                    if s.starts_with("csr:missing") {
                        tracing::debug!("scaleform: {path}.{function}: {s}");
                    }
                }
            }
            Command::RequestElement(element) => {
                self.call_as("csrRequestElement", vec![ExtValue::String(element.clone()), ExtValue::String(element)]);
            }
            Command::Set { path, property, value } => {
                self.call_as("csrSet", vec![ExtValue::String(path), ExtValue::String(property), to_ext(value)]);
            }
        }
    }

    fn call_as(&mut self, callback: &str, args: Vec<ExtValue>) -> ExtValue {
        self.player.lock().unwrap().call_internal_interface(callback, args)
    }

    /// A registered pip's own callback arms this single-event correction only
    /// while its widget reports native dragging. Keep Flash's x bounds/offsets.
    fn preserve_slider_drag_axis(&mut self) {
        let Some(axis) = self.dispatcher.borrow_mut().slider_axis_pending.take() else { return };
        let axis = Twips::from_pixels(axis);
        if axis == Twips::ZERO { return; }
        self.player.lock().unwrap().mutate_with_update_context(|context| {
            let Some(drag) = context.drag_object.as_mut() else { return };
            if drag.constraint.y_min != Twips::ZERO || drag.constraint.y_max != Twips::ZERO { return; }
            drag.constraint.y_min = axis;
            drag.constraint.y_max = axis;
            Player::update_drag(context);
        });
    }

    pub fn lifecycle(&mut self, active: bool) -> Result<(), Error> {
        let mut calls = Calls::default();
        let accepted = self.dispatcher.borrow().engine.borrow_mut().lifecycle(active, &mut calls);
        self.dispatcher.borrow_mut().calls.queue.extend(calls.queue);
        self.drain_calls();
        if accepted { Ok(()) } else { Err("The menu session limit was reached. Restart the game.".into()) }
    }

    /// Focus loss alone does not release Ruffle's pressed-key bookkeeping.
    /// Deliver ordinary releases while the engine action domain is inactive.
    pub fn cancel_input(&mut self) {
        for (name, vkey) in std::mem::take(&mut self.held_keys) {
            self.input(Input::Key { name, vkey, down: false });
        }
        for button in std::mem::take(&mut self.held_buttons) {
            self.input(Input::MouseUp { x: self.mouse.0, y: self.mouse.1, button });
        }
        self.input(Input::MouseLeave);
        self.input(Input::Focus(false));
        self.player.lock().unwrap().mutate_with_update_context(|context| { *context.drag_object = None; });
    }

    pub fn input(&mut self, input: Input) {
        let button = |b: MouseButton| match b {
            MouseButton::Left => RuffleButton::Left,
            MouseButton::Right => RuffleButton::Right,
            MouseButton::Middle => RuffleButton::Middle,
        };
        let event = match input {
            Input::MouseMove { x, y } => {
                self.mouse = (x, y);
                PlayerEvent::MouseMove { x, y }
            }
            Input::MouseDown { x, y, button: b } => {
                self.mouse = (x, y);
                self.held_buttons.insert(b);
                if b == MouseButton::Right {
                    // The engine also reported the right button as a key
                    // (`MOUSE_RIGHT` opens context menus).
                    self.key("MOUSE_RIGHT", 0, true);
                }
                PlayerEvent::MouseDown { x, y, button: button(b), index: None }
            }
            Input::MouseUp { x, y, button: b } => {
                self.mouse = (x, y);
                self.held_buttons.remove(&b);
                if b == MouseButton::Right {
                    self.key("MOUSE_RIGHT", 0, false);
                }
                PlayerEvent::MouseUp { x, y, button: button(b) }
            }
            Input::MouseWheel { lines } => PlayerEvent::MouseWheel { delta: MouseWheelDelta::Lines(lines) },
            Input::MouseLeave => PlayerEvent::MouseLeave,
            Input::Key { name, vkey, down } => {
                if down { self.held_keys.insert(name.clone(), vkey); } else { self.held_keys.remove(&name); }
                let key = flash_key(&name, vkey);
                self.player.lock().unwrap().handle_event(if down { PlayerEvent::KeyDown { key } } else { PlayerEvent::KeyUp { key } });
                self.key(&name, vkey, down);
                return;
            }
            Input::Char(c) => {
                self.player.lock().unwrap().handle_event(PlayerEvent::TextInput { codepoint: c });
                self.character(c);
                return;
            },
            Input::Edit { name, control, shift } => {
                let Some(code) = text_control(&name, control, shift) else { return; };
                self.player.lock().unwrap().handle_event(PlayerEvent::TextControl { code });
                if let Some(c) = authored_edit_character(&name, control, shift) {
                    self.character(c);
                } else {
                    self.drain_calls();
                }
                return;
            }
            Input::Focus(focused) => if focused { PlayerEvent::FocusGained } else { PlayerEvent::FocusLost },
            _ => return,
        };
        let phase = match &event {
            PlayerEvent::MouseMove { .. } => "mouse_move",
            PlayerEvent::MouseDown { .. } => "mouse_down",
            PlayerEvent::MouseUp { .. } => "mouse_up",
            _ => "movie_event",
        };
        let started = self.work_started();
        self.player.lock().unwrap().handle_event(event);
        self.preserve_slider_drag_axis();
        self.work_finished(phase, started);
        let started = self.work_started();
        self.drain_calls();
        self.work_finished("input_callbacks", started);
    }

    /// Keys went to the movies through `_global.KeyDownEvent`/`KeyUpEvent`
    /// with Source key names (`KEY_ESCAPE`, `MOUSE_RIGHT`...).
    fn key(&mut self, name: &str, vkey: u32, down: bool) {
        self.call_as(
            "csrKey",
            vec![ExtValue::String(name.to_string()), ExtValue::Number(vkey as f64), ExtValue::Bool(down)],
        );
        self.drain_calls();
    }

    fn character(&mut self, typed: char) {
        self.call_as("csrChar", character_args(typed));
        self.drain_calls();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport_generation = self.viewport_generation.checked_add(1)
            .expect("menu viewport generation limit");
        let mut player = self.player.lock().unwrap();
        player.set_viewport_dimensions(ViewportDimensions { width: width.max(1), height: height.max(1), scale_factor: 1.0 });
    }

    pub fn needs_render(&self) -> bool {
        self.player.lock().unwrap().needs_render()
    }

    pub fn readback_pending(&self) -> bool {
        self.pending_readback.is_some()
    }

    /// TextureTarget writes the same transfer buffer on every render. Never
    /// render again until its pending mapping has been completed and unmapped.
    pub fn begin_render(&mut self, presentation_revision: u64) -> Result<(), Error> {
        if self.pending_readback.is_some() {
            return Err("menu render attempted during pending readback".into());
        }
        let started = (self.render_trace_remaining != 0).then(Instant::now);
        let (buffer, dimensions, device, width, height) = {
            let mut player = self.player.lock().unwrap();
            player.render();
            let renderer = <dyn Any>::downcast_mut::<WgpuRenderBackend<TextureTarget>>(player.renderer_mut())
                .ok_or("unexpected renderer")?;
            let target = renderer.target();
            let transfer = target.buffer.as_ref().ok_or("no frame captured")?;
            let (buffer, dimensions) = transfer.buffer.inner();
            (buffer.clone(), dimensions.clone(), renderer.device().clone(), target.size.width, target.size.height)
        };
        let (completion, receiver) = mpsc::sync_channel(1);
        self.pending_readback = Some(PendingReadback {
            buffer, device, dimensions, width, height, presentation_revision,
            viewport_generation: self.viewport_generation, completion: receiver,
            started, submitted: Instant::now(),
        });
        if started.is_some() {
            self.render_trace_remaining -= 1;
        }
        // Install the cleanup guard before requesting the map. The callback
        // only posts one result and tolerates receiver teardown; it never
        // touches the player, pixels, frontend or Pause readiness.
        self.pending_readback.as_ref().unwrap().buffer.slice(..)
            .map_async(wgpu::MapMode::Read, move |result| { let _ = completion.try_send(result); });
        Ok(())
    }

    /// Poll once without waiting for the GPU. This also runs while the movie is
    /// inactive so completed/obsolete mappings do not retain the transfer buffer.
    pub fn poll_render(&mut self) -> Result<Option<CapturedFrame>, Error> {
        let Some(pending) = self.pending_readback.as_ref() else { return Ok(None); };
        let poll_started = pending.started.map(|_| Instant::now());
        if let Err(error) = pending.device.poll(wgpu::PollType::Poll) {
            self.pending_readback.take();
            return Err(format!("menu readback poll failed: {error}"));
        }
        match pending.completion.try_recv() {
            Err(TryRecvError::Empty) => return Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.pending_readback.take();
                return Err("menu readback callback disconnected".into());
            }
            Ok(Err(error)) => {
                self.pending_readback.take();
                return Err(format!("menu readback mapping failed: {error}"));
            }
            Ok(Ok(())) => (),
        }
        let pending = self.pending_readback.take().unwrap();
        if pending.viewport_generation != self.viewport_generation {
            return Ok(None); // Guard unmaps the obsolete target, without copying it.
        }
        let mapped_at = pending.started.map(|_| Instant::now());
        let height = if self.overlay { pending.height.div_ceil(2) } else { pending.height };
        let mut rgba = {
            let mapped = pending.buffer.slice(..).get_mapped_range()
                .map_err(|error| format!("menu readback range failed: {error}"))?;
            crate::capture::copy_rows(&mapped, pending.width, height,
                pending.dimensions.padded_bytes_per_row as usize)
                .map_err(str::to_owned)?
        }; // Drop the mapped view before PendingReadback unmaps, including errors.
        let copied_at = pending.started.map(|_| Instant::now());
        crate::capture::straight_alpha(&mut rgba);
        if let Some(started) = pending.started {
            tracing::info!(target: "scaleform::render_timing", width = pending.width, height = pending.height,
                submit_ms = pending.submitted.duration_since(started).as_secs_f64() * 1000.,
                completion_age_ms = pending.submitted.elapsed().as_secs_f64() * 1000.,
                poll_ms = poll_started.zip(mapped_at).map_or(0., |(begin, end)| end.duration_since(begin).as_secs_f64() * 1000.),
                copy_ms = mapped_at.zip(copied_at).map_or(0., |(begin, end)| end.duration_since(begin).as_secs_f64() * 1000.),
                alpha_ms = copied_at.map_or(0., |begin| begin.elapsed().as_secs_f64() * 1000.),
                "UI readback completed (age includes intervening input and movie work)");
        }
        Ok(Some(CapturedFrame { width: pending.width, height, rgba,
            presentation_revision: pending.presentation_revision }))
    }
}

fn character_args(typed: char) -> Vec<ExtValue> {
    // CharTyped accepts the character string, not its integer codepoint.
    vec![ExtValue::String(typed.to_string())]
}

fn authored_edit_character(name: &str, control: bool, shift: bool) -> Option<char> {
    match (name, control, shift) {
        ("KEY_ENTER", false, false) => Some('\r'),
        ("KEY_BACKSPACE", false, _) => Some('\x08'),
        _ => None,
    }
}

fn text_control(name: &str, control: bool, shift: bool) -> Option<TextControlCode> {
    use TextControlCode::*;
    Some(match (name, control, shift) {
        ("KEY_A", true, _) => SelectAll,
        ("KEY_BACKSPACE", true, _) => BackspaceWord,
        ("KEY_BACKSPACE", false, _) => Backspace,
        ("KEY_DELETE", true, _) => DeleteWord,
        ("KEY_DELETE", false, _) => Delete,
        ("KEY_ENTER", _, _) => Enter,
        ("KEY_LEFT", false, false) => MoveLeft,
        ("KEY_LEFT", true, false) => MoveLeftWord,
        ("KEY_LEFT", false, true) => SelectLeft,
        ("KEY_LEFT", true, true) => SelectLeftWord,
        ("KEY_RIGHT", false, false) => MoveRight,
        ("KEY_RIGHT", true, false) => MoveRightWord,
        ("KEY_RIGHT", false, true) => SelectRight,
        ("KEY_RIGHT", true, true) => SelectRightWord,
        ("KEY_HOME", false, false) => MoveLeftLine,
        ("KEY_HOME", true, false) => MoveLeftDocument,
        ("KEY_HOME", false, true) => SelectLeftLine,
        ("KEY_HOME", true, true) => SelectLeftDocument,
        ("KEY_END", false, false) => MoveRightLine,
        ("KEY_END", true, false) => MoveRightDocument,
        ("KEY_END", false, true) => SelectRightLine,
        ("KEY_END", true, true) => SelectRightDocument,
        _ => return None,
    })
}

// The Scaleform callbacks are separate from Flash's Key state/listeners.
// Send both, so editable fields and Key.isDown see the same transitions.
fn flash_key(name: &str, vkey: u32) -> KeyDescriptor {
    use NamedKey::*;
    let named = match vkey {
        8 => Some(Backspace), 9 => Some(Tab), 13 => Some(Enter),
        16 | 160 | 161 => Some(Shift), 17 | 162 | 163 => Some(Control),
        18 | 164 | 165 => Some(Alt), 27 => Some(Escape),
        33 => Some(PageUp), 34 => Some(PageDown), 35 => Some(End), 36 => Some(Home),
        37 => Some(ArrowLeft), 38 => Some(ArrowUp), 39 => Some(ArrowRight), 40 => Some(ArrowDown),
        45 => Some(Insert), 46 => Some(Delete),
        _ => None,
    };
    let logical_key = named.map(LogicalKey::Named).unwrap_or_else(|| match vkey {
        32 | 48..=57 => LogicalKey::Character(char::from_u32(vkey).unwrap()),
        65..=90 => LogicalKey::Character(char::from_u32(vkey + 32).unwrap()),
        96..=105 => LogicalKey::Character(char::from_u32(vkey - 96 + 48).unwrap()),
        _ => LogicalKey::Unknown,
    });
    let key_location = if name.starts_with("KEY_PAD_") { KeyLocation::Numpad }
        else if matches!(name, "KEY_LSHIFT" | "KEY_LCONTROL" | "KEY_LALT") { KeyLocation::Left }
        else if matches!(name, "KEY_RSHIFT" | "KEY_RCONTROL" | "KEY_RALT") { KeyLocation::Right }
        else { KeyLocation::Standard };
    KeyDescriptor { physical_key: PhysicalKey::Unknown, logical_key, key_location }
}

/// Compiles the bridge shim (our own ActionScript 2) with rascal.
fn compile_shim() -> Result<Vec<u8>, Error> {
    struct Source;
    impl rascal::SourceProvider for Source {
        fn load(&self, path: &str) -> Result<String, std::io::Error> {
            match path {
                "shim.as" => Ok(include_str!("shim.as").to_string()),
                "death_notice.as" => Ok(include_str!("death_notice.as").to_string()),
                _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, path.to_string())),
            }
        }
        fn is_file(&self, path: &str) -> bool {
            matches!(path, "shim.as" | "death_notice.as")
        }
    }
    // SWF 10: Ruffle turns empty strings crossing ExternalInterface into
    // "null" for SWF 8 and older, as Flash Player did.
    let options = rascal::CompileOptions::default().with_swf_version(10);
    let mut builder = rascal::ProgramBuilder::new(Source).with_compile_options(options);
    builder.add_script("shim.as");
    builder.add_script("death_notice.as");
    let program = builder.build().map_err(|e| format!("compiling the bridge shim: {e:?}"))?;
    let swf_options = rascal::SwfOptions::default().with_frame_rate(30.0).with_stage_size(0.0, 0.0, 1280.0, 720.0);
    program.compile().to_swf(&swf_options).map_err(|e| format!("writing the bridge shim: {e}"))
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires a GPU and installed CS:GO Legacy; does not launch the native game"]
    fn readback_preserves_revision_discards_resize_and_unmaps_on_drop() {
        use super::*;

        struct Offline;
        impl Engine for Offline {
            fn call(&mut self, _: &str, _: &str, _: &[Value], _: &mut Calls) -> Option<Value> { None }
        }
        fn finish(host: &mut Host) -> Option<CapturedFrame> {
            let deadline = Instant::now() + Duration::from_secs(10);
            while host.readback_pending() {
                if let Some(frame) = host.poll_render().unwrap() { return Some(frame); }
                assert!(Instant::now() < deadline, "GPU readback did not complete");
                std::thread::sleep(Duration::from_millis(1));
            }
            None
        }

        let config = Config {
            game_dir: vfs::find_csgo_dir().expect("installed CS:GO Legacy required"),
            width: 64, height: 48, language: "english".into(), elements: vec![],
        };
        let (events, _receiver) = mpsc::channel();
        let mut host = Host::new(&config, Box::new(Offline), events).unwrap();
        host.begin_render(11).unwrap();
        assert!(host.begin_render(12).is_err(), "must not reuse a pending target");
        let frame = finish(&mut host).expect("original capture should complete");
        assert_eq!((frame.width, frame.height, frame.presentation_revision), (64, 48, 11));

        host.begin_render(12).unwrap();
        host.resize(80, 60);
        assert!(finish(&mut host).is_none(), "old viewport must not be published");
        host.begin_render(13).unwrap();
        let frame = finish(&mut host).expect("resized capture should complete");
        assert_eq!((frame.width, frame.height, frame.presentation_revision), (80, 60, 13));
        assert_eq!(frame.rgba.len(), 80 * 60 * 4);

        host.begin_render(14).unwrap();
        let pending = host.pending_readback.as_ref().unwrap();
        let (buffer, device) = (pending.buffer.clone(), pending.device.clone());
        drop(host);
        // A new map on the retained buffer proves teardown canceled/unmapped
        // the old request, even if its callback had not run before shutdown.
        let (completion, receiver) = mpsc::sync_channel(1);
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = completion.try_send(result);
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            device.poll(wgpu::PollType::Poll).unwrap();
            match receiver.try_recv() {
                Ok(result) => { result.unwrap(); break; }
                Err(TryRecvError::Empty) => (),
                Err(error) => panic!("remap callback disconnected: {error}"),
            }
            assert!(Instant::now() < deadline, "retained buffer remap did not complete");
            std::thread::sleep(Duration::from_millis(1));
        }
        buffer.unmap();
    }

    #[test]
    fn fetched_loader_identity_is_absolute_and_content_stays_local() {
        use super::*;
        use futures::executor::block_on;

        struct Images;
        impl Engine for Images {
            fn call(&mut self, _: &str, _: &str, _: &[Value], _: &mut Calls) -> Option<Value> { None }
            fn image(&mut self, url: &str) -> Option<Vec<u8>> {
                (url == "img://owned/tile").then(|| vec![4, 5, 6])
            }
        }
        let mut fs = vfs::Vfs::new();
        fs.push_back("synthetic", vfs::Mount::Memory(std::collections::BTreeMap::from([
            ("resource/flash/icons/tile.png".into(), Arc::<[u8]>::from(vec![1, 2, 3])),
        ])));
        let files = Rc::new(Files {
            vfs: fs, engine: Rc::new(RefCell::new(Box::new(Images))),
            loc: Rc::new(Localization::from_bytes(&[0xff, 0xfe]).unwrap()),
            cache: Default::default(), stats: Default::default(),
        });
        let executor = NullExecutor::new();
        let navigator = InstallNavigator { files, spawner: executor.spawner(), base: Url::parse(BASE_URL).unwrap() };
        for (request, expected, bytes) in [
            ("icons/tile.png?label=One%20Two#selected", "file:///csgo/resource/flash/icons/tile.png?label=One%20Two#selected", vec![1, 2, 3]),
            ("file:///csgo/resource/flash/icons/tile.png", "file:///csgo/resource/flash/icons/tile.png", vec![1, 2, 3]),
            ("img://owned/tile?variant=One#selected", "img://owned/tile?variant=One#selected", vec![4, 5, 6]),
        ] {
            let response = block_on(navigator.fetch(Request::get(request.into())))
                .unwrap_or_else(|error| panic!("fetch failed: {}", error.error));
            assert_eq!(response.url(), expected);
            assert!(Url::parse(&response.url()).is_ok());
            assert_eq!(block_on(response.body()).unwrap(), bytes);
        }
        for rejected in ["missing.png", "../icons/tile.png", "http://[", "https://example.test/csgo/resource/flash/icons/tile.png", "img://unowned/tile"] {
            assert!(block_on(navigator.fetch(Request::get(rejected.into()))).is_err(), "{rejected}");
        }
    }

    #[test]
    fn authored_character_contract_uses_strings_and_preserves_unicode() {
        for typed in ['3', '\u{e9}', '\u{1f600}', '\r'] {
            assert!(matches!(&super::character_args(typed)[0], ruffle_core::external::Value::String(text) if text == &typed.to_string()));
        }
        assert_eq!(super::authored_edit_character("KEY_ENTER", false, false), Some('\r'));
        assert_eq!(super::authored_edit_character("KEY_ENTER", true, false), None);
        assert_eq!(super::authored_edit_character("KEY_BACKSPACE", false, false), Some('\x08'));
        assert_eq!(super::authored_edit_character("KEY_BACKSPACE", true, false), None);
        assert_eq!(super::authored_edit_character("KEY_DELETE", false, false), None);
    }

    #[test]
    fn editing_distinguishes_caret_selection_and_word_deletion() {
        use ruffle_core::events::TextControlCode::*;
        assert_eq!(super::text_control("KEY_LEFT", true, true), Some(SelectLeftWord));
        assert_eq!(super::text_control("KEY_BACKSPACE", true, false), Some(BackspaceWord));
        assert_eq!(super::text_control("KEY_HOME", false, false), Some(MoveLeftLine));
        assert_eq!(super::text_control("KEY_A", false, false), None);
    }

    #[test]
    fn flash_keys_preserve_navigation_and_key_location() {
        use ruffle_core::events::{LogicalKey, NamedKey, KeyLocation};
        assert_eq!(super::flash_key("KEY_ESCAPE", 27).logical_key, LogicalKey::Named(NamedKey::Escape));
        assert_eq!(super::flash_key("KEY_A", 65).logical_key, LogicalKey::Character('a'));
        assert_eq!(super::flash_key("KEY_PAD_7", 103).logical_key, LogicalKey::Character('7'));
        assert_eq!(super::flash_key("KEY_PAD_ENTER", 13).key_location, KeyLocation::Numpad);
        assert_eq!(super::flash_key("KEY_RCONTROL", 17).key_location, KeyLocation::Right);
    }

    #[test]
    fn bridge_shim_compiles_with_the_key_table_and_callbacks() {
        let swf = super::compile_shim().unwrap();
        assert!(swf.starts_with(b"FWS") || swf.starts_with(b"CWS"));
        assert_eq!(swf[3], 10);
    }
}
