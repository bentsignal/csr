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
version = "0.5.0"
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

/// The AEMS banks the native board/player audio loads (the public rewrite's
/// `player_audio::BANKS` and `OPTIONAL_BANKS` plus its two boot utilities).
const AUDIO_BANKS: &[&str] = &[
    "emitter_utility",
    "Common",
    "GRINDS",
    "sense_of_speed",
    "FOOT_DRAG",
    "WHEEL_SKID_BANK",
    "Brd_Squeaks",
    "Seams_Bank",
    "fstep_skateshoe1_sm",
    "Foley_Cloth",
    "Bodyslide",
    "PatchBank_Rolling_Surfaces",
    "PatchBank_SpiderCracks",
    "PatchBank_Objects",
    "PatchBank_RocksBounce",
    "Rolling_Rattles",
    "board_scrapes",
    "Sk8_Air_Flip_Tricks",
    "Treatments",
];
/// Splice one-shot trees: board contacts, metal collisions, shoe foley
/// (`player_audio::SPLICE_BANKS`); the front-end bank is added from the manifest.
const SPLICE_BANKS: &[&str] = &["Skate_Collisions", "Skate_Metal", "sk8_foley"];

/// The owner's decoded board audio in its install layout with a trimmed
/// manifest: only the skater's banks, grains, wheel spins and their tuning.
/// No speech, music, ambience, world emitters or unrelated banks are copied.
fn audio(assets: &Path, data: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    use serde_json::{Map, Value, json};
    let root = assets.join("private/audio");
    let refresh = |what: &str| {
        format!("Owned Skate audio {what} is missing. Refresh the converted Skate installation first.")
    };
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("audio_manifest.json")).map_err(|_| refresh("manifest"))?,
    )
    .map_err(|e| e.to_string())?;
    if manifest["version"].as_u64() != Some(5) {
        return Err(refresh("(native AEMS export)"));
    }
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let aems = &manifest["aems"];
    let projects: Vec<String> = aems["projects"]
        .as_array()
        .ok_or_else(|| refresh("AEMS project list"))?
        .iter()
        .filter_map(text)
        .collect();
    let mixmap = text(&aems["mixmap"]).ok_or_else(|| refresh("MixMap"))?;
    let frontend = manifest["frontend"].clone();
    let frontend_bank = text(&frontend["bank"]).ok_or_else(|| refresh("front-end sounds"))?;
    let mut abk = Map::new();
    for stem in AUDIO_BANKS {
        abk.insert((*stem).into(), Value::String(text(&aems["banks"][*stem]).ok_or_else(|| refresh(stem))?));
    }
    let mut splice = Map::new();
    for stem in SPLICE_BANKS.iter().copied().chain([frontend_bank.as_str()]) {
        splice.insert(stem.into(), Value::String(text(&aems["splice"][stem]).ok_or_else(|| refresh(stem))?));
    }
    let entries = |v: &Value| -> Vec<Value> {
        v.as_array()
            .map(|a| a.iter().filter_map(|e| text(&e["file"])).map(|f| json!({ "file": f })).collect())
            .unwrap_or_default()
    };
    let mut banks = Map::new();
    for stem in AUDIO_BANKS.iter().copied().chain(SPLICE_BANKS.iter().copied()).chain([frontend_bank.as_str()]) {
        banks.insert(stem.into(), Value::Array(entries(&manifest["banks"][stem])));
    }
    let mut grains = Map::new();
    for (name, g) in manifest["grains"].as_object().ok_or_else(|| refresh("grains"))? {
        if let (Some(file), Some(grain)) = (text(&g["file"]), text(&g["grain"])) {
            grains.insert(name.clone(), json!({ "file": file, "grain": grain }));
        }
    }
    let mut wheels = Map::new();
    for name in ["Whls_spins_Jump_1", "Whls_spins_Man_1"] {
        let file = text(&manifest["wheels"][name]["file"]).ok_or_else(|| refresh(name))?;
        wheels.insert(name.into(), json!({ "file": file }));
    }
    let trimmed = json!({
        "version": 5,
        "grains": grains,
        "wheels": wheels,
        "banks": banks,
        "aems": { "projects": projects, "banks": abk, "mixmap": mixmap, "splice": splice },
        "grain_player": manifest["grain_player"],
        "player_tuning": manifest["player_tuning"],
        "bus_tuning": manifest["bus_tuning"],
        "frontend": frontend,
    });
    let mut files = std::collections::BTreeSet::new();
    files.extend(projects.iter().cloned());
    files.insert(mixmap);
    files.extend(abk.values().chain(splice.values()).filter_map(text));
    for list in banks.values() {
        files.extend(list.as_array().into_iter().flatten().filter_map(|e| text(&e["file"])));
    }
    for g in grains.values().chain(wheels.values()) {
        files.extend(text(&g["file"]));
        files.extend(text(&g["grain"]));
    }
    for file in files {
        let path = root.join(&file);
        let bytes = fs::read(&path).map_err(|e| format!("Owned Skate audio {}: {e}. Refresh the converted Skate installation first.", path.display()))?;
        data.push((format!("private/audio/{file}"), bytes));
    }
    data.push((
        "private/audio/audio_manifest.json".into(),
        serde_json::to_vec(&trimmed).map_err(|e| e.to_string())?,
    ));
    Ok(())
}

fn payload(assets: &Path) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let mut data = Vec::new();
    audio(assets, &mut data)?;
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
    let hud: serde_json::Value = serde_json::from_slice(
        &fs::read(assets.join("private/hud/runtime/trickdisplay.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
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
