//! Map loading shared by the headless simulation and the renderer.

pub mod maps;

use collision::{CollisionWorld, MapCollisionOptions};
use content::{ContentId, ContentKind, GameOrigin};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub struct Level {
    pub id: ContentId,
    pub name: String,
    pub game_dir: PathBuf,
    pub vfs: vfs::Vfs,
    pub bsp: bsp::Bsp,
    pub collision: Arc<CollisionWorld>,
    /// Weapon catalog from `scripts/items/items_game.txt`.
    pub catalog: Arc<weapons::Catalog>,
    /// Navigation mesh from `maps/<name>.nav`, if the map ships one.
    pub nav: Option<Arc<nav::NavMesh>>,
    /// Private asset-map metadata never falls back to an installed map with
    /// the same basename. Recipe and built-in maps retain their own source.
    metadata_pack: Option<String>,
    pack_archive_hash: Option<String>,
    pub timings: Vec<(&'static str, f32)>,
}

#[derive(Debug)]
pub enum LoadError {
    GameNotFound,
    Io(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::GameNotFound => write!(
                f,
                "CS:GO Legacy not found. Install Counter-Strike: Global Offensive (CS:GO Legacy) from Steam, or set CSR_GAME_DIR to the folder holding gameinfo.txt"
            ),
            LoadError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Mounts the game's file system (no map).
pub fn mount_game(game_dir: Option<&Path>) -> Result<(PathBuf, vfs::Vfs), LoadError> {
    let game_dir = match game_dir {
        Some(d) => d.to_path_buf(),
        None => vfs::find_csgo_dir().ok_or(LoadError::GameNotFound)?,
    };
    let mut fs = vfs::Vfs::new();
    fs.mount_game_dir(&game_dir).map_err(|e| LoadError::Io(format!("mounting {}: {e}", game_dir.display())))?;
    // Old map versions in the player's data folder (lowest priority).
    workshop::Store::current().mount(&mut fs);
    Ok((game_dir, fs))
}

/// Fetches an old map version on first use (from a Steam library, or from
/// Valve's CDN), printing progress.
fn fetch_old_map(map: &workshop::OldMap) -> Result<(), LoadError> {
    eprintln!("Fetching {} ({}, Workshop item {})...", map.title, map.name, map.workshop_id);
    let mut last = (None, 0u64);
    workshop::Store::current()
        .fetch(map, &mut |p| {
            let tenth = if p.total > 0 { p.done * 10 / p.total } else { 0 };
            if last != (Some(p.stage), tenth) {
                last = (Some(p.stage), tenth);
                eprintln!("  {:?} {}%", p.stage, tenth * 10);
            }
            true
        })
        .map(|_| ())
        .map_err(|e| LoadError::Io(format!("{}: {e}", map.name)))
}

impl Level {
    /// Loads `maps/<name>.bsp` and builds its collision world.
    pub fn load(name: &str, game_dir: Option<&Path>) -> Result<Level, LoadError> {
        if let Ok(id) = name.parse::<ContentId>() {
            return Self::load_id(&id, game_dir);
        }
        if !packs::valid_bsp_name(name) { return Err(LoadError::Io("invalid map name".into())); }
        let t = Instant::now();
        let (game_dir, fs) = mount_game(game_dir)?;
        let mount_time = t.elapsed().as_secs_f32();
        let map_path = format!("maps/{name}.bsp");
        if !fs.exists(&map_path) {
            if let Some(old) = workshop::find(name) {
                fetch_old_map(old)?;
            }
        }
        let mut level = Self::load_from_vfs(ContentId::csgo(ContentKind::Map, name), name, game_dir, fs)?;
        level.timings.insert(0, ("mount", mount_time));
        Ok(level)
    }

    /// Canonical identity loading for menu selections and remote map requests.
    /// Built-in names come from install/archive definitions, never ID.name.
    pub fn load_id(id: &ContentId, game_dir: Option<&Path>) -> Result<Level, LoadError> {
        if id.kind != ContentKind::Map { return Err(LoadError::Io(format!("{id} is not a map"))); }
        match &id.origin {
            GameOrigin::Csgo => {
                let (game_dir, fs) = mount_game(game_dir)?;
                let store = workshop::Store::current();
                let entry = maps::definition(id, &game_dir, &fs, &store).map_err(LoadError::Io)?;
                // Known archived definitions retain their existing zero-step
                // acquisition. Fetch uses the fixed definition, not user paths.
                if let Some(old) = entry.old && entry.status != maps::MapStatus::Ready { fetch_old_map(old)?; }
                Self::load_local_id_from_vfs(id, game_dir, fs, &store)
            }
            GameOrigin::Mod | GameOrigin::Other(_) => {
                let (pack, map) = packs::Installed::current().resolve_map(id).map_err(LoadError::Io)?;
                let (game_dir, fs) = mount_game(game_dir)?;
                Self::load_pack(&pack, &map, game_dir, fs)
            }
            origin => Err(LoadError::Io(format!("no map reader is available for origin {}", origin.prefix()))),
        }
    }

    /// Resolve only already local install/archive data in a caller-owned VFS.
    /// This does not fetch or fall back to another source with the same basename.
    pub fn load_local_id_from_vfs(id: &ContentId, game_dir: PathBuf, fs: vfs::Vfs, store: &workshop::Store) -> Result<Level, LoadError> {
        let entry = maps::definition(id, &game_dir, &fs, store).map_err(LoadError::Io)?;
        let source = maps::local_source(&entry, &fs, store).map_err(LoadError::Io)?;
        Self::load_from_vfs_scoped(entry.id, &entry.name, game_dir, fs, None, Some(&source))
    }

    /// Keep a menu's selected immutable entry bound to the bytes mounted by
    /// this level. A concurrent pack update requires a fresh selection.
    pub fn load_catalog_map(store: &packs::Installed, entry: &packs::CatalogEntry, id: &ContentId, game_dir: Option<&Path>) -> Result<Level, LoadError> {
        let (pack, map) = store.resolve_catalog_map(entry, id).map_err(LoadError::Io)?;
        let (game_dir, fs) = mount_game(game_dir)?;
        Self::load_pack(&pack, &map, game_dir, fs)
    }

    /// The same hash-bound selection for headless/self-contained asset packs.
    pub fn load_catalog_map_from_vfs(store: &packs::Installed, entry: &packs::CatalogEntry, id: &ContentId, game_dir: PathBuf, fs: vfs::Vfs) -> Result<Level, LoadError> {
        let (pack, map) = store.resolve_catalog_map(entry, id).map_err(LoadError::Io)?;
        Self::load_pack(&pack, &map, game_dir, fs)
    }

    pub fn pack_archive_hash(&self) -> Option<&str> { self.pack_archive_hash.as_deref() }

    /// Loads a validated pack selection into a session-owned VFS. Asset maps
    /// can use an empty VFS without a Legacy install; recipes need a mounted
    /// Legacy VFS. No other installed pack is automatically mounted.
    pub fn load_pack(pack: &packs::Pack, map: &packs::MapDef, game_dir: PathBuf, mut fs: vfs::Vfs) -> Result<Level, LoadError> {
        if !pack.manifest().maps.contains(map) { return Err(LoadError::Io("map is not defined by selected pack".into())); }
        let metadata_pack = if map.source == packs::MapSource::Pack {
            let label = format!("pack:{}", pack.manifest().id);
            fs.push_front(label.clone(), pack.mount());
            Some(label)
        } else {
            // Recipe aliases may not read a runtime Workshop archive in
            // place of a BSP from the player's actual Legacy install.
            let archives = fs.labels().into_iter().filter(|l| l.starts_with("workshop:")).map(str::to_owned).collect::<Vec<_>>();
            for label in archives { fs.unmount(&label); }
            None
        };
        let id = map.id.parse().map_err(|e| LoadError::Io(format!("{e}")))?;
        let mut level = Self::load_from_vfs_scoped(id, &map.bsp, game_dir, fs, metadata_pack, None)?;
        level.pack_archive_hash = Some(pack.archive_hash().into());
        Ok(level)
    }

    /// Loads a map from an already mounted file system, including content
    /// pack mounts. Does not detect Steam or fetch Workshop data. `game_dir`
    /// records the source directory; the loader reads all data through VFS.
    /// `name` selects the BSP filename independently of the stable content ID.
    pub fn load_from_vfs(id: ContentId, name: &str, game_dir: PathBuf, fs: vfs::Vfs) -> Result<Level, LoadError> {
        Self::load_from_vfs_scoped(id, name, game_dir, fs, None, None)
    }

    fn load_from_vfs_scoped(id: ContentId, name: &str, game_dir: PathBuf, mut fs: vfs::Vfs, metadata_pack: Option<String>, bsp_mount: Option<&str>) -> Result<Level, LoadError> {
        if id.kind != ContentKind::Map {
            return Err(LoadError::Io(format!("{id} is not a map")));
        }
        if !packs::valid_bsp_name(name) { return Err(LoadError::Io("invalid map filename".into())); }
        let mut timings = Vec::new();
        let t = Instant::now();
        let map_path = format!("maps/{name}.bsp");
        let bytes = match bsp_mount { Some(label) => fs.read_from(label, &map_path), None => fs.read(&map_path) }
            .ok_or_else(|| LoadError::Io(format!("{map_path} not found in its selected source")))?;
        let bsp = bsp::Bsp::parse(&bytes).map_err(|e| LoadError::Io(format!("{map_path}: {e}")))?;
        drop(bytes);
        fs.mount_map_pakfile(bsp.pakfile.clone())
            .map_err(|e| LoadError::Io(format!("pakfile: {e}")))?;
        timings.push(("bsp", t.elapsed().as_secs_f32()));

        let t = Instant::now();
        let loader = |model: &str| -> Option<Vec<mdl::ConvexHull>> {
            let base = model.trim_end_matches(".mdl");
            let bytes = fs.read(&format!("{base}.phy"))?;
            mdl::Phy::parse(&bytes).ok().map(|p| p.hulls)
        };
        let collision = CollisionWorld::from_bsp(&bsp, &MapCollisionOptions::default(), &loader);
        timings.push(("collision", t.elapsed().as_secs_f32()));

        let t = Instant::now();
        let catalog = fs
            .read_string("scripts/items/items_game.txt")
            .and_then(|text| weapons::Catalog::from_items_game(&text).ok())
            .unwrap_or_default();
        timings.push(("weapons", t.elapsed().as_secs_f32()));

        let nav = scoped_map_resource(&fs, &id, name, "maps", "nav", metadata_pack.as_deref())
            .and_then(|bytes| nav::NavMesh::parse(&bytes).ok())
            .map(Arc::new);

        Ok(Level {
            id,
            name: name.to_string(),
            game_dir,
            vfs: fs,
            bsp,
            collision: Arc::new(collision),
            catalog: Arc::new(catalog),
            nav,
            metadata_pack,
            pack_archive_hash: None,
            timings,
        })
    }

    /// The map's radar overview description (`resource/overviews/<map>.txt`),
    /// which names the bomb sites.
    pub fn overview(&self) -> Option<String> {
        scoped_map_resource(&self.vfs, &self.id, &self.name, "resource/overviews", "txt", self.metadata_pack.as_deref())
            .map(|bytes| kv::decode_text(&bytes))
    }

    /// A config file from the game's `cfg/` folder, e.g.
    /// `gamemode_competitive.cfg`.
    pub fn cfg(&self, name: &str) -> Option<String> {
        self.vfs.read_string(&format!("cfg/{name}"))
    }
}

fn scoped_map_resource(fs: &vfs::Vfs, id: &ContentId, name: &str, folder: &str, extension: &str, pack: Option<&str>) -> Option<Vec<u8>> {
    match pack {
        Some(label) => {
            let path = format!("{folder}/{name}.{extension}");
            fs.read_from("map", &path).or_else(|| fs.read_from(label, &path))
        }
        None => map_resource(fs, id, name, folder, extension),
    }
}

/// Archived BSPs keep the original map name inside their pakfile. Resolve
/// their metadata only there, never against the installed version's files.
fn map_resource(fs: &vfs::Vfs, id: &ContentId, name: &str, folder: &str, extension: &str) -> Option<Vec<u8>> {
    let old = (id.origin == GameOrigin::Csgo).then(|| workshop::find(name)).flatten();
    match old {
        Some(old) => fs.read_from("map", &format!("{folder}/{}.{extension}", old.base)),
        None => fs.read(&format!("{folder}/{name}.{extension}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_map_metadata_uses_selected_pack_and_never_installed_or_unrelated_data() {
        use std::collections::BTreeMap;
        let id = ContentId::new(GameOrigin::Mod, ContentKind::Map, "arena");
        let mut fs = vfs::Vfs::new();
        let files = |value: &'static [u8]| BTreeMap::from([
            ("maps/arena.nav".into(), Arc::<[u8]>::from(value)),
            ("resource/overviews/arena.txt".into(), Arc::<[u8]>::from(value)),
        ]);
        fs.push_back("install", vfs::Mount::Memory(files(b"installed")));
        fs.push_front("pack:unrelated", vfs::Mount::Memory(files(b"unrelated")));
        for (folder, extension) in [("maps", "nav"), ("resource/overviews", "txt")] {
            assert!(scoped_map_resource(&fs, &id, "arena", folder, extension, Some("pack:selected")).is_none());
        }
        fs.push_front("pack:selected", vfs::Mount::Memory(files(b"selected")));
        assert_eq!(scoped_map_resource(&fs, &id, "arena", "maps", "nav", Some("pack:selected")).unwrap(), b"selected");
        fs.push_front("map", vfs::Mount::Memory(files(b"embedded")));
        assert_eq!(scoped_map_resource(&fs, &id, "arena", "maps", "nav", Some("pack:selected")).unwrap(), b"embedded");
    }

    #[test]
    fn archived_map_metadata_uses_its_own_original_name_without_install_fallback() {
        let root = std::env::temp_dir().join(format!("csr-map-metadata-{}", std::process::id()));
        let install = root.join("install");
        let embedded = root.join("embedded");
        std::fs::create_dir_all(install.join("maps")).unwrap();
        std::fs::create_dir_all(embedded.join("maps")).unwrap();
        std::fs::write(install.join("maps/de_dust2.nav"), b"new navigation").unwrap();
        std::fs::write(embedded.join("maps/de_dust2.nav"), b"old navigation").unwrap();
        let mut fs = vfs::Vfs::new();
        fs.push_back("install", vfs::Mount::Dir(install));
        fs.push_front("map", vfs::Mount::Dir(embedded));
        let archived = workshop::find("de_dust2_2017").unwrap().content_id();
        assert_eq!(map_resource(&fs, &archived, "de_dust2_2017", "maps", "nav").unwrap(), b"old navigation");
        fs.unmount("map");
        assert!(map_resource(&fs, &archived, "de_dust2_2017", "maps", "nav").is_none());
        let installed = ContentId::csgo(ContentKind::Map, "de_dust2");
        assert_eq!(map_resource(&fs, &installed, "de_dust2", "maps", "nav").unwrap(), b"new navigation");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_a_pack_map_without_a_game_install_and_preserves_its_origin() {
        let id = ContentId::new(GameOrigin::Mod, ContentKind::Map, "test_arena");
        let mut bsp = vec![0; 8 + 64 * 16 + 4];
        bsp[..4].copy_from_slice(b"VBSP");
        bsp[4..8].copy_from_slice(&21i32.to_le_bytes());
        let map = packs::MapDef { id: id.to_string(), bsp: "test-arena".into(), name: "Test arena".into(), source: packs::MapSource::Pack };
        let manifest = packs::Manifest {
            format_version: 1, id: "test_pack".into(), name: "Test pack".into(), version: "1.0.0".into(),
            origin: "mod".into(), kind: packs::PackKind::Assets, provides: vec![id.to_string()],
            content_hash: String::new(), min_game_version: "0.0.0".into(), description: String::new(), maps: vec![map.clone()], resources: Vec::new(),
        };
        let pack = packs::Pack::from_bytes(packs::build(&manifest, &[("maps/test-arena.bsp", &bsp)]).unwrap()).unwrap();
        let level = Level::load_pack(&pack, &map, PathBuf::from("no-game-install"), vfs::Vfs::new()).unwrap();
        assert_eq!(level.id, id);
        assert_eq!(level.name, "test-arena");
        assert_eq!(level.bsp.version, 21);
        assert!(level.vfs.labels().contains(&"pack:test_pack"));
    }
}

#[cfg(test)]
mod selection_tests;
