//! Search-path file system.
//!
//! Mirrors how the game resolves a relative path such as
//! `materials/de_dust/sandwall01.vmt`: mounts are searched in order and the
//! first hit wins. A typical CS:GO stack, highest priority first:
//!
//! 1. the current map's embedded pakfile (cubemap-patched materials etc.)
//! 2. `csgo/pak01_dir.vpk`
//! 3. loose files under `csgo/`
//!
//! Game data is only ever read. Nothing here writes to the install.

pub mod zipfs;

use std::io;
use std::path::{Path, PathBuf};

pub use zipfs::ZipArchive;

pub enum Mount {
    Dir(PathBuf),
    Vpk(vpk::Vpk),
    Zip(ZipArchive),
}

impl Mount {
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        match self {
            Mount::Dir(root) => {
                let full = root.join(path);
                match std::fs::read(&full) {
                    Ok(b) => Ok(Some(b)),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e),
                }
            }
            Mount::Vpk(v) => v.read(path),
            Mount::Zip(z) => z.read(path),
        }
    }

    fn exists(&self, path: &str) -> bool {
        match self {
            Mount::Dir(root) => root.join(path).is_file(),
            Mount::Vpk(v) => v.contains(path),
            Mount::Zip(z) => z.contains(path),
        }
    }
}

struct Mounted {
    label: String,
    mount: Mount,
}

#[derive(Default)]
pub struct Vfs {
    mounts: Vec<Mounted>,
}

impl std::fmt::Debug for Vfs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.mounts.iter().map(|m| &m.label))
            .finish()
    }
}

impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a mount at the lowest priority.
    pub fn push_back(&mut self, label: impl Into<String>, mount: Mount) {
        self.mounts.push(Mounted {
            label: label.into(),
            mount,
        });
    }

    /// Inserts a mount at the highest priority.
    pub fn push_front(&mut self, label: impl Into<String>, mount: Mount) {
        self.mounts.insert(
            0,
            Mounted {
                label: label.into(),
                mount,
            },
        );
    }

    /// Removes every mount with this label (e.g. the previous map's pakfile).
    pub fn unmount(&mut self, label: &str) {
        self.mounts.retain(|m| m.label != label);
    }

    pub fn labels(&self) -> Vec<&str> {
        self.mounts.iter().map(|m| m.label.as_str()).collect()
    }

    /// Mounts a game directory the way `gameinfo.txt` asks: each `Game`
    /// search path contributes its `pak01_dir.vpk` and then its loose files.
    pub fn mount_game_dir(&mut self, game_dir: &Path) -> io::Result<()> {
        let gameinfo = std::fs::read(game_dir.join("gameinfo.txt"))?;
        let doc = kv::parse_bytes(&gameinfo)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let install_root = game_dir.parent().unwrap_or(game_dir).to_path_buf();
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(sp) = doc
            .path(&["GameInfo", "FileSystem", "SearchPaths"])
            .and_then(kv::Value::as_block)
        {
            for (key, value) in sp.iter() {
                // Keys can be combined, e.g. "game+mod"; we only need "game".
                if !key.to_ascii_lowercase().split('+').any(|k| k == "game") {
                    continue;
                }
                let Some(v) = value.as_str() else { continue };
                let dir = if let Some(rest) = v.strip_prefix("|gameinfo_path|") {
                    game_dir.join(rest)
                } else if let Some(rest) = v.strip_prefix("|all_source_engine_paths|") {
                    install_root.join(rest)
                } else {
                    install_root.join(v)
                };
                let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
                if dir.is_dir() && !dirs.contains(&dir) {
                    dirs.push(dir);
                }
            }
        }
        if dirs.is_empty() {
            dirs.push(game_dir.to_path_buf());
        }
        for dir in dirs {
            let pak = dir.join("pak01_dir.vpk");
            if pak.is_file() {
                self.push_back(format!("vpk:{}", pak.display()), Mount::Vpk(vpk::Vpk::open(&pak)?));
            }
            self.push_back(format!("dir:{}", dir.display()), Mount::Dir(dir));
        }
        Ok(())
    }

    /// Mounts a map pakfile at the highest priority under the label `map`.
    pub fn mount_map_pakfile(&mut self, pakfile: Vec<u8>) -> io::Result<usize> {
        self.unmount("map");
        let zip = ZipArchive::from_bytes(pakfile)?;
        let n = zip.len();
        self.push_front("map", Mount::Zip(zip));
        Ok(n)
    }

    /// Reads the first match for `path`. I/O errors in one mount are skipped
    /// so that a single bad archive does not hide lower-priority copies.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let path = vpk::normalize(path);
        for m in &self.mounts {
            match m.mount.read(&path) {
                Ok(Some(bytes)) => return Some(bytes),
                Ok(None) => {}
                Err(_) => {}
            }
        }
        None
    }

    pub fn read_string(&self, path: &str) -> Option<String> {
        self.read(path).map(|b| kv::decode_text(&b))
    }

    pub fn exists(&self, path: &str) -> bool {
        let path = vpk::normalize(path);
        self.mounts.iter().any(|m| m.mount.exists(&path))
    }

    /// Paths known to archive mounts (VPK and pakfile) under a prefix.
    /// Loose directories are not enumerated.
    pub fn archive_paths_with_prefix(&self, prefix: &str) -> Vec<String> {
        let prefix = vpk::normalize(prefix);
        let mut out = Vec::new();
        for m in &self.mounts {
            match &m.mount {
                Mount::Vpk(v) => out.extend(v.paths().filter(|p| p.starts_with(&prefix)).map(String::from)),
                Mount::Zip(z) => out.extend(z.paths().filter(|p| p.starts_with(&prefix)).map(String::from)),
                Mount::Dir(_) => {}
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

/// Steam app ID of CS:GO Legacy, the standalone CS:GO that Steam installs
/// next to Counter-Strike 2.
pub const LEGACY_APP_ID: u32 = 4465480;

/// Folders under `steamapps/common` that can hold CS:GO Legacy: its own app
/// (`csgo legacy`), and the older `csgo_legacy` beta branch of app 730.
pub const LEGACY_INSTALL_DIRS: [&str; 2] = ["csgo legacy", "Counter-Strike Global Offensive"];

/// Locates a CS:GO game directory (the folder holding `gameinfo.txt`).
///
/// Order: `CSR_GAME_DIR`, then every Steam library listed in
/// `libraryfolders.vdf` (default Steam path first).
pub fn find_csgo_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CSR_GAME_DIR") {
        let p = PathBuf::from(dir);
        if p.join("gameinfo.txt").is_file() {
            return Some(p);
        }
        if p.join("csgo").join("gameinfo.txt").is_file() {
            return Some(p.join("csgo"));
        }
    }
    let mut steam_roots = vec![
        PathBuf::from(r"C:\Program Files (x86)\Steam"),
        PathBuf::from(r"C:\Program Files\Steam"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        steam_roots.push(PathBuf::from(&home).join(".steam/steam"));
        steam_roots.push(PathBuf::from(&home).join(".local/share/Steam"));
        steam_roots.push(PathBuf::from(&home).join("Library/Application Support/Steam"));
    }
    let mut libraries = Vec::new();
    for root in &steam_roots {
        libraries.push(root.clone());
        if let Ok(bytes) = std::fs::read(root.join("steamapps/libraryfolders.vdf")) {
            if let Ok(doc) = kv::parse_bytes(&bytes) {
                if let Some(folders) = doc.block("libraryfolders") {
                    for (_, v) in folders.iter() {
                        if let Some(p) = v.as_block().and_then(|b| b.str("path")) {
                            libraries.push(PathBuf::from(p));
                        }
                    }
                }
            }
        }
    }
    for lib in libraries {
        for dir in LEGACY_INSTALL_DIRS {
            let game = lib.join("steamapps/common").join(dir).join("csgo");
            if game.join("gameinfo.txt").is_file() {
                return Some(game);
            }
        }
    }
    None
}

/// Steam's install folder from the registry (`reg` ships with Windows).
fn steam_from_registry() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for (key, value) in [
        (r"HKCU\Software\Valve\Steam", "SteamPath"),
        (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
    ] {
        let Ok(o) = std::process::Command::new("reg").args(["query", key, "/v", value]).output() else { continue };
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            if let Some((_, path)) = line.split_once("REG_SZ") {
                out.push(PathBuf::from(path.trim()));
            }
        }
    }
    out
}

/// Steam library folders: registered ones (from every Steam install's
/// `libraryfolders.vdf`) plus the usual places on every drive. Some may not
/// exist; callers check for what they need.
pub fn steam_libraries() -> Vec<PathBuf> {
    let mut roots = steam_from_registry();
    roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    roots.push(PathBuf::from(r"C:\Program Files\Steam"));
    for letter in b'C'..=b'Z' {
        let drive = PathBuf::from(format!("{}:\\", letter as char));
        if !drive.is_dir() {
            continue;
        }
        for sub in ["SteamLibrary", "Steam", r"Program Files (x86)\Steam", r"Program Files\Steam", r"Games\Steam", r"Games\SteamLibrary", "Games"] {
            roots.push(drive.join(sub));
        }
    }
    libraries_from(roots)
}

/// Each root plus the libraries its `steamapps\libraryfolders.vdf` lists.
pub fn libraries_from(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for root in roots {
        if let Ok(bytes) = std::fs::read(root.join("steamapps/libraryfolders.vdf")) {
            if let Some(folders) = kv::parse_bytes(&bytes).ok().as_ref().and_then(|d| d.block("libraryfolders")) {
                for (_, v) in folders.iter() {
                    if let Some(p) = v.as_block().and_then(|b| b.str("path")) {
                        libraries.push(PathBuf::from(p.replace(r"\\", r"\")));
                    }
                }
            }
        }
        libraries.push(root);
    }
    let mut seen = std::collections::HashSet::new();
    libraries.retain(|p| seen.insert(p.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string()));
    libraries
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_order_first_mount_wins() {
        let tmp = std::env::temp_dir().join(format!("vfs_test_{}", std::process::id()));
        let a = tmp.join("a");
        let b = tmp.join("b");
        std::fs::create_dir_all(a.join("scripts")).unwrap();
        std::fs::create_dir_all(b.join("scripts")).unwrap();
        std::fs::write(a.join("scripts/x.txt"), "from a").unwrap();
        std::fs::write(b.join("scripts/x.txt"), "from b").unwrap();
        std::fs::write(b.join("scripts/y.txt"), "only b").unwrap();
        let mut vfs = Vfs::new();
        vfs.push_back("a", Mount::Dir(a));
        vfs.push_back("b", Mount::Dir(b));
        assert_eq!(vfs.read_string("SCRIPTS\\x.txt").unwrap(), "from a");
        assert_eq!(vfs.read_string("scripts/y.txt").unwrap(), "only b");
        assert!(vfs.read("scripts/z.txt").is_none());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
