// SPDX-License-Identifier: GPL-3.0-only
//! Read-only conversion from an owner's prepared Skate installation.
//! No disc executables, original code, skater bodies or map data are copied.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
};

fn manifest() -> String {
    format!(
        r#"format_version = 2
id = "skate3_riding"
name = "Skate 3 crossover"
version = "0.4.0"
origin = "skate3"
kind = "assets"
min_game_version = "0.5.0"
provides = ["skate3_mode_riding"]
description = "Full Skate simulation, animations and Xbox controls on CS maps. View or X switches modes."
content_hash = "sha256:{}"

[[mechanics]]
id = "skate3_mode_riding"
name = "Skate 3"
module = "mechanics/skate.csrdata"
api_version = 2
provider = "skate3-v2"
config = "mechanics/skate.csrconfig"
"#,
        "0".repeat(64)
    )
}

fn payload(assets: &Path) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let mut data = Vec::new();
    let manifest:serde_json::Value=serde_json::from_slice(&fs::read(assets.join("private/audio/audio_manifest.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let mut tuning=serde_json::Map::new();
    for name in ["concrete_smooth_hard","concrete_rough_hard","asphalt_rough_hard","wood_ramp_hard","metal_smooth_hard"] {
        let entry=manifest["grain_player"]["surfaces"][name].clone();
        if entry.is_null(){return Err(format!("missing original grain tuning {name}"));}
        tuning.insert(name.into(),entry);
        data.push((format!("private/crossover-audio/{name}.wav"),fs::read(assets.join(format!("private/audio/grains/{name}.wav"))).map_err(|e|e.to_string())?));
    }
    data.push(("private/crossover-audio/grains.json".into(),serde_json::to_vec(&tuning).map_err(|e|e.to_string())?));
    // Read only the owner's decoded board foley. No original executable,
    // program, speech/music, unrelated banks or map audio is copied.
    for (name,relative) in [
        ("roll","grains/concrete_smooth_hard/2.wav"),
        ("pop","banks/Skate_Collisions/0000.wav"),
        ("land","banks/Skate_Collisions/0001.wav"),
        ("push","banks/sk8_foley/0000.wav"),
        ("grind","banks/GRINDS/0000.wav"),
        ("brake","banks/FOOT_DRAG/0000.wav"),
        ("marker-open","banks/sk8_menu/0058.wav"),
        ("marker-set","banks/sk8_menu/0059.wav"),
        ("marker-return","banks/sk8_menu/0036.wav"),
    ] {
        let path=assets.join("private/audio").join(relative);
        let bytes=fs::read(&path).map_err(|e|format!("Owned board sound {}: {e}. Refresh the converted Skate installation first.",path.display()))?;
        if bytes.len()>4*1024*1024 || !bytes.starts_with(b"RIFF") || bytes.get(8..12)!=Some(b"WAVE") {return Err("invalid or oversized decoded Skate sound".into());}
        data.push((format!("private/crossover-audio/{name}.wav"),bytes));
    }
    for path in [
        "private/game.json",
        "private/skater.glb",
        "private/stock/skater-collections.json",
        "private/stock/physics-skeletons.json",
        "private/stock/data/anim/OnBoard.abin",
        "private/stock/data/anim/OffBoard.abin",
    ] {
        data.push((
            path.to_string(),
            fs::read(assets.join(path)).map_err(|e| format!("{path}: {e}"))?,
        ));
    }
    fn collect(
        root: &Path,
        relative: &str,
        data: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(root.join(relative)).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = format!("{relative}/{}", entry.file_name().to_string_lossy());
            if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
                return Err("symlink in prepared data".into());
            }
            if entry.path().is_dir() {
                collect(root, &path, data)?;
            } else if ["stategraph", "pat", "shk"].contains(
                &entry
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or(""),
            ) {
                data.push((path, fs::read(entry.path()).map_err(|e| e.to_string())?));
            }
        }
        Ok(())
    }
    for directory in [
        "private/stock/data/state",
        "private/stock/data/script",
        "private/stock/data/joystick",
        "private/stock/data/camera",
    ] {
        collect(assets, directory, &mut data)?;
    }
    let hud_bytes = fs::read(assets.join("private/hud/runtime/trickdisplay.json"))
        .map_err(|e| e.to_string())?;
    let hud: serde_json::Value = serde_json::from_slice(&hud_bytes).map_err(|e| e.to_string())?;
    hud_display(assets, &mut data)?;
    data.push(("private/hud/runtime/trickdisplay.json".into(), hud_bytes));
    let language = hud["language"]
        .as_object()
        .ok_or("converted trick language table missing")?;
    let language: std::collections::BTreeMap<_, _> = language
        .iter()
        .filter(|(key, _)| key.starts_with("ID_TRICK_") || key.starts_with("ID_STANCE"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    data.push((
        "private/trick-language.json".into(),
        serde_json::to_vec(&language).map_err(|e| e.to_string())?,
    ));
    data.sort_by(|a, b| a.0.cmp(&b.0));
    let bundle = mechanics::data::build(&data)?;
    Ok(vec![
        ("mechanics/skate.csrdata", bundle),
        (
            "mechanics/skate.csrconfig",
            serde_json::to_vec(&vec![0f32; 256]).unwrap(),
        ),
        ("licenses/gpl3.txt", include_bytes!("../LICENSE").to_vec()),
        ("licenses/notice.txt", include_bytes!("../NOTICE").to_vec()),
    ])
}

/// The original trick display's font atlases and bitmaps, and the converted
/// session-marker display list. Only straight RGBA payloads and their JSON
/// descriptions are read; no movie bytecode files or other screens.
fn hud_display(assets: &Path, data: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    fn textures(root: &Path, relative: &str, data: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
        for entry in fs::read_dir(root.join(relative)).map_err(|e| format!("{relative}: {e}"))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = format!("{relative}/{}", entry.file_name().to_string_lossy());
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("symlink in prepared data".into());
            }
            if kind.is_dir() {
                textures(root, &path, data)?;
            } else if path.ends_with(".rgba") {
                data.push((path, fs::read(entry.path()).map_err(|e| e.to_string())?));
            }
        }
        Ok(())
    }
    textures(assets, "private/hud/assets/data/fe", data)?;
    let marker = assets.join("private/session-marker");
    if let Ok(bytes) = fs::read(marker.join("hud.json")) {
        for entry in fs::read_dir(&marker).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().map_err(|e| e.to_string())?.is_file()
                && name.starts_with("texture-")
                && name.ends_with(".rgba")
            {
                data.push((
                    format!("private/session-marker/{name}"),
                    fs::read(entry.path()).map_err(|e| e.to_string())?,
                ));
            }
        }
        data.push(("private/session-marker/hud.json".into(), bytes));
    }
    Ok(())
}

fn assets_from_game(game: &Path) -> Result<PathBuf, String> {
    let root = game
        .parent()
        .ok_or("select skate3rust.exe from your prepared installation")?;
    if root
        .join("assets/private/stock/skater-collections.json")
        .is_file()
    {
        return Ok(root.join("assets"));
    }
    let data = root
        .join("data")
        .canonicalize()
        .map_err(|_| "Run the Skate rewrite's setup with your own game copy first.")?;
    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(data.join("installation.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let directory = Path::new(
        index["directory"]
            .as_str()
            .ok_or("prepared installation directory missing")?,
    );
    if directory.is_absolute()
        || directory
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err("invalid prepared installation path".into());
    }
    let assets = data
        .join(directory)
        .join("assets")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !assets.starts_with(data) {
        return Err("prepared assets must remain in the Skate installation".into());
    }
    Ok(assets)
}

fn create(game: &Path, output: &Path) -> Result<(), String> {
    let assets = assets_from_game(game)?;
    let files = payload(&assets)?;
    let manifest = packs::Manifest::parse(&manifest())?;
    let files: Vec<_> = files.iter().map(|(p, b)| (*p, b.as_slice())).collect();
    let bytes = packs::build(&manifest, &files)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn gui() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let response = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-STA",
            "-Command",
            include_str!("../picker.ps1"),
        ])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| e.to_string())?;
    if !response.status.success() {
        return Err(String::from_utf8_lossy(&response.stderr).trim().into());
    }
    let text = String::from_utf8_lossy(&response.stdout);
    if text.trim().is_empty() {
        return Ok(());
    }
    let selected: Vec<String> = serde_json::from_str(text.trim()).map_err(|e| e.to_string())?;
    if selected.len() != 2 {
        return Err("invalid file selection".into());
    }
    create(Path::new(&selected[0]), Path::new(&selected[1]))?;
    message(
        "Your Skate pack is ready. Open the .csrpack file to install it, then load any CS map and press X to skate.",
    );
    Ok(())
}
#[cfg(not(windows))]
fn gui() -> Result<(), String> {
    Err("Use --create <skate3rust.exe> <new output.csrpack> on this platform.".into())
}

fn message(text: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut child = Command::new("powershell.exe").args(["-NoProfile","-STA","-Command",
            "Add-Type -AssemblyName System.Windows.Forms; $text=[Console]::In.ReadToEnd(); [void][Windows.Forms.MessageBox]::Show($text,'Skate pack creator')"])
            .creation_flags(0x0800_0000).stdin(Stdio::piped()).spawn();
        if let Ok(child) = &mut child {
            if let Some(mut input) = child.stdin.take() {
                let _ = input.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }
    #[cfg(not(windows))]
    eprintln!("{text}");
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        return gui();
    }
    if args.len() == 3 && args[0] == "--create" {
        return create(Path::new(&args[1]), Path::new(&args[2]));
    }
    Err("Usage: prepare-skate-pack --create <skate3rust.exe> <new output.csrpack>".into())
}
fn main() {
    if let Err(error) = run() {
        if std::env::args_os().len() == 1 {
            message(&error);
        } else {
            eprintln!("{error}");
        }
        std::process::exit(1);
    }
}
