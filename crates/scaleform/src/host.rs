//! Ruffle with CS:GO's movies: an in-memory navigator serving the install's
//! `resource/flash` folder (plus our bridge shim and the virtual font
//! library), an ExternalInterface provider wired to the [`Engine`], and an
//! offscreen wgpu target read back for the game.

use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::Sender as EventSender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_channel::{Receiver, Sender};
use ruffle_core::backend::navigator::{
    ErrorResponse, NavigationMethod, NavigatorBackend, NullExecutor, NullSpawner, OwnedFuture,
    Request, SuccessResponse, create_fetch_error,
};
use ruffle_core::events::{MouseButton as RuffleButton, MouseWheelDelta, PlayerEvent};
use ruffle_core::external::{ExternalInterfaceProvider, Value as ExtValue};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error as LoadError;
use ruffle_core::socket::{ConnectionState, SocketAction, SocketHandle};
use ruffle_core::{FloatDuration, Player, PlayerBuilder, StageAlign, StageScaleMode, ViewportDimensions};
use ruffle_render_wgpu::backend::{WgpuRenderBackend, create_wgpu_instance, request_adapter_and_device};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::TextureTarget;
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
        // Steam avatars came from the engine's image cache; until the game
        // supplies pictures, every avatar is a neutral tile.
        if rel.starts_with("img://avatar_") {
            return Some(Rc::new(solid_png(64, 64, [58, 64, 72])));
        }
        let Some(raw) = self.vfs.read(&format!("resource/flash/{rel}")) else {
            tracing::debug!("scaleform: no file {rel}");
            return None;
        };
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
        let url = request.url().to_string();
        let result = match self.files.get(&url) {
            Some(data) => Ok(Box::new(MemResponse { url, data: Some(data) }) as Box<dyn SuccessResponse>),
            None => Err(create_fetch_error(&url, "not in the install")),
        };
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
    engine: Box<dyn Engine>,
    loc: Rc<Localization>,
    calls: Calls,
    events: EventSender<Event>,
    root_ready: bool,
}

impl Dispatcher {
    fn call(&mut self, name: &str, args: &[Value]) -> Value {
        let (object, method) = name.split_once('.').unwrap_or(("", name));
        match (object, method) {
            ("csr", "RootReady") => {
                self.root_ready = true;
                return Value::Undefined;
            }
            (_, "OnLoadFinished") if !object.starts_with("CScaleformComponent_") => {
                let _ = self.events.send(Event::Loaded(object.to_string()));
            }
            _ => {}
        }
        if let Some(v) = self.engine.call(object, method, args, &mut self.calls) {
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

pub(crate) struct Host {
    player: Arc<Mutex<Player>>,
    dispatcher: Rc<RefCell<Dispatcher>>,
    executor: NullExecutor,
    mouse: (f64, f64),
}

impl Host {
    pub fn new(config: &Config, engine: Box<dyn Engine>, events: EventSender<Event>) -> Result<Self, Error> {
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&config.game_dir).map_err(|e| format!("mounting {}: {e}", config.game_dir.display()))?;
        let loc_path = format!("resource/csgo_{}.txt", config.language);
        let loc = Rc::new(
            fs.read(&loc_path)
                .ok_or_else(|| format!("{loc_path} not found"))
                .and_then(|b| Localization::from_bytes(&b).map_err(|e| e.to_string()))?,
        );
        let fontlib = fs.read("resource/flash/fontlib.swf").ok_or("resource/flash/fontlib.swf not found")?;
        let mapping = fs.read_string("resource/flash/fontmapping.cfg").ok_or("fontmapping.cfg not found")?;
        let mut cache = HashMap::new();
        cache.insert(SHIM_NAME.to_string(), Rc::new(compile_shim()?));
        let gfxfontlib = rewrite::build_gfxfontlib(&fontlib, &mapping).map_err(|e| e.to_string())?;
        cache.insert("gfxfontlib.swf".to_string(), Rc::new(gfxfontlib));
        let files = Rc::new(Files { vfs: fs, loc: loc.clone(), cache: RefCell::new(cache), stats: Default::default() });

        let dispatcher = Rc::new(RefCell::new(Dispatcher {
            engine,
            loc,
            calls: Calls::default(),
            events,
            root_ready: false,
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
        player.lock().unwrap().fetch_root_movie(
            format!("{BASE_URL}{SHIM_NAME}"),
            vec![
                ("csrRootMovie".to_string(), ROOT_MOVIE.to_string()),
                ("csrRootLevel".to_string(), ROOT_LEVEL.to_string()),
            ],
            Box::new(|_| {}),
        );
        Ok(Self { player, dispatcher, executor, mouse: (0.0, 0.0) })
    }

    pub fn root_ready(&self) -> bool {
        self.dispatcher.borrow().root_ready
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

    pub fn step(&mut self, dt: Duration) {
        self.executor.run();
        self.player.lock().unwrap().tick(FloatDuration::from_secs(dt.as_secs_f64()));
        self.executor.run();
        self.drain_calls();
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
        self.run_command_now(command);
        self.drain_calls();
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
        }
    }

    fn call_as(&mut self, callback: &str, args: Vec<ExtValue>) -> ExtValue {
        self.player.lock().unwrap().call_internal_interface(callback, args)
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
                if b == MouseButton::Right {
                    // The engine also reported the right button as a key
                    // (`MOUSE_RIGHT` opens context menus).
                    self.key("MOUSE_RIGHT", 0, true);
                }
                PlayerEvent::MouseDown { x, y, button: button(b), index: None }
            }
            Input::MouseUp { x, y, button: b } => {
                if b == MouseButton::Right {
                    self.key("MOUSE_RIGHT", 0, false);
                }
                PlayerEvent::MouseUp { x, y, button: button(b) }
            }
            Input::MouseWheel { lines } => PlayerEvent::MouseWheel { delta: MouseWheelDelta::Lines(lines) },
            Input::MouseLeave => PlayerEvent::MouseLeave,
            Input::Key { name, vkey, down } => {
                self.key(&name, vkey, down);
                return;
            }
            Input::Char(c) => PlayerEvent::TextInput { codepoint: c },
            _ => return,
        };
        self.player.lock().unwrap().handle_event(event);
        self.drain_calls();
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

    pub fn resize(&mut self, width: u32, height: u32) {
        let mut player = self.player.lock().unwrap();
        player.set_viewport_dimensions(ViewportDimensions { width: width.max(1), height: height.max(1), scale_factor: 1.0 });
    }

    pub fn needs_render(&self) -> bool {
        self.player.lock().unwrap().needs_render()
    }

    pub fn render(&mut self) -> Result<(u32, u32, Vec<u8>), Error> {
        let mut player = self.player.lock().unwrap();
        player.render();
        let renderer = <dyn Any>::downcast_mut::<WgpuRenderBackend<TextureTarget>>(player.renderer_mut())
            .ok_or("unexpected renderer")?;
        let image = renderer.capture_frame().ok_or("no frame captured")?;
        Ok((image.width(), image.height(), image.into_raw()))
    }
}

/// Compiles the bridge shim (our own ActionScript 2) with rascal.
fn compile_shim() -> Result<Vec<u8>, Error> {
    struct Source;
    impl rascal::SourceProvider for Source {
        fn load(&self, path: &str) -> Result<String, std::io::Error> {
            match path {
                "shim.as" => Ok(include_str!("shim.as").to_string()),
                _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, path.to_string())),
            }
        }
        fn is_file(&self, path: &str) -> bool {
            path == "shim.as"
        }
    }
    // SWF 10: Ruffle turns empty strings crossing ExternalInterface into
    // "null" for SWF 8 and older, as Flash Player did.
    let options = rascal::CompileOptions::default().with_swf_version(10);
    let mut builder = rascal::ProgramBuilder::new(Source).with_compile_options(options);
    builder.add_script("shim.as");
    let program = builder.build().map_err(|e| format!("compiling the bridge shim: {e:?}"))?;
    let swf_options = rascal::SwfOptions::default().with_frame_rate(30.0).with_stage_size(0.0, 0.0, 1280.0, 720.0);
    program.compile().to_swf(&swf_options).map_err(|e| format!("writing the bridge shim: {e}"))
}
