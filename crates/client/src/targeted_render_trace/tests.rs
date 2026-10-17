//! Authored source fixtures; UNRUN in R20 (no Cargo/build/test authorization).
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
        assert_eq!(&fields[1..4], &["100", "3840", "30.000000000"]);
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
    assert!(capture.text.contains("lower_previous\t99\t3838\t29.990000000\t1\t"));
    assert!(capture.text.contains("lower\t100\t3840\t30.000000000\t10\t"));
    assert!(capture.text.contains("upper\t102\t7680\t60.000000000\t100\t"));
    assert!(capture.text.ends_with("reason=window_end records=14 seen_mask=31 open_scopes=0\n"));
    let closed = capture.text.clone();
    capture.context(200, frame(103, 61.0));
    sample(&mut capture, 210);
    assert_eq!(capture.text, closed);
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
    for bad in [f64::NAN, f64::INFINITY, -1.0, 60.0] {
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
    let workspace = PathBuf::from("/source/trace-r20");
    let private_data = PathBuf::from("/private/userdata");
    let roots = [workspace.clone(), private_data.clone(), private_data];
    assert!(outside_protected(&workspace.join("captures"), &roots).is_err());
    assert!(outside_protected(&workspace, &roots).is_err());
    assert!(outside_protected(Path::new("/private/userdata/captures"), &roots).is_err());
    assert!(outside_protected(Path::new("/private/evidence"), &roots).is_ok());
    assert!(outside_protected(Path::new("/source/trace-r20-sibling"), &roots).is_ok());
}
