//! Friend-facing file opening; the frontend uses the same InstallJob for drops.
use crate::{dialog, presence::Game};
use std::path::{Path, PathBuf};

pub fn is_pack(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case(packs::EXTENSION))
}

pub fn request(args: &[String]) -> Option<Result<PathBuf, String>> {
    match args.first().map(String::as_str) {
        Some("--open-pack") => Some(if args.len() == 2 {
            Ok(PathBuf::from(&args[1]))
        } else {
            Err("Choose one .csrpack file to open.".into())
        }),
        Some(path) if is_pack(path) => Some(if args.len() == 1 {
            Ok(PathBuf::from(path))
        } else {
            Err("Choose one .csrpack file to open.".into())
        }),
        _ => None,
    }
}

pub(crate) fn status(progress: packs::Progress) -> String {
    use packs::Stage::*;
    let text = match progress.stage {
        Reading => "Reading the pack",
        Validating => "Checking the pack",
        Waiting => "Waiting for another pack to finish",
        CheckingInstalled => "Checking installed content",
        Writing => "Installing the pack",
        Publishing => "Finishing installation",
    };
    if matches!(progress.stage, Reading | Writing) && progress.total > 0 {
        format!(
            "{text}… {}%",
            progress
                .bytes
                .saturating_mul(100)
                .checked_div(progress.total)
                .unwrap_or(0)
                .min(100)
        )
    } else {
        format!("{text}…")
    }
}

/// Success feedback follows committed installation, including when an existing
/// game means this launcher exits. The notification helper stays alive to show
/// its message independently of this process. Errors remain visible in a dialog.
pub fn run(source: PathBuf, data: &Path) -> Result<Option<Game>, String> {
    let mut window = dialog::Waiting::progress("Install content pack", "Checking the pack…");
    let mut job = packs::InstallJob::start(packs::Installed::new(data), source);
    let mut last = None;
    let outcome = loop {
        if window.dismissed() {
            job.cancel();
        }
        let progress = job.progress();
        if last != Some(progress) {
            window.status(&status(progress));
            last = Some(progress);
        }
        if let Some(outcome) = job.poll() {
            break outcome;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    // Wait for the terminal outcome before closing: pending-file cleanup has
    // completed, and a publish that beat Cancel must still report success.
    window.close();
    match outcome {
        packs::Outcome::Installed(receipt) => {
            let game = Game::launch_if_absent(data).map_err(|e| {
                format!("The pack was installed, but the game couldn't be opened: {e}")
            })?;
            dialog::notify(
                "Content pack installed",
                &format!(
                    "{} {} was installed.",
                    receipt.manifest.name, receipt.manifest.version
                ),
            );
            Ok(game)
        }
        packs::Outcome::Cancelled => Ok(None),
        packs::Outcome::Failed(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_open_preserves_one_literal_path_and_rejects_extra_arguments() {
        let path = "friend's & $special pack.CSRPACK";
        assert_eq!(
            request(&[path.into()]).unwrap().unwrap(),
            PathBuf::from(path)
        );
        assert_eq!(
            request(&["--open-pack".into(), path.into()])
                .unwrap()
                .unwrap(),
            PathBuf::from(path)
        );
        assert!(request(&["--open-pack".into()]).unwrap().is_err());
        assert!(request(&[path.into(), "play".into()]).unwrap().is_err());
        assert!(request(&["play".into(), "de_dust2".into()]).is_none());
    }
}
