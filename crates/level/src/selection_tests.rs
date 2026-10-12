use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    root: PathBuf,
    game: PathBuf,
    archives: workshop::Store,
    packs: packs::Installed,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "csr-selection-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let game = root.join("game");
        std::fs::create_dir_all(game.join("maps")).unwrap();
        Self {
            archives: workshop::Store::with_libraries(&root.join("data"), Vec::new()),
            packs: packs::Installed::new(&root.join("data")),
            root,
            game,
        }
    }
    fn fs(&self) -> vfs::Vfs {
        let mut fs = vfs::Vfs::new();
        fs.push_back(
            format!("dir:{}", self.game.display()),
            vfs::Mount::Dir(self.game.clone()),
        );
        self.archives.mount(&mut fs);
        fs
    }
    fn installed(&self, name: &str, revision: i32) {
        std::fs::write(
            self.game.join("maps").join(format!("{name}.bsp")),
            bsp(revision),
        )
        .unwrap();
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
fn memory(entries: &[(&str, Vec<u8>)]) -> vfs::Mount {
    vfs::Mount::Memory(
        entries
            .iter()
            .map(|(path, bytes)| ((*path).into(), Arc::<[u8]>::from(bytes.as_slice())))
            .collect::<BTreeMap<_, _>>(),
    )
}
fn error(result: Result<Level, LoadError>) -> String {
    match result {
        Err(e) => e.to_string(),
        Ok(_) => panic!("unexpected successful load"),
    }
}

#[test]
fn canonical_installed_id_uses_discovered_game_source_and_not_pack_shadow() {
    let fixture = Fixture::new();
    fixture.installed("de_fixture", 1);
    let id: ContentId = "csgo_map_de_fixture".parse().unwrap();
    let mut fs = fixture.fs();
    fs.push_front(
        "pack:unrelated",
        memory(&[
            ("maps/de_fixture.bsp", bsp(99)),
            ("maps/de_private.bsp", bsp(99)),
        ]),
    );
    let level =
        Level::load_local_id_from_vfs(&id, fixture.game.clone(), fs, &fixture.archives).unwrap();
    assert_eq!(level.id, id);
    assert_eq!(level.name, "de_fixture");
    assert_eq!(level.bsp.map_revision, 1);
    assert!(level.pack_archive_hash().is_none());
    let mut fs = fixture.fs();
    fs.push_front(
        "pack:unrelated",
        memory(&[("maps/de_private.bsp", bsp(99))]),
    );
    let private: ContentId = "csgo_map_de_private".parse().unwrap();
    assert!(
        error(Level::load_local_id_from_vfs(
            &private,
            fixture.game.clone(),
            fs,
            &fixture.archives
        ))
        .contains("not in the local")
    );
}

#[test]
fn canonical_id_can_use_a_game_archive_definition_without_a_loose_bsp() {
    let fixture = Fixture::new();
    let mut fs = fixture.fs();
    // Stand-in bytes for a game archive mount; source labels match mount_game.
    fs.push_front(
        "vpk:synthetic-game",
        memory(&[("maps/de_archive_only.bsp", bsp(2))]),
    );
    let id: ContentId = "csgo_map_de_archive_only".parse().unwrap();
    let level =
        Level::load_local_id_from_vfs(&id, fixture.game.clone(), fs, &fixture.archives).unwrap();
    assert_eq!(level.id, id);
    assert_eq!(level.bsp.map_revision, 2);
}

#[test]
fn archived_identity_loads_its_own_store_and_never_an_install_alias() {
    let fixture = Fixture::new();
    let old = workshop::find("de_dust2_2017").unwrap();
    fixture.installed(old.name, 99);
    std::fs::create_dir_all(fixture.archives.bsp_path(old).parent().unwrap()).unwrap();
    std::fs::write(fixture.archives.bsp_path(old), bsp(3)).unwrap();
    let level = Level::load_local_id_from_vfs(
        &old.content_id(),
        fixture.game.clone(),
        fixture.fs(),
        &fixture.archives,
    )
    .unwrap();
    assert_eq!(level.id, old.content_id());
    assert_eq!(level.bsp.map_revision, 3);
    std::fs::remove_file(fixture.archives.bsp_path(old)).unwrap();
    assert!(
        error(Level::load_local_id_from_vfs(
            &old.content_id(),
            fixture.game.clone(),
            fixture.fs(),
            &fixture.archives
        ))
        .contains("not installed")
    );
}

#[test]
fn unsupported_origins_kinds_unknown_and_ambiguous_ids_never_become_paths() {
    let fixture = Fixture::new();
    fixture.installed("de_fixture", 1);
    fixture.installed("de_test-arena", 1);
    fixture.installed("de_test_arena", 2);
    for text in [
        "iw4_map_de_fixture",
        "csgo_weapon_de_fixture",
        "mod_map_de_fixture",
        "csgo_map_missing",
    ] {
        let id: ContentId = text.parse().unwrap();
        assert!(
            !error(Level::load_local_id_from_vfs(
                &id,
                fixture.game.clone(),
                fixture.fs(),
                &fixture.archives
            ))
            .is_empty()
        );
    }
    let unsupported: ContentId = "iw4_map_de_fixture".parse().unwrap();
    assert!(error(Level::load_id(&unsupported, Some(&fixture.game))).contains("no map reader"));
    let forged = ContentId {
        origin: GameOrigin::Csgo,
        kind: ContentKind::Map,
        name: "../../outside".into(),
    };
    assert!(
        error(Level::load_local_id_from_vfs(
            &forged,
            fixture.game.clone(),
            fixture.fs(),
            &fixture.archives
        ))
        .contains("not in the local")
    );
    let ambiguous: ContentId = "csgo_map_de_test_arena".parse().unwrap();
    assert!(
        error(Level::load_local_id_from_vfs(
            &ambiguous,
            fixture.game.clone(),
            fixture.fs(),
            &fixture.archives
        ))
        .contains("ambiguous")
    );
}

#[test]
fn catalog_selected_level_retains_hash_and_refuses_changed_pack_bytes() {
    let fixture = Fixture::new();
    let id: ContentId = "mod_map_arena".parse().unwrap();
    let map = packs::MapDef {
        id: id.to_string(),
        bsp: "test-arena".into(),
        name: "Arena".into(),
        source: packs::MapSource::Pack,
    };
    let mut manifest = packs::Manifest {
        format_version: 1,
        id: "arena_pack".into(),
        name: "Arena pack".into(),
        version: "1.0.0".into(),
        origin: "mod".into(),
        kind: packs::PackKind::Assets,
        provides: vec![id.to_string()],
        content_hash: String::new(),
        min_game_version: "0.0.0".into(),
        description: String::new(),
        maps: vec![map],
        resources: Vec::new(),
    };
    let pack = packs::Pack::from_bytes(
        packs::build(&manifest, &[("maps/test-arena.bsp", &bsp(4))]).unwrap(),
    )
    .unwrap();
    fixture.packs.install_validated(&pack).unwrap();
    let catalog = fixture.packs.catalog();
    let entry = &catalog.entries[0];
    let level = Level::load_catalog_map_from_vfs(
        &fixture.packs,
        entry,
        &id,
        fixture.game.clone(),
        vfs::Vfs::new(),
    )
    .unwrap();
    assert_eq!(level.id, id);
    assert_eq!(level.pack_archive_hash(), Some(entry.archive_hash()));
    assert_eq!(level.bsp.map_revision, 4);
    manifest.version = "1.0.1".into();
    let update = packs::Pack::from_bytes(
        packs::build(&manifest, &[("maps/test-arena.bsp", &bsp(5))]).unwrap(),
    )
    .unwrap();
    fixture.packs.install_validated(&update).unwrap();
    assert!(
        error(Level::load_catalog_map_from_vfs(
            &fixture.packs,
            entry,
            &id,
            fixture.game.clone(),
            vfs::Vfs::new()
        ))
        .contains("installed pack changed")
    );
    // The running level remains pinned to its old immutable mount.
    assert_eq!(level.pack_archive_hash(), Some(entry.archive_hash()));
    assert_eq!(level.vfs.read("maps/test-arena.bsp").unwrap(), bsp(4));
    let refreshed = fixture.packs.catalog();
    let fresh = Level::load_catalog_map_from_vfs(
        &fixture.packs,
        &refreshed.entries[0],
        &id,
        fixture.game.clone(),
        vfs::Vfs::new(),
    )
    .unwrap();
    assert_eq!(fresh.bsp.map_revision, 5);
    assert_eq!(fresh.pack_archive_hash(), Some(update.archive_hash()));
}
