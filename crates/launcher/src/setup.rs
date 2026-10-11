//! Install setup: where player data lives and where the game assets are.
//!
//! * **Data folder.** Settings, skins, cases and loadouts live in:
//!   - installed copies (Setup.exe, see `update.rs`):
//!     `%LocalAppData%\CSGO-Rewrite\userdata`, outside the app folder that
//!     every update replaces. The first run copies in the data of an older
//!     copy if it finds one (a `userdata\` folder in the install folder, or an
//!     unzipped release on the Desktop, in Downloads or in Documents);
//!   - unzipped releases (a `csgo-rewrite.portable` file next to the exe):
//!     `userdata\` beside the exe, so a fresh unzip starts clean;
//!   - source checkouts: the repository root.
//!
//!   `CSR_DATA_DIR` overrides all three. The process runs inside the data
//!   folder, so every relative save path (`config\`) lands there.
//! * **CS:GO Legacy.** Player builds (installed or unzipped) search every
//!   Steam library on the PC (Steam's registry entry, its
//!   `libraryfolders.vdf`, and the usual library folders on each drive) for
//!   CS:GO Legacy (Steam app 4465480, or the older `csgo_legacy` beta of app
//!   730) and use what they find without asking. If it is missing they offer
//!   to install it through Steam, waiting for the download to finish, or to
//!   browse for `csgo.exe`. The choice is checked and remembered in
//!   `config\paths.cfg`. `csr setup` asks again; `csr find-csgo` prints what
//!   the search sees. `CSR_GAME_DIR` overrides.

use std::path::{Path, PathBuf};

use crate::dialog;

pub const PORTABLE_MARKER: &str = "csgo-rewrite.portable";
const PATHS_FILE: &str = "config/paths.cfg";
/// Folder under `%LocalAppData%` holding an installed copy's player data
/// (Velopack's own install folder is `CSGORewrite`, and is wiped on uninstall).
const DATA_FOLDER: &str = "CSGO-Rewrite";
const TITLE: &str = "CSGO Rewrite";

/// Returned by [`prepare`] when the player chose to quit: exit quietly.
pub const QUIT: &str = "";

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

/// How this copy of the game was put on the PC.
#[derive(Clone, Debug, PartialEq)]
pub enum Layout {
    /// Installed with Setup.exe: the Velopack install folder.
    Installed(PathBuf),
    /// An unzipped release: the folder holding the exe.
    Portable(PathBuf),
    /// Built from a source checkout: the repository root.
    Source(PathBuf),
    Unknown,
}

impl Layout {
    pub fn current() -> Layout {
        if let Some(root) = crate::update::installed_root() {
            return Layout::Installed(root);
        }
        if let Some(dir) = exe_dir().filter(|d| d.join(PORTABLE_MARKER).is_file()) {
            return Layout::Portable(dir);
        }
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        if repo.join("Cargo.toml").is_file() {
            return Layout::Source(repo);
        }
        Layout::Unknown
    }

    /// Copies handed to players: they get dialogs, not console prompts.
    pub fn for_players(&self) -> bool {
        matches!(self, Layout::Installed(_) | Layout::Portable(_))
    }

    /// The player data folder, given `%LocalAppData%`.
    pub fn data_dir(&self, local_app_data: Option<&Path>) -> Option<PathBuf> {
        match self {
            Layout::Installed(_) => local_app_data.map(|d| d.join(DATA_FOLDER).join("userdata")),
            Layout::Portable(dir) => Some(dir.join("userdata")),
            Layout::Source(repo) => Some(repo.clone()),
            Layout::Unknown => None,
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Fills a data folder that has never been used from the most recently used
/// of `candidates` (older copies' data folders). The old data is copied, not
/// moved. Returns where it came from.
fn migrate_into(data: &Path, candidates: Vec<PathBuf>) -> Option<PathBuf> {
    if data.join("config").exists() {
        return None;
    }
    let newest = candidates
        .into_iter()
        .filter(|c| c.join("config").is_dir() && c.as_path() != data)
        .max_by_key(|c| std::fs::metadata(c.join("config")).and_then(|m| m.modified()).ok())?;
    copy_dir(&newest, data).ok()?;
    Some(newest)
}

/// `userdata` folders of unzipped releases up to `depth` folders below `dir`.
fn portable_copies(dir: &Path, depth: u32) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        if path.join(PORTABLE_MARKER).is_file() {
            out.push(path.join("userdata"));
        } else if depth > 1 {
            out.extend(portable_copies(&path, depth - 1));
        }
    }
    out
}

/// Where an installed copy looks for data from an older copy.
fn old_copies(install_root: &Path) -> Vec<PathBuf> {
    let mut out = vec![install_root.join("userdata"), install_root.join(r"current\userdata")];
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for place in ["Desktop", "Downloads", "Documents", r"OneDrive\Desktop", r"OneDrive\Documents"] {
            out.extend(portable_copies(&home.join(place), 2));
        }
    }
    out
}

/// The CS:GO game folder (the one holding `gameinfo.txt`) for a folder the
/// player picked: the install root, its `csgo` folder, or a file inside it.
pub fn csgo_game_dir(picked: &Path) -> Result<PathBuf, String> {
    let picked = if picked.is_file() { picked.parent().unwrap_or(picked) } else { picked };
    let candidates = [picked.to_path_buf(), picked.join("csgo"), picked.parent().map(Path::to_path_buf).unwrap_or_default()];
    for dir in &candidates {
        if dir.join("gameinfo.txt").is_file() && dir.join("pak01_dir.vpk").is_file() {
            return Ok(dir.clone());
        }
    }
    if picked.join("game/csgo/gameinfo.gi").is_file() || picked.join("gameinfo.gi").is_file() || picked.join("csgo/gameinfo.gi").is_file() {
        return Err("That folder holds Counter-Strike 2, not CS:GO Legacy.\n\n\
            Install \"Counter-Strike: Global Offensive\" (CS:GO Legacy) from your Steam \
            library, then select the csgo.exe in its \"csgo legacy\" folder."
            .into());
    }
    Err(format!(
        "No CS:GO Legacy game files in\n{}\n\nSelect csgo.exe inside the \"csgo legacy\" \
        folder in Steam's steamapps\\common (next to a \"csgo\" folder with pak01_dir.vpk).",
        picked.display()
    ))
}

fn read_paths(data: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(data.join(PATHS_FILE))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

fn write_paths(data: &Path, entries: &[(String, String)]) -> std::io::Result<()> {
    let path = data.join(PATHS_FILE);
    std::fs::create_dir_all(path.parent().unwrap_or(data))?;
    let text: String = entries.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(path, text)
}


/// What a search of this PC found: CS:GO Legacy game folders (the `csgo`
/// folders holding `gameinfo.txt`) and Counter-Strike 2 installs that have
/// not been switched to the legacy branch.
#[derive(Debug, Default)]
pub struct Detected {
    pub legacy: Vec<PathBuf>,
    pub cs2_only: Vec<PathBuf>,
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
/// `libraryfolders.vdf`) plus the usual places on every drive.
fn steam_libraries() -> Vec<PathBuf> {
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
fn libraries_from(roots: Vec<PathBuf>) -> Vec<PathBuf> {
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

/// Looks for CS:GO Legacy in every Steam library on this PC.
pub fn detect() -> Detected {
    detect_in(steam_libraries())
}

fn detect_in(libraries: Vec<PathBuf>) -> Detected {
    let mut found = Detected::default();
    for lib in libraries {
        for dir in vfs::LEGACY_INSTALL_DIRS {
            let install = lib.join(r"steamapps\common").join(dir);
            if !install.is_dir() {
                continue;
            }
            match csgo_game_dir(&install) {
                Ok(game) if !found.legacy.contains(&game) => found.legacy.push(game),
                Ok(_) => {}
                Err(_) if install.join(r"game\csgo\gameinfo.gi").is_file() => found.cs2_only.push(install),
                Err(_) => {}
            }
        }
    }
    found
}


/// The CS:GO Legacy game folder in `library` once Steam has finished
/// installing it there (its app manifest says "fully installed").
fn finished_install(library: &Path) -> Option<PathBuf> {
    let acf = std::fs::read(library.join(format!("steamapps/appmanifest_{}.acf", vfs::LEGACY_APP_ID))).ok()?;
    let manifest = kv::parse_bytes(&acf).ok()?;
    let state = manifest.block("AppState")?;
    let flags: u32 = state.str("StateFlags")?.parse().ok()?;
    if flags & 4 == 0 || flags & 2 != 0 {
        return None; // not installed yet, or still updating
    }
    csgo_game_dir(&library.join("steamapps/common").join(state.str("installdir")?)).ok()
}

/// Opens Steam's installer for CS:GO Legacy and waits until it is ready.
/// `None` if the player gives up.
fn install_with_steam() -> Option<PathBuf> {
    dialog::open_url(&format!("steam://install/{}", vfs::LEGACY_APP_ID));
    let mut window = dialog::Waiting::show(
        TITLE,
        "Steam is opening the installer for CS:GO Legacy. Click Install there.\n\n\
         CSGO Rewrite starts by itself as soon as the download has finished, so you can leave \
         this window open. It is a big download and can take a while.",
        "Quit",
    );
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if window.dismissed() {
            return None;
        }
        if let Some(game) = steam_libraries().iter().find_map(|lib| finished_install(lib)) {
            window.close();
            return Some(game);
        }
    }
}

/// Asks the player to find `csgo.exe` themselves; `None` if they cancel.
fn pick_csgo_exe(start: Option<&Path>) -> Option<PathBuf> {
    let start = start.map(|p| format!("$d.InitialDirectory={};", dialog::ps_quote(&p.display().to_string()))).unwrap_or_default();
    let script = format!(
        "$d=New-Object System.Windows.Forms.OpenFileDialog; \
         $d.Title='Select csgo.exe in your CS:GO Legacy folder'; \
         $d.Filter='CS:GO Legacy (csgo.exe)|csgo.exe|All files (*.*)|*.*'; {start} \
         $top=New-Object System.Windows.Forms.Form -Property @{{TopMost=$true}}; \
         if($d.ShowDialog($top) -eq [System.Windows.Forms.DialogResult]::OK){{[Console]::Out.Write($d.FileName)}}"
    );
    match dialog::powershell(&script) {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
        Some(_) => None,
        None => {
            // No PowerShell: ask on the console instead.
            eprint!("Path to csgo.exe (in your CS:GO Legacy folder): ");
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).ok()?;
            let line = line.trim().trim_matches('"');
            (!line.is_empty()).then(|| PathBuf::from(line))
        }
    }
}

/// Picks the CS:GO Legacy folder for a player build. The first install found
/// is used without a question unless `force_ask` (`csr setup`); otherwise
/// the player confirms one, installs it through Steam or browses for it.
/// `Err(QUIT)` if they quit.
fn choose_csgo(found: &Detected, force_ask: bool) -> Result<PathBuf, String> {
    if !force_ask {
        if let Some(game) = found.legacy.first() {
            return Ok(game.clone());
        }
    }
    for game in &found.legacy {
        let install = game.parent().unwrap_or(game);
        let text = format!("CS:GO Legacy is installed here:\n\n{}\n\nPlay with this copy?", install.display());
        match dialog::choose(TITLE, &text, &["Use this one", "Choose another folder", "Quit"]) {
            Some(0) => return Ok(game.clone()),
            Some(1) => continue,
            _ => return Err(QUIT.into()),
        }
    }
    if found.legacy.is_empty() {
        let mut text = String::from(
            "CSGO Rewrite plays using the game files of CS:GO Legacy, which is free on Steam. \
             In your Steam library it is called \"Counter-Strike: Global Offensive\".\n\n",
        );
        text += if found.cs2_only.is_empty() {
            "It is not installed on this PC yet."
        } else {
            "This PC has Counter-Strike 2, but not CS:GO Legacy, which is a separate download."
        };
        match dialog::choose(TITLE, &text, &["Install it with Steam", "It is installed somewhere else", "Quit"]) {
            Some(0) => return install_with_steam().ok_or_else(|| QUIT.to_string()),
            Some(1) => {}
            _ => return Err(QUIT.into()),
        }
    }
    let start = found.legacy.first().and_then(|g| g.parent()).or(found.cs2_only.first().map(PathBuf::as_path)).map(Path::to_path_buf);
    loop {
        let picked = pick_csgo_exe(start.as_deref()).ok_or_else(|| QUIT.to_string())?;
        match csgo_game_dir(&picked) {
            Ok(dir) => return Ok(dir),
            Err(e) => dialog::message(TITLE, &e),
        }
    }
}

/// `csr find-csgo`: what the automatic search sees (for troubleshooting).
pub fn print_detected() {
    let found = detect();
    for g in &found.legacy {
        println!("CS:GO Legacy: {}", g.display());
    }
    for c in &found.cs2_only {
        println!("Counter-Strike 2 (not CS:GO Legacy): {}", c.display());
    }
    if found.legacy.is_empty() && found.cs2_only.is_empty() {
        println!("No CS:GO install found in any Steam library.");
    }
}


/// Commands that run before setup: `csr find-csgo`, and `csr art <folder>`
/// (writes the icon, installer splash and Steam artwork, for packaging).
pub fn early_command(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some("find-csgo") => {
            print_detected();
            Some(0)
        }
        Some("art") => {
            let dir = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("art"));
            let mut files: Vec<(String, Vec<u8>)> = vec![("icon.ico".into(), crate::art::ico()), ("splash.png".into(), crate::art::splash().png())];
            for (suffix, image) in crate::art::steam_library() {
                files.push((format!("steam{suffix}.png"), image.png()));
            }
            for (name, bytes) in files {
                if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join(&name), bytes)) {
                    eprintln!("error: {}: {e}", dir.join(name).display());
                    return Some(1);
                }
            }
            println!("Artwork written to {}", dir.display());
            Some(0)
        }
        _ => None,
    }
}

/// Prepares the process before any command runs: data folder, CS:GO Legacy
/// folder, and for installed copies the one-time "Add to Steam" offer.
/// `force_ask` re-runs the CS:GO folder question (`csr setup`).
pub fn prepare(force_ask: bool) -> Result<(), String> {
    let layout = Layout::current();
    if matches!(layout, Layout::Installed(_)) {
        dialog::hide_own_console();
    }
    let data_override = std::env::var_os("CSR_DATA_DIR").map(PathBuf::from);
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let data = data_override.clone().or_else(|| layout.data_dir(local_app_data.as_deref()));
    if let Some(data) = &data {
        if let (Layout::Installed(root), None) = (&layout, &data_override) {
            if let Some(from) = migrate_into(data, old_copies(root)) {
                eprintln!("Copied your saved data from {}", from.display());
            }
        }
        std::fs::create_dir_all(data).map_err(|e| format!("Cannot create {}: {e}", data.display()))?;
        std::env::set_current_dir(data).map_err(|e| format!("Cannot use {}: {e}", data.display()))?;
    }
    let data = std::env::current_dir().map_err(|e| e.to_string())?;

    if std::env::var_os("CSR_GAME_DIR").is_none() || force_ask {
        if let Some(game) = find_game(&layout, &data, force_ask)? {
            // SAFETY: single-threaded start-up, before anything reads the environment.
            unsafe { std::env::set_var("CSR_GAME_DIR", game) };
        }
    }
    if matches!(layout, Layout::Installed(_)) && !force_ask {
        crate::steam::offer(&data, false);
    }
    Ok(())
}

fn find_game(layout: &Layout, data: &Path, force_ask: bool) -> Result<Option<PathBuf>, String> {
    let mut paths = read_paths(data);
    let saved = paths.iter().find(|(k, _)| k == "csgo").map(|(_, v)| PathBuf::from(v));
    if let Some(Ok(dir)) = saved.as_deref().map(csgo_game_dir) {
        if !force_ask {
            return Ok(Some(dir));
        }
    }
    if !layout.for_players() && !force_ask {
        // Source checkouts find the Steam install by themselves.
        return Ok(vfs::find_csgo_dir().or_else(|| detect().legacy.into_iter().next()));
    }
    let dir = choose_csgo(&detect(), force_ask)?;
    paths.retain(|(k, _)| k != "csgo");
    paths.push(("csgo".into(), dir.display().to_string()));
    write_paths(data, &paths).map_err(|e| format!("Cannot save {PATHS_FILE}: {e}"))?;
    eprintln!("CS:GO Legacy: {}", dir.display());
    Ok(Some(dir))
}

/// Shows a start-up failure to a player who may not be watching a console.
/// Quitting on purpose ([`QUIT`]) shows nothing.
pub fn report(error: &str) {
    if error.is_empty() {
        return;
    }
    eprintln!("error: {error}");
    if Layout::current().for_players() {
        dialog::message(TITLE, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("csr-setup-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn accepts_install_root_or_csgo_folder_and_rejects_cs2() {
        let root = temp("legacy");
        let game = root.join("csgo");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("gameinfo.txt"), "").unwrap();
        std::fs::write(game.join("pak01_dir.vpk"), "").unwrap();
        assert_eq!(csgo_game_dir(&root).unwrap(), game);
        assert_eq!(csgo_game_dir(&game).unwrap(), game);
        assert_eq!(csgo_game_dir(&game.join("pak01_dir.vpk")).unwrap(), game);

        let cs2 = temp("cs2");
        std::fs::create_dir_all(cs2.join("game/csgo")).unwrap();
        std::fs::write(cs2.join("game/csgo/gameinfo.gi"), "").unwrap();
        assert!(csgo_game_dir(&cs2).unwrap_err().contains("Counter-Strike 2"));
        assert!(csgo_game_dir(&temp("empty")).is_err());
    }

    #[test]
    fn finds_legacy_in_listed_libraries_and_flags_cs2() {
        // Steam on one "drive" lists libraries holding CS:GO Legacy as its
        // own app, the old 730 beta branch, and a plain Counter-Strike 2.
        let steam = temp("steam");
        let legacy_lib = temp("lib-legacy");
        let beta_lib = temp("lib-beta");
        let cs2_lib = temp("lib-cs2");
        std::fs::create_dir_all(steam.join("steamapps")).unwrap();
        let vdf = format!(
            "\"libraryfolders\"\n{{\n\"0\" {{ \"path\" \"{}\" }}\n\"1\" {{ \"path\" \"{}\" }}\n\"2\" {{ \"path\" \"{}\" }}\n}}\n",
            legacy_lib.display().to_string().replace('\\', "\\\\"),
            beta_lib.display().to_string().replace('\\', "\\\\"),
            cs2_lib.display().to_string().replace('\\', "\\\\")
        );
        std::fs::write(steam.join(r"steamapps\libraryfolders.vdf"), vdf).unwrap();
        let game = legacy_lib.join(r"steamapps\common\csgo legacy\csgo");
        let beta = beta_lib.join(r"steamapps\common\Counter-Strike Global Offensive\csgo");
        for dir in [&game, &beta] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("gameinfo.txt"), "").unwrap();
            std::fs::write(dir.join("pak01_dir.vpk"), "").unwrap();
        }
        let cs2 = cs2_lib.join(r"steamapps\common\Counter-Strike Global Offensive");
        std::fs::create_dir_all(cs2.join(r"game\csgo")).unwrap();
        std::fs::write(cs2.join(r"game\csgo\gameinfo.gi"), "").unwrap();

        let found = detect_in(libraries_from(vec![steam]));
        assert_eq!(found.legacy, vec![game, beta]);
        assert_eq!(found.cs2_only, vec![cs2]);
    }

    #[test]
    fn paths_file_round_trips() {
        let data = temp("paths");
        write_paths(&data, &[("csgo".into(), r"D:\Games\CS:GO\csgo".into())]).unwrap();
        assert_eq!(read_paths(&data), vec![("csgo".to_string(), r"D:\Games\CS:GO\csgo".to_string())]);
    }

    #[test]
    fn data_folder_survives_updates_for_installed_copies() {
        let local = PathBuf::from(r"C:\Users\Friend\AppData\Local");
        let installed = Layout::Installed(local.join("CSGORewrite"));
        // Not inside the install folder, which updates replace and uninstall deletes.
        assert_eq!(installed.data_dir(Some(&local)), Some(local.join(r"CSGO-Rewrite\userdata")));
        assert_eq!(installed.data_dir(None), None);
        let unzipped = PathBuf::from(r"D:\Games\CSGO-Rewrite");
        assert_eq!(Layout::Portable(unzipped.clone()).data_dir(Some(&local)), Some(unzipped.join("userdata")));
        let repo = PathBuf::from(r"D:\dev\global-offensive");
        assert_eq!(Layout::Source(repo.clone()).data_dir(Some(&local)), Some(repo));
        assert!(installed.for_players() && Layout::Portable(unzipped).for_players());
        assert!(!Layout::Source(PathBuf::new()).for_players());
    }

    #[test]
    fn first_run_copies_data_from_an_unzipped_release() {
        let home = temp("migrate");
        // An unzipped v0.2.0 in Downloads (the zip nests one folder) and a
        // stale empty one on the Desktop.
        let old = home.join(r"Downloads\CSGO-Rewrite-0.2.0-windows-x64\CSGO-Rewrite");
        std::fs::create_dir_all(old.join(r"userdata\config")).unwrap();
        std::fs::write(old.join(PORTABLE_MARKER), "").unwrap();
        std::fs::write(old.join(r"userdata\config\paths.cfg"), "csgo=D:\\csgo\n").unwrap();
        std::fs::write(old.join(r"userdata\config\cosmetics.cfg"), "case=1\n").unwrap();
        let empty = home.join(r"Desktop\CSGO-Rewrite");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::write(empty.join(PORTABLE_MARKER), "").unwrap();

        let found: Vec<PathBuf> = ["Desktop", "Downloads"].iter().flat_map(|p| portable_copies(&home.join(p), 2)).collect();
        assert_eq!(found, vec![empty.join("userdata"), old.join("userdata")]);

        let data = home.join(r"Local\CSGO-Rewrite\userdata");
        assert_eq!(migrate_into(&data, found.clone()), Some(old.join("userdata")));
        assert_eq!(std::fs::read_to_string(data.join(r"config\cosmetics.cfg")).unwrap(), "case=1\n");
        assert!(old.join(r"userdata\config\paths.cfg").is_file(), "the old copy is kept");
        // Only ever into a fresh folder: later runs keep the player's newer data.
        std::fs::write(data.join(r"config\cosmetics.cfg"), "case=2\n").unwrap();
        assert_eq!(migrate_into(&data, found), None);
        assert_eq!(std::fs::read_to_string(data.join(r"config\cosmetics.cfg")).unwrap(), "case=2\n");
    }

    #[test]
    fn waits_for_steam_to_finish_installing() {
        let lib = temp("acf");
        let game = lib.join(r"steamapps\common\csgo legacy\csgo");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("gameinfo.txt"), "").unwrap();
        std::fs::write(game.join("pak01_dir.vpk"), "").unwrap();
        let acf = |flags: u32| {
            let text = format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"4465480\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"csgo legacy\"\n}}\n");
            std::fs::write(lib.join(r"steamapps\appmanifest_4465480.acf"), text).unwrap();
        };
        assert_eq!(finished_install(&lib), None);
        acf(1026); // downloading
        assert_eq!(finished_install(&lib), None);
        acf(4); // fully installed
        assert_eq!(finished_install(&lib), Some(game));
    }
}
