//! Private, bounded selected CPU wall scopes. No engine replacement.
//! Compiled only by `targeted-render-trace`; see docs/TARGETED-RENDER-TRACE.md.
use bevy::{diagnostic::FrameCount, prelude::*, render::{Extract, ExtractSchedule, RenderApp}};
use std::{
    fmt::Write as _, fs::{File, OpenOptions}, io::Write as _, path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, thread::ThreadId, time::Instant,
};
use tracing::{Metadata, field::{Field, Visit}, span::{Attributes, Id}};
use tracing_subscriber::{Layer, Registry, filter::filter_fn, layer::Context};

const SYSTEMS: [&str; 5] = [
    "bevy_pbr::render::skin::extract_skins",
    "bevy_pbr::render::skin::prepare_skins",
    "bevy_render::erased_render_asset::prepare_erased_assets<bevy_pbr::mesh_material::MeshMaterial3d<client::material::WorldMaterial>>",
    "bevy_pbr::render::mesh::prepare_mesh_bind_groups",
    "bevy_render::renderer::render_system",
];
struct Scope { kind: &'static str, name: &'static str, target: &'static str }
const PHASE_SCOPES: [Scope; 5] = [
    Scope { kind: "system", name: "bevy_render::view::window::prepare_windows", target: "bevy_ecs::system::function_system" },
    Scope { kind: "system", name: "bevy_pbr::material::queue_material_meshes", target: "bevy_ecs::system::function_system" },
    Scope { kind: "system", name: "bevy_render::view::prepare_view_targets", target: "bevy_ecs::system::function_system" },
    Scope { kind: "span", name: "queue_submit", target: "bevy_core_pipeline::schedule" },
    Scope { kind: "span", name: "present_frames", target: "bevy_render::renderer" },
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Profile { Legacy, RenderPhases }
impl Profile {
    fn parse(value: Option<&std::ffi::OsStr>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Legacy),
            Some(value) if value == "render-phases" => Ok(Self::RenderPhases),
            _ => Err("CSR_RENDER_TRACE_SCOPES must be absent or exactly render-phases".into()),
        }
    }
    fn system(self, name: &str) -> Option<usize> {
        match self {
            Self::Legacy => SYSTEMS.iter().position(|expected| name == *expected),
            Self::RenderPhases => PHASE_SCOPES[..3].iter().position(|scope| name == scope.name),
        }
    }
    fn direct(self, meta: &Metadata<'_>) -> Option<usize> {
        if self != Self::RenderPhases || !meta.is_span() || *meta.level() != tracing::Level::INFO { return None; }
        PHASE_SCOPES.iter().enumerate().skip(3)
            .find(|(_, scope)| meta.name() == scope.name && meta.target() == scope.target)
            .map(|(index, _)| index)
    }
    fn metadata(self, meta: &Metadata<'_>) -> bool {
        (system_metadata(meta) && (self == Self::Legacy || *meta.level() == tracing::Level::INFO))
            || self.direct(meta).is_some()
    }
}
const FOOTER_RESERVE: usize = 256;
const MAX_RECORDS: usize = 100_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[cfg(test)]
fn selected(name: &str) -> Option<usize> { Profile::Legacy.system(name) }
fn system_metadata(meta: &Metadata<'_>) -> bool {
    meta.is_span() && meta.name() == "system"
        && meta.target() == "bevy_ecs::system::function_system"
}
// ExclusiveFunctionSystem uses SystemMeta's same callsite/target.
struct Name { profile: Profile, selected: Option<usize> }
impl Visit for Name {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "name" { self.selected = self.profile.system(value); }
    }
    fn record_debug(&mut self, _: &Field, _: &dyn std::fmt::Debug) {}
}
#[derive(Clone, Copy)]
struct Selected(usize);

#[derive(Clone, Copy)]
struct Window { start: f64, end: f64, wall_ns: u64, records: usize, bytes: usize }
impl Window {
    fn validate(self) -> Result<Self, String> {
        if !self.start.is_finite() || !self.end.is_finite() || self.start <= 0.0
            || self.end <= self.start || self.end - self.start > 120.0
            || self.wall_ns == 0 || self.wall_ns > 300_000_000_000
            || !(1..=MAX_RECORDS).contains(&self.records)
            || !(4096..=MAX_BYTES).contains(&self.bytes) {
            return Err("require finite 0 < START_S < END_S (a lower predecessor is required), width <= 120s, WALL_S in (0,300], MAX_RECORDS in 1..100000, MAX_BYTES in 4096..16777216".into());
        }
        Ok(self)
    }
}
#[derive(Clone, Copy)]
struct Frame { frame: u32, tick: u64, sim_s: f64 }
#[derive(Clone, Copy)]
struct Started { at_ns: u64, frame: Frame, span: u64, thread: ThreadId }

struct Capture {
    profile: Profile,
    window: Window,
    text: String,
    records: usize,
    current: Option<Frame>,
    previous: Option<(Frame, u64)>,
    entered_window: bool,
    context_seen: u8,
    current_at_ns: u64,
    running: [Option<Started>; 5],
    seen: u8,
    reason: Option<&'static str>,
    file: Option<File>,
}
impl Capture {
    #[cfg(test)]
    fn new(window: Window, run: &str, file: Option<File>) -> Self {
        Self::with_profile(window, run, file, Profile::Legacy)
    }
    fn with_profile(window: Window, run: &str, file: Option<File>, profile: Profile) -> Self {
        let schema = match profile { Profile::Legacy => "v2", Profile::RenderPhases => "v3" };
        let scope_set = match profile { Profile::Legacy => "", Profile::RenderPhases => " scope_set=render-phases" };
        let mut text = format!("# csr_targeted_render_trace_{schema} run={run} feature=targeted-render-trace bevy=0.19.1 clock=monotonic_cpu_wall_ns{scope_set} start_s={} end_s={} wall_ns={} max_records={} max_bytes={}\n", window.start, window.end, window.wall_ns, window.records, window.bytes);
        match profile {
            Profile::Legacy => for (index, name) in SYSTEMS.iter().enumerate() {
                writeln!(text, "# system {index} {name}").unwrap();
            },
            Profile::RenderPhases => for (index, scope) in PHASE_SCOPES.iter().enumerate() {
                writeln!(text, "# scope {index} kind={} name={} target={} level=INFO", scope.kind, scope.name, scope.target).unwrap();
            },
        }
        let column = match profile { Profile::Legacy => "system", Profile::RenderPhases => "scope" };
        writeln!(text, "kind\tframe\tsim_tick\tsim_s\tstart_ns\tduration_ns\t{column}\tspan\tthread\tcontext_changed").unwrap();
        Self { profile, window, text, records: 0, current: None, previous: None,
            entered_window: false, context_seen: 0, current_at_ns: 0,
            running: [None; 5], seen: 0, reason: None, file }
    }
    fn append(&mut self, line: &str) {
        if self.reason.is_some() { return; }
        if self.records >= self.window.records { self.close("record_cap"); return; }
        if line.len() > self.window.bytes.saturating_sub(self.text.len() + FOOTER_RESERVE) {
            self.close("byte_cap"); return;
        }
        self.text.push_str(line);
        self.records += 1;
    }
    fn close(&mut self, reason: &'static str) {
        if self.reason.is_some() { return; }
        self.reason = Some(reason);
        self.current = None;
        // Fixed fields; FOOTER_RESERVE covers this even at maximum record count.
        writeln!(self.text, "# end reason={reason} records={} seen_mask={} open_scopes={}",
            self.records, self.seen, self.running.iter().filter(|s| s.is_some()).count()).unwrap();
        if let Some(mut file) = self.file.take() {
            if let Err(error) = file.write_all(self.text.as_bytes()).and_then(|_| file.flush()) {
                // Do not recurse through tracing or repeat this error per frame.
                eprintln!("targeted render trace write failed (artifact incomplete): {error}");
            }
        }
    }
    fn context(&mut self, at_ns: u64, frame: Option<Frame>) {
        if self.reason.is_some() { return; }
        if at_ns >= self.window.wall_ns { self.close("wall_limit"); return; }
        let Some(frame) = frame else {
            self.current = None;
            self.previous = None;
            if self.entered_window { self.close("context_lost"); }
            return;
        };
        if !frame.sim_s.is_finite() || frame.sim_s < 0.0 { self.close("invalid_sim_time"); return; }
        if self.previous.is_some_and(|(last, at)| frame.tick < last.tick || frame.frame <= last.frame || frame.sim_s < last.sim_s || at_ns < at) {
            self.close("context_rewind"); return;
        }
        if frame.sim_s < self.window.start {
            self.previous = Some((frame, at_ns));
            self.current = None;
            return;
        }
        let kind = if self.entered_window { "context" } else { "lower" };
        if !self.entered_window {
            let Some((last, at)) = self.previous else { self.close("missed_start"); return; };
            // A first observation inside the window, even exactly at START,
            // cannot establish the crossing. Retain only its immediate predecessor.
            if last.sim_s >= self.window.start || frame.sim_s >= self.window.end {
                self.close("missed_start"); return;
            }
            self.context_row("lower_previous", last, at);
        }
        // Retain the incoming observation even when it reveals a gap or the
        // preceding context was partial. It is diagnostic context, not a sample.
        self.context_row(if frame.sim_s >= self.window.end { "upper" } else { kind }, frame, at_ns);
        if self.reason.is_some() { return; }
        if self.previous.is_some_and(|(last, _)| last.frame.checked_add(1) != Some(frame.frame)) {
            self.close("context_gap"); return;
        }
        if self.running.iter().any(Option::is_some) { self.close("context_changed"); return; }
        if self.entered_window && self.context_seen != 31 { self.close("missing_function"); return; }
        if frame.sim_s >= self.window.end { self.close("window_end"); return; }
        self.entered_window = true;
        self.previous = Some((frame, at_ns));
        self.current = Some(frame);
        self.current_at_ns = at_ns;
        self.context_seen = 0;
    }
    fn context_row(&mut self, kind: &str, frame: Frame, at_ns: u64) {
        // Display round-trips the f64 used for membership, as in the header.
        self.append(&format!("{kind}\t{}\t{}\t{}\t{at_ns}\t\t\t\t\t\n", frame.frame, frame.tick, frame.sim_s));
    }
    fn enter(&mut self, system: usize, span: u64, thread: ThreadId, at_ns: u64) {
        if self.reason.is_some() { return; }
        if at_ns >= self.window.wall_ns { self.close("wall_limit"); return; }
        let Some(frame) = self.current else { return; };
        if at_ns < self.current_at_ns || span == 0 { self.close("scope_mismatch"); return; }
        if self.running[system].is_some() { self.close("overlapping_same_system"); return; }
        self.running[system] = Some(Started { at_ns, frame, span, thread });
    }
    fn exit(&mut self, system: usize, span: u64, thread: ThreadId, at_ns: u64) {
        if self.reason.is_some() { return; }
        if at_ns >= self.window.wall_ns { self.close("wall_limit"); return; }
        let Some(start) = self.running[system].take() else { return; };
        if start.span != span || start.thread != thread || at_ns < start.at_ns {
            self.close("scope_mismatch"); return;
        }
        let changed = self.current.is_none_or(|frame| frame.frame != start.frame.frame);
        if changed { self.close("context_changed"); return; }
        self.append(&format!("sample\t{}\t{}\t{}\t{}\t{}\t{system}\t{span}\t{thread:?}\tfalse\n",
            start.frame.frame, start.frame.tick, start.frame.sim_s, start.at_ns, at_ns - start.at_ns));
        if self.reason.is_none() {
            self.seen |= 1 << system;
            self.context_seen |= 1 << system;
        }
    }
}

struct Shared { capture: Mutex<Capture>, born: Instant, active: AtomicBool, closed: AtomicBool }
#[derive(Resource, Clone)]
struct Handle(Arc<Shared>);
struct TraceLayer(Arc<Shared>, Profile);
impl Layer<Registry> for TraceLayer {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, Registry>) {
        // Recheck exact metadata here as well as in the layer filter. Direct
        // spans use metadata, never a user-supplied `name` field or suffix.
        if !self.1.metadata(attrs.metadata()) { return; }
        let selected = if system_metadata(attrs.metadata()) {
            let mut name = Name { profile: self.1, selected: None };
            attrs.record(&mut name);
            name.selected
        } else { self.1.direct(attrs.metadata()) };
        if let Some(system) = selected {
            if let Some(span) = ctx.span(id) { span.extensions_mut().insert(Selected(system)); }
        }
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, Registry>) {
        if !self.0.active.load(Ordering::Relaxed) { return; }
        let Some(span) = ctx.span(id) else { return; };
        let Some(system) = span.extensions().get::<Selected>().copied() else { return; };
        let mut capture = self.0.capture.lock().unwrap();
        capture.enter(system.0, id.into_u64(), std::thread::current().id(), elapsed(&self.0));
        update_flags(&self.0, &capture);
    }
    fn on_exit(&self, id: &Id, ctx: Context<'_, Registry>) {
        if !self.0.active.load(Ordering::Relaxed) { return; }
        let Some(span) = ctx.span(id) else { return; };
        let Some(system) = span.extensions().get::<Selected>().copied() else { return; };
        let mut capture = self.0.capture.lock().unwrap();
        capture.exit(system.0, id.into_u64(), std::thread::current().id(), elapsed(&self.0));
        update_flags(&self.0, &capture);
    }
}
fn elapsed(shared: &Shared) -> u64 { shared.born.elapsed().as_nanos().min(u64::MAX as u128) as u64 }
fn update_flags(shared: &Shared, capture: &Capture) {
    shared.active.store(capture.current.is_some() && capture.reason.is_none(), Ordering::Relaxed);
    shared.closed.store(capture.reason.is_some(), Ordering::Relaxed);
}

fn required(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("missing or non-Unicode {name}"))
}
fn number<T: std::str::FromStr>(name: &str) -> Result<T, String> {
    required(name)?.parse().map_err(|_| format!("invalid {name}"))
}
// Deliberately narrow local Windows namespace policy, not an identity guard for
// arbitrary Windows paths. ASCII permits conservative case-insensitive matching
// even in case-sensitive directories; unsupported Unicode fails closed.
fn local_drive_key(path: &Path, canonical: bool) -> Result<String, String> {
    let text = path.to_str().filter(|text| text.is_ascii())
        .ok_or("trace path policy supports ASCII local Windows drive paths only")?;
    let text = text.replace('/', "\\");
    // Only canonicalize's drive form is accepted here. User-supplied verbatim,
    // UNC (including verbatim UNC), device and generic verbatim paths are denied.
    let text = if canonical { text.strip_prefix("\\\\?\\").unwrap_or(&text) } else { &text };
    let bytes = text.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\'
        || text[2..].contains(':') {
        return Err("trace path requires a local absolute Windows drive path; UNC/network/device/verbatim input is unsupported".into());
    }
    if canonical && text[3..].split('\\').any(|part| part == "." || part == "..") {
        return Err("trace path was not canonicalized".into());
    }
    Ok(text.to_ascii_lowercase())
}
fn require_local_drive(path: &Path, canonical: bool, drive_type: &impl Fn(u8) -> u32) -> Result<(), String> {
    let key = local_drive_key(path, canonical)?;
    // GetDriveTypeW: removable, fixed and RAM disk only; remote/unknown/missing
    // drives fail closed, including mapped network drive letters.
    if !matches!(drive_type(key.as_bytes()[0]), 2 | 3 | 6) {
        return Err("trace path requires an available local drive; network/unknown drives are unsupported".into());
    }
    Ok(())
}
fn outside_protected(parent: &Path, protected: &[PathBuf]) -> Result<(), String> {
    let parent = local_drive_key(parent, true)?;
    let parent = parent.trim_end_matches('\\');
    for root in protected {
        let root = local_drive_key(root, true)?;
        let root = root.trim_end_matches('\\');
        if parent == root || parent.strip_prefix(root).is_some_and(|suffix| suffix.starts_with('\\')) {
            return Err("trace must be outside game/data/workspace directories".into());
        }
    }
    Ok(())
}
// The injected read-only resolvers let fixtures exercise this exact decision
// path without files, shares, drive mappings or environment changes.
fn private_path_with(
    path: &Path, protected: &[PathBuf],
    canonicalize: impl Fn(&Path) -> Result<PathBuf, String>, drive_type: impl Fn(u8) -> u32,
) -> Result<PathBuf, String> {
    require_local_drive(path, false, &drive_type)?;
    let name = path.file_name().ok_or("PATH needs a file name")?;
    let parent = canonicalize(path.parent().ok_or("PATH needs an existing private parent directory")?)
        .map_err(|e| format!("trace parent: {e}"))?;
    require_local_drive(&parent, true, &drive_type)?;
    let roots = protected.iter().map(|root| {
        let root = canonicalize(root).map_err(|e| format!("protected path {}: {e}", root.display()))?;
        require_local_drive(&root, true, &drive_type)?;
        Ok(root)
    }).collect::<Result<Vec<_>, String>>()?;
    outside_protected(&parent, &roots)?;
    Ok(parent.join(name))
}
#[cfg(windows)]
fn drive_type(drive: u8) -> u32 {
    #[link(name = "kernel32")]
    unsafe extern "system" { fn GetDriveTypeW(root: *const u16) -> u32; }
    let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
    // A NUL-terminated drive root; no handle or file is opened.
    unsafe { GetDriveTypeW(root.as_ptr()) }
}
#[cfg(not(windows))]
fn drive_type(_: u8) -> u32 { 0 } // This local diagnostic path policy is Windows-only.
fn private_path(path: &Path, protected: &[PathBuf]) -> Result<PathBuf, String> {
    private_path_with(path, protected, |path| path.canonicalize().map_err(|e| e.to_string()), drive_type)
}

/// Called before LogPlugin: absent PATH installs no capture resources or clocks.
pub(crate) fn configure(app: &mut App, game_dir: &Path) -> Result<Option<TraceGuard>, String> {
    let profile = Profile::parse(std::env::var_os("CSR_RENDER_TRACE_SCOPES").as_deref())?;
    let Some(path) = std::env::var_os("CSR_RENDER_TRACE_PATH") else { return Ok(None); };
    // Bevy's outer EnvFilter must retain INFO system spans. Reject the known
    // full-Chrome environment switch too; QA must not compile Chrome/Tracy.
    for name in ["RUST_LOG", "TRACE_CHROME"] {
        if std::env::var_os(name).is_some() { return Err(format!("unset {name} for this diagnostic")); }
    }
    let wall_s: f64 = number("CSR_RENDER_TRACE_WALL_S")?;
    if !wall_s.is_finite() || wall_s <= 0.0 || wall_s > 300.0 { return Err("invalid WALL_S".into()); }
    let window = Window {
        start: number("CSR_RENDER_TRACE_START_S")?, end: number("CSR_RENDER_TRACE_END_S")?,
        wall_ns: (wall_s * 1e9) as u64, records: number("CSR_RENDER_TRACE_MAX_RECORDS")?,
        bytes: number("CSR_RENDER_TRACE_MAX_BYTES")?,
    }.validate()?;
    let run = required("CSR_RENDER_TRACE_RUN_ID")?;
    if run.is_empty() || run.len() > 64 || !run.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)) {
        return Err("RUN_ID must contain 1..64 ASCII letters/digits/-_.".into());
    }
    // Diagnostic deny boundary only. Never a runtime save-path base. This
    // remains independent when the launcher changes CWD to private CSR_DATA_DIR.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent)
        .ok_or("cannot determine diagnostic source workspace")?.to_path_buf();
    let mut protected = vec![workspace, std::env::current_dir().map_err(|e| e.to_string())?, game_dir.to_path_buf()];
    if let Some(data) = std::env::var_os("CSR_DATA_DIR") { protected.push(data.into()); }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let userdata = PathBuf::from(local).join("CSGO-Rewrite/userdata");
        if userdata.exists() { protected.push(userdata); }
    }
    let path = private_path(Path::new(&path), &protected)?;
    let file = OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
    let handle = Handle(Arc::new(Shared { capture: Mutex::new(Capture::with_profile(window, &run, Some(file), profile)),
        born: Instant::now(), active: AtomicBool::new(false), closed: AtomicBool::new(false) }));
    app.insert_resource(handle.clone());
    Ok(Some(TraceGuard(handle)))
}
pub(crate) fn layer(app: &mut App) -> Option<bevy::log::BoxedLayer> {
    let handle = app.world().get_resource::<Handle>()?;
    let profile = handle.0.capture.lock().unwrap().profile;
    Some(Box::new(TraceLayer(handle.0.clone(), profile).with_filter(filter_fn(move |meta| profile.metadata(meta)))))
}
pub(crate) fn install(app: &mut App) {
    let Some(handle) = app.world().get_resource::<Handle>().cloned() else { return; };
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.insert_resource(handle).add_systems(ExtractSchedule,
            context.before(bevy::pbr::extract_skins));
    } else {
        handle.0.capture.lock().unwrap().close("missing_render_app");
    }
}
fn context(handle: Res<Handle>, frame: Extract<Res<FrameCount>>, sim: Extract<Option<Res<crate::player::SimWorld>>>) {
    if handle.0.closed.load(Ordering::Relaxed) { return; }
    let snapshot = sim.as_ref().map(|sim| Frame { frame: frame.0, tick: sim.0.tick, sim_s: f64::from(sim.0.time()) });
    let mut capture = handle.0.capture.lock().unwrap();
    capture.context(elapsed(&handle.0), snapshot);
    update_flags(&handle.0, &capture);
}
// App::run replaces the caller with App::empty(), so retain a handle outside
// the App rather than trying to recover its resources after the runner returns.
pub(crate) struct TraceGuard(Handle);
impl Drop for TraceGuard {
    fn drop(&mut self) {
        if let Ok(mut capture) = self.0.0.capture.lock() {
            capture.close("app_exit");
            update_flags(&self.0.0, &capture);
        }
    }
}

#[cfg(test)]
mod tests;
