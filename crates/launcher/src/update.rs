//! Installer integration and self-update (Velopack).
//!
//! `Setup.exe` from a GitHub Release installs the game per user into
//! `%LocalAppData%\CSGORewrite` (`current\` holds the exe and is replaced by
//! each update). The desktop launcher checks the saved Stable/Dev track,
//! downloads an update (a delta when possible), and applies it before Play.
//! Source/portable builds and explicit game/automation commands skip updates.
//!
//! `CSR_NO_UPDATE` turns the check off. `CSR_UPDATE_URL` points it at another
//! feed (a GitHub repository, a web folder or a local `Releases` folder from
//! `vpk pack`), and `CSR_UPDATE_PRERELEASE=1` includes GitHub pre-releases;
//! both are for testing releases.

use std::path::PathBuf;

use velopack::locator::{LocationContext, VelopackLocator, auto_locate_app_manifest};
use velopack::sources::{AutoSource, GithubSource, HttpSource, UpdateSource};
use velopack::{UpdateManager, UpdateOptions, VelopackApp};

pub const REPO_URL: &str = "https://github.com/bentsignal/csr";

/// Must run first in `main`: handles the installer's hooks (it may exit
/// the process) and applies an update downloaded by an earlier run.
pub fn startup() {
    VelopackApp::build()
        // Updates are applied explicitly after the launcher checks game presence.
        .set_auto_apply_on_startup(false)
        .on_after_install_fast_callback(|_| crate::fileassoc::register())
        .on_after_update_fast_callback(|_| crate::fileassoc::register())
        .on_before_uninstall_fast_callback(|_| {
            crate::fileassoc::remove();
            crate::steam::remove_everywhere();
        })
        .on_restarted(|version| {
            crate::dialog::notify(
                "Counter-Strike Rust updated",
                &format!("You are now on version {version}."),
            )
        })
        .run();
}

fn locator() -> Option<VelopackLocator> {
    auto_locate_app_manifest(LocationContext::FromCurrentExe)
        .ok()
        .filter(|l| !l.get_is_portable())
}

/// The install folder (`%LocalAppData%\CSGORewrite`) if this copy was
/// installed with Setup.exe.
pub fn installed_root() -> Option<PathBuf> {
    locator().map(|l| l.get_root_dir())
}

/// The exe that shortcuts should start: `current\CSGO-Rewrite.exe`, the
/// same target as the Start menu shortcut Velopack makes (updates replace
/// the folder's contents, never its path).
pub fn launch_exe() -> Option<PathBuf> {
    locator().map(|l| l.get_main_exe_path())
}

fn source(dev: bool) -> Box<dyn UpdateSource> {
    let prerelease = dev || std::env::var("CSR_UPDATE_PRERELEASE").is_ok_and(|v| v == "1");
    match std::env::var("CSR_UPDATE_URL") {
        Ok(url) if url.to_ascii_lowercase().starts_with("https://github.com/") => {
            Box::new(GithubSource::new(&url, None, prerelease))
        }
        Ok(url) => Box::new(AutoSource::new(&url)),
        Err(_) if prerelease => Box::new(GithubSource::new(REPO_URL, None, true)),
        // /latest remains Stable even after hundreds of Dev releases. The
        // SDK's GitHub source only searches its ten most recent releases.
        Err(_) => Box::new(HttpSource::new(&format!(
            "{REPO_URL}/releases/latest/download"
        ))),
    }
}

pub fn manager(dev: bool) -> Result<UpdateManager, String> {
    if std::env::var_os("CSR_NO_UPDATE").is_some() {
        return Err("Update checks are off for this session (CSR_NO_UPDATE).".into());
    }
    if installed_root().is_none() {
        return Err("Source and portable copies do not update automatically.".into());
    }
    UpdateManager::new_boxed(source(dev), Some(options()), None).map_err(|e| e.to_string())
}

pub(crate) fn options() -> UpdateOptions {
    UpdateOptions {
        AllowVersionDowngrade: true,
        ExplicitChannel: Some("win".into()),
        MaximumDeltasBeforeFallback: 10,
    }
}
