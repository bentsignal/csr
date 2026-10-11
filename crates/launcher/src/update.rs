//! Installer integration and self-update (Velopack).
//!
//! `Setup.exe` from a GitHub Release installs the game per user into
//! `%LocalAppData%\CSGORewrite` (`current\` holds the exe and is replaced by
//! each update). At every launch the installed game checks GitHub Releases in
//! the background, downloads a newer version (a small delta when possible)
//! and shows a notification; Velopack applies it the next time the game
//! starts. Startup never waits on the network. Source and portable builds
//! skip all of this.
//!
//! `CSR_NO_UPDATE` turns the check off. `CSR_UPDATE_URL` points it at another
//! feed (a GitHub repository, a web folder or a local `Releases` folder from
//! `vpk pack`), and `CSR_UPDATE_PRERELEASE=1` includes GitHub pre-releases;
//! both are for testing releases.

use std::path::PathBuf;

use velopack::locator::{LocationContext, VelopackLocator, auto_locate_app_manifest};
use velopack::sources::{AutoSource, GithubSource, UpdateSource};
use velopack::{UpdateCheck, UpdateManager, VelopackApp};

pub const REPO_URL: &str = "https://github.com/bentsignal/global-offensive";

/// Must run first in `main`: handles the installer's hooks (it may exit
/// the process) and applies an update downloaded by an earlier run.
pub fn startup() {
    VelopackApp::build()
        .on_before_uninstall_fast_callback(|_| crate::steam::remove_everywhere())
        .on_restarted(|version| crate::dialog::notify("CSGO Rewrite updated", &format!("You are now on version {version}.")))
        .run();
}

fn locator() -> Option<VelopackLocator> {
    auto_locate_app_manifest(LocationContext::FromCurrentExe).ok().filter(|l| !l.get_is_portable())
}

/// The install folder (`%LocalAppData%\CSGORewrite`) if this copy was
/// installed with Setup.exe.
pub fn installed_root() -> Option<PathBuf> {
    locator().map(|l| l.get_root_dir())
}

/// The exe that shortcuts should start: Velopack's launcher in the install
/// folder when there is one (its path never changes), else `current\`.
pub fn launch_exe() -> Option<PathBuf> {
    let l = locator()?;
    let main = l.get_main_exe_path();
    let stub = l.get_root_dir().join(main.file_name()?);
    Some(if stub.is_file() { stub } else { main })
}

fn source() -> Box<dyn UpdateSource> {
    let prerelease = std::env::var("CSR_UPDATE_PRERELEASE").is_ok_and(|v| v == "1");
    match std::env::var("CSR_UPDATE_URL") {
        Ok(url) if url.to_ascii_lowercase().starts_with("https://github.com/") => Box::new(GithubSource::new(&url, None, prerelease)),
        Ok(url) => Box::new(AutoSource::new(&url)),
        Err(_) => Box::new(GithubSource::new(REPO_URL, None, prerelease)),
    }
}

/// Starts the background update check (installed copies only).
pub fn check_in_background() {
    if std::env::var_os("CSR_NO_UPDATE").is_some() {
        return;
    }
    let Ok(manager) = UpdateManager::new_boxed(source(), None, None) else { return };
    let _ = std::thread::Builder::new().name("update-check".into()).spawn(move || match download(&manager) {
        Ok(Some(version)) => crate::dialog::notify(
            "CSGO Rewrite update ready",
            &format!("Version {version} has been downloaded. It installs the next time you start the game."),
        ),
        Ok(None) => eprintln!("CSGO Rewrite {} is up to date.", manager.get_current_version_as_string()),
        Err(e) => eprintln!("Update check failed (the game works offline): {e}"),
    });
}

/// Downloads the newest release if there is one; `Some(version)` once it is
/// ready to apply.
fn download(manager: &UpdateManager) -> Result<Option<String>, velopack::Error> {
    if manager.get_update_pending_restart().is_some() {
        return Ok(None);
    }
    match manager.check_for_updates()? {
        UpdateCheck::UpdateAvailable(update) => {
            eprintln!("Downloading CSGO Rewrite {}...", update.TargetFullRelease.Version);
            manager.download_updates(&update, None)?;
            Ok(Some(update.TargetFullRelease.Version.clone()))
        }
        UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(None),
    }
}
