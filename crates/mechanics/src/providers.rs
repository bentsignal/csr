//! Optional, publisher-owned native providers. Packs select a known provider;
//! they never supply an executable, URL, shell command or extraction path.
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::{Path, PathBuf}, sync::{Mutex, OnceLock}};

static SOURCE: OnceLock<String> = OnceLock::new();
static DOWNLOAD: Mutex<()> = Mutex::new(());
pub fn configure_source(source: &str) { let _ = SOURCE.set(source.into()); }
fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn ordinary(path: &Path) -> Result<(), String> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        #[cfg(windows)] {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 { return Err("Provider paths cannot be links".into()); }
        }
        if meta.file_type().is_symlink() { return Err("Provider paths cannot be links".into()); }
    }
    Ok(())
}
fn get(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent("CSGO-Rewrite optional content runtime").build().new_agent();
    let mut response = agent.get(url).call().map_err(|e| format!("Could not download content runtime: {e}"))?;
    let mut data = vec![];
    response.body_mut().as_reader().take(limit + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > limit { return Err("Content runtime download exceeded its limit".into()); }
    Ok(data)
}
pub fn executable(provider: &str) -> Result<PathBuf, String> {
    let (asset_name, executable) = match provider {
        "skate3-v2" => ("Skate-Runtime-windows-x64.zip", "Skate-Runtime.exe"),
        _ => return Err("Unknown content runtime".into()),
    };
    // Explicit local developer override. Never read an executable path from a pack.
    if let Some(dir) = std::env::var_os("CSR_PROVIDER_DEV_DIR") {
        let path = PathBuf::from(dir).join(executable);
        return if path.is_file() { Ok(path) } else { Err("Developer runtime is missing".into()) };
    }
    let source = SOURCE.get().ok_or("Content runtime source identity is unavailable")?;
    if source.len()!=40 || !source.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("This source build needs CSR_PROVIDER_DEV_DIR for native content runtimes".into());
    }
    let _guard = DOWNLOAD.lock().map_err(|_| "Content runtime download lock failed")?;
    let data = std::env::var_os("CSR_DATA_DIR").map(PathBuf::from).unwrap_or(std::env::current_dir().map_err(|e|e.to_string())?);
    let mut dir = data;
    for part in ["providers", source.as_str(), provider] {
        ordinary(&dir)?; dir.push(part); ordinary(&dir)?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let exe = dir.join(executable);
    let receipt = dir.join("sha256.txt");
    ordinary(&exe)?; ordinary(&receipt)?;
    if let (Ok(bytes), Ok(expected)) = (fs::read(&exe), fs::read_to_string(&receipt)) {
        if hash(&bytes) == expected.trim() { return Ok(exe); }
    }
    // Resolve a source-bound tag rather than only the newest releases: a
    // Stable client must still acquire its runtime after many Dev updates.
    let refs: serde_json::Value = serde_json::from_slice(&get("https://api.github.com/repos/bentsignal/csr/git/matching-refs/tags/v", 16<<20)?).map_err(|e|e.to_string())?;
    let tag=refs.as_array().ok_or("Invalid runtime tag index")?.iter().find_map(|r| {
        let tag=r["ref"].as_str()?.strip_prefix("refs/tags/")?;
        (r["object"]["sha"]==source.as_str() && tag.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'.'||b==b'-'))
            .then_some(tag)
    }).ok_or("The matching optional runtime has not been published yet")?;
    let release:serde_json::Value=serde_json::from_slice(&get(&format!("https://api.github.com/repos/bentsignal/csr/releases/tags/{tag}"),2<<20)?).map_err(|e|e.to_string())?;
    if release["draft"]!=false || !release["body"].as_str().is_some_and(|body|body.contains(&format!("Source: {source}"))) {
        return Err("Content runtime release identity mismatch".into());
    }
    let asset = release["assets"].as_array().ok_or("Missing runtime release assets")?.iter()
        .find(|a| a["name"]==asset_name).ok_or("Optional runtime asset is missing from this release")?;
    let url = asset["browser_download_url"].as_str().ok_or("Runtime download URL missing")?;
    if !url.starts_with("https://github.com/bentsignal/csr/releases/download/") { return Err("Untrusted runtime source".into()); }
    let bytes = get(url,64<<20)?;
    if asset["digest"].as_str()!=Some(format!("sha256:{}",hash(&bytes)).as_str()) { return Err("Runtime archive checksum mismatch".into()); }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e|e.to_string())?;
    let mut payload = None;
    for name in [executable,"Skate-GPL-3.0.txt","Skate-NOTICE.txt","Skate-SOURCE.txt"] {
        let mut entry = archive.by_name(name).map_err(|e|e.to_string())?;
        if entry.size()>16<<20 { return Err("Runtime payload exceeds its limit".into()); }
        let mut bytes = vec![]; entry.read_to_end(&mut bytes).map_err(|e|e.to_string())?;
        if name=="Skate-SOURCE.txt" && !String::from_utf8_lossy(&bytes).contains(source) {
            return Err("Runtime corresponding-source identity mismatch".into());
        }
        if name==executable { payload=Some(bytes); }
        else {
            let path=dir.join(name); ordinary(&path)?;
            fs::write(path,bytes).map_err(|e|e.to_string())?;
        }
    }
    let bytes=payload.ok_or("Runtime executable missing")?;
    // The lock serializes publication; the executable is never launched until
    // both its archive digest and final on-disk hash have been checked.
    fs::write(&exe,&bytes).map_err(|e|e.to_string())?;
    fs::write(receipt,hash(&bytes)).map_err(|e|e.to_string())?;
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_pack_cannot_select_a_command_or_download_source() {
        for name in ["", "../Skate-Runtime.exe", "https://example.com/program", "cmd.exe", "skate3-v2 --help"] {
            assert!(executable(name).unwrap_err().contains("Unknown content runtime"));
        }
    }
}
