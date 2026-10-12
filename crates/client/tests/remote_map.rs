// Compile the production leaf while the frontend owns its module declaration.
#[path = "../src/remote_map.rs"]
mod remote_map;

use remote_map::{LocalMapRequest, PrepareError, manifest_for_level};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    root: PathBuf,
    game: PathBuf,
    packs: packs::Installed,
    archives: workshop::Store,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "csr-remote-map-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let game = root.join("game");
        std::fs::create_dir_all(game.join("maps")).unwrap();
        std::fs::create_dir_all(game.join("scripts/items")).unwrap();
        std::fs::write(game.join("gameinfo.txt"), "GameInfo {}").unwrap();
        std::fs::write(
            game.join("scripts/items/items_game.txt"),
            b"items_game { items {} }",
        )
        .unwrap();
        Self {
            packs: packs::Installed::new(&root.join("data")),
            archives: workshop::Store::with_libraries(&root.join("data"), Vec::new()),
            root,
            game,
        }
    }
    fn bsp(&self, name: &str, revision: i32) {
        std::fs::write(self.game.join(format!("maps/{name}.bsp")), bsp(revision)).unwrap();
    }
    fn fs(&self) -> vfs::Vfs {
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&self.game).unwrap();
        self.archives.mount(&mut fs);
        fs
    }
    fn manifest(&self, id: &str) -> netcode::protocol::ContentManifest {
        let level = level::Level::load_local_id_from_vfs(
            &id.parse().unwrap(),
            self.game.clone(),
            self.fs(),
            &self.archives,
        )
        .unwrap();
        manifest_for_level(&level).unwrap()
    }
    fn request(
        &self,
        manifest: &netcode::protocol::ContentManifest,
    ) -> Result<LocalMapRequest, PrepareError> {
        LocalMapRequest::resolve(manifest, &self.game, &self.packs, &self.archives)
    }
    fn pack(&self, revision: i32) {
        let manifest = packs::Manifest {
            format_version: 1,
            id: "arena_pack".into(),
            name: "Arena pack".into(),
            version: format!("1.0.{revision}"),
            origin: "mod".into(),
            kind: packs::PackKind::Assets,
            provides: vec!["mod_map_arena".into()],
            content_hash: String::new(),
            min_game_version: "0.0.0".into(),
            description: String::new(),
            maps: vec![packs::MapDef {
                id: "mod_map_arena".into(),
                bsp: "de_private".into(),
                name: "Arena".into(),
                source: packs::MapSource::Pack,
            }],
            resources: vec![],
        };
        let bytes = bsp(revision);
        let pack = packs::Pack::from_bytes(
            packs::build(&manifest, &[("maps/de_private.bsp", &bytes)]).unwrap(),
        )
        .unwrap();
        self.packs.install_validated(&pack).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn bsp(revision: i32) -> Vec<u8> {
    let mut bytes = vec![0; 8 + 64 * 16 + 4];
    bytes[..4].copy_from_slice(b"VBSP");
    bytes[4..8].copy_from_slice(&21i32.to_le_bytes());
    bytes[8 + 64 * 16..].copy_from_slice(&revision.to_le_bytes());
    bytes
}
fn advertised(id: &str) -> netcode::protocol::ContentManifest {
    netcode::protocol::ContentManifest {
        map_id: id.into(),
        map_name: "de_fixture".into(),
        map_hash: "0".repeat(40),
        catalog_hash: "0".repeat(40),
    }
}
fn error<T>(result: Result<T, PrepareError>) -> PrepareError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("unexpected success"),
    }
}

#[test]
fn exact_local_preparation_is_map_only_and_advertised_name_never_selects_a_path() {
    let f = Fixture::new();
    f.bsp("de_fixture", 1);
    f.bsp("de_other", 99);
    let manifest = f.manifest("csgo_map_de_fixture");
    let request = f.request(&manifest).unwrap();
    assert_eq!(request.id().to_string(), manifest.map_id);
    let prepared = request.prepare().unwrap();
    assert_eq!(prepared.manifest, manifest);
    assert_eq!(prepared.level.bsp.map_revision, 1);
    assert!(prepared.world.state().players.is_empty());
    assert_eq!(prepared.world.state().tick, 0);
    let mut wrong = manifest.clone();
    wrong.map_name = "de_other".into();
    assert_eq!(
        error(f.request(&wrong).unwrap().prepare()),
        PrepareError::ContentMismatch
    );
    let mut wrong = manifest.clone();
    wrong.map_hash = "0".repeat(40);
    assert_eq!(
        error(f.request(&wrong).unwrap().prepare()),
        PrepareError::ContentMismatch
    );
    let mut wrong = manifest;
    wrong.catalog_hash = "0".repeat(40);
    assert_eq!(
        error(f.request(&wrong).unwrap().prepare()),
        PrepareError::ContentMismatch
    );
}

#[test]
fn missing_unsupported_ambiguous_and_invalid_selections_are_typed_without_acquisition() {
    let f = Fixture::new();
    for id in [
        "csgo_map_de_missing",
        "mod_map_arena",
        "csgo_map_de_dust2_2017",
    ] {
        assert_eq!(
            error(f.request(&advertised(id))),
            PrepareError::MissingLocalContent
        );
    }
    assert!(!f.archives.root().exists());
    assert_eq!(
        error(f.request(&advertised("iw4_map_de_fixture"))),
        PrepareError::UnsupportedOrigin
    );
    f.bsp("de_test-arena", 1);
    f.bsp("de_test_arena", 2);
    assert_eq!(
        error(f.request(&advertised("csgo_map_de_test_arena"))),
        PrepareError::AmbiguousLocalContent
    );
    let mut unsafe_name = advertised("csgo_map_de_fixture");
    unsafe_name.map_name = "../outside".into();
    assert!(matches!(
        error(f.request(&unsafe_name)),
        PrepareError::InvalidAdvertisement(_)
    ));
}

#[test]
fn missing_or_malformed_catalog_cannot_admit_a_fallback_catalog() {
    for bytes in [None, Some(b"items_game {".as_slice())] {
        let f = Fixture::new();
        f.bsp("de_fixture", 1);
        let path = f.game.join("scripts/items/items_game.txt");
        if let Some(bytes) = bytes {
            std::fs::write(&path, bytes).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        assert_eq!(
            error(
                f.request(&advertised("csgo_map_de_fixture"))
                    .unwrap()
                    .prepare()
            ),
            PrepareError::MissingCatalogDigest
        );
    }
}

#[test]
fn archive_uses_only_the_explicit_installed_store_not_a_same_named_game_map() {
    let f = Fixture::new();
    let old = workshop::find("de_dust2_2017").unwrap();
    f.bsp(old.name, 99);
    assert_eq!(
        error(f.request(&advertised(&old.content_id().to_string()))),
        PrepareError::MissingLocalContent
    );
    std::fs::create_dir_all(f.archives.bsp_path(old).parent().unwrap()).unwrap();
    std::fs::write(f.archives.bsp_path(old), bsp(3)).unwrap();
    let manifest = f.manifest(&old.content_id().to_string());
    let request = f.request(&manifest).unwrap();
    assert_eq!(request.prepare().unwrap().level.bsp.map_revision, 3);
    let empty = workshop::Store::with_libraries(&f.root.join("other-data"), Vec::new());
    assert_eq!(
        error(LocalMapRequest::resolve(
            &manifest, &f.game, &f.packs, &empty
        )),
        PrepareError::MissingLocalContent
    );
    std::fs::remove_file(f.archives.bsp_path(old)).unwrap();
    assert_eq!(error(request.prepare()), PrepareError::MissingLocalContent);
}

#[test]
fn private_request_pins_catalog_entry_and_refuses_concurrent_replacement() {
    let f = Fixture::new();
    f.pack(4);
    f.bsp("de_private", 99);
    let id = "mod_map_arena".parse().unwrap();
    let entry = f.packs.catalog().entries.remove(0);
    let level =
        level::Level::load_catalog_map_from_vfs(&f.packs, &entry, &id, f.game.clone(), f.fs())
            .unwrap();
    let manifest = manifest_for_level(&level).unwrap();
    let request = f.request(&manifest).unwrap();
    let prepared = request.prepare().unwrap();
    assert_eq!(prepared.level.bsp.map_revision, 4);
    assert_eq!(
        prepared.level.pack_archive_hash(),
        Some(entry.archive_hash())
    );
    f.pack(5);
    assert_eq!(error(request.prepare()), PrepareError::SelectionChanged);
    assert_eq!(
        error(f.request(&manifest).unwrap().prepare()),
        PrepareError::ContentMismatch
    );
    // The already prepared running world retains its selected immutable mount.
    assert_eq!(
        prepared.level.vfs.read("maps/de_private.bsp").unwrap(),
        bsp(4)
    );
}

#[test]
fn unhealthy_private_catalog_is_never_silently_used() {
    let f = Fixture::new();
    f.pack(4);
    std::fs::write(f.root.join("data/packs/broken.csrpack"), b"invalid archive").unwrap();
    assert_eq!(
        error(f.request(&advertised("mod_map_arena"))),
        PrepareError::CatalogUnavailable
    );
}
