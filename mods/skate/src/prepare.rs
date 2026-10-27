// SPDX-License-Identifier: GPL-3.0-only
//! Read-only conversion from an owner's prepared Skate installation.
//! No disc executables, original code, skater bodies or map data are copied.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use glam::{Mat4, Vec3};
use serde_json::json;
use skate_data::collections::Collections;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn curve(data: &Collections, class: &str, key: &str, name: &str) -> Result<Vec<f32>, String> {
    let field = data.field(class, key, name)?;
    let values = match field.data.len() {
        128 => data
            .words::<16>(class, key, name)?
            .into_iter()
            .map(f32::from_bits)
            .collect(),
        160 => data.words::<20>(class, key, name)?[4..]
            .iter()
            .copied()
            .map(f32::from_bits)
            .collect(),
        256 => data
            .words::<32>(class, key, name)?
            .into_iter()
            .map(f32::from_bits)
            .collect(),
        _ => return Err(format!("unsupported curve {class}/{name}")),
    };
    Ok(values)
}

fn configuration(assets: &Path) -> Result<Vec<f32>, String> {
    let data = Collections::load(assets)?;
    let mut config = vec![0.; mechanics_api::CONFIG_WORDS];
    let f = |class, name| data.float(class, "default", name);
    let m = |name| data.float("physics_mode", "normal", name);
    config[0] = f32::from_bits(data.words::<4>("physics", "default", "WorldGravity")?[1]).abs();
    config[1] = f("physics_push", "MaxPushableSpeed")?;
    config[2] = m("MaxPushDVStart")?;
    config[3] = m("MaxPushDVEnd")?;
    config[4] = f("physics_brakes", "FootBrakeForce")?;
    config[5] = f("physics_brakes", "TailBrakeForce")?;
    config[6] = f("physics_brakes", "SlowSpeed")?;
    let factor = f("physics_world", "SkateboardMassFactor")?;
    config[7] = (f("physicsdeck", "DeckMass")?
        + 2. * f("physicstrucks", "TruckMass")?
        + 4. * f("physicswheels", "WheelMass")?)
        * factor;
    config[8] = data.float("physics_surfaces", "default", "BrakingScalar")?;
    config[9] = f("physicsdeck", "DeckMidLength")?
        + f("physicstrucks", "TruckZPosFront")?
        + f("physicstrucks", "TruckZPosBack")?;
    config[11] = f("physics_steering", "GeneralScalar")?;
    for (at, name) in [
        (12, "HardTurnIncrease"),
        (13, "Damping"),
        (14, "SteeringVsSpeedMaxSpeed"),
        (15, "PushScalarIncriment"),
        (16, "PushScalarDecriment"),
        (17, "PushScalarMin"),
        (19, "SteeringScalar_TightTrucks"),
        (20, "SteeringTiltBlending"),
    ] {
        config[at] = f("physics_steering", name)?;
    }
    config[18] = f("physics_manual", "Tilt_TiltScalar")?;
    for (at, class, name) in [
        (24, "physics_steering", "SteeringVsSpeed"),
        (40, "physics_steering", "SteeringVsInput"),
        (56, "physics_surfaces", "FrictionVsSpeedNew"),
        (84, "physics_jump", "VerticalResponse"),
        (116, "physics_jump", "JumpYScalarVsGroundNormalY"),
        (132, "physics_jump", "JumpSpeedScalarVsAngle"),
        (148, "physics_jump", "MinHeightVsSpeed"),
        (164, "physics_jump", "MaxHeightVsSpeed"),
    ] {
        let values = curve(&data, class, "default", name)?;
        config[at..at + values.len()].copy_from_slice(&values);
    }
    config[72] = m("Hash_BE3F74F978D777E5")?;
    config[73] = m("JumpMinHeight")?;
    config[74] = m("JumpMaxHeight")?;
    for (at, name) in [
        (75, "SpeedResponseMaxSpeed"),
        (76, "MinScalar"),
        (77, "JumpYBonusMax"),
        (78, "JumpAdjustZFactor"),
        (79, "JumpAdjustXFactor"),
        (80, "AbsoluteMinHeight"),
        (81, "HippyJumpMinHeight"),
        (82, "HippyJumpMaxHeight"),
    ] {
        config[at] = f("physics_jump", name)?;
    }
    if config.iter().any(|v| !v.is_finite()) || config[0] <= 0. || config[9] <= 0. {
        return Err("invalid source physics settings".into());
    }
    Ok(config)
}

fn board(assets: &Path) -> Result<serde_json::Value, String> {
    let bytes = fs::read(assets.join("private/skater.glb")).map_err(|e| e.to_string())?;
    let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
    let blob = gltf
        .blob
        .as_deref()
        .ok_or("converted skater needs embedded geometry")?;
    let skin = gltf
        .skins()
        .next()
        .ok_or("converted skater has no skeleton")?;
    let joints: Vec<_> = skin.joints().collect();
    let board_joint = joints
        .iter()
        .position(|j| j.name() == Some("SKATEBOARD_ROOT"))
        .ok_or("skateboard joint missing")?;
    let board_joints: Vec<usize> = joints
        .iter()
        .enumerate()
        .filter(|(_, j)| {
            j.name().is_some_and(|n| {
                n == "SKATEBOARD_ROOT" || n.starts_with("TRUCK_") || n.contains("WHEEL")
            })
        })
        .map(|(i, _)| i)
        .collect();
    let binds: Vec<_> = skin
        .reader(|_| Some(blob))
        .read_inverse_bind_matrices()
        .ok_or("inverse bind matrices missing")?
        .collect();
    let board_from_mesh = Mat4::from_cols_array_2d(&binds[board_joint]);
    let mut parts = Vec::new();
    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = primitive.reader(|_| Some(blob));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("missing mesh positions")?
                .collect();
            let Some(weights) = reader.read_weights(0) else {
                continue;
            };
            let Some(indices_joints) = reader.read_joints(0) else {
                continue;
            };
            let board_vertex: Vec<bool> = weights
                .into_f32()
                .zip(indices_joints.into_u16())
                .map(|(w, j)| {
                    w.into_iter()
                        .zip(j)
                        .filter(|(_, j)| board_joints.contains(&(*j as usize)))
                        .map(|(w, _)| w)
                        .sum::<f32>()
                        > 0.5
                })
                .collect();
            let normals: Vec<_> = reader
                .read_normals()
                .ok_or("missing mesh normals")?
                .collect();
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|i| i.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            let mut remap = std::collections::BTreeMap::<u32, u32>::new();
            let mut out_pos = Vec::<[f32; 3]>::new();
            let mut out_normal = Vec::<[f32; 3]>::new();
            let mut out_indices = Vec::new();
            for triangle in indices.chunks_exact(3) {
                if !triangle.iter().all(|i| board_vertex[*i as usize]) {
                    continue;
                }
                for &i in triangle {
                    let next = remap.len() as u32;
                    let index = *remap.entry(i).or_insert_with(|| {
                        let v = board_from_mesh
                            .transform_point3(Vec3::from_array(positions[i as usize]));
                        let n = board_from_mesh
                            .transform_vector3(Vec3::from_array(normals[i as usize]))
                            .normalize_or_zero();
                        // The converted SKATEBOARD_ROOT bind frame has +Y
                        // along the deck and -Z above it (not the physics
                        // board's +Z-forward/+Y-up frame).
                        out_pos.push([v.y / 0.0254, v.x / 0.0254, -v.z / 0.0254]);
                        out_normal.push([n.y, n.x, -n.z]);
                        next
                    });
                    out_indices.push(index);
                }
            }
            if !out_indices.is_empty() {
                parts.push(
                    json!({"positions":out_pos,"normals":out_normal,"indices":out_indices,
                    "color":primitive.material().pbr_metallic_roughness().base_color_factor()}),
                );
            }
        }
    }
    if parts.is_empty() {
        return Err("no board geometry found in the converted skater".into());
    }
    let floor = parts
        .iter()
        .flat_map(|p| p["positions"].as_array().unwrap())
        .map(|v| v[2].as_f64().unwrap())
        .fold(f64::INFINITY, f64::min);
    for part in &mut parts {
        for vertex in part["positions"].as_array_mut().unwrap() {
            vertex[2] = json!(vertex[2].as_f64().unwrap() - floor);
        }
    }
    Ok(json!({"parts":parts}))
}

fn manifest() -> String {
    format!(
        r#"format_version = 2
id = "skate3_riding"
name = "Skate 3 crossover"
version = "0.1.0"
origin = "skate3"
kind = "assets"
min_game_version = "0.5.0"
provides = ["skate3_mode_riding", "skate3_model_board"]
description = "Ride CS maps with CS characters and weapons. Press X to switch movement modes."
content_hash = "sha256:{}"

[[mechanics]]
id = "skate3_mode_riding"
name = "Skate 3"
module = "mechanics/skate.wasm"
api_version = 1
config = "mechanics/skate.csrconfig"
model = "models/board.csrmesh"

[[resources]]
id = "skate3_model_board"
path = "models/board.csrmesh"
"#,
        "0".repeat(64)
    )
}

fn payload(assets: &Path, module: Vec<u8>) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let mut config = configuration(assets)?;
    let model = board(assets)?;
    let top = model["parts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["positions"].as_array().unwrap())
        .map(|v| v[2].as_f64().unwrap() as f32)
        .fold(0f32, f32::max);
    config[180] = top.min(12.);
    Ok(vec![
        ("mechanics/skate.wasm", module),
        (
            "mechanics/skate.csrconfig",
            serde_json::to_vec(&config).unwrap(),
        ),
        ("models/board.csrmesh", serde_json::to_vec(&model).unwrap()),
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
    let files = payload(
        &assets,
        include_bytes!("../target/wasm32-unknown-unknown/release/csr_skate.wasm").to_vec(),
    )?;
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
    if args.len() != 3 {
        return Err("Usage: prepare-skate-pack <converted assets folder> <csr_skate.wasm> <new private output folder>".into());
    }
    let assets = PathBuf::from(&args[0]);
    let wasm = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let parent = out
        .parent()
        .ok_or("output needs a parent folder")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if parent.starts_with(&repo) || out.exists() {
        return Err("choose a NEW output folder outside the CS repository".into());
    }
    // Read/convert everything before publishing the output folder.
    let module = fs::read(wasm).map_err(|e| e.to_string())?;
    let files = payload(&assets, module)?;
    fs::create_dir(&out).map_err(|e| e.to_string())?;
    for dir in ["mechanics", "models", "licenses"] {
        fs::create_dir(out.join(dir)).map_err(|e| e.to_string())?;
    }
    for (path, bytes) in files {
        fs::write(out.join(path), bytes).map_err(|e| e.to_string())?;
    }
    fs::write(out.join("pack.toml"), manifest()).map_err(|e| e.to_string())?;
    println!(
        "Prepared private Skate pack in {}. Original files were read only.",
        out.display()
    );
    Ok(())
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
