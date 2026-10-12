//! Normal-startup responsiveness probe. Uses the production input bridge and
//! owned Bevy window state; it does not claim OS input or compositor evidence.
use std::time::{Duration, Instant};
use bevy::{prelude::*, window::PrimaryWindow};
use crate::frontend::Frontend;

pub(crate) fn enabled() -> bool {
    std::env::var("CSR_TEST_MENU_PERFORMANCE").as_deref() == Ok("1")
}

#[derive(Resource)]
pub(crate) struct Proof {
    started: Instant,
    ready: Option<Instant>,
    last_update: Option<Instant>,
    last_presented: Option<Instant>,
    serial: u64,
    sequence: u64,
    pending: Option<(u64, Instant)>,
    last_probe: Option<Instant>,
    updates: Vec<f64>,
    presentation: Vec<f64>,
    latency: Vec<f64>,
    click_stage: u8,
    click_since: Option<Instant>,
    click_sent: Option<Instant>,
    navigation_ms: Option<f64>,
    finished: bool,
}
impl Default for Proof {
    fn default() -> Self {
        Self { started: Instant::now(), ready: None, last_update: None,
            last_presented: None, serial: 0, sequence: 0, pending: None,
            last_probe: None, updates: Vec::new(), presentation: Vec::new(),
            latency: Vec::new(), click_stage: 0, click_since: None,
            click_sent: None, navigation_ms: None, finished: false }
    }
}

pub(crate) fn motion(mut proof: ResMut<Proof>, window: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    mut mouse: MessageWriter<bevy::input::mouse::MouseButtonInput>) {
    use bevy::input::ButtonState;
    let Some(ready) = proof.ready else { return };
    let seconds = ready.elapsed().as_secs_f64();
    if seconds < 3.0 || proof.finished { return; }
    let (entity, mut window) = window.into_inner();
    window.focused = true;
    if seconds < 23.0 {
        let x = (0.15 + 0.65 * (seconds * 9.0).sin().abs()) * window.physical_width() as f64;
        let y = (0.04 + 0.25 * (seconds * 7.0).cos().abs()) * window.physical_height() as f64;
        window.set_physical_cursor_position(Some(glam::DVec2::new(x, y)));
        return;
    }
    let settled = proof.click_since.is_some_and(|since| since.elapsed() >= Duration::from_millis(100));
    let position = match proof.click_stage { 0 => Some((486.,26.)), 3 if settled => Some((503.,138.)), _ => None };
    if let Some((x,y)) = position {
        let position = glam::DVec2::new(x * window.physical_width() as f64 / 1920., y * window.physical_height() as f64 / 1080.);
        window.set_physical_cursor_position(Some(position));
        proof.click_stage += 1;
        proof.click_since = Some(Instant::now());
    } else if settled && matches!(proof.click_stage, 1|2|4|5) {
        let press = matches!(proof.click_stage, 1|4);
        if proof.click_stage == 4 { proof.click_sent = Some(Instant::now()); }
        mouse.write(bevy::input::mouse::MouseButtonInput { window: entity,
            button: MouseButton::Left, state: if press { ButtonState::Pressed } else { ButtonState::Released } });
        proof.click_stage += 1;
        proof.click_since = Some(Instant::now());
    }
}

fn summary(values: &[f64]) -> (f64, f64, f64) {
    if values.is_empty() { return (0.0, f64::INFINITY, f64::INFINITY); }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    // Nearest-rank percentile also reports the slow sample in tiny failed runs.
    (values.iter().sum::<f64>() / values.len() as f64,
     values[(values.len() as f64 * 0.95).ceil() as usize - 1],
     *values.last().unwrap())
}

pub(crate) fn measure(mut proof: ResMut<Proof>, mut front: ResMut<Frontend>,
    window: Single<&Window, With<PrimaryWindow>>, mut exit: MessageWriter<AppExit>) {
    if proof.finished { return; }
    if proof.started.elapsed() > Duration::from_secs(75) {
        error!("menu performance: deadline exceeded; failures: 1");
        proof.finished = true;
        exit.write(AppExit::error());
        return;
    }
    if proof.ready.is_none() {
        if !front.movie_ready("MainMenu") { return; }
        info!("menu performance: MainMenu ready in {:.3}s; logical {}x{}, physical {}x{}, scale {}; normal startup/input path",
            proof.started.elapsed().as_secs_f64(), window.width(), window.height(),
            window.physical_width(), window.physical_height(), window.scale_factor());
        proof.ready = Some(Instant::now());
    }
    let now = Instant::now();
    let seconds = proof.ready.unwrap().elapsed().as_secs_f64();
    if proof.navigation_ms.is_none() && front.lobby_phase() == crate::frontend_lobby::Phase::Browser {
        if let Some(sent) = proof.click_sent {
            let ms = sent.elapsed().as_secs_f64() * 1000.;
            proof.navigation_ms = Some(ms);
            info!("menu performance: real Play -> Friends pointer navigation in {ms:.3}ms");
        }
    }
    if let Some((id, received)) = front.take_performance_ack() {
        if let Some((expected, sent)) = proof.pending {
            if id == expected {
                let ms = received.duration_since(sent).as_secs_f64() * 1000.0;
                info!("menu performance: input receipt {id} latency {ms:.3}ms");
                proof.latency.push(ms);
                proof.pending = None;
            }
        }
    }
    if (3.0..23.0).contains(&seconds) {
        if let Some(previous) = proof.last_update.replace(now) {
            proof.updates.push(now.duration_since(previous).as_secs_f64() * 1000.0);
        }
        if front.presented_serial() != proof.serial {
            proof.serial = front.presented_serial();
            if let Some(previous) = proof.last_presented.replace(now) {
                proof.presentation.push(now.duration_since(previous).as_secs_f64() * 1000.0);
            }
        }
        if proof.pending.is_none() && proof.last_probe.is_none_or(|last| last.elapsed() >= Duration::from_millis(500)) {
            proof.sequence += 1;
            let id = proof.sequence;
            front.send(scaleform::Input::Inspect(format!("_global.csrPerformanceProbe.{id}")));
            proof.pending = Some((id, now));
            proof.last_probe = Some(now);
        }
    }
    if seconds < 28.0 { return; }
    let (update_mean, update_p95, update_max) = summary(&proof.updates);
    let (present_mean, present_p95, present_max) = summary(&proof.presentation);
    let (_, input_p95, input_max) = summary(&proof.latency);
    let failed = proof.pending.is_some() || proof.latency.len() < 20 || proof.presentation.len() < 400
        || input_p95 > 100.0 || input_max > 250.0 || present_p95 > 75.0 || present_max > 250.0
        || proof.navigation_ms.is_none_or(|ms| ms > 250.0) || !front.movie_ready("LobbyBrowser");
    info!("menu performance: update mean/p95/max {update_mean:.3}/{update_p95:.3}/{update_max:.3}ms; presentation {} frames mean/p95/max {present_mean:.3}/{present_p95:.3}/{present_max:.3}ms; input {} receipts p95/max {input_p95:.3}/{input_max:.3}ms; pending {}; failures: {}",
        proof.presentation.len(), proof.latency.len(), proof.pending.is_some(), u8::from(failed));
    proof.finished = true;
    exit.write(if failed { AppExit::error() } else { AppExit::Success });
}
