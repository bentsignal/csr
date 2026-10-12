use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "csr-pack-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest() -> Manifest {
    Manifest {
        format_version: 1,
        id: "test_arena".into(),
        name: "Test arena".into(),
        version: "1.0.0".into(),
        origin: "mod".into(),
        kind: PackKind::Assets,
        provides: vec!["mod_map_arena".into()],
        content_hash: content_hash(&BTreeMap::new()),
        min_game_version: "0.0.0".into(),
        description: "A \"quoted\"\nname ×".into(),
        maps: vec![MapDef {
            id: "mod_map_arena".into(),
            bsp: "arena_file".into(),
            name: "Arena".into(),
            source: MapSource::Pack,
        }],
        resources: Vec::new(),
    }
}

#[test]
fn cancellation_during_archive_reads_and_decompression_never_returns_a_pack() {
    let temp = Temp::new();
    let mut seed = 42u64;
    let data: Vec<u8> = (0..job::CHUNK * 3)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect();
    let bytes = build(&manifest(), &[("maps/arena_file.bsp", &data)]).unwrap();
    let path = temp.write("source.csrpack", &bytes);
    for stage in [Stage::Reading, Stage::Validating] {
        let mut control = Control::default();
        let cancel = control.clone();
        control.hook = Some(Arc::new(move |p| {
            if p.stage == stage && p.bytes >= job::CHUNK as u64 {
                cancel.cancel();
            }
        }));
        let error = Pack::open_controlled(&path, Limits::default(), &control).unwrap_err();
        assert_eq!(error, "pack installation cancelled");
    }
}

#[test]
fn cancellation_during_writing_and_at_publish_preserves_old_pack_and_cleans_pending() {
    let temp = Temp::new();
    let store = Installed::new(&temp.0);
    let old = build(&manifest(), &[("maps/arena_file.bsp", b"old")]).unwrap();
    store
        .install_validated(&Pack::from_bytes(old.clone()).unwrap())
        .unwrap();
    let mut update = manifest();
    update.version = "1.0.1".into();
    let data = vec![9; job::CHUNK * 3];
    // Stored ZIP makes the archive write itself span multiple chunks.
    let mut files = BTreeMap::new();
    files.insert(
        "maps/arena_file.bsp".into(),
        Arc::<[u8]>::from(data.as_slice()),
    );
    update.content_hash = content_hash(&files);
    let pack = Pack::from_bytes(raw_zip(&update, &[("maps/arena_file.bsp", &data)])).unwrap();
    for stage in [Stage::Writing, Stage::Publishing] {
        let mut control = Control::default();
        let cancel = control.clone();
        control.hook = Some(Arc::new(move |p| {
            if p.stage == stage && p.bytes > 0 {
                cancel.cancel();
            }
        }));
        assert_eq!(
            store.install_controlled(&pack, &control).unwrap_err(),
            "pack installation cancelled"
        );
        assert_eq!(
            fs::read(store.dir().join("test_arena.csrpack")).unwrap(),
            old
        );
        assert!(
            !fs::read_dir(store.dir()).unwrap().any(|e| e
                .unwrap()
                .path()
                .extension()
                .is_some_and(|s| s == "part"))
        );
    }
}

#[test]
fn install_job_waits_cancellably_for_lock_and_reports_only_committed_receipts() {
    use std::time::{Duration, Instant};
    let temp = Temp::new();
    let store = Installed::new(&temp.0);
    let bytes = build(&manifest(), &[("maps/arena_file.bsp", b"synthetic")]).unwrap();
    let source = temp.write("source.csrpack", &bytes);
    fs::create_dir_all(store.dir()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(store.dir().join(".install.lock"))
        .unwrap();
    lock.lock().unwrap();
    let mut job = InstallJob::start(store.clone(), source.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while job.progress().stage != Stage::Waiting && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(job.progress().stage, Stage::Waiting);
    assert!(job.poll().is_none());
    job.cancel();
    let outcome = loop {
        if let Some(outcome) = job.poll() {
            break outcome;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(matches!(outcome, Outcome::Cancelled));
    assert!(!store.dir().join("test_arena.csrpack").exists());
    lock.unlock().unwrap();
    let mut job = InstallJob::start(store.clone(), source);
    let outcome = loop {
        if let Some(outcome) = job.poll() {
            break outcome;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    let Outcome::Installed(receipt) = outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(receipt.manifest.provides, ["mod_map_arena"]);
    assert_eq!(
        fs::read(store.dir().join("test_arena.csrpack")).unwrap(),
        bytes
    );
    assert_eq!(store.load().0[0].archive_hash(), receipt.archive_hash);
    assert!(job.poll().is_none());
}

fn raw_zip(manifest: &Manifest, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(MANIFEST, opts).unwrap();
    zip.write_all(manifest.to_toml().unwrap().as_bytes())
        .unwrap();
    for (name, data) in files {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn manifest_is_strict_and_round_trips_unicode_and_escapes() {
    let m = manifest();
    assert_eq!(Manifest::parse(&m.to_toml().unwrap()).unwrap(), m);
    for mutate in [
        |m: &mut Manifest| m.id = "../escape".into(),
        |m: &mut Manifest| m.id = "con".into(),
        |m: &mut Manifest| m.origin = "my_game".into(),
        |m: &mut Manifest| m.version = "1.0".into(),
        |m: &mut Manifest| m.version = "01.0.0".into(),
        |m: &mut Manifest| m.min_game_version = "999.0.0".into(),
        |m: &mut Manifest| m.format_version = 99,
        |m: &mut Manifest| m.provides.push("mod_map_arena".into()),
        |m: &mut Manifest| m.provides = vec!["iw4_map_arena".into()],
        |m: &mut Manifest| m.provides = vec!["mod_weapon_arena".into()],
        |m: &mut Manifest| m.maps[0].bsp = "../arena".into(),
        |m: &mut Manifest| m.content_hash = "sha256:bad".into(),
    ] {
        let mut bad = m.clone();
        mutate(&mut bad);
        assert!(Manifest::parse(&bad.to_toml().unwrap()).is_err());
    }
    assert!(Manifest::parse(&format!("surprise = 1\n{}", m.to_toml().unwrap())).is_err());
}

#[test]
fn rejects_archive_aliases_code_symlinks_hash_edits_and_limits() {
    let m = manifest();
    for path in [
        "../escape",
        "a/../../escape",
        "/root",
        "C:/root",
        "models\\x.mdl",
        "models/./x.mdl",
        "models//x.mdl",
        "models/x.mdl.",
        "models/nul.mdl",
        "models/x.mdl:stream",
        "models/é.mdl",
        "mods/code.wasm",
        "models/code.dll",
        "resource/ui.swf",
        "defs/weapons/x.toml",
    ] {
        assert!(
            Pack::from_bytes(raw_zip(&m, &[(path, b"test")])).is_err(),
            "{path}"
        );
    }
    let mut modified = m.clone();
    modified.content_hash = content_hash(&BTreeMap::from([(
        "maps/arena_file.bsp".into(),
        Arc::from(&b"original"[..]),
    )]));
    assert!(
        Pack::from_bytes(raw_zip(&modified, &[("maps/arena_file.bsp", b"edited")]))
            .unwrap_err()
            .contains("hash")
    );
    // Case aliases survive ZIP's exact-name uniqueness but must never reach VFS.
    assert!(
        Pack::from_bytes(raw_zip(
            &m,
            &[("maps/arena_file.bsp", b"a"), ("MAPS/ARENA_FILE.BSP", b"a")]
        ))
        .unwrap_err()
        .contains("duplicate")
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink(
        "models/link.mdl",
        "../outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    assert!(
        Pack::from_bytes(zip.finish().unwrap().into_inner())
            .unwrap_err()
            .contains("non-regular")
    );
    let valid = build(&m, &[("maps/arena_file.bsp", &[0; 2048])]).unwrap();
    let defaults = Limits::default();
    for limits in [
        Limits {
            archive: 32,
            ..defaults
        },
        Limits {
            entry: 1024,
            ..defaults
        },
        Limits {
            expanded: 1024,
            ..defaults
        },
        Limits {
            entries: 1,
            ..defaults
        },
        Limits {
            manifest: 16,
            ..defaults
        },
    ] {
        assert!(Pack::from_bytes_with_limits(valid.clone(), limits).is_err());
    }
}

#[test]
fn recipe_and_provides_claims_require_real_supported_definitions() {
    let mut m = manifest();
    m.kind = PackKind::Recipe;
    m.maps[0].source = MapSource::Legacy;
    let recipe = Pack::from_bytes(build(&m, &[]).unwrap()).unwrap();
    assert!(!matches!(recipe.mount(), vfs::Mount::Dir(_)));
    assert!(build(&m, &[("maps/arena_file.bsp", b"not in recipes")]).is_err());
    m.kind = PackKind::Assets;
    m.maps[0].source = MapSource::Pack;
    assert!(build(&m, &[]).unwrap_err().contains("missing"));
    assert!(
        build(
            &m,
            &[("maps/arena_file.bsp", b"a"), ("maps/undeclared.bsp", b"b")]
        )
        .unwrap_err()
        .contains("undeclared")
    );
}

#[test]
fn mount_and_install_use_the_validated_snapshot_and_bad_updates_preserve_old_data() {
    let temp = Temp::new();
    let m = manifest();
    let bytes = build(&m, &[("maps/arena_file.bsp", b"version one")]).unwrap();
    let src = temp.write("download.csrpack", &bytes);
    let pack = Pack::open(&src).unwrap();
    fs::write(&src, b"replaced after validation").unwrap();
    let installed = Installed::new(&temp.0);
    installed.install_validated(&pack).unwrap();
    let target = installed.dir().join("test_arena.csrpack");
    assert_eq!(fs::read(&target).unwrap(), bytes);
    let mut fs_ = vfs::Vfs::new();
    fs_.push_front("pack", pack.mount());
    assert_eq!(fs_.read("maps/arena_file.bsp").unwrap(), b"version one");
    assert!(installed.install(&src).is_err());
    assert_eq!(fs::read(&target).unwrap(), bytes);
    // A valid reinstall repairs corrupt storage; an unrelated corrupt pack
    // does not force a friend to manually repair the entire data directory.
    fs::write(&target, b"corrupt installed data").unwrap();
    fs::write(
        installed.dir().join("unrelated.csrpack"),
        b"corrupt unrelated pack",
    )
    .unwrap();
    installed.install_validated(&pack).unwrap();
    assert_eq!(fs::read(&target).unwrap(), bytes);
    let mut v2 = m.clone();
    v2.version = "1.0.1".into();
    let update = temp.write(
        "update.csrpack",
        &build(&v2, &[("maps/arena_file.bsp", b"version two")]).unwrap(),
    );
    installed.install(&update).unwrap();
    assert_eq!(installed.load().0[0].manifest.version, "1.0.1");
    assert_eq!(
        fs_.read("maps/arena_file.bsp").unwrap(),
        b"version one",
        "already loaded levels retain their own immutable content"
    );
    assert!(installed.remove("../download").is_err());
    assert!(src.is_file());
    assert!(installed.remove("test_arena").unwrap());
    assert!(!installed.remove("test_arena").unwrap());
}

#[test]
fn concurrent_install_and_failed_publish_keep_existing_pack() {
    let temp = Temp::new();
    let bytes = build(&manifest(), &[("maps/arena_file.bsp", b"original")]).unwrap();
    let pack = Pack::from_bytes(bytes.clone()).unwrap();
    let store = Installed::new(&temp.0);
    store.install_validated(&pack).unwrap();
    let target = store.dir().join("test_arena.csrpack");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(store.dir().join(".install.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(
        store
            .install_validated(&pack)
            .unwrap_err()
            .contains("in progress")
    );
    assert_eq!(fs::read(&target).unwrap(), bytes);
    drop(lock);
    // Windows refuses replacing a file opened without delete sharing.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target)
            .unwrap();
        assert!(store.install_validated(&pack).is_err());
        assert_eq!(fs::read(&target).unwrap(), bytes);
        assert!(
            !std::fs::read_dir(store.dir())
                .unwrap()
                .flatten()
                .any(|e| e.path().extension().is_some_and(|e| e == "part"))
        );
        drop(held);
    }
}

#[test]
fn registry_conflict_is_transactional_and_install_conflicts_do_not_publish() {
    let temp = Temp::new();
    let m = manifest();
    let first = Pack::from_bytes(build(&m, &[("maps/arena_file.bsp", b"a")]).unwrap()).unwrap();
    let mut other = m.clone();
    other.id = "other_pack".into();
    other.provides.insert(0, "mod_map_before_collision".into());
    let mut map = other.maps[0].clone();
    map.id = "mod_map_before_collision".into();
    other.maps.insert(0, map);
    let second =
        Pack::from_bytes(build(&other, &[("maps/arena_file.bsp", b"a")]).unwrap()).unwrap();
    let mut registry = Registry::default();
    registry.add_pack(&first).unwrap();
    assert!(registry.add_pack(&second).is_err());
    assert_eq!(registry.entries().count(), 1);
    let store = Installed::new(&temp.0);
    store.install_validated(&first).unwrap();
    assert!(store.install_validated(&second).is_err());
    assert!(!store.dir().join("other_pack.csrpack").exists());
    // Full archive hash binds metadata too, unlike the compression-independent payload hash.
    assert_eq!(first.manifest.content_hash, second.manifest.content_hash);
    assert_ne!(first.archive_hash(), second.archive_hash());
}

#[test]
fn malformed_crc_and_invalid_utf8_do_not_mount() {
    let m = manifest();
    let mut bytes = raw_zip(&m, &[("maps/arena_file.bsp", b"unique payload bytes")]);
    let at = bytes
        .windows(20)
        .position(|w| w == b"unique payload bytes")
        .unwrap();
    bytes[at] ^= 1;
    assert!(Pack::from_bytes(bytes).is_err());
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(MANIFEST, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&[0xff]).unwrap();
    assert!(
        Pack::from_bytes(zip.finish().unwrap().into_inner())
            .unwrap_err()
            .contains("UTF-8")
    );
}

#[test]
fn reserved_origins_cannot_shadow_builtin_ids_or_replace_an_installed_pack() {
    let temp = Temp::new();
    let m = manifest();
    let bytes = build(&m, &[("maps/arena_file.bsp", b"original")]).unwrap();
    let original = Pack::from_bytes(bytes.clone()).unwrap();
    let store = Installed::new(&temp.0);
    store.install_validated(&original).unwrap();
    let target = store.dir().join("test_arena.csrpack");
    for origin in [
        "csgo",
        "t4",
        "iw4",
        "t5",
        "iw5",
        "t6",
        "mw2",
        "minecraft",
        "cs2",
    ] {
        let mut reserved = m.clone();
        reserved.origin = origin.into();
        reserved.provides = vec![format!("{origin}_map_arena")];
        reserved.maps[0].id = reserved.provides[0].clone();
        reserved.content_hash = original.manifest().content_hash.clone();
        let incoming = temp.write(
            "reserved.csrpack",
            &raw_zip(&reserved, &[("maps/arena_file.bsp", b"original")]),
        );
        assert!(
            store
                .install(&incoming)
                .unwrap_err()
                .contains("reserved built-in origin"),
            "{origin}"
        );
        assert_eq!(fs::read(&target).unwrap(), bytes, "{origin}");
        let (installed, errors) = store.load();
        assert!(errors.is_empty());
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].manifest().provides, ["mod_map_arena"]);
    }
}

#[test]
fn entry_count_and_directory_limits_are_checked_before_eager_zip_parsing() {
    let mut bytes = build(&manifest(), &[("maps/arena_file.bsp", b"a")]).unwrap();
    let end = bytes.len() - 22;
    bytes[end + 8..end + 12].copy_from_slice(&[0xff; 4]);
    assert!(
        Pack::from_bytes(bytes)
            .unwrap_err()
            .contains("entry count limit")
    );
    let mut bytes = build(&manifest(), &[("maps/arena_file.bsp", b"a")]).unwrap();
    let end = bytes.len() - 22;
    bytes[end + 12..end + 16].copy_from_slice(&(9u32 << 20).to_le_bytes());
    assert!(
        Pack::from_bytes(bytes)
            .unwrap_err()
            .contains("metadata limits")
    );
    let mut bytes = build(&manifest(), &[("maps/arena_file.bsp", b"a")]).unwrap();
    let end = bytes.len() - 22;
    bytes[end + 8..end + 12].copy_from_slice(&[1, 0, 1, 0]);
    assert!(
        Pack::from_bytes(bytes)
            .unwrap_err()
            .contains("directory mismatch")
    );
}
