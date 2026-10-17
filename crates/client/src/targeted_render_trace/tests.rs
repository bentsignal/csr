//! Authored source fixtures; UNRUN in R18 (no Cargo/build/test authorization).
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

#[test]
fn finite_half_open_sim_window_and_wall_deadline_do_not_rearm() {
    let mut capture = Capture::new(window(), "window", None);
    sample(&mut capture, 1); // no frame identity: no sample
    capture.context(10, frame(1, 29.999)); sample(&mut capture, 20);
    capture.context(50, frame(2, 30.0)); sample(&mut capture, 60);
    capture.context(100, frame(3, 59.999)); sample(&mut capture, 110);
    capture.context(150, frame(4, 60.0)); sample(&mut capture, 160);
    let closed = capture.text.clone();
    capture.context(200, frame(5, 30.0)); sample(&mut capture, 210);
    assert_eq!(capture.reason, Some("window_end"));
    assert_eq!(capture.text, closed);
    assert_eq!(capture.text.lines().filter(|l| l.starts_with("sample\t")).count(), 2);

    let mut capture = Capture::new(window(), "wall", None);
    capture.context(window().wall_ns, frame(1, 30.0));
    assert_eq!(capture.reason, Some("wall_limit"));
    assert_eq!(capture.records, 0);
    for bad in [f64::NAN, f64::INFINITY, -1.0, 60.0] {
        assert!(Window { start: bad, ..window() }.validate().is_err());
    }
    assert!(Window { end: f64::INFINITY, ..window() }.validate().is_err());
    assert!(Window { records: MAX_RECORDS + 1, ..window() }.validate().is_err());
    assert!(Window { bytes: MAX_BYTES + 1, ..window() }.validate().is_err());
}

#[test]
fn caps_keep_complete_rows_and_a_footer_with_bounded_total_bytes() {
    for (cap, reason) in [
        (Window { records: 3, ..window() }, "record_cap"),
        (Window { records: MAX_RECORDS, bytes: 4096, ..window() }, "byte_cap"),
    ] {
        let mut capture = Capture::new(cap, "cap", None);
        capture.context(0, frame(1, 30.0));
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
}

#[test]
fn context_changes_and_overlapping_invocations_fail_closed() {
    let mut capture = Capture::new(window(), "rewind", None);
    capture.context(0, frame(2, 31.0));
    capture.context(10, frame(3, 30.0));
    assert_eq!(capture.reason, Some("context_rewind"));
    let mut capture = Capture::new(window(), "lost", None);
    capture.context(0, frame(1, 30.0)); capture.context(10, None);
    assert_eq!(capture.reason, Some("context_lost"));
    let mut capture = Capture::new(window(), "overlap", None);
    capture.context(0, frame(1, 30.0));
    let thread = std::thread::current().id();
    capture.enter(0, 7, thread, 1); capture.enter(0, 8, thread, 2);
    assert_eq!(capture.reason, Some("overlapping_same_system"));
    let mut capture = Capture::new(window(), "mismatch", None);
    capture.context(0, frame(1, 30.0));
    capture.enter(0, 7, thread, 1); capture.exit(0, 8, thread, 2);
    assert_eq!(capture.reason, Some("scope_mismatch"));
}
