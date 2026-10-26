//! Desktop shell. Rust owns updates, processes and validated pack operations;
//! Windows Forms displays JSON state and emits literal JSON actions.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use velopack::{UpdateCheck, UpdateManager, VelopackAsset};

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
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        fs::rename(tmp, path).map_err(|e| e.to_string())
    }
}

pub fn requested(args: &[String]) -> bool {
    args.is_empty() || args.first().is_some_and(|a| a == "launcher")
}

#[derive(Serialize)]
struct PackRow {
    id: String,
    name: String,
    version: String,
}

#[derive(Serialize)]
struct State {
    version: String,
    build: String,
    track: Track,
    status: String,
    notes: String,
    busy: bool,
    running: bool,
    ready: bool,
    progress: i16,
    packs: Vec<PackRow>,
    pack_errors: String,
    installing: bool,
}

#[derive(Deserialize)]
struct Action {
    action: String,
    #[serde(default)]
    value: String,
}

enum Event {
    Status(String, i16),
    Notes(String),
    Checked(Result<Option<(UpdateManager, VelopackAsset)>, String>),
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
            let _ = sender.send(Event::Notes(asset.NotesMarkdown.clone()));
            let _ = sender.send(Event::Status(format!("Downloading {}…", asset.Version), 0));
            let (progress, receiver) = mpsc::channel();
            let updates = sender.clone();
            let version = asset.Version.clone();
            let reporter = std::thread::spawn(move || {
                for percent in receiver {
                    let _ = updates.send(Event::Status(
                        format!("Downloading {version}… {percent}%"),
                        percent,
                    ));
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

fn packs(state: &mut State, root: &Path) {
    let catalog = packs::Installed::new(root).catalog();
    state.packs = catalog
        .entries
        .iter()
        .map(|entry| {
            let m = entry.manifest();
            PackRow {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
            }
        })
        .collect();
    state.pack_errors = catalog.errors.join("\n");
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

struct Window(Child);
impl std::ops::Deref for Window {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for Window {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn launch(root: &Path, track: Track) -> Result<Child, String> {
    fs::create_dir_all(root.join("logs")).map_err(|e| e.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.args(["play", "de_dust2"]).env("CSR_DATA_DIR", root);
    if track == Track::Dev {
        command.arg("--playtest");
    }
    command.stdout(fs::File::create(root.join("logs/game.out.log")).map_err(|e| e.to_string())?);
    command.stderr(fs::File::create(root.join("logs/game.log")).map_err(|e| e.to_string())?);
    hidden(&mut command);
    command
        .spawn()
        .map_err(|e| format!("Could not start the game: {e}"))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let guard = crate::presence::file(&root, "launcher.instance.lock")?;
    if let Err(error) = guard.try_lock() {
        if matches!(error, fs::TryLockError::WouldBlock) {
            let _ = crate::dialog::powershell(
                "$shell=New-Object -ComObject WScript.Shell; [void]$shell.AppActivate('CSGO Rewrite Launcher')",
            );
            return Ok(());
        }
        return Err(error.to_string());
    }
    let mut preferences = Preferences::load(&root);
    let script = root.join("config/launcher-window.ps1");
    let state_path = root.join("config/launcher-state.json");
    fs::write(&script, format!("\u{feff}{}", include_str!("desktop.ps1")))
        .map_err(|e| e.to_string())?;
    let current = velopack::locator::auto_locate_app_manifest(
        velopack::locator::LocationContext::FromCurrentExe,
    )
    .ok()
    .map(|l| l.get_manifest_version().to_string())
    .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
    let notes = std::env::current_exe().ok().and_then(|e| fs::read_to_string(e.with_file_name("RELEASE-NOTES.md")).ok())
        .unwrap_or_else(|| "Updates and release notes appear here when you refresh.\n\nStable is recommended for friends. Dev receives the latest tested changes from master.".into());
    let mut state = State {
        version: current,
        build: option_env!("CSR_BUILD_SHA")
            .unwrap_or("source")
            .chars()
            .take(8)
            .collect(),
        track: preferences.track,
        status: "Checking for updates…".into(),
        notes: notes.clone(),
        busy: true,
        running: false,
        ready: false,
        progress: -1,
        packs: vec![],
        pack_errors: String::new(),
        installing: false,
    };
    packs(&mut state, &root);
    fs::write(
        &state_path,
        serde_json::to_vec(&state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .arg("-StateFile")
        .arg(&state_path)
        .arg("-IconFile")
        .arg(std::env::current_exe().map_err(|e| e.to_string())?)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(root.join("config/launcher-ui.log")).map_err(|e| e.to_string())?);
    hidden(&mut command);
    if let Some(path) = std::env::var_os("CSR_LAUNCHER_PREVIEW") {
        command.arg("-Preview").arg(path);
    }
    let mut window = Window(
        command
            .spawn()
            .map_err(|e| format!("Could not open launcher: {e}"))?,
    );
    let (actions, receiver) = mpsc::channel();
    let output = window.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if let Ok(action) = serde_json::from_str::<Action>(&line) {
                let _ = actions.send(action);
            }
        }
    });
    let (sender, events) = mpsc::channel();
    let mut ready: Option<(UpdateManager, VelopackAsset)> = None;
    let mut game = if args.iter().any(|a| a == "--launch-after-update") {
        Some(launch(&root, state.track)?)
    } else {
        None
    };
    let mut job: Option<packs::InstallJob> = None;
    check(state.track, sender.clone());
    loop {
        if let Some(exit) = window.try_wait().map_err(|e| e.to_string())? {
            if let Some(mut job) = job {
                job.cancel();
                while job.poll().is_none() {
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
            return if exit.success() {
                Ok(())
            } else {
                Err("The launcher window closed unexpectedly. See config/launcher-ui.log.".into())
            };
        }
        let mut dirty = false;
        while let Ok(event) = events.try_recv() {
            dirty = true;
            match event {
                Event::Status(text, percent) => {
                    state.status = text;
                    state.progress = percent;
                }
                Event::Notes(text) if !text.is_empty() => state.notes = text,
                Event::Notes(_) => {}
                Event::Checked(result) => {
                    state.busy = false;
                    state.progress = 0;
                    match result {
                        Ok(Some(update)) => {
                            state.status = format!("{} is ready to install.", update.1.Version);
                            state.ready = true;
                            ready = Some(update);
                        }
                        Ok(None) => state.status = "You're up to date. Ready to play.".into(),
                        Err(error) => {
                            state.status =
                                format!("Update check unavailable. You can still play.\n{error}")
                        }
                    }
                }
            }
        }
        if let Some(child) = game.as_mut() {
            if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
                game = None;
                state.status = if exit.success() {
                    "Game closed. Ready to play.".into()
                } else {
                    "The game stopped unexpectedly. Open the logs folder for details.".into()
                };
                dirty = true;
            }
        }
        let running = game.is_some() || crate::presence::Game::running(&root)?;
        if state.running != running {
            state.running = running;
            dirty = true;
        }
        if let Some(install) = job.as_mut() {
            let progress = install.progress();
            let percent = if progress.total > 0 {
                (progress.bytes.saturating_mul(100) / progress.total).min(100) as i16
            } else {
                -1
            };
            let text = format!("Installing content pack… {:?}", progress.stage);
            if state.status != text || state.progress != percent {
                state.status = text;
                state.progress = percent;
                dirty = true;
            }
            if let Some(outcome) = install.poll() {
                state.status = match outcome {
                    packs::Outcome::Installed(receipt) => format!(
                        "{} {} installed.",
                        receipt.manifest.name, receipt.manifest.version
                    ),
                    packs::Outcome::Cancelled => "Pack installation cancelled.".into(),
                    packs::Outcome::Failed(error) => format!("Pack installation failed: {error}"),
                };
                job = None;
                state.installing = false;
                state.busy = false;
                state.progress = 0;
                packs(&mut state, &root);
                dirty = true;
            }
        }
        if let Ok(action) = receiver.recv_timeout(Duration::from_millis(100)) {
            dirty = true;
            match action.action.as_str() {
                "cancel" => {
                    if let Some(job) = &job {
                        job.cancel();
                    }
                }
                "logs" | "packs-folder" => {
                    let dir = root.join(if action.action == "logs" {
                        "logs"
                    } else {
                        "packs"
                    });
                    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let _ = Command::new("explorer.exe").arg(dir).spawn();
                }
                "refresh" | "track" if !state.busy => {
                    if action.action == "track" {
                        let track = match action.value.as_str() {
                            "stable" => Track::Stable,
                            "dev" => Track::Dev,
                            _ => continue,
                        };
                        let previous = preferences.track;
                        preferences.track = track;
                        if let Err(error) = preferences.save(&root) {
                            preferences.track = previous;
                            state.status = format!("Could not save update track: {error}");
                            fs::write(
                                &state_path,
                                serde_json::to_vec(&state).map_err(|e| e.to_string())?,
                            )
                            .map_err(|e| e.to_string())?;
                            continue;
                        }
                        state.track = track;
                    }
                    // Auto-apply is disabled: cached packages from a previous
                    // track cannot be applied unless this check selects them.
                    ready = None;
                    state.ready = false;
                    state.notes = notes.clone();
                    packs(&mut state, &root);
                    state.busy = true;
                    state.status = "Checking for updates…".into();
                    state.progress = -1;
                    check(state.track, sender.clone());
                }
                "play" | "apply"
                    if !state.running
                        && !state.installing
                        && (action.action == "play" || !state.busy) =>
                {
                    if let Some((manager, asset)) = &ready {
                        let restart = if action.action == "play" {
                            vec!["launcher", "--launch-after-update"]
                        } else {
                            vec!["launcher"]
                        };
                        match manager.wait_exit_then_apply_updates(asset, false, true, restart) {
                            Ok(()) => {
                                let _ = window.kill();
                                let _ = window.wait();
                                return Ok(());
                            }
                            Err(e) => state.status = format!("Could not apply update: {e}"),
                        }
                    } else if action.action == "play" {
                        match launch(&root, state.track) {
                            Ok(child) => {
                                game = Some(child);
                                state.running = true;
                                state.status = "Game is starting…".into();
                            }
                            Err(e) => state.status = e,
                        }
                    }
                }
                "install" if !state.busy && !state.running => {
                    job = Some(packs::InstallJob::start(
                        packs::Installed::new(&root),
                        PathBuf::from(action.value),
                    ));
                    state.busy = true;
                    state.installing = true;
                }
                "remove" if !state.busy && !state.running => {
                    state.status = match packs::Installed::new(&root).remove(&action.value) {
                        Ok(true) => "Content pack removed.".into(),
                        Ok(false) => "That pack is no longer installed.".into(),
                        Err(e) => e,
                    };
                    packs(&mut state, &root);
                }
                _ => {}
            }
        }
        if dirty {
            fs::write(
                &state_path,
                serde_json::to_vec(&state).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
    }
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
