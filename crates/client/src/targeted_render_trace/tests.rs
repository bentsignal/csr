//! Source fixtures; R23 additions UNRUN (no Cargo/build/test authorization).
use super::*;
use bevy::ecs::system::{IntoSystem, System};
use tracing_subscriber::prelude::*;

fn window() -> Window {
    Window { start: 30.0, end: 60.0, wall_ns: 90_000_000_000, records: 100, bytes: 8192 }
}
fn frame(n: u32, sim_s: f64) -> Option<Frame> {
    Some(Frame { frame: n, tick: (sim_s * 128.0) as u64, sim_s })
}
fn sample(capture: &mut Capture, n: u64) {
    let thread = std::thread::current().id();
    capture.enter(0, 7, thread, n);
    capture.exit(0, 7, thread, n + 25);
}

#[test]
fn names_are_actual_public_bevy_function_system_names() {
    fn name<M, S: IntoSystem<(), (), M>>(system: S) -> String {
        IntoSystem::into_system(system).name().to_string()
    }
    let actual = [
        name(bevy::pbr::extract_skins), name(bevy::pbr::prepare_skins),
        name(bevy::render::erased_render_asset::prepare_erased_assets::<bevy::pbr::MeshMaterial3d<crate::material::WorldMaterial>>),
        name(bevy::pbr::prepare_mesh_bind_groups), name(bevy::render::renderer::render_system),
    ];
    for (index, name) in actual.iter().enumerate() {
        assert_eq!(selected(name), Some(index), "registry name drift: {name}");
    }
    for name in [
        "prepare_skins", "other::prepare_skins", "bevy_pbr::render::skin::prepare_skins_extra",
        "bevy_render::erased_render_asset::prepare_erased_assets<bevy_pbr::mesh_material::MeshMaterial3d<other::WorldMaterial>>",
        "<Enable the debug feature to see the name>",
    ] { assert_eq!(selected(name), None, "false attribution: {name}"); }
}

#[test]
fn real_subscriber_routes_only_exact_selected_system_spans_without_file_io() {
    let mut capture = Capture::new(window(), "fixture", None);
    capture.context(0, frame(99, 29.99));
    capture.context(0, frame(100, 30.0));
    let shared = Arc::new(Shared { capture: Mutex::new(capture), born: Instant::now(),
        active: AtomicBool::new(true), closed: AtomicBool::new(false) });
    let subscriber = Registry::default().with(TraceLayer(shared.clone()).with_filter(filter_fn(system_metadata)));
    tracing::subscriber::with_default(subscriber, || {
        // String follows the exact `name = name.clone().to_string()` engine path.
        let selected_name = SYSTEMS[0].to_string();
        let span = tracing::info_span!(target: "bevy_ecs::system::function_system", "system", name = selected_name);
        { let _entered = span.enter(); }
        { let _entered = span.enter(); } // reused system span: distinct invocation
        let _wrong_name = tracing::info_span!(target: "bevy_ecs::system::function_system", "system", name = "other::extract_skins").entered();
        let _wrong_kind = tracing::info_span!(target: "bevy_ecs::system::function_system", "system_commands", name = SYSTEMS[0]).entered();
        let _wrong_target = tracing::info_span!(target: "other", "system", name = SYSTEMS[0]).entered();
        tracing::info!("events must not become records");
    });
    let capture = shared.capture.lock().unwrap();
    let rows: Vec<_> = capture.text.lines().filter(|row| row.starts_with("sample\t")).collect();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 10);
        assert_eq!(&fields[1..4], &["100", "3840", "30"]);
        assert_eq!(fields[6], "0");
        assert_eq!(fields[9], "false");
    }
}

// All tests below are pure collector/path-policy fixtures; no files or renderer.
fn armed() -> Capture {
    let mut capture = Capture::new(window(), "fixture", None);
    capture.context(1, frame(99, 29.99));
    capture.context(10, frame(100, 30.0));
    capture
}
fn all_functions(capture: &mut Capture, at: u64) {
    let thread = std::thread::current().id();
    // Different functions overlap; rows finish in a different order to starts.
    for system in 0..5 { capture.enter(system, system as u64 + 1, thread, at + system as u64); }
    for system in (0..5).rev() { capture.exit(system, system as u64 + 1, thread, at + 10 + (4 - system) as u64); }
}

#[test]
fn successful_interval_has_actual_boundary_receipts_and_every_function_per_context() {
    let mut capture = armed();
    all_functions(&mut capture, 20);
    capture.context(50, frame(101, 59.99));
    all_functions(&mut capture, 60);
    capture.context(100, frame(102, 60.0));
    assert_eq!(capture.reason, Some("window_end"));
    assert_eq!(capture.records, 14); // predecessor + two contexts + ten invocations + upper
    assert!(capture.text.contains("lower_previous\t99\t3838\t29.99\t1\t"));
    assert!(capture.text.contains("lower\t100\t3840\t30\t10\t"));
    assert!(capture.text.contains("upper\t102\t7680\t60\t100\t"));
    assert!(capture.text.ends_with("reason=window_end records=14 seen_mask=31 open_scopes=0\n"));
    let closed = capture.text.clone();
    capture.context(200, frame(103, 61.0));
    sample(&mut capture, 210);
    assert_eq!(capture.text, closed);
}

#[test]
fn fractional_20_tick_boundaries_and_samples_preserve_membership_exactly() {
    let window = Window { start: 0.0500000009, end: 0.2, ..window() }.validate().unwrap();
    // Same supported non-power-of-two rate and f32 division as World::time().
    assert!(sim::TICK_RATE_RANGE.contains(&20));
    let tick_frame = |tick: u64| Some(Frame {
        frame: tick as u32, tick, sim_s: f64::from(tick as f32 / 20.0_f32),
    });
    let mut capture = Capture::new(window, "r23-fractional", None);
    capture.context(1, tick_frame(1));
    capture.context(10, tick_frame(2));
    all_functions(&mut capture, 20);
    capture.context(50, tick_frame(3));
    all_functions(&mut capture, 60);
    capture.context(100, tick_frame(4));
    assert_eq!(capture.reason, Some("window_end"));
    assert!(capture.text.contains("start_s=0.0500000009 end_s=0.2 "));
    // Independent literal Python fixture uses these same Display row values.
    let expected = ["0.05000000074505806", "0.10000000149011612",
        "0.15000000596046448", "0.20000000298023224"];
    let mut samples = 0;
    for row in capture.text.lines().skip(7).take(capture.records) {
        let fields: Vec<_> = row.split('\t').collect();
        let tick: usize = fields[2].parse().unwrap();
        assert_eq!(fields[3], expected[tick - 1]);
        let emitted: f64 = fields[3].parse().unwrap();
        assert_eq!(emitted.to_bits(), tick_frame(tick as u64).unwrap().sim_s.to_bits());
        match fields[0] {
            "lower_previous" => {
                assert!(emitted < window.start);
                // The previous fixed-nine representation crosses START.
                assert!("0.050000001".parse::<f64>().unwrap() > window.start);
            }
            "lower" | "context" | "sample" => assert!(window.start <= emitted && emitted < window.end),
            "upper" => assert!(emitted >= window.end),
            kind => panic!("unexpected row: {kind}"),
        }
        if fields[0] == "sample" { samples += 1; }
    }
    assert_eq!(samples, 10);
    assert_eq!(capture.records, 14);
    assert!(capture.text.ends_with("reason=window_end records=14 seen_mask=31 open_scopes=0\n"));
}

#[test]
fn late_or_absent_lower_receipt_cannot_complete() {
    for sim_s in [30.0, 59.9, 60.0] {
        let mut capture = Capture::new(window(), "late", None);
        capture.context(1, frame(100, sim_s));
        all_functions(&mut capture, 10);
        capture.context(100, frame(101, 60.0));
        assert_eq!(capture.reason, Some("missed_start"));
        assert!(!capture.text.contains("reason=window_end"));
    }
    let mut capture = Capture::new(window(), "jump", None);
    capture.context(1, frame(99, 29.99));
    capture.context(10, frame(100, 60.0));
    assert_eq!(capture.reason, Some("missed_start"));
}

#[test]
fn missing_later_functions_contexts_and_upper_receipt_are_incomplete() {
    let mut capture = armed();
    all_functions(&mut capture, 20);
    capture.context(50, frame(101, 59.99));
    sample(&mut capture, 60); // all five were seen earlier; four now absent
    capture.context(100, frame(102, 60.0));
    assert_eq!(capture.seen, 31);
    assert_eq!(capture.reason, Some("missing_function"));
    assert!(capture.text.contains("upper\t102\t"));
    let mut capture = armed();
    all_functions(&mut capture, 20);
    capture.context(50, frame(102, 59.99));
    assert_eq!(capture.reason, Some("context_gap"));
    let mut capture = armed();
    all_functions(&mut capture, 20);
    capture.close("app_exit"); // no terminal crossing, even with all five
    assert_eq!(capture.reason, Some("app_exit"));
    assert!(!capture.text.contains("upper\t"));
}

#[test]
fn caps_deadlines_and_invalid_windows_fail_closed() {
    for (cap, reason) in [
        (Window { records: 3, ..window() }, "record_cap"),
        (Window { records: MAX_RECORDS, bytes: 4096, ..window() }, "byte_cap"),
    ] {
        let mut capture = Capture::new(cap, "cap", None);
        capture.context(0, frame(99, 29.99));
        capture.context(1, frame(100, 30.0));
        for n in 1..200 { sample(&mut capture, n * 100); }
        assert_eq!(capture.reason, Some(reason));
        assert!(capture.text.len() <= cap.bytes);
        assert!(capture.records <= cap.records);
        assert!(capture.text.ends_with('\n'));
        assert_eq!(capture.text.lines().filter(|l| l.starts_with("# end ")).count(), 1);
        for row in capture.text.lines().filter(|l| !l.starts_with('#')) {
            assert_eq!(row.split('\t').count(), 10, "partial row: {row}");
        }
    }
    let mut capture = armed();
    capture.context(window().wall_ns, frame(101, 60.0));
    assert_eq!(capture.reason, Some("wall_limit"));
    for bad in [f64::NAN, f64::INFINITY, -1.0, -0.0, 0.0, 60.0] {
        assert!(Window { start: bad, ..window() }.validate().is_err());
    }
    assert!(Window { end: f64::INFINITY, ..window() }.validate().is_err());
    assert!(Window { records: MAX_RECORDS + 1, ..window() }.validate().is_err());
    assert!(Window { bytes: MAX_BYTES + 1, ..window() }.validate().is_err());
}

#[test]
fn context_and_scope_failures_cannot_become_complete() {
    let mut capture = armed();
    capture.context(50, frame(101, 29.0));
    assert_eq!(capture.reason, Some("context_rewind"));
    let mut capture = armed();
    capture.context(50, None);
    assert_eq!(capture.reason, Some("context_lost"));
    let thread = std::thread::current().id();
    let mut capture = armed();
    capture.enter(0, 7, thread, 20);
    capture.enter(0, 8, thread, 21);
    assert_eq!(capture.reason, Some("overlapping_same_system"));
    let mut capture = armed();
    capture.enter(0, 7, thread, 20);
    capture.exit(0, 8, thread, 21);
    assert_eq!(capture.reason, Some("scope_mismatch"));
    let mut capture = armed();
    capture.enter(0, 7, thread, 20);
    capture.context(50, frame(101, 31.0));
    assert_eq!(capture.reason, Some("context_changed"));
    let mut capture = armed();
    capture.enter(0, 7, thread, 9);
    assert_eq!(capture.reason, Some("scope_mismatch"));
}

#[test]
fn workspace_boundary_survives_distinct_private_data_and_cwd_without_io() {
    // Inputs represent independently canonicalized paths; no CWD/env/config mutation.
    let workspace = PathBuf::from(r"C:\source\trace-r20");
    let private_data = PathBuf::from(r"C:\private\userdata");
    let roots = [workspace.clone(), private_data.clone(), private_data];
    assert!(outside_protected(Path::new(r"\\?\c:\SOURCE\TRACE-R20\captures"), &roots).is_err());
    assert!(outside_protected(&workspace, &roots).is_err());
    assert!(outside_protected(Path::new(r"C:\private\userdata\captures"), &roots).is_err());
    assert!(outside_protected(Path::new(r"C:\private\evidence"), &roots).is_ok());
    assert!(outside_protected(Path::new(r"C:\source\trace-r20-sibling"), &roots).is_ok());
    assert!(outside_protected(Path::new(r"C:\evidence"), &[PathBuf::from(r"c:\")]).is_err());
    assert!(outside_protected(Path::new(r"D:\evidence"), &[PathBuf::from(r"c:\")]).is_ok());
    // The local-share alias from R21 is unsupported even if it exposes source.
    assert!(outside_protected(Path::new(r"\\localhost\csr-source\trace-r20\captures"), &roots).is_err());
    assert!(outside_protected(Path::new(r"\\?\UNC\localhost\csr-source\trace-r20\captures"), &roots).is_err());
}

#[test]
fn local_windows_namespace_and_drive_policy_without_io() {
    for path in [r"C:\private\new.tsv", "c:/private/new.tsv"] {
        assert!(require_local_drive(Path::new(path), false, &|_| 3).is_ok());
    }
    assert_eq!(local_drive_key(Path::new(r"\\?\C:\Private"), true).unwrap(), r"c:\private");
    for path in [r"\\localhost\csr-source\new.tsv", r"\\server\share\new.tsv",
        r"\\?\UNC\server\share\new.tsv", r"\\.\C:\private\new.tsv",
        r"\\?\C:\private\new.tsv", r"\\?\Volume{abc}\new.tsv", r"\??\C:\new.tsv",
        "//?/C:/private/new.tsv", "//server/share/new.tsv", r"C:private\new.tsv",
        r"\private\new.tsv", "/private/new.tsv", "new.tsv", "C:\\privat\u{00e9}\\new.tsv"] {
        assert!(require_local_drive(Path::new(path), false, &|_| panic!("namespace must reject before drive lookup")).is_err(), "{path}");
    }
    for path in [r"\\?\UNC\server\share", r"\\.\C:\private", r"\\?\Volume{abc}\private"] {
        assert!(local_drive_key(Path::new(path), true).is_err());
    }
    for kind in [0, 1, 4, 5] { // unknown, unavailable, mapped remote, unsupported optical
        assert!(require_local_drive(Path::new(r"Z:\evidence\new.tsv"), false, &|_| kind).is_err());
    }
    for kind in [2, 3, 6] {
        assert!(require_local_drive(Path::new(r"Z:\evidence\new.tsv"), false, &|_| kind).is_ok());
    }
}

#[test]
#[cfg(windows)]
fn private_path_checks_resolved_namespaces_and_unavailable_roots_without_io() {
    let roots = [PathBuf::from(r"C:\source\trace-r20"), PathBuf::from(r"C:\private\userdata")];
    let path = Path::new(r"C:\evidence\new.tsv");
    let local = |_: u8| 3;
    let identity = |path: &Path| Ok(path.to_path_buf());
    assert_eq!(private_path_with(path, &roots, identity, local).unwrap(), path);
    // Resolver simulates canonicalize failures, not a guessed nonexistent disk path.
    for unavailable in &roots {
        assert!(private_path_with(path, &roots, |p| {
            if p == unavailable.as_path() { Err("unavailable protected root".into()) } else { Ok(p.to_path_buf()) }
        }, local).unwrap_err().contains("protected path"));
    }
    assert!(private_path_with(path, &roots, |_| Err("unavailable parent".into()), local)
        .unwrap_err().contains("trace parent"));
    for resolved in [r"\\?\UNC\localhost\csr-source\trace-r20\captures",
        r"\\.\C:\evidence", r"\\?\Volume{abc}\evidence", r"Z:\evidence",
        r"\\?\c:\SOURCE\TRACE-R20\captures"] {
        assert!(private_path_with(path, &roots, |p| {
            Ok(if p == Path::new(r"C:\evidence") { PathBuf::from(resolved) } else { p.to_path_buf() })
        }, |drive| if drive == b'z' { 4 } else { 3 }).is_err(), "{resolved}");
    }
    assert!(private_path_with(path, &roots, |p| {
        Ok(if p == roots[0].as_path() { PathBuf::from(r"\\?\UNC\server\source") } else { p.to_path_buf() })
    }, local).is_err());
    // An allowed raw namespace may resolve to the normal local verbatim form.
    assert!(private_path_with(path, &roots, |p| {
        Ok(PathBuf::from(format!(r"\\?\{}", p.display())))
    }, local).is_ok());
    // Raw UNC/verbatim is rejected before even the injected resolver is called.
    for raw in [r"\\localhost\csr-source\new.tsv", r"\\?\C:\evidence\new.tsv"] {
        assert!(private_path_with(Path::new(raw), &roots, |_| panic!("no resolution allowed"), local).is_err());
    }
}
