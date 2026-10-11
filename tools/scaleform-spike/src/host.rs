//! Runs Ruffle headless with CS:GO's movies: an in-memory navigator serving
//! the install's `resource/flash` folder (plus our bridge shim and the virtual
//! font library), an ExternalInterface provider wired to [`Bridge`], and an
//! offscreen wgpu target we can read back.

use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_channel::{Receiver, Sender};
use ruffle_core::backend::navigator::{
    ErrorResponse, NavigationMethod, NavigatorBackend, NullExecutor, NullSpawner, OwnedFuture,
    Request, SuccessResponse, create_fetch_error,
};
use ruffle_core::external::{ExternalInterfaceProvider, Value};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error as LoadError;
use ruffle_core::socket::{ConnectionState, SocketAction, SocketHandle};
use ruffle_core::{FloatDuration, Player, PlayerBuilder, StageAlign, StageScaleMode};
use ruffle_render_wgpu::backend::{WgpuRenderBackend, create_wgpu_instance, request_adapter_and_device};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::TextureTarget;
use ruffle_render_wgpu::wgpu;
use url::Url;

use crate::bridge::Bridge;
use crate::rewrite::{self, Localization, RewriteStats};

const BASE_URL: &str = "file:///csgo/resource/flash/";
const SHIM_NAME: &str = "csr_bridge.swf";

/// Files the movies can load: `resource/flash/...` from the game's file system
/// (loose files, then `pak01_dir.vpk`), plus our generated movies. Keyed by
/// lower-case path relative to `resource/flash/`.
pub struct Files {
    vfs: vfs::Vfs,
    loc: Rc<Localization>,
    cache: RefCell<HashMap<String, Rc<Vec<u8>>>>,
    pub stats: RefCell<RewriteStats>,
    pub fetched: RefCell<Vec<String>>,
    pub missing: RefCell<Vec<String>>,
}

impl Files {
    pub fn new(game_dir: &std::path::Path, loc: Rc<Localization>, shim: Vec<u8>) -> Result<Self> {
        let mut vfs = vfs::Vfs::new();
        vfs.mount_game_dir(game_dir).context("mounting the game folder")?;
        let fontlib = vfs.read("resource/flash/fontlib.swf").context("fontlib.swf")?;
        let mapping = vfs.read_string("resource/flash/fontmapping.cfg").context("fontmapping.cfg")?;
        let gfxfontlib = rewrite::build_gfxfontlib(&fontlib, &mapping)?;
        let mut cache = HashMap::new();
        cache.insert(SHIM_NAME.to_string(), Rc::new(shim));
        cache.insert("gfxfontlib.swf".to_string(), Rc::new(gfxfontlib));
        Ok(Self {
            vfs,
            loc,
            cache: RefCell::new(cache),
            stats: RefCell::default(),
            fetched: RefCell::default(),
            missing: RefCell::default(),
        })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.vfs.read(path)
    }

    fn get(&self, url: &str) -> Option<Rc<Vec<u8>>> {
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let lower = path.to_ascii_lowercase();
        const PREFIX: &str = "/csgo/resource/flash/";
        let rel = match lower.find(PREFIX) {
            Some(i) => lower[i + PREFIX.len()..].to_string(),
            None => lower.clone(),
        };
        tracing::debug!("fetch {rel}");
        self.fetched.borrow_mut().push(rel.clone());
        if let Some(data) = self.cache.borrow().get(&rel) {
            return Some(data.clone());
        }
        let Some(raw) = self.vfs.read(&format!("resource/flash/{rel}")) else {
            self.missing.borrow_mut().push(rel);
            return None;
        };
        let data = if rel.ends_with(".swf") {
            match rewrite::translate_movie(&raw, &self.loc, &mut self.stats.borrow_mut()) {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!("translating {rel}: {e}");
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

    /// Serves an engine-provided image (`img://...`) under its URL.
    pub fn insert(&self, rel: &str, data: Vec<u8>) {
        self.cache.borrow_mut().insert(rel.to_ascii_lowercase(), Rc::new(data));
    }
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
        tracing::info!("navigate_to_url {url} ({target})");
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

struct BridgeProvider(Rc<RefCell<Bridge>>);

impl ExternalInterfaceProvider for BridgeProvider {
    fn call_method(&self, _context: &mut ruffle_core::context::UpdateContext<'_>, name: &str, args: &[Value]) -> Value {
        self.0.borrow_mut().call(name, args)
    }
    fn on_callback_available(&self, _name: &str) {}
    fn get_id(&self) -> Option<String> {
        None
    }
}

pub struct Host {
    pub player: Arc<Mutex<Player>>,
    pub bridge: Rc<RefCell<Bridge>>,
    pub files: Rc<Files>,
    executor: NullExecutor,
    frame_ms: f64,
}

impl Host {
    pub fn new(game_dir: &std::path::Path, loc: Rc<Localization>, width: u32, height: u32) -> Result<Self> {
        let shim = compile_shim()?;
        let files = Rc::new(Files::new(game_dir, loc.clone(), shim)?);
        let bridge = Rc::new(RefCell::new(Bridge::new(loc)));
        let executor = NullExecutor::new();
        let navigator = InstallNavigator {
            files: files.clone(),
            spawner: executor.spawner(),
            base: Url::parse(BASE_URL)?,
        };

        let instance = create_wgpu_instance(wgpu::Backends::PRIMARY, wgpu::BackendOptions::default(), None);
        let (adapter, device, queue) = futures::executor::block_on(request_adapter_and_device(
            wgpu::Backends::PRIMARY,
            &instance,
            None,
            wgpu::PowerPreference::HighPerformance,
        ))
        .map_err(|e| anyhow!(e.to_string()))?;
        let descriptors = Arc::new(Descriptors::new(instance, adapter, device, queue));
        let target = TextureTarget::new(&descriptors.device, (width, height)).map_err(|e| anyhow!(e.to_string()))?;
        let renderer = WgpuRenderBackend::new(descriptors, target).map_err(|e| anyhow!(e.to_string()))?;

        let player = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_navigator(navigator)
            .with_external_interface(Box::new(BridgeProvider(bridge.clone())))
            .with_viewport_dimensions(width, height, 1.0)
            // The root movie sets Stage.scaleMode = "noScale", align "TL".
            .with_scale_mode(StageScaleMode::NoScale, true)
            .with_align(StageAlign::TOP | StageAlign::LEFT, true)
            .with_autoplay(true)
            .with_max_execution_duration(Duration::from_secs(30))
            .build();
        player.lock().unwrap().fetch_root_movie(
            format!("{BASE_URL}{SHIM_NAME}"),
            vec![
                ("csrRootMovie".to_string(), "mainuirootmovie.swf".to_string()),
                ("csrRootLevel".to_string(), "9000".to_string()),
            ],
            Box::new(|_| {}),
        );
        Ok(Self { player, bridge, files, executor, frame_ms: 1000.0 / 30.0 })
    }

    /// Advances the player by one host frame and runs pending loads.
    pub fn step(&mut self) {
        self.executor.run();
        self.player.lock().unwrap().tick(FloatDuration::from_millis(self.frame_ms));
        self.executor.run();
    }

    pub fn call_as(&mut self, callback: &str, args: Vec<Value>) -> Value {
        self.player.lock().unwrap().call_internal_interface(callback, args)
    }

    pub fn capture(&mut self) -> Result<image::RgbaImage> {
        let mut player = self.player.lock().unwrap();
        player.render();
        let renderer = <dyn Any>::downcast_mut::<WgpuRenderBackend<TextureTarget>>(player.renderer_mut())
            .context("renderer type")?;
        renderer.capture_frame().context("no frame captured")
    }
}

/// Compiles the bridge shim (our own ActionScript 2) with rascal.
fn compile_shim() -> Result<Vec<u8>> {
    struct Source;
    impl rascal::SourceProvider for Source {
        fn load(&self, path: &str) -> std::result::Result<String, std::io::Error> {
            match path {
                "shim.as" => Ok(include_str!("shim.as").to_string()),
                _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, path.to_string())),
            }
        }
        fn is_file(&self, path: &str) -> bool {
            path == "shim.as"
        }
    }
    let options = rascal::CompileOptions::default().with_swf_version(8);
    let mut builder = rascal::ProgramBuilder::new(Source).with_compile_options(options);
    builder.add_script("shim.as");
    let program = builder.build().map_err(|e| anyhow!("compiling shim: {e:?}"))?;
    let swf_options = rascal::SwfOptions::default().with_frame_rate(30.0).with_stage_size(0.0, 0.0, 1280.0, 720.0);
    program
        .compile()
        .to_swf(&swf_options)
        .map_err(|e| anyhow!("writing shim: {e}"))
}
