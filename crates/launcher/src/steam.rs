//! "Add to Steam": a non-Steam-game shortcut for CSGO Rewrite, with our own
//! library artwork, in every local Steam account.
//!
//! Steam keeps non-Steam games in `userdata\<account>\config\shortcuts.vdf`,
//! a binary KeyValues file, and their artwork in `config\grid` named after
//! the shortcut's app id. Steam reads the file when it starts and rewrites it
//! when it exits, so edits only stick while Steam is closed: the offer asks to
//! restart Steam when it is running.

use std::path::{Path, PathBuf};

use crate::dialog;

pub const APP_NAME: &str = "CSGO Rewrite";
const SETUP_FILE: &str = "config/setup.cfg";
const OFFER_KEY: &str = "add_to_steam";

// --- Binary KeyValues --------------------------------------------------------

/// A binary KeyValues value. Keys and strings stay raw bytes so entries we do
/// not touch are written back byte for byte.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Map(Map),
    Str(Vec<u8>),
    Int(i32),
    Float(f32),
    Ptr(u32),
    Color(u32),
    U64(u64),
    I64(i64),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map(pub Vec<(Vec<u8>, Value)>);

const T_MAP: u8 = 0x00;
const T_STR: u8 = 0x01;
const T_INT: u8 = 0x02;
const T_FLOAT: u8 = 0x03;
const T_PTR: u8 = 0x04;
const T_COLOR: u8 = 0x06;
const T_U64: u8 = 0x07;
const T_END: u8 = 0x08;
const T_I64: u8 = 0x0A;
const T_END_ALT: u8 = 0x0B;

impl Map {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k.eq_ignore_ascii_case(key.as_bytes())).map(|(_, v)| v)
    }

    /// Sets `key` (matched case-insensitively, keeping its stored spelling),
    /// appending it if missing.
    pub fn set(&mut self, key: &str, value: Value) {
        match self.0.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key.as_bytes())) {
            Some((_, v)) => *v = value,
            None => self.0.push((key.as_bytes().to_vec(), value)),
        }
    }

    fn str(&self, key: &str) -> Option<String> {
        match self.get(key) {
            Some(Value::Str(s)) => Some(String::from_utf8_lossy(s).into_owned()),
            _ => None,
        }
    }

    fn map_mut(&mut self, key: &str) -> Option<&mut Map> {
        match self.0.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key.as_bytes())) {
            Some((_, Value::Map(m))) => Some(m),
            _ => None,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len()).ok_or("unexpected end of file")?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn cstr(&mut self) -> Result<Vec<u8>, String> {
        let len = self.bytes[self.at..].iter().position(|&b| b == 0).ok_or("unterminated string")?;
        let s = self.bytes[self.at..self.at + len].to_vec();
        self.at += len + 1;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// Entries up to the map's end marker (or the end of the file at the root).
    fn map(&mut self, root: bool) -> Result<Map, String> {
        let mut map = Map::default();
        loop {
            if root && self.at == self.bytes.len() {
                return Ok(map);
            }
            let kind = self.take(1)?[0];
            if kind == T_END || kind == T_END_ALT {
                return Ok(map);
            }
            let key = self.cstr()?;
            let value = match kind {
                T_MAP => Value::Map(self.map(false)?),
                T_STR => Value::Str(self.cstr()?),
                T_INT => Value::Int(self.u32()? as i32),
                T_FLOAT => Value::Float(f32::from_bits(self.u32()?)),
                T_PTR => Value::Ptr(self.u32()?),
                T_COLOR => Value::Color(self.u32()?),
                T_U64 => Value::U64(self.u64()?),
                T_I64 => Value::I64(self.u64()? as i64),
                other => return Err(format!("unsupported value type 0x{other:02x} at byte {}", self.at - 1)),
            };
            map.0.push((key, value));
        }
    }
}

/// Parses a binary KeyValues file (the root map's entries).
pub fn parse(bytes: &[u8]) -> Result<Map, String> {
    let mut r = Reader { bytes, at: 0 };
    let map = r.map(true)?;
    if r.at != bytes.len() {
        return Err(format!("{} unexpected bytes after the end", bytes.len() - r.at));
    }
    Ok(map)
}

fn write_map(out: &mut Vec<u8>, map: &Map) {
    for (key, value) in &map.0 {
        let kind = match value {
            Value::Map(_) => T_MAP,
            Value::Str(_) => T_STR,
            Value::Int(_) => T_INT,
            Value::Float(_) => T_FLOAT,
            Value::Ptr(_) => T_PTR,
            Value::Color(_) => T_COLOR,
            Value::U64(_) => T_U64,
            Value::I64(_) => T_I64,
        };
        out.push(kind);
        out.extend_from_slice(key);
        out.push(0);
        match value {
            Value::Map(m) => write_map(out, m),
            Value::Str(s) => {
                out.extend_from_slice(s);
                out.push(0);
            }
            Value::Int(v) => out.extend_from_slice(&v.to_le_bytes()),
            Value::Float(v) => out.extend_from_slice(&v.to_bits().to_le_bytes()),
            Value::Ptr(v) | Value::Color(v) => out.extend_from_slice(&v.to_le_bytes()),
            Value::U64(v) => out.extend_from_slice(&v.to_le_bytes()),
            Value::I64(v) => out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    out.push(T_END);
}

/// Serialises a root map the way Steam writes it.
pub fn write(root: &Map) -> Vec<u8> {
    let mut out = Vec::new();
    write_map(&mut out, root);
    out
}

// --- Shortcuts ---------------------------------------------------------------

/// The 32-bit app id Steam gives a non-Steam shortcut: the CRC-32 of the
/// quoted exe path followed by the name, with the top bit set. Artwork in
/// `config\grid` is named after it.
pub fn shortcut_app_id(exe_quoted: &str, name: &str) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(exe_quoted.as_bytes());
    h.update(name.as_bytes());
    h.finalize() | 0x8000_0000
}

/// What the shortcut launches.
#[derive(Clone, Debug)]
pub struct Shortcut {
    pub name: String,
    pub exe: PathBuf,
}

impl Shortcut {
    pub fn exe_quoted(&self) -> String {
        format!("\"{}\"", self.exe.display())
    }

    pub fn app_id(&self) -> u32 {
        shortcut_app_id(&self.exe_quoted(), &self.name)
    }

    fn entry(&self, icon: &str) -> Vec<(&'static str, Value)> {
        let s = |v: &str| Value::Str(v.as_bytes().to_vec());
        let dir = self.exe.parent().map(|d| format!("\"{}\"", d.display())).unwrap_or_default();
        vec![
            ("appid", Value::Int(self.app_id() as i32)),
            ("AppName", s(&self.name)),
            ("Exe", s(&self.exe_quoted())),
            ("StartDir", s(&dir)),
            ("icon", s(icon)),
            ("ShortcutPath", s("")),
            ("LaunchOptions", s("")),
            ("IsHidden", Value::Int(0)),
            ("AllowDesktopConfig", Value::Int(1)),
            ("AllowOverlay", Value::Int(1)),
            ("OpenVR", Value::Int(0)),
            ("Devkit", Value::Int(0)),
            ("DevkitGameID", s("")),
            ("DevkitOverrideAppID", Value::Int(0)),
            ("LastPlayTime", Value::Int(0)),
            ("FlatpakAppID", s("")),
            ("tags", Value::Map(Map::default())),
        ]
    }
}

/// True if `entry` is our shortcut: the same app id, or our name and exe
/// (a shortcut added by hand).
fn is_ours(entry: &Map, shortcut: &Shortcut) -> bool {
    if let Some(Value::Int(id)) = entry.get("appid") {
        if *id as u32 == shortcut.app_id() {
            return true;
        }
    }
    entry.str("AppName").is_some_and(|n| n == shortcut.name)
        && entry.str("Exe").is_some_and(|e| e.trim_matches('"').eq_ignore_ascii_case(&shortcut.exe.display().to_string()))
}

fn shortcuts_mut(root: &mut Map) -> &mut Map {
    if root.map_mut("shortcuts").is_none() {
        root.0.push((b"shortcuts".to_vec(), Value::Map(Map::default())));
    }
    root.map_mut("shortcuts").unwrap()
}

/// Adds the shortcut to a parsed `shortcuts.vdf`, or refreshes our existing
/// entry (keeping fields Steam owns, like play time and categories). Other
/// entries are left exactly as they were.
pub fn upsert(root: &mut Map, shortcut: &Shortcut, icon: &str) {
    let list = shortcuts_mut(root);
    let fields = shortcut.entry(icon);
    if let Some((_, Value::Map(entry))) = list.0.iter_mut().find(|(_, v)| matches!(v, Value::Map(m) if is_ours(m, shortcut))) {
        for (k, v) in fields {
            if !matches!(k, "LastPlayTime" | "tags" | "IsHidden") || entry.get(k).is_none() {
                entry.set(k, v);
            }
        }
        return;
    }
    let next = list.0.iter().filter_map(|(k, _)| std::str::from_utf8(k).ok()?.parse::<u32>().ok()).max().map_or(0, |n| n + 1);
    let mut entry = Map::default();
    for (k, v) in fields {
        entry.set(k, v);
    }
    list.0.push((next.to_string().into_bytes(), Value::Map(entry)));
}

/// Removes our shortcut and renumbers the rest. Returns whether it was there.
pub fn remove(root: &mut Map, shortcut: &Shortcut) -> bool {
    let list = shortcuts_mut(root);
    let before = list.0.len();
    list.0.retain(|(_, v)| !matches!(v, Value::Map(m) if is_ours(m, shortcut)));
    for (i, (k, _)) in list.0.iter_mut().enumerate() {
        *k = i.to_string().into_bytes();
    }
    list.0.len() != before
}

pub fn contains(root: &Map, shortcut: &Shortcut) -> bool {
    match root.get("shortcuts") {
        Some(Value::Map(list)) => list.0.iter().any(|(_, v)| matches!(v, Value::Map(m) if is_ours(m, shortcut))),
        _ => false,
    }
}

// --- This PC's Steam -----------------------------------------------------------

fn reg_value(key: &str, value: &str) -> Option<String> {
    let out = std::process::Command::new("reg").args(["query", key, "/v", value]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_string()))
}

/// Steam's install folder (`CSR_STEAM_DIR` overrides, for tests).
pub fn steam_root() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CSR_STEAM_DIR") {
        return Some(PathBuf::from(d));
    }
    let path = reg_value(r"HKCU\Software\Valve\Steam", "SteamPath")?;
    Some(PathBuf::from(path.replace('/', "\\"))).filter(|p| p.join("userdata").is_dir())
}

/// `config` folders of every Steam account that has used this PC.
pub fn account_configs(steam: &Path) -> Vec<PathBuf> {
    let Ok(dirs) = std::fs::read_dir(steam.join("userdata")) else { return Vec::new() };
    let mut out: Vec<PathBuf> = dirs
        .flatten()
        .filter(|d| d.file_name().to_str().and_then(|n| n.parse::<u64>().ok()).is_some_and(|id| id != 0))
        .map(|d| d.path().join("config"))
        .collect();
    out.sort();
    out
}

pub fn steam_running() -> bool {
    if std::env::var_os("CSR_STEAM_DIR").is_some() {
        return std::env::var_os("CSR_STEAM_RUNNING").is_some();
    }
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq steam.exe", "/NH"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("steam.exe"))
}

/// Asks Steam to exit and waits up to 30 seconds for it to close.
fn shut_down_steam(steam: &Path) -> bool {
    let exe = reg_value(r"HKCU\Software\Valve\Steam", "SteamExe").map(PathBuf::from).unwrap_or_else(|| steam.join("steam.exe"));
    if std::process::Command::new(exe).arg("-shutdown").spawn().is_err() {
        return false;
    }
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !steam_running() {
            // Let it finish writing its files.
            std::thread::sleep(std::time::Duration::from_secs(2));
            return true;
        }
    }
    false
}

fn start_steam() {
    dialog::open_url("steam://open/games");
}

/// What the shortcut should launch: the installed app's stable entry point,
/// or this exe.
pub fn this_install() -> Option<Shortcut> {
    let exe = crate::update::launch_exe().or_else(|| std::env::current_exe().ok())?;
    Some(Shortcut { name: APP_NAME.to_string(), exe })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("csr-tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Adds (or refreshes) the shortcut and its artwork in one account's
/// `config` folder.
pub fn add_to_account(config: &Path, shortcut: &Shortcut) -> Result<(), String> {
    let file = config.join("shortcuts.vdf");
    let mut root = match std::fs::read(&file) {
        Ok(bytes) => parse(&bytes).map_err(|e| format!("{} is not readable ({e}); left unchanged", file.display()))?,
        Err(_) => Map::default(),
    };
    let grid = config.join("grid");
    let id = shortcut.app_id();
    for (suffix, image) in crate::art::steam_library() {
        write_atomic(&grid.join(format!("{id}{suffix}.png")), &image.png()).map_err(|e| format!("Cannot write artwork to {}: {e}", grid.display()))?;
    }
    upsert(&mut root, shortcut, &grid.join(format!("{id}_icon.png")).display().to_string());
    write_atomic(&file, &write(&root)).map_err(|e| format!("Cannot write {}: {e}", file.display()))
}

/// Removes the shortcut and its artwork from one account.
pub fn remove_from_account(config: &Path, shortcut: &Shortcut) -> Result<bool, String> {
    let file = config.join("shortcuts.vdf");
    let Ok(bytes) = std::fs::read(&file) else { return Ok(false) };
    let mut root = parse(&bytes)?;
    let id = shortcut.app_id();
    for (suffix, _) in [("p", ()), ("", ()), ("_hero", ()), ("_logo", ()), ("_icon", ())] {
        let _ = std::fs::remove_file(config.join("grid").join(format!("{id}{suffix}.png")));
    }
    if !remove(&mut root, shortcut) {
        return Ok(false);
    }
    write_atomic(&file, &write(&root)).map_err(|e| e.to_string())?;
    Ok(true)
}

fn in_every_account(configs: &[PathBuf], shortcut: &Shortcut) -> bool {
    configs.iter().all(|c| std::fs::read(c.join("shortcuts.vdf")).ok().and_then(|b| parse(&b).ok()).is_some_and(|root| contains(&root, shortcut)))
}

fn add_everywhere(configs: &[PathBuf], shortcut: &Shortcut) -> Result<(), String> {
    let errors: Vec<String> = configs.iter().filter_map(|c| add_to_account(c, shortcut).err()).collect();
    if errors.len() == configs.len() { Err(errors.join("\n")) } else { Ok(()) }
}

fn read_setup(data: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(data.join(SETUP_FILE))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

fn remember(data: &Path, answer: &str) {
    let mut entries = read_setup(data);
    entries.retain(|(k, _)| k != OFFER_KEY);
    entries.push((OFFER_KEY.into(), answer.into()));
    let text: String = entries.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    let _ = write_atomic(&data.join(SETUP_FILE), text.as_bytes());
}

/// Offers to add the game to Steam, once: the answer is kept in
/// `config\setup.cfg` unless the player picks "Not now". `force` (`csr
/// add-to-steam`) asks even if they answered before. Returns whether the
/// shortcut is in Steam afterwards.
pub fn offer(data: &Path, force: bool) -> bool {
    if !force && read_setup(data).iter().any(|(k, _)| k == OFFER_KEY) {
        return false;
    }
    let (Some(steam), Some(shortcut)) = (steam_root(), this_install()) else {
        if force {
            dialog::message(APP_NAME, "Steam is not installed on this PC, so CSGO Rewrite cannot be added to it.");
        }
        return false;
    };
    let configs = account_configs(&steam);
    if configs.is_empty() {
        return false;
    }
    if in_every_account(&configs, &shortcut) && !force {
        remember(data, "added");
        return true;
    }
    let intro = "Add CSGO Rewrite to your Steam library?\n\n\
        It shows up with your other games, with its own artwork, so you can start it from Steam.";
    let running = steam_running();
    let add = if running {
        let text = format!(
            "{intro}\n\nSteam is open and has to restart to show it. CSGO Rewrite closes Steam, \
             adds the game and opens Steam again. If you are in a Steam call or chat, pick \"Not now\" \
             and you will be asked again next time."
        );
        match dialog::choose(APP_NAME, &text, &["Add and restart Steam", "Not now", "No thanks"]) {
            Some(0) => true,
            Some(2) => {
                remember(data, "declined");
                false
            }
            _ => false,
        }
    } else {
        match dialog::choose(APP_NAME, intro, &["Add to Steam", "No thanks"]) {
            Some(0) => true,
            Some(1) => {
                remember(data, "declined");
                false
            }
            _ => false,
        }
    };
    if !add {
        return false;
    }
    if running && !shut_down_steam(&steam) {
        dialog::message(APP_NAME, "Steam did not close, so CSGO Rewrite was not added this time. You will be asked again next time you start the game.");
        return false;
    }
    let result = add_everywhere(&configs, &shortcut);
    if running {
        start_steam();
    }
    match result {
        Ok(()) => {
            remember(data, "added");
            eprintln!("Added {APP_NAME} to Steam (shortcut app id {}).", shortcut.app_id());
            true
        }
        Err(e) => {
            dialog::message(APP_NAME, &format!("Could not add CSGO Rewrite to Steam:\n\n{e}"));
            false
        }
    }
}

/// Uninstall hook: takes our shortcut and artwork out of Steam again.
pub fn remove_everywhere() {
    let (Some(steam), Some(shortcut)) = (steam_root(), this_install()) else { return };
    for config in account_configs(&steam) {
        let _ = remove_from_account(&config, &shortcut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shortcuts.vdf as Steam writes it, with one game added by hand
    /// (including a category tag and a float/uint64 value we never touch).
    fn fixture() -> Vec<u8> {
        let mut b = Vec::new();
        let kv = |b: &mut Vec<u8>, t: u8, k: &str| {
            b.push(t);
            b.extend_from_slice(k.as_bytes());
            b.push(0);
        };
        let s = |b: &mut Vec<u8>, k: &str, v: &str| {
            kv(b, T_STR, k);
            b.extend_from_slice(v.as_bytes());
            b.push(0);
        };
        let i = |b: &mut Vec<u8>, k: &str, v: i32| {
            kv(b, T_INT, k);
            b.extend_from_slice(&v.to_le_bytes());
        };
        kv(&mut b, T_MAP, "shortcuts");
        kv(&mut b, T_MAP, "0");
        i(&mut b, "appid", -1_234_567_890);
        s(&mut b, "AppName", "Emulator ✓");
        s(&mut b, "Exe", "\"C:\\Emu\\emu.exe\"");
        s(&mut b, "StartDir", "\"C:\\Emu\\\"");
        s(&mut b, "icon", "");
        s(&mut b, "LaunchOptions", "-fullscreen");
        i(&mut b, "IsHidden", 0);
        i(&mut b, "LastPlayTime", 1_700_000_000);
        kv(&mut b, T_FLOAT, "Scale");
        b.extend_from_slice(&1.5f32.to_bits().to_le_bytes());
        kv(&mut b, T_U64, "Big");
        b.extend_from_slice(&u64::MAX.to_le_bytes());
        kv(&mut b, T_MAP, "tags");
        s(&mut b, "0", "Favorites");
        b.push(T_END); // tags
        b.push(T_END); // "0"
        b.push(T_END); // shortcuts
        b.push(T_END); // root
        b
    }

    fn ours() -> Shortcut {
        Shortcut { name: APP_NAME.into(), exe: PathBuf::from(r"C:\Users\Friend\AppData\Local\CSGORewrite\current\CSGO-Rewrite.exe") }
    }

    #[test]
    fn round_trips_byte_for_byte() {
        let bytes = fixture();
        assert_eq!(write(&parse(&bytes).unwrap()), bytes);
        // Steam's file with no shortcuts.
        let empty = b"\x00shortcuts\x00\x08\x08".to_vec();
        assert_eq!(write(&parse(&empty).unwrap()), empty);
    }

    #[test]
    fn adding_keeps_existing_entries_and_appends_ours() {
        let original = fixture();
        let mut root = parse(&original).unwrap();
        upsert(&mut root, &ours(), r"C:\grid\icon.png");
        let out = write(&root);

        // The existing entry is untouched: its bytes appear verbatim.
        let first_entry = &original[b"\x00shortcuts\x00".len()..original.len() - 2];
        assert!(out.windows(first_entry.len()).any(|w| w == first_entry));

        let reread = parse(&out).unwrap();
        let Some(Value::Map(list)) = reread.get("shortcuts") else { panic!("no shortcuts") };
        assert_eq!(list.0.len(), 2);
        assert_eq!(list.0[1].0, b"1");
        let Value::Map(entry) = &list.0[1].1 else { panic!() };
        assert_eq!(entry.get("appid"), Some(&Value::Int(ours().app_id() as i32)));
        assert_eq!(entry.str("Exe").unwrap(), format!("\"{}\"", ours().exe.display()));
        assert_eq!(entry.str("StartDir").unwrap(), r#""C:\Users\Friend\AppData\Local\CSGORewrite\current""#);
        assert_eq!(entry.str("AppName").unwrap(), "CSGO Rewrite");
        assert!(contains(&reread, &ours()));

        // Adding again refreshes instead of duplicating, and keeps play time.
        let mut again = reread.clone();
        if let Some(Value::Map(list)) = again.0.iter_mut().find(|(k, _)| k == b"shortcuts").map(|(_, v)| v) {
            if let Value::Map(e) = &mut list.0[1].1 {
                e.set("LastPlayTime", Value::Int(42));
            }
        }
        upsert(&mut again, &ours(), r"C:\grid\icon.png");
        let Some(Value::Map(list)) = again.get("shortcuts") else { panic!() };
        assert_eq!(list.0.len(), 2);
        let Value::Map(e) = &list.0[1].1 else { panic!() };
        assert_eq!(e.get("LastPlayTime"), Some(&Value::Int(42)));

        // Removing restores the original file exactly.
        assert!(remove(&mut again, &ours()));
        assert_eq!(write(&again), original);
    }

    #[test]
    fn creates_the_file_structure_when_missing() {
        let mut root = Map::default();
        upsert(&mut root, &ours(), "");
        let out = write(&root);
        assert!(out.starts_with(b"\x00shortcuts\x00\x000\x00\x02appid\x00"));
        assert!(out.ends_with(b"\x08\x08\x08\x08"));
    }

    #[test]
    fn rejects_damaged_files() {
        assert!(parse(b"\x00shortcuts\x00\x00").is_err());
        assert!(parse(b"\x05weird\x00").is_err());
        assert!(parse(b"\x00shortcuts\x00\x08\x08trailing").is_err());
    }

    #[test]
    fn app_id_matches_steams_formula() {
        // CRC-32 (IEEE) of the quoted exe plus the name, top bit set:
        // Python: zlib.crc32(rb'"C:\Games\game.exe"My Game') | 0x80000000
        assert_eq!(shortcut_app_id("\"C:\\Games\\game.exe\"", "My Game"), 3_017_974_625);
        assert!(ours().app_id() & 0x8000_0000 != 0);
    }

    #[test]
    fn adds_to_every_account_with_artwork() {
        let steam = std::env::temp_dir().join(format!("csr-steam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&steam);
        for id in ["0", "111", "222", "anonymous"] {
            std::fs::create_dir_all(steam.join("userdata").join(id).join("config")).unwrap();
        }
        std::fs::write(steam.join(r"userdata\111\config\shortcuts.vdf"), fixture()).unwrap();
        let configs = account_configs(&steam);
        assert_eq!(configs, vec![steam.join(r"userdata\111\config"), steam.join(r"userdata\222\config")]);
        add_everywhere(&configs, &ours()).unwrap();
        assert!(in_every_account(&configs, &ours()));
        let id = ours().app_id();
        for c in &configs {
            for suffix in ["p", "", "_hero", "_logo", "_icon"] {
                assert!(c.join("grid").join(format!("{id}{suffix}.png")).is_file());
            }
        }
        for c in &configs {
            assert!(remove_from_account(c, &ours()).unwrap());
        }
        assert_eq!(std::fs::read(steam.join(r"userdata\111\config\shortcuts.vdf")).unwrap(), fixture());
    }
}
