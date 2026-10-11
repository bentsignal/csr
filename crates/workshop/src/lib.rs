//! Old map versions from Valve's own Steam Workshop snapshots.
//!
//! Whenever Valve changed a competitive map it uploaded the old version to
//! the app 730 Workshop as "`<Map>` [Compatibility Version 1.xx.x.x]", so old
//! demos keep playing (research: `docs/research/2018-era.md` §1). Those
//! uploads are the classic map archives this crate retrieves. The archives
//! contain BSPs only: bot navigation and fidelity still need validation.
//!
//! * [`OLD_MAPS`] is a built-in table of map name → Workshop item. The ids,
//!   sizes and SHA-1s are facts checked against Steam's public, keyless
//!   `GetPublishedFileDetails` API; no map data lives in this repository.
//! * [`Store`] keeps the maps in the player's data folder
//!   (`workshop\maps\<name>.bsp`). [`Store::fetch`] takes the item from a
//!   Steam library if the player already has it
//!   (`steamapps\workshop\content\730\<id>\*_legacy.bin`), and otherwise
//!   downloads the item's `file_url` straight from Valve's CDN (owner
//!   decision 6: zero friction). Downloads resume, are size and SHA-1
//!   checked, and every file appears atomically (written to `.part`, then
//!   renamed).
//! * [`Store::mount`] adds the folder to a [`vfs::Vfs`] so `maps/<name>.bsp`
//!   loads like any installed map.
//!
//! Map names are `<original>_<year>`, for example `de_dust2_2017`; see
//! [`OldMap::name`]. They never collide with the install's own maps.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha1::{Digest, Sha1};

/// SteamID64 of Valve's official Workshop account, which uploaded every
/// compatibility version.
pub const VALVE_CREATOR: &str = "76561198082857351";
/// The Workshop the snapshots live in (app 730, the CS:GO / CS2 app).
pub const WORKSHOP_APP_ID: u32 = 730;
const DETAILS_URL: &str = "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";

/// One classic map version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OldMap {
    /// Map name in this game: `<original>_<year>`. For a map from before a
    /// rework, the year is the year Valve archived it. A snapshot's archive
    /// date does not prove it matches an earlier version of the map.
    pub name: &'static str,
    /// The original map name (`de_dust2`), also the name inside the BSP.
    pub base: &'static str,
    /// Menu title.
    pub title: &'static str,
    /// What sets this version apart, for the menu.
    pub note: &'static str,
    pub workshop_id: u64,
    /// The game build Valve archived the map from.
    pub version: &'static str,
    /// Size in bytes of the Workshop file (a zip holding one `.bsp`).
    pub size: u64,
    /// SHA-1 of the Workshop file (also the last part of its CDN URL).
    pub sha1: &'static str,
}

impl OldMap {
    /// Stable identity for recipes, content packs and server map selection.
    pub fn content_id(&self) -> content::ContentId {
        content::ContentId::csgo(content::ContentKind::Map, self.name)
    }

    pub fn workshop_page(&self) -> String {
        format!("https://steamcommunity.com/sharedfiles/filedetails/?id={}", self.workshop_id)
    }
}

/// Valve's compatibility versions, checked 2026-10-11 with
/// `GetPublishedFileDetails`: creator [`VALVE_CREATOR`], consumer app 730,
/// filename `mymaps/<base>.bsp`, and the size and SHA-1 of `file_url`.
pub const OLD_MAPS: &[OldMap] = &[
    OldMap {
        name: "de_dust2_2017",
        base: "de_dust2",
        title: "Dust II (2017)",
        note: "Classic Dust II, before the October 2017 rework",
        workshop_id: 1164734849,
        version: "1.36.0.6",
        size: 20584011,
        sha1: "17c69430f44440af3fe6e3a5cd78194224891aa7",
    },
    OldMap {
        name: "de_inferno_2016",
        base: "de_inferno",
        title: "Inferno (2016)",
        note: "Classic Inferno, before the October 2016 rework",
        workshop_id: 778803540,
        version: "1.35.5.2",
        size: 13031292,
        sha1: "83147c2c763edec6474d6f583f30fbf95af6b6cc",
    },
    OldMap {
        name: "de_nuke_2016",
        base: "de_nuke",
        title: "Nuke (2016)",
        note: "Classic Nuke, before the February 2016 rework",
        workshop_id: 626513993,
        version: "1.35.2.0",
        size: 9841028,
        sha1: "49a77959b2e6911856ce5c5b6f1f4f122287406d",
    },
    OldMap {
        name: "de_train_2015",
        base: "de_train",
        title: "Train (2015)",
        note: "Valve's October 2015 snapshot (after the December 2014 rework)",
        workshop_id: 538091991,
        version: "1.35.0.6",
        size: 20440332,
        sha1: "0953152d6d3993515b56a8b2647f9e3095185647",
    },
    OldMap {
        name: "de_vertigo_2017",
        base: "de_vertigo",
        title: "Vertigo (2017)",
        note: "Original Vertigo, before the 2019 rework",
        workshop_id: 1193870504,
        version: "1.36.1.2",
        size: 13313912,
        sha1: "1c44a4a54521765b79b5a1593bca52c2038e0ae7",
    },
    OldMap {
        name: "de_dust2_2019",
        base: "de_dust2",
        title: "Dust II (2019)",
        note: "Valve's June 2019 snapshot; August 2018 parity unverified",
        workshop_id: 1773333823,
        version: "1.37.0.1",
        size: 86340110,
        sha1: "4090bbbcd2b695552a5b3daec8a0e53664950f0c",
    },
    OldMap {
        name: "de_inferno_2020",
        base: "de_inferno",
        title: "Inferno (2020)",
        note: "Valve's April 2020 snapshot; August 2018 parity unverified",
        workshop_id: 2054468706,
        version: "1.37.4.6",
        size: 27011674,
        sha1: "083200b3f067164c581a23318c14958df7f71429",
    },
    OldMap {
        name: "de_nuke_2019",
        base: "de_nuke",
        title: "Nuke (2019)",
        note: "Valve's October 2019 snapshot; August 2018 parity unverified",
        workshop_id: 1885085042,
        version: "1.37.1.8",
        size: 64946679,
        sha1: "557e979caebd4c3966e6633c394914ebc641f3f8",
    },
    OldMap {
        name: "de_cache_2019",
        base: "de_cache",
        title: "Cache (2019)",
        note: "Valve's October 2019 snapshot; August 2018 parity unverified",
        workshop_id: 1891729882,
        version: "1.37.1.9",
        size: 72588212,
        sha1: "2ac990b187266d0d0705ac4375273e2398cb3474",
    },
    OldMap {
        name: "de_train_2020",
        base: "de_train",
        title: "Train (2020)",
        note: "Valve's June 2020 snapshot; August 2018 parity unverified",
        workshop_id: 2126172171,
        version: "1.37.5.5",
        size: 31315723,
        sha1: "330448b620a7648d489952e47e4a1f31bff10dc7",
    },
    OldMap {
        name: "de_mirage_2019",
        base: "de_mirage",
        title: "Mirage (2019)",
        note: "Valve's December 2019 snapshot; August 2018 parity unverified",
        workshop_id: 1935967249,
        version: "1.37.3.2",
        size: 16805449,
        sha1: "0767980881e0c7b7c0bcaebbb9ca95235d2683fd",
    },
    OldMap {
        name: "de_overpass_2019",
        base: "de_overpass",
        title: "Overpass (2019)",
        note: "Valve's July 2019 snapshot; August 2018 parity unverified",
        workshop_id: 1821117172,
        version: "1.37.0.7",
        size: 15074693,
        sha1: "7d47c821e5a2b5c061d6df360e71e639166ac8d3",
    },
    OldMap {
        name: "de_cbble_2018",
        base: "de_cbble",
        title: "Cobblestone (2018)",
        note: "Valve's October 2018 snapshot",
        workshop_id: 1542127528,
        version: "1.36.5.6",
        size: 31389737,
        sha1: "950f3db8938767344a68cce66077aac64e813df6",
    },
    OldMap {
        name: "de_canals_2018",
        base: "de_canals",
        title: "Canals (2018)",
        note: "Valve's October 2018 snapshot",
        workshop_id: 1534022416,
        version: "1.36.5.5",
        size: 58850847,
        sha1: "e10c2ff4ffca45052293366e531df69dcc173f65",
    },
];

/// The table entry for a map name (`de_dust2_2017`), if it is an old map.
pub fn find(name: &str) -> Option<&'static OldMap> {
    OLD_MAPS.iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// Where an old map can come from right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Map data downloaded: `maps/<name>.bsp` exists in the data folder.
    Installed,
    /// Not installed, but a Steam library already holds the Workshop file;
    /// fetching copies it out without downloading.
    InSteam(PathBuf),
    /// Needs a download from Valve's CDN.
    Download,
}

/// What [`Store::fetch`] is doing, for progress bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Asking Steam's API for the download address.
    Query,
    /// Downloading: `done` of `total` bytes.
    Download,
    /// Checking the SHA-1 of the Workshop file.
    Verify,
    /// Unpacking the `.bsp`: `done` of `total` bytes.
    Extract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub done: u64,
    pub total: u64,
}

/// The player's old maps: `<data>\workshop`.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
    libraries: Vec<PathBuf>,
}

/// Subfolder of the data folder that holds the old maps.
pub const FOLDER: &str = "workshop";

impl Store {
    /// The store in `data_dir` (the player data folder), searching every
    /// Steam library on this PC for copies the player already has.
    pub fn new(data_dir: &Path) -> Store {
        Store::with_libraries(data_dir, vfs::steam_libraries())
    }

    /// The store in the current data folder: `CSR_DATA_DIR`, or the working
    /// directory (the launcher runs inside the data folder).
    pub fn current() -> Store {
        let data = std::env::var_os("CSR_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        Store::new(&data)
    }

    /// As [`Store::new`], with explicit Steam library folders (tests).
    pub fn with_libraries(data_dir: &Path, libraries: Vec<PathBuf>) -> Store {
        Store {
            root: data_dir.join(FOLDER),
            libraries,
        }
    }

    /// `<data>\workshop`, the search path [`Store::mount`] adds.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the map's `.bsp` lives once installed.
    pub fn bsp_path(&self, map: &OldMap) -> PathBuf {
        self.root.join("maps").join(format!("{}.bsp", map.name))
    }

    fn part_path(&self, map: &OldMap) -> PathBuf {
        self.root.join("download").join(format!("{}.bin.part", map.workshop_id))
    }

    pub fn status(&self, map: &OldMap) -> Status {
        if self.bsp_path(map).is_file() {
            return Status::Installed;
        }
        match self.steam_copy(map) {
            Some(bin) => Status::InSteam(bin),
            None => Status::Download,
        }
    }

    /// Every old map with its status, in table order (for menus).
    pub fn list(&self) -> Vec<(&'static OldMap, Status)> {
        OLD_MAPS.iter().map(|m| (m, self.status(m))).collect()
    }

    /// Adds the store to a file system at the lowest priority, so
    /// `maps/<name>.bsp` resolves for installed old maps and nothing in the
    /// game install can be shadowed. Maps fetched later resolve too.
    pub fn mount(&self, fs: &mut vfs::Vfs) {
        fs.push_back(format!("workshop:{}", self.root.display()), vfs::Mount::Dir(self.root.clone()));
    }

    /// A Steam library's copy of the item, if it is the exact file
    /// (`steamapps\workshop\content\730\<id>\*_legacy.bin`, right size). Its
    /// SHA-1 is checked when it is used.
    pub fn steam_copy(&self, map: &OldMap) -> Option<PathBuf> {
        for lib in &self.libraries {
            let dir = lib
                .join("steamapps/workshop/content")
                .join(WORKSHOP_APP_ID.to_string())
                .join(map.workshop_id.to_string());
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let legacy = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase().ends_with("_legacy.bin"));
                if legacy && entry.metadata().is_ok_and(|m| m.len() == map.size) {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Retrieves the map archive and returns its `.bsp`. Already installed:
    /// returns at once. Otherwise copies it out of a Steam library, or
    /// downloads it from Valve's CDN. `progress` is called as work proceeds;
    /// return `false` from it to cancel (a partial download is kept and
    /// resumed next time).
    pub fn fetch(&self, map: &OldMap, progress: &mut dyn FnMut(Progress) -> bool) -> Result<PathBuf, String> {
        let bsp = self.bsp_path(map);
        if bsp.is_file() {
            return Ok(bsp);
        }
        if let Some(bin) = self.steam_copy(map) {
            if sha1_file(&bin, map.size, &mut |p| progress(p)).map_err(|e| format!("{}: {e}", bin.display()))? == map.sha1 {
                extract(&bin, &bsp, progress)?;
                return Ok(bsp);
            }
            // A different file under the same item id: ignore it, download.
        }
        let part = self.part_path(map);
        self.download(map, &part, progress)?;
        if !progress(Progress { stage: Stage::Verify, done: 0, total: map.size }) {
            return Err(CANCELLED.into());
        }
        let hash = sha1_file(&part, map.size, &mut |p| progress(p)).map_err(|e| format!("{}: {e}", part.display()))?;
        if hash != map.sha1 {
            let _ = fs::remove_file(&part);
            return Err(format!("{} downloaded with the wrong checksum (got {hash}); try again", map.title));
        }
        extract(&part, &bsp, progress)?;
        let _ = fs::remove_file(&part);
        Ok(bsp)
    }

    /// Deletes an installed old map (and any partial download).
    pub fn remove(&self, map: &OldMap) -> io::Result<bool> {
        let _ = fs::remove_file(self.part_path(map));
        match fs::remove_file(self.bsp_path(map)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn download(&self, map: &OldMap, part: &Path, progress: &mut dyn FnMut(Progress) -> bool) -> Result<(), String> {
        if let Some(dir) = part.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let have = prepare_partial(part, map.size).map_err(|e| format!("{}: {e}", part.display()))?;
        if have == map.size {
            return Ok(());
        }
        if !progress(Progress { stage: Stage::Query, done: 0, total: 0 }) {
            return Err(CANCELLED.into());
        }
        let agent = agent();
        let url = file_url(&agent, map)?;
        let mut request = agent.get(&url);
        if have > 0 {
            request = request.header("Range", format!("bytes={have}-"));
        }
        let response = request.call().map_err(|e| format!("Could not download {} from Steam: {e}", map.title))?;
        let done = response_offset(response.status().as_u16(), response.headers().get("Content-Range").and_then(|v| v.to_str().ok()), have, map.size)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(part)
            .map_err(|e| format!("{}: {e}", part.display()))?;
        let mut done = done;
        file.set_len(done).and_then(|_| file.seek(SeekFrom::Start(done))).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut body = response.into_body().into_reader();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = body.read(&mut buf).map_err(|e| format!("Download of {} interrupted: {e}", map.title))?;
            if n == 0 {
                break;
            }
            done += n as u64;
            if done > map.size {
                drop(file);
                let _ = fs::remove_file(part);
                return Err(format!("{} is bigger than expected; try again", map.title));
            }
            file.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
            if !progress(Progress { stage: Stage::Download, done, total: map.size }) {
                return Err(CANCELLED.into());
            }
        }
        file.flush().map_err(|e| e.to_string())?;
        if done != map.size {
            return Err(format!("Download of {} stopped early ({done} of {} bytes); try again", map.title, map.size));
        }
        Ok(())
    }
}

/// Returned by [`Store::fetch`] when the progress callback cancelled.
pub const CANCELLED: &str = "cancelled";

/// Keep resumable bytes, but restart an oversized partial in this attempt.
fn prepare_partial(part: &Path, total: u64) -> io::Result<u64> {
    match fs::metadata(part) {
        Ok(meta) if meta.len() > total => {
            fs::OpenOptions::new().write(true).open(part)?.set_len(0)?;
            Ok(0)
        }
        Ok(meta) => Ok(meta.len()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}

/// A CDN can ignore Range and send a full response; only append when it
/// confirms the requested offset and the pinned file size.
fn response_offset(status: u16, range: Option<&str>, have: u64, total: u64) -> Result<u64, String> {
    if status == 200 { return Ok(0); }
    let valid = range.and_then(|s| s.strip_prefix("bytes "))
        .and_then(|s| s.split_once('/'))
        .and_then(|(span, size)| Some((span.split_once('-')?, size.parse::<u64>().ok()?)))
        .and_then(|((start, end), size)| Some((start.parse::<u64>().ok()?, end.parse::<u64>().ok()?, size)))
        .is_some_and(|(start, end, size)| start == have && end >= start && end < total && size == total);
    if status == 206 && valid { Ok(have) }
    else { Err("Steam's download response does not match the requested byte range; try again".into()) }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .user_agent(concat!("csgo-rewrite/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent()
}

/// Asks Steam's public API where the item's file is, and checks the answer
/// against the table: Valve's account, app 730, same size and SHA-1.
fn file_url(agent: &ureq::Agent, map: &OldMap) -> Result<String, String> {
    let id = map.workshop_id.to_string();
    let mut response = agent
        .post(DETAILS_URL)
        .send_form([("itemcount", "1"), ("publishedfileids[0]", id.as_str())])
        .map_err(|e| format!("Could not reach Steam to fetch {}: {e}", map.title))?;
    let text = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse_details(&text, map)
}

fn parse_details(text: &str, map: &OldMap) -> Result<String, String> {
    let doc: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Steam answered with bad data: {e}"))?;
    let item = &doc["response"]["publishedfiledetails"][0];
    let bad = |what: &str| Err(format!("Steam's details for Workshop item {} do not match ({what})", map.workshop_id));
    if item["result"].as_i64() != Some(1) {
        return Err(format!("Workshop item {} is not available on Steam any more", map.workshop_id));
    }
    if item["creator"].as_str() != Some(VALVE_CREATOR) {
        return bad("creator");
    }
    if item["consumer_app_id"].as_u64() != Some(WORKSHOP_APP_ID as u64) {
        return bad("app");
    }
    if item["publishedfileid"].as_str().and_then(|s| s.parse::<u64>().ok()) != Some(map.workshop_id) {
        return bad("item id");
    }
    if item["filename"].as_str() != Some(format!("mymaps/{}.bsp", map.base).as_str()) {
        return bad("filename");
    }
    let size = item["file_size"].as_u64().or_else(|| item["file_size"].as_str().and_then(|s| s.parse().ok()));
    if size != Some(map.size) {
        return bad("size");
    }
    let Some(url) = item["file_url"].as_str() else { return bad("no file_url") };
    if !url.starts_with("https://cdn.steamusercontent.com/ugc/") {
        return bad("CDN address");
    }
    if !url.trim_end_matches('/').to_ascii_lowercase().ends_with(map.sha1) {
        return bad("checksum");
    }
    Ok(url.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-1 of a file, as lowercase hex.
fn sha1_file(path: &Path, total: u64, progress: &mut dyn FnMut(Progress) -> bool) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        if !progress(Progress { stage: Stage::Verify, done, total }) {
            return Err(io::Error::other(CANCELLED));
        }
    }
    Ok(hex(&hasher.finalize()))
}

/// Unpacks the one `.bsp` in a Workshop `.bin` (a zip) to `bsp`, through a
/// `.part` file so a half-written map never appears. The zip reader checks
/// the entry's CRC.
fn extract(bin: &Path, bsp: &Path, progress: &mut dyn FnMut(Progress) -> bool) -> Result<(), String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", bin.display());
    let mut zip = zip::ZipArchive::new(File::open(bin).map_err(|e| err(&e))?).map_err(|e| err(&e))?;
    let index = (0..zip.len())
        .find(|&i| zip.by_index(i).is_ok_and(|f| f.name().to_ascii_lowercase().ends_with(".bsp")))
        .ok_or_else(|| err(&"no .bsp inside"))?;
    let mut entry = zip.by_index(index).map_err(|e| err(&e))?;
    let total = entry.size();
    let dir = bsp.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = bsp.with_extension("bsp.part");
    let result = (|| -> io::Result<()> {
        let mut out = io::BufWriter::new(File::create(&tmp)?);
        let mut buf = vec![0u8; 1 << 16];
        let mut done = 0u64;
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
            if !progress(Progress { stage: Stage::Extract, done, total }) {
                return Err(io::Error::other(CANCELLED));
            }
        }
        out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        if done != total {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated map in the Workshop file"));
        }
        fs::rename(&tmp, bsp)
    })();
    result.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        if e.to_string() == CANCELLED { CANCELLED.to_string() } else { err(&e) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("csr-workshop-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// A Workshop-style `.bin`: a zip holding one `.bsp`.
    fn fake_bin(bsp: &[u8]) -> Vec<u8> {
        let mut out = io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("de_test.bsp", opts).unwrap();
        zip.write_all(bsp).unwrap();
        zip.finish().unwrap();
        out.into_inner()
    }

    fn fake_map(bin: &[u8]) -> OldMap {
        let sha1 = Box::leak(hex(&Sha1::digest(bin)).into_boxed_str());
        OldMap {
            name: "de_test_2001",
            base: "de_test",
            title: "Test (2001)",
            note: "",
            workshop_id: 42,
            version: "1.0.0.0",
            size: bin.len() as u64,
            sha1,
        }
    }

    #[test]
    fn table_is_consistent() {
        let mut names = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for m in OLD_MAPS {
            assert!(names.insert(m.name), "duplicate {}", m.name);
            assert!(ids.insert(m.content_id()), "duplicate content id {}", m.name);
            assert!(m.name.starts_with(&format!("{}_", m.base)), "{}", m.name);
            assert_eq!(m.sha1.len(), 40);
            assert!(m.sha1.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(m.size > 1_000_000);
        }
        assert_eq!(find("DE_DUST2_2017").unwrap().workshop_id, 1164734849);
        assert_eq!(find("de_dust2_2017").unwrap().content_id().to_string(), "csgo_map_de_dust2_2017");
        assert!(find("de_dust2").is_none());
    }

    #[test]
    fn takes_the_map_from_a_steam_library_without_downloading() {
        let bsp = b"VBSP fake map contents".repeat(1000);
        let bin = fake_bin(&bsp);
        let map = fake_map(&bin);
        let lib = temp("lib");
        let item = lib.join(r"steamapps\workshop\content\730\42");
        fs::create_dir_all(&item).unwrap();
        fs::write(item.join("123456789_legacy.bin"), &bin).unwrap();

        let data = temp("data");
        let store = Store::with_libraries(&data, vec![temp("empty-lib"), lib.clone()]);
        assert_eq!(store.status(&map), Status::InSteam(item.join("123456789_legacy.bin")));
        let mut stages = Vec::new();
        let path = store
            .fetch(&map, &mut |p| {
                stages.push(p.stage);
                true
            })
            .unwrap();
        assert!(!stages.contains(&Stage::Download) && !stages.contains(&Stage::Query));
        assert_eq!(path, data.join(r"workshop\maps\de_test_2001.bsp"));
        assert_eq!(fs::read(&path).unwrap(), bsp);
        assert_eq!(store.status(&map), Status::Installed);
        assert!(!data.join(r"workshop\maps\de_test_2001.bsp.part").exists());

        // Mounted, the map resolves like an installed one.
        let mut fs_ = vfs::Vfs::new();
        store.mount(&mut fs_);
        assert_eq!(fs_.read("maps/de_test_2001.bsp").unwrap(), bsp);

        assert!(store.remove(&map).unwrap());
        assert!(!store.remove(&map).unwrap());
    }

    #[test]
    fn ignores_steam_copies_that_are_not_the_snapshot() {
        let bin = fake_bin(b"VBSP right");
        let map = fake_map(&bin);
        let lib = temp("wrong-lib");
        let item = lib.join(r"steamapps\workshop\content\730\42");
        fs::create_dir_all(&item).unwrap();
        // Same size, different bytes.
        let mut wrong = bin.clone();
        let last = wrong.len() - 30;
        wrong[last] ^= 0xFF;
        fs::write(item.join("1_legacy.bin"), &wrong).unwrap();
        let store = Store::with_libraries(&temp("wrong-data"), vec![lib]);
        assert!(matches!(store.status(&map), Status::InSteam(_)));
        // Falls through to the download, which we cancel at the first step.
        let err = store.fetch(&map, &mut |p| p.stage != Stage::Query).unwrap_err();
        assert_eq!(err, CANCELLED);
        assert!(!store.bsp_path(&map).exists());
    }

    #[test]
    fn never_uses_cs2_workshop_vpks() {
        let bin = fake_bin(b"VBSP legacy bytes");
        let map = fake_map(&bin);
        let lib = temp("cs2-lib");
        let item = lib.join(r"steamapps\workshop\content\730\42");
        fs::create_dir_all(&item).unwrap();
        // Even an exact-size/checksum file must have the Legacy archive name.
        for name in ["42.vpk", "42_dir.vpk", "publish_data.txt"] {
            fs::write(item.join(name), &bin).unwrap();
        }
        let store = Store::with_libraries(&temp("cs2-data"), vec![lib]);
        assert_eq!(store.status(&map), Status::Download);
        assert_eq!(store.fetch(&map, &mut |p| p.stage != Stage::Query).unwrap_err(), CANCELLED);
        assert!(!store.bsp_path(&map).exists());
    }

    #[test]
    fn checks_steams_answer_against_the_table() {
        let map = find("de_nuke_2016").unwrap();
        let good = r#"{"response":{"result":1,"resultcount":1,"publishedfiledetails":[{"publishedfileid":"626513993","result":1,
            "creator":"76561198082857351","creator_app_id":730,"consumer_app_id":730,"filename":"mymaps/de_nuke.bsp","file_size":"9841028",
            "file_url":"https://cdn.steamusercontent.com/ugc/313368332380238705/49A77959B2E6911856CE5C5B6F1F4F122287406D/"}]}}"#;
        assert!(parse_details(good, map).unwrap().starts_with("https://cdn.steamusercontent.com/ugc/"));
        assert!(parse_details(&good.replace("76561198082857351", "76561190000000000"), map).unwrap_err().contains("creator"));
        assert!(parse_details(&good.replace("9841028", "9841029"), map).unwrap_err().contains("size"));
        assert!(parse_details(&good.replace("49A7", "49A8"), map).unwrap_err().contains("checksum"));
        assert!(parse_details(&good.replace("626513993", "626513994"), map).unwrap_err().contains("item id"));
        assert!(parse_details(&good.replace("mymaps/de_nuke.bsp", "mymaps/de_other.bsp"), map).unwrap_err().contains("filename"));
        assert!(parse_details(&good.replace("cdn.steamusercontent.com", "example.com"), map).unwrap_err().contains("CDN address"));
        assert!(parse_details(&good.replace("\"result\":1,\n", "\"result\":9,\n"), map).is_err());
    }

    #[test]
    fn finishes_a_complete_partial_without_contacting_steam() {
        let contents = b"VBSP cached download";
        let bin = fake_bin(contents);
        let map = fake_map(&bin);
        let store = Store::with_libraries(&temp("complete-partial"), Vec::new());
        let part = store.part_path(&map);
        fs::create_dir_all(part.parent().unwrap()).unwrap();
        fs::write(&part, &bin).unwrap();
        let path = store.fetch(&map, &mut |p| p.stage != Stage::Query).unwrap();
        assert_eq!(fs::read(path).unwrap(), contents);
        assert!(!part.exists());
    }

    #[test]
    fn oversized_partial_can_be_restarted_in_the_same_attempt() {
        let bin = fake_bin(b"VBSP restart");
        let map = fake_map(&bin);
        let store = Store::with_libraries(&temp("oversized-partial"), Vec::new());
        let part = store.part_path(&map);
        fs::create_dir_all(part.parent().unwrap()).unwrap();
        fs::write(&part, vec![0; map.size as usize + 1]).unwrap();
        assert_eq!(store.fetch(&map, &mut |p| p.stage != Stage::Query).unwrap_err(), CANCELLED);
        assert_eq!(fs::metadata(&part).unwrap().len(), 0, "restart must request byte zero, not treat missing bytes as complete");
    }

    #[test]
    fn resumes_only_at_the_requested_offset_and_size() {
        assert_eq!(response_offset(206, Some("bytes 40-99/100"), 40, 100).unwrap(), 40);
        assert_eq!(response_offset(200, None, 40, 100).unwrap(), 0, "a full response restarts the file");
        for range in [None, Some("bytes 0-99/100"), Some("bytes 40-99/101"), Some("bytes 40-100/100"), Some("bytes */100")] {
            assert!(response_offset(206, range, 40, 100).is_err(), "{range:?}");
        }
        assert!(response_offset(204, None, 40, 100).is_err());
    }
}
