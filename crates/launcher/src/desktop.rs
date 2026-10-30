//! Desktop launcher. Rust owns updates, processes and validated pack
//! operations here; `desktop_ui` draws the frameless Bevy window and turns
//! clicks into [`Act`]s.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{Mutex, mpsc},
    time::Duration,
};
use velopack::{UpdateCheck, UpdateManager, VelopackAsset};

/// The launcher window's title (taskbar, Alt+Tab).
pub const TITLE: &str = "Counter-Strike Rust";
/// The launcher window's class, used to find a running launcher.
pub const WINDOW_CLASS: &str = "CounterStrikeRustLauncher";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    #[default]
    Stable,
    Dev,
}

#[derive(Default, Deserialize, Serialize)]
struct Preferences {
    #[serde(default)]
    track: Track,
}

impl Preferences {
    fn load(root: &Path) -> Self {
        fs::read(root.join("config/launcher.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn save(&self, root: &Path) -> Result<(), String> {
        let path = root.join("config/launcher.json");
        fs::create_dir_all(root.join("config")).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        fs::rename(tmp, path).map_err(|e| e.to_string())
    }
}

pub fn requested(args: &[String]) -> bool {
    args.is_empty() || args.first().is_some_and(|a| a == "launcher")
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackRow {
    pub id: String,
    pub name: String,
    pub version: String,
}

enum Event {
    Status(String, i16),
    Notes(String, String),
    Checked(Result<Option<(UpdateManager, VelopackAsset)>, String>),
    Picked(Option<PathBuf>),
}

fn check(track: Track, sender: mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        let result = crate::update::manager(track == Track::Dev)
            .and_then(|manager| check_and_download(manager, &sender));
        let _ = sender.send(Event::Checked(result));
    });
}

fn check_and_download(
    manager: UpdateManager,
    sender: &mpsc::Sender<Event>,
) -> Result<Option<(UpdateManager, VelopackAsset)>, String> {
    match manager.check_for_updates().map_err(|e| e.to_string())? {
        UpdateCheck::UpdateAvailable(info) => {
            let asset = info.TargetFullRelease.clone();
            let _ = sender.send(Event::Notes(asset.Version.clone(), asset.NotesMarkdown.clone()));
            let _ = sender.send(Event::Status(format!("Downloading {}", asset.Version), 0));
            let (progress, receiver) = mpsc::channel();
            let updates = sender.clone();
            let version = asset.Version.clone();
            let reporter = std::thread::spawn(move || {
                for percent in receiver {
                    let _ = updates.send(Event::Status(format!("Downloading {version}"), percent));
                }
            });
            let downloaded = manager
                .download_updates(&info, Some(progress))
                .map_err(|e| e.to_string());
            let _ = reporter.join();
            downloaded?;
            Ok(Some((manager, asset)))
        }
        UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(None),
    }
}

fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn launch(root: &Path, track: Track) -> Result<Child, String> {
    fs::create_dir_all(root.join("logs")).map_err(|e| e.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.args(["play", "de_dust2"]).env("CSR_DATA_DIR", root);
    if track == Track::Dev {
        command.arg("--dev-tools");
    }
    command.stdout(fs::File::create(root.join("logs/game.out.log")).map_err(|e| e.to_string())?);
    command.stderr(fs::File::create(root.join("logs/game.log")).map_err(|e| e.to_string())?);
    hidden(&mut command);
    command.spawn().map_err(|e| format!("Could not start the game: {e}"))
}

/// What the player asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Play,
    /// Install the downloaded update and reopen the launcher.
    UpdateOnly,
    Refresh,
    Track(Track),
    PickPack,
    Cancel,
    Remove(String),
    Logs,
    PacksFolder,
}

/// The launcher's state, owned by the UI thread. Background work reports
/// through a channel drained by [`Launcher::pump`].
pub struct Launcher {
    pub root: PathBuf,
    preferences: Preferences,
    pub version: String,
    pub build: String,
    pub track: Track,
    pub status: String,
    /// A quieter second line under the status (errors, hints).
    pub detail: String,
    pub notes: String,
    /// The release the notes describe, when they come from an update.
    pub notes_version: Option<String>,
    default_notes: String,
    /// Checking for or downloading an update.
    pub busy: bool,
    /// Download percentage, or -1 while the amount is unknown.
    pub progress: i16,
    pub running: bool,
    /// The version of a downloaded update ready to install.
    pub ready: Option<String>,
    update: Mutex<Option<(UpdateManager, VelopackAsset)>>,
    pub installing: bool,
    pub install_progress: i16,
    pub picking: bool,
    pub packs: Vec<PackRow>,
    pub pack_errors: Vec<String>,
    pub pack_status: String,
    events: Mutex<mpsc::Receiver<Event>>,
    sender: mpsc::Sender<Event>,
    game: Option<Child>,
    job: Mutex<Option<packs::InstallJob>>,
    /// Real update checks and processes; off for screenshot fixtures.
    live: bool,
}

impl Launcher {
    pub fn new(root: PathBuf, live: bool) -> Self {
        let preferences = Preferences::load(&root);
        let version = velopack::locator::auto_locate_app_manifest(
            velopack::locator::LocationContext::FromCurrentExe,
        )
        .ok()
        .map(|l| l.get_manifest_version().to_string())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
        let default_notes = std::env::current_exe()
            .ok()
            .and_then(|e| fs::read_to_string(e.with_file_name("RELEASE-NOTES.md")).ok())
            .unwrap_or_default();
        let (sender, events) = mpsc::channel();
        let mut launcher = Launcher {
            track: preferences.track,
            preferences,
            version,
            build: option_env!("CSR_BUILD_SHA").unwrap_or("source").chars().take(8).collect(),
            status: String::new(),
            detail: String::new(),
            notes: default_notes.clone(),
            notes_version: None,
            default_notes,
            busy: false,
            progress: -1,
            running: false,
            ready: None,
            update: Mutex::new(None),
            installing: false,
            install_progress: -1,
            picking: false,
            packs: vec![],
            pack_errors: vec![],
            pack_status: String::new(),
            events: Mutex::new(events),
            sender,
            game: None,
            job: Mutex::new(None),
            live,
            root,
        };
        launcher.reload_packs();
        if live {
            launcher.start_check();
        }
        launcher
    }

    /// Starts the game straight away (after an "Update & play" restart).
    pub fn launch_now(&mut self) {
        match launch(&self.root, self.track) {
            Ok(child) => {
                self.game = Some(child);
                self.running = true;

            }
            Err(e) => self.set_status("Could not start the game", &e),
        }
    }

    fn set_status(&mut self, status: &str, detail: &str) {
        self.status = status.into();
        self.detail = detail.into();
    }

    fn start_check(&mut self) {
        *self.update.lock().unwrap() = None;
        self.ready = None;
        self.notes = self.default_notes.clone();
        self.notes_version = None;
        self.busy = true;
        self.progress = -1;
        self.set_status("Checking for updates", "");
        check(self.track, self.sender.clone());
    }

    fn reload_packs(&mut self) {
        let catalog = packs::Installed::new(&self.root).catalog();
        self.packs = catalog
            .entries
            .iter()
            .map(|entry| {
                let m = entry.manifest();
                PackRow { id: m.id.clone(), name: m.name.clone(), version: m.version.clone() }
            })
            .collect();
        self.pack_errors = catalog.errors;
    }

    /// True while background work needs frequent polling.
    pub fn active(&self) -> bool {
        self.busy || self.installing || self.picking
    }

    /// Drains background events and polls child processes and pack jobs.
    pub fn pump(&mut self, check_presence: bool) {
        let events: Vec<Event> = self.events.lock().unwrap().try_iter().collect();
        for event in events {
            match event {
                Event::Status(text, percent) => {
                    self.status = text;
                    self.detail.clear();
                    self.progress = percent;
                }
                Event::Notes(version, text) if !text.trim().is_empty() => {
                    self.notes = text;
                    self.notes_version = Some(version);
                }
                Event::Notes(..) => {}
                Event::Checked(result) => {
                    self.busy = false;
                    self.progress = -1;
                    match result {
                        Ok(Some(update)) => {
                            self.set_status(&format!("Update {} ready", update.1.Version), "");
                            self.ready = Some(update.1.Version.clone());
                            *self.update.lock().unwrap() = Some(update);
                        }
                        Ok(None) => self.set_status("Up to date", ""),
                        Err(error) => self.set_status("Updates unavailable", &error),
                    }
                }
                Event::Picked(path) => {
                    self.picking = false;
                    if let Some(path) = path {
                        self.start_install(path);
                    }
                }
            }
        }
        if let Some(child) = self.game.as_mut() {
            if let Ok(Some(exit)) = child.try_wait() {
                self.game = None;
                if exit.success() {
                    self.detail.clear();
                } else {
                    self.set_status("The game stopped unexpectedly", "Open the logs for details.");
                }
            }
        }
        if self.live && (check_presence || self.game.is_some() != self.running) {
            self.running = self.game.is_some() || crate::presence::Game::running(&self.root).unwrap_or(false);
        }
        let mut finished = None;
        if let Some(install) = self.job.lock().unwrap().as_mut() {
            let progress = install.progress();
            self.install_progress = if progress.total > 0 {
                (progress.bytes.saturating_mul(100) / progress.total).min(100) as i16
            } else {
                -1
            };
            self.pack_status = crate::packopen::status(progress);
            finished = install.poll();
        }
        if let Some(outcome) = finished {
            *self.job.lock().unwrap() = None;
            self.pack_status = match outcome {
                packs::Outcome::Installed(receipt) => {
                    format!("{} {} installed", receipt.manifest.name, receipt.manifest.version)
                }
                packs::Outcome::Cancelled => "Installation cancelled".into(),
                packs::Outcome::Failed(error) => format!("Installation failed: {error}"),
            };
            self.installing = false;
            self.install_progress = -1;
            self.reload_packs();
        }
    }

    fn start_install(&mut self, path: PathBuf) {
        if self.installing || self.running {
            return;
        }
        *self.job.lock().unwrap() = Some(packs::InstallJob::start(packs::Installed::new(&self.root), path));
        self.installing = true;
        self.install_progress = -1;
        self.pack_status = "Reading the pack…".into();
    }

    pub fn can_play(&self) -> bool {
        !self.running && !self.installing
    }

    pub fn can_manage_packs(&self) -> bool {
        !self.running && !self.installing && !self.picking
    }

    /// Carries out `act`. Returns true when the launcher should close
    /// (an update is being installed).
    pub fn act(&mut self, act: Act) -> bool {
        match act {
            Act::Cancel => {
                if let Some(job) = self.job.lock().unwrap().as_ref() {
                    job.cancel();
                }
            }
            Act::Logs | Act::PacksFolder => {
                let dir = self.root.join(if act == Act::Logs { "logs" } else { "packs" });
                if self.live && fs::create_dir_all(&dir).is_ok() {
                    let _ = Command::new("explorer.exe").arg(dir).spawn();
                }
            }
            Act::Refresh if !self.busy => {
                self.reload_packs();
                if self.live {
                    self.start_check();
                }
            }
            Act::Track(track) if !self.busy && track != self.track => {
                let previous = self.preferences.track;
                self.preferences.track = track;
                if let Err(error) = self.preferences.save(&self.root) {
                    self.preferences.track = previous;
                    self.set_status("Could not save the update track", &error);
                    return false;
                }
                self.track = track;
                // Cached packages from the previous track are never applied
                // unless this check selects them.
                if self.live {
                    self.start_check();
                }
            }
            Act::Play | Act::UpdateOnly if self.can_play() && (act == Act::Play || !self.busy) => {
                if let Some((manager, asset)) = self.update.lock().unwrap().as_ref() {
                    let restart = if act == Act::Play {
                        vec!["launcher", "--launch-after-update"]
                    } else {
                        vec!["launcher"]
                    };
                    match manager.wait_exit_then_apply_updates(asset, false, true, restart) {
                        Ok(()) => return true,
                        Err(e) => {
                            self.status = "Could not install the update".into();
                            self.detail = e.to_string();
                            return false;
                        }
                    }
                }
                if act == Act::Play && self.live {
                    self.launch_now();
                }
            }
            Act::PickPack if self.can_manage_packs() => {
                self.picking = true;
                let sender = self.sender.clone();
                std::thread::spawn(move || {
                    let path = crate::dialog::pick_file(
                        "Install content pack",
                        "Content pack (*.csrpack)|*.csrpack",
                    );
                    let _ = sender.send(Event::Picked(path));
                });
            }
            Act::Remove(id) if self.can_manage_packs() => {
                self.pack_status = match packs::Installed::new(&self.root).remove(&id) {
                    Ok(true) => "Pack removed".into(),
                    Ok(false) => "That pack is no longer installed".into(),
                    Err(e) => e,
                };
                self.reload_packs();
            }
            _ => {}
        }
        false
    }

    /// Screenshot fixtures: a representative state without network,
    /// processes or installed packs.
    pub fn fake(&mut self, state: &str) {
        self.version = "0.5.0".into();
        self.build = "8f636c74".into();
        self.notes = FAKE_NOTES.into();
        self.notes_version = matches!(state, "downloading" | "ready").then(|| "0.5.1".into());
        self.packs = vec![
            PackRow { id: "skate".into(), name: "Skate".into(), version: "1.4.0".into() },
            PackRow { id: "de_cache_2015".into(), name: "Cache (2015)".into(), version: "2015.10.1".into() },
            PackRow { id: "surf-classics".into(), name: "Surf classics".into(), version: "0.9.2".into() },
        ];
        self.pack_errors.clear();
        self.pack_status.clear();
        self.set_status("Up to date", "");
        match state {
            "checking" => {
                self.busy = true;
                self.set_status("Checking for updates", "");
            }
            "downloading" => {
                self.busy = true;
                self.progress = 42;
                self.set_status("Downloading 0.5.1", "");
            }
            "ready" => {
                self.ready = Some("0.5.1".into());
                self.set_status("Update 0.5.1 ready", "");
            }
            "running" => {
                self.running = true;

            }
            "error" => self.set_status("Updates unavailable", "Source and portable copies don't update automatically."),
            "packs" | "installing" | "empty" => {
                if state == "installing" {
                    self.installing = true;
                    self.install_progress = 63;
                    self.pack_status = "Installing the pack… 63%".into();
                }
                if state == "empty" {
                    self.packs.clear();
                    self.pack_errors = vec!["broken.csrpack: manifest is missing a version".into()];
                }
            }
            _ => {}
        }
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        if let Some(mut job) = self.job.lock().unwrap().take() {
            job.cancel();
            while job.poll().is_none() {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

const FAKE_NOTES: &str = "## 0.5.1\n\n\
### Skate\n\
- Ramp collisions match the original transitions.\n\
- Surf and skate transfers keep their momentum.\n\
- Board carry sits naturally in the hands.\n\n\
### Game\n\
- Start at Home; maps and runtimes arrive through optional packs.\n\
- Original HUD and audio for the Skate crossover.\n\n\
### Launcher\n\
- New frameless launcher window.\n\
- Content packs install with progress and can be cancelled.\n\n\
Settings and content packs are kept across updates.";

/// Brings a running launcher to the front.
fn activate_existing() {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn FindWindowW(class: *const u16, title: *const u16) -> isize;
            fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
            fn IsIconic(hwnd: isize) -> i32;
            fn SetForegroundWindow(hwnd: isize) -> i32;
        }
        let class: Vec<u16> = WINDOW_CLASS.encode_utf16().chain([0]).collect();
        // SAFETY: plain Win32 calls with a NUL-terminated class name.
        unsafe {
            let hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
            if hwnd != 0 {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, 9); // SW_RESTORE
                }
                SetForegroundWindow(hwnd);
            }
        }
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let fixture = std::env::var("CSR_LAUNCHER_FAKE").ok();
    let guard = crate::presence::file(&root, "launcher.instance.lock")?;
    if fixture.is_none() {
        if let Err(error) = guard.try_lock() {
            if matches!(error, fs::TryLockError::WouldBlock) {
                activate_existing();
                return Ok(());
            }
            return Err(error.to_string());
        }
    }
    let mut launcher = Launcher::new(root, fixture.is_none());
    if let Some(state) = &fixture {
        launcher.fake(state);
    } else if args.iter().any(|a| a == "--launch-after-update") {
        launcher.launch_now();
    }
    let result = crate::desktop_ui::run(launcher);
    drop(guard);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_desktop_entry_points_open_the_launcher() {
        assert!(requested(&[]));
        assert!(requested(&["launcher".into()]));
        for args in [
            vec!["play", "de_dust2"],
            vec!["probe", "de_dust2"],
            vec!["sim", "de_dust2"],
            vec!["--screenshot", "a.png"],
        ] {
            assert!(!requested(
                &args.into_iter().map(String::from).collect::<Vec<_>>()
            ));
        }
    }
    #[test]
    fn stable_is_default_and_track_survives_restarts() {
        let root = std::env::temp_dir().join(format!("csr-launcher-prefs-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        assert_eq!(Preferences::load(&root).track, Track::Stable);
        Preferences { track: Track::Dev }.save(&root).unwrap();
        assert_eq!(Preferences::load(&root).track, Track::Dev);
        Preferences {
            track: Track::Stable,
        }
        .save(&root)
        .unwrap();
        assert_eq!(Preferences::load(&root).track, Track::Stable);
        fs::write(root.join("config/launcher.json"), b"{broken").unwrap();
        assert_eq!(Preferences::load(&root).track, Track::Stable);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn switching_track_saves_it_and_play_waits_for_packs() {
        let root = std::env::temp_dir().join(format!("csr-launcher-acts-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut launcher = Launcher::new(root.clone(), false);
        assert_eq!(launcher.track, Track::Stable);
        assert!(!launcher.act(Act::Track(Track::Dev)));
        assert_eq!(launcher.track, Track::Dev);
        assert_eq!(Preferences::load(&root).track, Track::Dev);
        // Busy checks lock the track; installs and running games lock Play.
        launcher.busy = true;
        launcher.act(Act::Track(Track::Stable));
        assert_eq!(launcher.track, Track::Dev);
        launcher.installing = true;
        assert!(!launcher.can_play() && !launcher.can_manage_packs());
        drop(launcher);
        fs::remove_dir_all(root).unwrap();
    }

    fn local_update(name: &str, current: &str) -> (PathBuf, UpdateManager) {
        let root =
            std::env::temp_dir().join(format!("csr-launcher-update-{}-{name}", std::process::id()));
        for dir in ["current", "packages", "feed"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("Update.exe"), b"never executed by these tests").unwrap();
        let manifest = root.join("current/sq.version");
        fs::write(&manifest, format!("<package><metadata><id>CSGORewrite</id><version>{current}</version><channel>win</channel><mainExe>csr.exe</mainExe></metadata></package>")).unwrap();
        let asset = VelopackAsset {
            PackageId: "CSGORewrite".into(),
            Version: "0.5.0".into(),
            Type: "Full".into(),
            FileName: "CSGORewrite-0.5.0-full.nupkg".into(),
            Size: 3,
            SHA256: "0".repeat(64),
            ..Default::default()
        };
        fs::write(root.join("feed").join(&asset.FileName), b"bad").unwrap();
        fs::write(
            root.join("feed/releases.win.json"),
            serde_json::to_vec(&velopack::VelopackAssetFeed {
                Assets: vec![asset],
            })
            .unwrap(),
        )
        .unwrap();
        let manager = UpdateManager::new(
            velopack::sources::AutoSource::new(&root.join("feed").to_string_lossy()),
            Some(crate::update::options()),
            Some(velopack::locator::VelopackLocatorConfig {
                RootAppDir: root.clone(),
                UpdateExePath: root.join("Update.exe"),
                PackagesDir: root.join("packages"),
                ManifestPath: manifest,
                CurrentBinaryDir: root.join("current"),
                IsPortable: false,
            }),
        )
        .unwrap();
        (root, manager)
    }

    #[test]
    fn returning_from_dev_offers_stable_and_corruption_cannot_become_ready() {
        let (root, manager) = local_update("downgrade", "0.5.1-dev.1.1");
        let UpdateCheck::UpdateAvailable(info) = manager.check_for_updates().unwrap() else {
            panic!("Stable downgrade must be offered")
        };
        assert!(info.IsDowngrade);
        assert_eq!(info.TargetFullRelease.Version, "0.5.0");
        let (sender, _receiver) = mpsc::channel();
        let error = match check_and_download(manager, &sender) {
            Err(error) => error,
            Ok(_) => panic!("Corrupt package must never become a ready update"),
        };
        assert!(error.to_lowercase().contains("checksum"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_stable_does_not_download_or_apply_any_package() {
        let (root, manager) = local_update("current", "0.5.0");
        let (sender, _receiver) = mpsc::channel();
        assert!(check_and_download(manager, &sender).unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
