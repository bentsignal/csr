//! Version-specific photos from Valve's verified compatibility Workshop items.
//! Only metadata ships here. JPEGs are downloaded into the private player store.
use super::{OldMap, Store, OLD_MAPS};
use sha1::{Digest, Sha1};
use std::{collections::HashSet, fs, io::{Read, Write}, sync::{Mutex, OnceLock}, time::Duration};

struct Pin<'a> { name: &'a str, url: &'a str, bytes: usize, sha1: &'a str }
// Creator, consumer app, archive file hash and preview were checked together.
const PINS: &[Pin<'static>] = &[
    Pin { name: "de_dust2_2017", url: "https://images.steamusercontent.com/ugc/140004481095992383/5E22D2C6C37A037B66E48C509A4652A77CA9E035/", bytes: 141185, sha1: "5e22d2c6c37a037b66e48c509a4652a77ca9e035" },
    Pin { name: "de_inferno_2016", url: "https://images.steamusercontent.com/ugc/258220577767501746/362C26DB9D756D24E9088E9A9D26B2D008BFAA57/", bytes: 148226, sha1: "362c26db9d756d24e9088e9a9d26b2d008bfaa57" },
    Pin { name: "de_nuke_2016", url: "https://images.steamusercontent.com/ugc/313368332380238804/928348203AD525A780B2D0FBCFAF2F02056C8A67/", bytes: 130957, sha1: "928348203ad525a780b2d0fbcfaf2f02056c8a67" },
    Pin { name: "de_train_2015", url: "https://images.steamusercontent.com/ugc/389918106243920010/7C170D1FDADF6ED9B36C98A4B20AE70A0B602F3E/", bytes: 136209, sha1: "7c170d1fdadf6ed9b36c98a4b20ae70a0b602f3e" },
    Pin { name: "de_vertigo_2017", url: "https://images.steamusercontent.com/ugc/858360646838530019/F1F07C14683706C51021940B3BFD8E523CB2C8BD/", bytes: 142499, sha1: "f1f07c14683706c51021940b3bfd8e523cb2c8bd" },
    Pin { name: "de_cache_2015", url: "https://images.steamusercontent.com/ugc/423692561595836400/E2C4A809AE4FAC61BEE4E0922C81B9542759E4D5/", bytes: 164936, sha1: "e2c4a809ae4fac61bee4e0922c81b9542759e4d5" },
    Pin { name: "de_mirage_2019", url: "https://images.steamusercontent.com/ugc/387665671598503104/9BC8E9D876916173C915233460D559231FF4E4E3/", bytes: 150376, sha1: "9bc8e9d876916173c915233460d559231ff4e4e3" },
    Pin { name: "de_cbble_2018", url: "https://images.steamusercontent.com/ugc/263838243293284444/65DE5CFFED79F09420B34795D9F4C90409FC0212/", bytes: 138758, sha1: "65de5cffed79f09420b34795d9f4c90409fc0212" },

];

pub fn supported(name: &str) -> bool { PINS.iter().any(|p| p.name == name) }

fn valid(pin: &Pin, bytes: &[u8]) -> bool {
    bytes.len() == pin.bytes && pin.bytes <= 4 * 1024 * 1024
        && bytes.starts_with(&[0xff, 0xd8])
        && format!("{:x}", Sha1::digest(bytes)) == pin.sha1
}

impl Store {
    fn preview_path(&self, map: &OldMap) -> std::path::PathBuf {
        self.root.join("previews").join(format!("{}.jpg", map.name))
    }

    /// Read only the selected version's exact, bounded cached JPEG.
    pub fn preview_bytes(&self, map: &OldMap) -> Option<Vec<u8>> {
        let pin = PINS.iter().find(|p| p.name == map.name)?;
        let path = self.preview_path(map);
        if fs::symlink_metadata(&path).ok()?.file_type().is_symlink() { return None; }
        let file = fs::File::open(path).ok()?;
        if file.metadata().ok()?.len() != pin.bytes as u64 { return None; }
        let mut bytes = Vec::with_capacity(pin.bytes);
        file.take(pin.bytes as u64 + 1).read_to_end(&mut bytes).ok()?;
        valid(pin, &bytes).then_some(bytes)
    }

    /// One background pass per map/store/process; never perform network I/O on the
    /// UI movie thread. Existing valid photos work entirely offline.
    pub fn prefetch_previews(&self, installed: &HashSet<String>) {
        let mut maps: Vec<_> = OLD_MAPS.iter().filter(|m| m.menu_supported()
            && installed.contains(m.name) && supported(m.name)
            && self.bsp_path(m).is_file()
            && self.preview_bytes(m).is_none()).copied().collect();
        if maps.is_empty() { return; }
        static STARTED: OnceLock<Mutex<HashSet<(std::path::PathBuf, &'static str)>>> = OnceLock::new();
        let mut started = STARTED.get_or_init(Default::default).lock().unwrap();
        maps.retain(|map| started.insert((self.root.clone(), map.name)));
        drop(started);
        if maps.is_empty() { return; }
        let store = self.clone();
        let _ = std::thread::Builder::new().name("classic-map-photos".into()).spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5))).build().into();
            for map in maps { let _ = store.fetch_preview(&agent, &map); }
        });
    }

    fn fetch_preview(&self, agent: &ureq::Agent, map: &OldMap) -> Result<(), String> {
        if self.preview_bytes(map).is_some() { return Ok(()); }
        let pin = PINS.iter().find(|p| p.name == map.name).ok_or("No photo for this archive")?;
        let response = agent.get(pin.url).call().map_err(|e| e.to_string())?;
        let mut bytes = Vec::with_capacity(pin.bytes);
        response.into_body().into_reader().take(pin.bytes as u64 + 1)
            .read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if !valid(pin, &bytes) { return Err("Archive photo identity mismatch".into()); }
        let path = self.preview_path(map);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let part = path.with_extension("jpg.part");
        // Another process may own this download; never truncate its temporary file.
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
            drop(file);
            if self.preview_bytes(map).is_some() { return Ok(()); }
            if path.exists() {
                if fs::symlink_metadata(&path).map_err(|e| e.to_string())?.file_type().is_symlink() {
                    return Err("Refusing linked preview target".into());
                }
                fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
            fs::rename(&part, &path).map_err(|e| e.to_string())
        })();
        let _ = fs::remove_file(&part);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn photo_identity_rejects_truncation_changes_and_unbounded_declarations() {
        let bytes = [0xff, 0xd8, 1, 2, 0xff, 0xd9];
        let hash = format!("{:x}", Sha1::digest(bytes));
        let pin = Pin { name: "synthetic", url: "", bytes: bytes.len(), sha1: &hash };
        assert!(valid(&pin, &bytes));
        assert!(!valid(&pin, &bytes[..bytes.len()-1]));
        let mut changed = bytes; changed[2] ^= 1; assert!(!valid(&pin, &changed));
        let oversized = Pin { bytes: 4 * 1024 * 1024 + 1, ..pin };
        assert!(!valid(&oversized, &bytes));
        assert!(!supported("de_nuke"));
        assert!(supported("de_nuke_2016"));
    }
}
