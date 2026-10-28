//! Versioned private data bundle. Only the known provider's non-executable
//! inputs are admitted. No paths or program selection come from game scripts.
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
const MAGIC: &[u8; 8] = b"CSRDATA1";
fn allowed(path: &str) -> bool {
    matches!(
        path,
        "private/game.json"
            | "private/skater.glb"
            | "private/trick-language.json"
            | "private/stock/physics-skeletons.json"
            | "private/stock/skater-collections.json"
            | "private/stock/data/anim/OnBoard.abin"
            | "private/stock/data/anim/OffBoard.abin"
            | "private/crossover-audio/roll.wav"
            | "private/crossover-audio/pop.wav"
            | "private/crossover-audio/land.wav"
            | "private/crossover-audio/push.wav"
            | "private/crossover-audio/grind.wav"
            | "private/crossover-audio/brake.wav"
            | "private/crossover-audio/marker-open.wav"
            | "private/crossover-audio/marker-set.wav"
            | "private/crossover-audio/marker-return.wav"
            | "private/crossover-audio/grains.json"
            | "private/crossover-audio/concrete_smooth_hard.wav"
            | "private/crossover-audio/concrete_rough_hard.wav"
            | "private/crossover-audio/asphalt_rough_hard.wav"
            | "private/crossover-audio/wood_ramp_hard.wav"
            | "private/crossover-audio/metal_smooth_hard.wav"
            | "private/hud/runtime/trickdisplay.json"
            | "private/session-marker/hud.json"
    ) || [
        ("private/hud/assets/data/fe/", "rgba"),
        ("private/session-marker/texture-", "rgba"),
        ("private/stock/data/state/", "stategraph"),
        ("private/stock/data/script/", "stategraph"),
        ("private/stock/data/joystick/", "pat"),
        ("private/stock/data/camera/", "shk"),
        ("private/custom/", "json"),
    ]
    .iter()
    .any(|(prefix, extension)| path.starts_with(prefix) && path.ends_with(&format!(".{extension}")))
}
/// A safe relative bundle path that the provider data allowlist admits.
pub fn admitted(path: &str) -> bool {
    path.is_ascii()
        && !path.bytes().any(|b| b < 32 || b"\\:<>\"|?*".contains(&b))
        && !path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with(['.', ' ']))
        && allowed(path)
}
pub fn parse(bytes: &[u8]) -> Result<Vec<(&str, &[u8])>, String> {
    if bytes.len() > 128 << 20 || bytes.get(..8) != Some(MAGIC) {
        return Err("invalid runtime data bundle".into());
    }
    let mut at = 8;
    let mut files = vec![];
    let mut names = BTreeSet::new();
    while at < bytes.len() {
        let size = bytes
            .get(at..at + 6)
            .ok_or("truncated runtime data header")?;
        let pathlen = u16::from_le_bytes(size[..2].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(size[2..].try_into().unwrap()) as usize;
        at += 6;
        if pathlen > 240 || size > 64 << 20 || files.len() >= 2048 {
            return Err("runtime data limit exceeded".into());
        }
        let path = std::str::from_utf8(bytes.get(at..at + pathlen).ok_or("truncated data path")?)
            .map_err(|_| "invalid data path")?;
        at += pathlen;
        if !admitted(path) || !names.insert(path.to_ascii_lowercase()) {
            return Err("unsafe or unsupported runtime data path".into());
        }
        let data = bytes
            .get(at..at + size)
            .ok_or("truncated runtime data file")?;
        at += size;
        if path=="private/crossover-audio/grains.json" {
            if data.len()>64*1024 || serde_json::from_slice::<serde_json::Value>(data).is_err(){return Err("invalid grain tuning data".into());}
        }else if path.starts_with("private/crossover-audio/") && (data.len()>4*1024*1024 || !data.starts_with(b"RIFF") || data.get(8..12)!=Some(b"WAVE")) {
            return Err("invalid or oversized board audio data".into());
        } else if path.ends_with(".rgba") && (data.len() > 16 << 20 || data.len() % 4 != 0) {
            return Err("invalid or oversized HUD texture".into());
        } else if (path.starts_with("private/hud/") || path.starts_with("private/session-marker/"))
            && path.ends_with(".json")
            && (data.len() > 16 << 20
                || serde_json::from_slice::<serde::de::IgnoredAny>(data).is_err())
        {
            return Err("invalid or oversized HUD data".into());
        }
        files.push((path, data));
    }
    for required in [
        "private/game.json",
        "private/skater.glb",
        "private/stock/skater-collections.json",
        "private/stock/physics-skeletons.json",
        "private/stock/data/anim/OnBoard.abin",
        "private/stock/data/anim/OffBoard.abin",
    ] {
        if !files.iter().any(|(name, _)| *name == required) {
            return Err(format!("runtime data missing {required}"));
        }
    }
    Ok(files)
}
pub fn build(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut bytes = MAGIC.to_vec();
    for (path, data) in files {
        bytes.extend((path.len() as u16).to_le_bytes());
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(path.as_bytes());
        bytes.extend(data);
    }
    parse(&bytes)?;
    Ok(bytes)
}
pub fn materialize(bytes: &[u8], cache: &Path) -> Result<PathBuf, String> {
    let files = parse(bytes)?;
    let hash = format!("{:x}", Sha256::digest(bytes));
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let cache = cache.canonicalize().map_err(|e| e.to_string())?;
    let root = cache.join(hash);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !root.starts_with(&cache) {
        return Err("runtime cache leaves data directory".into());
    }
    for (path, data) in files {
        let file = root.join(path);
        let parent = file.parent().ok_or("invalid cache path")?;
        // Check each existing component before creating the next one, so a
        // replaced cache directory cannot redirect even directory creation.
        let mut checked = root.clone();
        for component in parent
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?
            .components()
        {
            checked.push(component);
            if !checked.exists() {
                std::fs::create_dir(&checked).map_err(|e| e.to_string())?;
            }
            if !checked
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&root)
            {
                return Err("runtime cache path leaves bundle".into());
            }
        }
        if file.exists()
            && !file
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&root)
        {
            return Err("runtime cache file leaves bundle".into());
        }
        if std::fs::read(&file).ok().as_deref() != Some(data) {
            std::fs::write(&file, data).map_err(|e| e.to_string())?;
        }
    }
    Ok(root)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn files() -> Vec<(String, Vec<u8>)> {
        [
            "private/game.json",
            "private/skater.glb",
            "private/stock/skater-collections.json",
            "private/stock/physics-skeletons.json",
            "private/stock/data/anim/OnBoard.abin",
            "private/stock/data/anim/OffBoard.abin",
        ]
        .map(|name| (name.into(), b"private test bytes".to_vec()))
        .into()
    }
    #[test]
    fn board_audio_admits_only_bounded_wave_files_at_known_paths() {
        let mut entries=files();
        entries.push(("private/crossover-audio/roll.wav".into(),b"RIFF0000WAVE".to_vec()));
        assert!(build(&entries).is_ok());
        for path in ["private/crossover-audio/code.wav.exe","private/crossover-audio/other.wav","private/crossover-audio/../roll.wav"] {
            let mut bad=files();bad.push((path.into(),b"RIFF0000WAVE".to_vec()));assert!(build(&bad).is_err());
        }
        entries.last_mut().unwrap().1=b"not wave data".to_vec();assert!(build(&entries).is_err());
        entries.last_mut().unwrap().1=vec![0;4*1024*1024+1];assert!(build(&entries).is_err());
    }
    #[test]
    fn original_hud_admits_only_its_display_data_and_rgba_textures() {
        let mut entries = files();
        entries.extend([
            ("private/hud/runtime/trickdisplay.json".into(), b"{}".to_vec()),
            ("private/session-marker/hud.json".into(), b"{\"version\":1}".to_vec()),
            ("private/session-marker/texture-0.rgba".into(), vec![0; 16]),
            (
                "private/hud/assets/data/fe/fonts/futuraheavy/0000_futuraheavy.Texture.rgba".into(),
                vec![0; 64],
            ),
        ]);
        assert!(build(&entries).is_ok());
        for (path, data) in [
            ("private/hud/runtime/other.json", b"{}".to_vec()),
            ("private/hud/assets/data/fe/movie.apt", vec![]),
            ("private/session-marker/texture-0.png", vec![0; 16]),
            ("private/session-marker/texture-1.rgba", vec![0; 15]),
            ("private/session-marker/hud.json", b"not json".to_vec()),
        ] {
            let mut bad = files();
            bad.push((path.into(), data));
            assert!(build(&bad).is_err(), "{path}");
        }
    }
    #[test]
    fn runtime_bundle_rejects_traversal_aliases_programs_and_truncation() {
        for path in [
            "private/../skater.glb",
            "private/custom/../../outside.json",
            "private/custom/a.json:stream",
            "private/custom/a.json ",
            "/private/game.json",
            "private/stock/data/script/execute.lua",
            "private/runtime.exe",
        ] {
            let mut entries = files();
            entries.push((path.into(), vec![]));
            assert!(build(&entries).is_err(), "{path}");
        }
        let mut entries = files();
        entries.push(("PRIVATE/GAME.JSON".into(), vec![]));
        assert!(build(&entries).is_err());
        let bytes = build(&files()).unwrap();
        assert!(parse(&bytes[..bytes.len() - 1]).is_err());
    }
    #[test]
    fn cached_private_data_is_repaired_without_using_the_install_as_a_save_path() {
        let cache = std::env::temp_dir().join(format!(
            "csr-data-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bytes = build(&files()).unwrap();
        let root = materialize(&bytes, &cache).unwrap();
        std::fs::write(root.join("private/skater.glb"), b"incomplete cached write").unwrap();
        assert_eq!(materialize(&bytes, &cache).unwrap(), root);
        assert_eq!(
            std::fs::read(root.join("private/skater.glb")).unwrap(),
            b"private test bytes"
        );
        std::fs::remove_dir_all(cache).unwrap();
    }
}
