//! Asset-free PC OpenXR preparation, not the game's VR player or cross-play.
//! Run only via the optional xr-smoke feature. Never changes the active runtime.
use bevy::{
    audio::AudioPlugin,
    prelude::*,
    render::pipelined_rendering::PipelinedRenderingPlugin,
    window::{ExitCondition, WindowCloseRequested},
    winit::WinitSettings,
};
use bevy_mod_openxr::{
    add_xr_plugins,
    environment_blend_mode::OxrEnvironmentBlendModes,
    graphics::GraphicsBackend,
    init::OxrInitPlugin,
    poll_events::{OxrEventHandlerExt, OxrEventIn},
    reference_space::OxrReferenceSpacePlugin,
    resources::{OxrFrameState, OxrInstance, OxrSessionConfig, OxrSessionStarted, OxrSystemId},
    session::OxrSession,
    types::AppInfo,
};
use bevy_mod_xr::{
    camera::XrCamera,
    session::{
        XrBeginSessionMessage, XrCreateSessionMessage, XrDestroySessionMessage,
        XrEndSessionMessage, XrRequestExitMessage, XrSessionDestroyedMessage, XrSessionPlugin,
        XrState, XrStateChanged,
    },
    spaces::XrPrimaryReferenceSpace,
};
use openxr::{
    EnvironmentBlendMode, Event, ReferenceSpaceType, SessionState, ViewConfigurationType,
};
use std::{
    num::NonZeroU8,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RUN_LIMIT: Duration = Duration::from_secs(45);
const CLEANUP_LIMIT: Duration = Duration::from_secs(10);
const OBSERVATIONS: u32 = 30;

fn failure(code: u8) -> AppExit {
    AppExit::Error(NonZeroU8::new(code).unwrap())
}

/// Native init/wait calls cannot be interrupted through the public plugin API.
/// The deadline covers init as well as the app loop, only killing this process.
struct Deadline {
    done: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
impl Deadline {
    fn start() -> std::io::Result<Self> {
        let done = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = done.clone();
        let thread = thread::Builder::new()
            .name("xr-smoke-deadline".into())
            .spawn(move || {
                let (lock, wake) = &*signal;
                let (done, _) = wake
                    .wait_timeout_while(lock.lock().unwrap(), RUN_LIMIT + CLEANUP_LIMIT, |done| !*done)
                    .unwrap();
                if !*done {
                    eprintln!("xr smoke: native call/cleanup exceeded hard deadline; forced process exit, cleanup unverified; failures: 1");
                    std::process::exit(4);
                }
            })?;
        Ok(Self {
            done,
            thread: Some(thread),
        })
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        let (lock, wake) = &*self.done;
        *lock.lock().unwrap() = true;
        wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn main() -> AppExit {
    let _deadline = match Deadline::start() {
        Ok(deadline) => deadline,
        Err(error) => {
            eprintln!("xr smoke: cannot own deadline worker: {error}; failures: 1");
            return failure(4);
        }
    };
    let start = Instant::now();
    match catch_unwind(AssertUnwindSafe(|| run(start))) {
        Ok(result) => result,
        Err(_) => {
            eprintln!(
                "xr smoke: dependency/runtime panic; smoke failed, graceful session cleanup unverified; failures: 1"
            );
            failure(3)
        }
    }
}

fn run(start: Instant) -> AppExit {
    eprintln!(
        "xr smoke: asset-free PRIMARY_STEREO, Vulkan, OPAQUE; run bound {} s plus {} s cleanup; no runtime/config writes",
        RUN_LIMIT.as_secs(),
        CLEANUP_LIMIT.as_secs()
    );
    let mut app = App::new();
    // Insert the session config before plugin build, not after initialization.
    app.insert_resource(OxrSessionConfig {
        blend_mode_preference: vec![EnvironmentBlendMode::OPAQUE],
        formats: None, // Let the runtime select a supported format.
        ..default()
    })
    .add_plugins(
        add_xr_plugins(
            DefaultPlugins
                .build()
                .disable::<PipelinedRenderingPlugin>()
                .disable::<AudioPlugin>(),
        )
        .set(OxrInitPlugin {
            app_info: AppInfo {
                name: "CSGO Rewrite XR Smoke".into(),
                ..default()
            },
            exts: default(), // No optional hand/passthrough extension requirement.
            backends: Some(vec![GraphicsBackend::Vulkan(())]),
            ..default()
        })
        .set(XrSessionPlugin { auto_handle: false })
        .set(OxrReferenceSpacePlugin {
            default_primary_ref_space: ReferenceSpaceType::LOCAL,
        })
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "XR smoke: desktop preview; headset observation required".into(),
                resolution: (960, 640).into(),
                ..default()
            }),
            exit_condition: ExitCondition::DontExit,
            close_when_requested: false,
            ..default()
        }),
    );
    if app.world().get_resource::<XrState>() == Some(&XrState::Unavailable) {
        eprintln!(
            "xr smoke: runtime, HMD or Vulkan unavailable (see initialization error); no stereo session created; failures: 1"
        );
        return failure(2);
    }
    if let Err(error) = validate_runtime(app.world()) {
        eprintln!("xr smoke: {error}; no stereo session created; failures: 1");
        return failure(2);
    }
    app.insert_resource(Smoke::new(start))
        .insert_resource(ClearColor(Color::srgb(0.025, 0.035, 0.055)))
        .insert_resource(WinitSettings::continuous())
        .add_oxr_event_handler(raw_event)
        .add_systems(Startup, setup)
        .add_systems(Update, drive)
        .run()
}

fn validate_runtime(world: &World) -> Result<(), String> {
    let instance = world
        .get_resource::<OxrInstance>()
        .ok_or("OpenXR instance missing")?;
    let system = world
        .get_resource::<OxrSystemId>()
        .ok_or("HMD system missing")?;
    let properties = instance.properties().map_err(|error| error.to_string())?;
    let views = instance
        .enumerate_view_configuration_views(system.0, ViewConfigurationType::PRIMARY_STEREO)
        .map_err(|error| error.to_string())?;
    let blends = instance
        .enumerate_environment_blend_modes(system.0, ViewConfigurationType::PRIMARY_STEREO)
        .map_err(|error| error.to_string())?;
    if views.len() != 2 || !blends.contains(&EnvironmentBlendMode::OPAQUE) {
        return Err("runtime does not expose two-eye PRIMARY_STEREO with OPAQUE".into());
    }
    eprintln!(
        "xr smoke: runtime {} {}, backend {:?}, stereo eyes {}, opaque supported",
        properties.runtime_name,
        properties.runtime_version,
        instance.backend(),
        views.len()
    );
    for (eye, view) in views.iter().enumerate() {
        eprintln!(
            "xr smoke: eye {eye} recommended {}x{}",
            view.recommended_image_rect_width, view.recommended_image_rect_height
        );
    }
    Ok(())
}

#[derive(Resource)]
struct Smoke {
    start: Instant,
    last_state: Option<XrState>,
    raw_state: Option<SessionState>,
    last_display_time: Option<i64>,
    observations: u32,
    stop: Option<(Instant, &'static str, bool)>,
    create_sent: bool,
    begin_sent: bool,
    end_sent: bool,
    request_sent: bool,
    destroy_sent: bool,
    exit_sent: bool,
}
impl Smoke {
    fn new(start: Instant) -> Self {
        Self {
            start,
            last_state: None,
            raw_state: None,
            last_display_time: None,
            observations: 0,
            stop: None,
            create_sent: false,
            begin_sent: false,
            end_sent: false,
            request_sent: false,
            destroy_sent: false,
            exit_sent: false,
        }
    }
    fn stop(&mut self, reason: &'static str, success: bool) {
        if self.stop.is_none() || (!success && self.stop.is_some_and(|(_, _, old)| old)) {
            self.stop = Some((Instant::now(), reason, success));
            info!(
                "xr smoke: stopping: {reason}; observations {}",
                self.observations
            );
        }
    }
}

fn raw_event(
    event: OxrEventIn,
    mut smoke: ResMut<Smoke>,
    mut state: ResMut<XrState>,
    mut changed: MessageWriter<XrStateChanged>,
    mut started: Option<ResMut<OxrSessionStarted>>,
) {
    match *event {
        Event::SessionStateChanged(event) => {
            let raw = event.state();
            smoke.raw_state = Some(raw);
            info!(
                "xr smoke: runtime session {raw:?}; focused {}",
                raw == SessionState::FOCUSED
            );
            if raw == SessionState::LOSS_PENDING || raw == SessionState::EXITING {
                if let Some(started) = started.as_mut() {
                    started.0 = false;
                }
                if smoke.stop.is_none() || raw == SessionState::LOSS_PENDING {
                    smoke.stop("runtime session lost/exited", false);
                }
            }
        }
        Event::InstanceLossPending(_) => {
            if let Some(started) = started.as_mut() {
                started.0 = false;
            }
            smoke.stop("runtime instance loss pending", false);
            // The backend ignores this event; mark it exiting so its published
            // destroy-message path can release both worlds without auto-restart.
            *state = XrState::Exiting {
                should_restart: false,
            };
            changed.write(XrStateChanged(*state));
        }
        Event::EventsLost(event) => {
            warn!("xr smoke: runtime lost {} events", event.lost_event_count());
            smoke.stop(
                "runtime event loss prevents a trustworthy lifecycle observation",
                false,
            );
        }
        _ => (),
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Metres in LOCAL space: no authored map, textures, fonts or game data.
    for (position, size, color) in [
        (
            Vec3::new(0.0, -1.5, -2.0),
            Vec3::new(6.0, 0.1, 6.0),
            Color::srgb(0.15, 0.18, 0.22),
        ),
        (
            Vec3::new(-0.65, -0.3, -2.2),
            Vec3::splat(0.45),
            Color::srgb(0.85, 0.15, 0.1),
        ),
        (
            Vec3::new(0.65, -0.3, -2.2),
            Vec3::splat(0.45),
            Color::srgb(0.1, 0.7, 0.3),
        ),
        (
            Vec3::new(0.0, 0.0, -3.1),
            Vec3::new(0.15, 1.3, 0.15),
            Color::WHITE,
        ),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(position),
        ));
    }
    // This independent desktop preview cannot demonstrate headset stereo.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.4, 1.0).looking_at(Vec3::new(0.0, -0.25, -2.5), Vec3::Y),
    ));
    info!(
        "xr smoke: generated scene ready; red left cube, green right cube, white distant pillar; desktop preview is not headset proof"
    );
}

// Exclusive system keeps optional no-session resources and message emission
// explicit, without mandatory Res parameters during destruction/fallback.
fn drive(world: &mut World) {
    let mut smoke = world.remove_resource::<Smoke>().unwrap();
    let state = world
        .get_resource::<XrState>()
        .copied()
        .unwrap_or(XrState::Unavailable);
    if smoke.last_state != Some(state) {
        info!("xr smoke: lifecycle {state:?}");
        smoke.last_state = Some(state);
    }
    let close = world
        .resource_mut::<Messages<WindowCloseRequested>>()
        .drain()
        .next()
        .is_some();
    if close
        || world
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::Escape)
    {
        smoke.stop("operator closed/cancelled smoke", false);
    }
    if smoke.stop.is_none() && smoke.start.elapsed() >= RUN_LIMIT {
        smoke.stop("unattended observation deadline reached", false);
    }

    if smoke.stop.is_none() {
        match state {
            XrState::Unavailable => smoke.stop("XR became unavailable", false),
            XrState::Available if !smoke.create_sent => {
                smoke.create_sent = true;
                world.write_message(XrCreateSessionMessage);
                info!("xr smoke: requested one session create");
            }
            XrState::Ready if !smoke.begin_sent => {
                smoke.begin_sent = true;
                world.write_message(XrBeginSessionMessage);
                info!("xr smoke: requested PRIMARY_STEREO begin");
            }
            _ => (),
        }
        observe(world, &mut smoke);
    }
    if state == XrState::Stopping && !smoke.end_sent {
        smoke.end_sent = true;
        world.write_message(XrEndSessionMessage);
        if smoke.stop.is_none() {
            smoke.stop("runtime requested stopping", false);
        }
        info!("xr smoke: requested session end");
    }
    if matches!(state, XrState::Exiting { .. }) {
        // Published destruction removes the waiter but retains Started. On a
        // loss path without STOPPING, leaving this true would schedule a frame
        // wait against removed resources before the destruction acknowledgment.
        if let Some(mut started) = world.get_resource_mut::<OxrSessionStarted>() {
            started.0 = false;
        }
        if smoke.stop.is_none() {
            smoke.stop("runtime requested exit", false);
        }
        if !smoke.destroy_sent {
            smoke.destroy_sent = true;
            world.write_message(XrDestroySessionMessage);
            info!("xr smoke: requested both-world session destruction; restart disabled");
        }
    }
    if let Some((since, reason, success)) = smoke.stop {
        if !world.contains_resource::<OxrSession>() && !smoke.create_sent {
            world.write_message(failure(2));
            smoke.exit_sent = true;
        } else if !smoke.request_sent && !smoke.destroy_sent {
            let started = world
                .get_resource::<OxrSessionStarted>()
                .is_some_and(|started| started.0);
            if started {
                world.write_message(XrRequestExitMessage);
                smoke.request_sent = true;
                info!("xr smoke: requested orderly runtime exit");
            } else if world.contains_resource::<OxrSession>() {
                world.insert_resource(XrState::Exiting {
                    should_restart: false,
                });
                world.write_message(XrDestroySessionMessage);
                smoke.destroy_sent = true;
                info!("xr smoke: destroying session that never began");
            }
        }
        let destroyed = world
            .resource_mut::<Messages<XrSessionDestroyedMessage>>()
            .drain()
            .next()
            .is_some();
        if destroyed && !smoke.exit_sent {
            let remaining = world.query::<&XrCamera>().iter(world).count();
            let clean = !world.contains_resource::<OxrSession>() && remaining == 0;
            info!(
                "xr smoke: destruction acknowledged; remaining XR cameras {remaining}; reason {reason}; observations {}; headset visual fidelity unverified",
                smoke.observations
            );
            world.write_message(if success && clean {
                AppExit::Success
            } else {
                failure(2)
            });
            smoke.exit_sent = true;
        }
        if since.elapsed() >= CLEANUP_LIMIT && !smoke.exit_sent {
            error!(
                "xr smoke: orderly cleanup deadline exhausted; process teardown, graceful cleanup unverified; failures: 1"
            );
            world.write_message(failure(4));
            smoke.exit_sent = true;
        }
    }
    world.insert_resource(smoke);
}

fn observe(world: &mut World, smoke: &mut Smoke) {
    let Some(frame) = world.get_resource::<OxrFrameState>() else {
        return;
    };
    let display = frame.predicted_display_time.as_nanos();
    if !frame.should_render
        || smoke.last_display_time.is_some_and(|last| display <= last)
        || smoke.raw_state != Some(SessionState::FOCUSED)
    {
        return;
    }
    let Some(session) = world.get_resource::<OxrSession>() else {
        return;
    };
    let Some(space) = world.get_resource::<XrPrimaryReferenceSpace>() else {
        return;
    };
    // Re-query validity: the backend's OxrViews may retain old poses on invalid
    // tracking. Never count a stale retained buffer as fresh tracking evidence.
    let Ok((flags, views)) = session.locate_views(
        ViewConfigurationType::PRIMARY_STEREO,
        frame.predicted_display_time,
        &space.0,
    ) else {
        return;
    };
    let valid = flags.contains(
        openxr::ViewStateFlags::ORIENTATION_VALID | openxr::ViewStateFlags::POSITION_VALID,
    );
    let opaque = world
        .get_resource::<OxrEnvironmentBlendModes>()
        .is_some_and(|mode| mode.blend_mode() == EnvironmentBlendMode::OPAQUE);
    let mut eyes: Vec<_> = world
        .query::<&XrCamera>()
        .iter(world)
        .map(|eye| eye.0)
        .collect();
    eyes.sort_unstable();
    if !valid || !opaque || views.len() != 2 || eyes != [0, 1] || !views.iter().all(finite_view) {
        return;
    }
    smoke.last_display_time = Some(display);
    smoke.observations += 1;
    if smoke.observations == 1 {
        info!(
            "xr smoke: focused fresh valid two-eye tracking with should_render=true and OPAQUE; eyes {eyes:?}, left pose {:?}, right pose {:?}",
            views[0].pose, views[1].pose
        );
    }
    if smoke.observations >= OBSERVATIONS {
        smoke.stop("focused stereo engine observations complete (compositor/visual verification still required)", true);
    }
}

fn finite_view(view: &openxr::View) -> bool {
    [
        view.pose.position.x,
        view.pose.position.y,
        view.pose.position.z,
        view.pose.orientation.x,
        view.pose.orientation.y,
        view.pose.orientation.z,
        view.pose.orientation.w,
        view.fov.angle_left,
        view.fov.angle_right,
        view.fov.angle_up,
        view.fov.angle_down,
    ]
    .into_iter()
    .all(f32::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loss_destroy_quiesces_frame_loop_and_never_requests_a_restart() {
        let mut world = World::new();
        world.insert_resource(Smoke::new(Instant::now()));
        world.insert_resource(XrState::Exiting {
            should_restart: true,
        });
        world.insert_resource(OxrSessionStarted(true));
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<Messages<WindowCloseRequested>>();
        world.init_resource::<Messages<XrDestroySessionMessage>>();
        world.init_resource::<Messages<XrSessionDestroyedMessage>>();
        world.init_resource::<Messages<AppExit>>();
        drive(&mut world);
        assert!(!world.resource::<OxrSessionStarted>().0);
        assert_eq!(
            world.resource::<Messages<XrDestroySessionMessage>>().len(),
            1
        );
        assert!(!world.resource::<Smoke>().create_sent);
        assert!(matches!(
            world.resource::<Smoke>().stop,
            Some((_, _, false))
        ));
    }
    #[test]
    fn runtime_loss_overrides_observations_and_success_cannot_replace_failure() {
        let mut smoke = Smoke::new(Instant::now());
        smoke.stop("observations complete", true);
        smoke.stop("runtime lost", false);
        smoke.stop("observations complete", true);
        assert!(matches!(smoke.stop, Some((_, "runtime lost", false))));
    }
}
