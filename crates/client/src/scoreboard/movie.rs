//! Separate original scoreboard surface; never shares the kill-feed crop.
use super::Board;
use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::{Extent3d, TextureDimension, TextureFormat}};
use scaleform::{Calls, Command, Config, Engine, Input, Value};
use std::{path::Path, sync::{Arc, Mutex, atomic::{AtomicU8, Ordering}}, time::Instant};

const API: &str = "_global.csrScoreboard";

#[derive(Default)]
struct Receipt {
    ready: AtomicU8,
    result: Mutex<Option<(u64, bool, u64)>>,
}
struct Bridge(Arc<Receipt>);
impl Engine for Bridge {
    fn call(&mut self, object: &str, method: &str, args: &[Value], calls: &mut Calls) -> Option<Value> {
        if object != "csr" { return None; }
        match method {
            "RootReady" => calls.call(API, "load", vec![]),
            "ScoreboardReady" => {
                info!("Original scoreboard ready: {:?}", args.first());
                self.0.ready.store(if matches!(args.first(), Some(Value::Bool(true))) { 1 } else { 2 }, Ordering::Release);
            }
            "ScoreboardResult" => {
                let [Value::String(stamp), Value::Bool(success), ..] = args else { return None };
                let stamp = stamp.parse().ok()?;
                info!("Original scoreboard receipt: revision={stamp} success={success} reason={:?}", args.get(2));
                let fence = if *success { calls.require_raster() } else { 0 };
                *self.0.result.lock().unwrap() = Some((stamp, *success, fence));
            }
            _ => return None,
        }
        Some(Value::Undefined)
    }
}

#[derive(Default)]
pub(super) struct Presentation {
    worker: Option<Mutex<scaleform::Menu>>,
    receipt: Arc<Receipt>,
    root: Option<Entity>,
    image: Handle<Image>,
    sent: Option<Board>,
    admitted: Option<Board>,
    size: (u32, u32),
    revision: u64,
    waiting_since: Option<Instant>,
    active: bool,
    failed: bool,
}

fn payload(board: &Board, local: sim::PlayerId, stamp: u64) -> Vec<Value> {
    vec![stamp.to_string().into(), board.title.clone().into(), Value::List(board.teams.iter().map(|(title, rows)| {
        let (name, score) = title.rsplit_once("    ").unwrap_or((title, ""));
        Value::List(vec![name.into(), Value::List(rows.iter().map(|row| Value::List(vec![
            row.name.clone().into(), row.status.clone().into(), row.money.clone().into(),
            row.kills.to_string().into(), row.deaths.to_string().into(), (row.id == local).into(),
        ])).collect()), score.into()])
    }).collect())]
}

fn accepts(frame: &scaleform::Frame, size: (u32, u32), revision: u64, receipt: Option<(u64, bool, u64)>) -> bool {
    (frame.width, frame.height) == size && frame.presentation_revision == revision
        && receipt.is_some_and(|(stamp, success, fence)| stamp == revision && success && frame.raster_fence >= fence)
}

impl Presentation {
    fn send(&self, input: Input) {
        if let Some(worker) = &self.worker { worker.lock().unwrap().send(input); }
    }
    fn active(&mut self, active: bool) {
        if self.active != active { self.send(Input::SetActive(active)); self.active = active; }
    }
    fn visible(&self, commands: &mut Commands, visible: bool) {
        if let Some(root) = self.root {
            commands.entity(root).insert(if visible { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    pub(super) fn update(&mut self, next: Option<&Board>, local: sim::PlayerId, game_dir: &Path,
        window: Option<&Window>, commands: &mut Commands, images: &mut Assets<Image>) -> bool {
        let (Some(next), Some(window)) = (next, window) else {
            self.active(false);
            self.waiting_since = None;
            self.visible(commands, false);
            return false;
        };
        if self.failed || std::env::var_os("CSR_NO_CLASSIC_SCOREBOARD").is_some()
            || next.teams.len() != 2 || next.teams.iter().any(|(_, rows)| rows.len() > 12) {
            self.active(false);
            self.visible(commands, false);
            return false;
        }
        let size = (window.resolution.physical_width().max(1), window.resolution.physical_height().max(1));
        if self.worker.is_none() {
            self.image = images.add(Image::new(Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                TextureDimension::D2, vec![0; 4], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default()));
            self.root = Some(commands.spawn((ImageNode::new(self.image.clone()), Visibility::Hidden,
                GlobalZIndex(20), Node { position_type: PositionType::Absolute, left: px(0), top: px(0),
                    width: percent(100), height: percent(100), ..default() })).id());
            self.worker = Some(Mutex::new(scaleform::Menu::spawn_named(Config {
                game_dir: game_dir.to_owned(), width: size.0, height: size.1,
                language: "english".into(), elements: vec![],
            }, Box::new(Bridge(self.receipt.clone())), "classic-scoreboard".into())));
            self.size = size;
            self.active = true;
            self.waiting_since = Some(Instant::now());
        }
        if size != self.size {
            self.send(Input::Resize { width: size.0, height: size.1 });
            self.size = size;
            self.sent = None;
            self.admitted = None;
        }
        if self.admitted.as_ref().is_some_and(|old| !old.same_view(next)) {
            // Money/equipment are filtered for this viewer. Never retain a
            // raster belonging to a previous team, player or map/round heading.
            self.admitted = None;
        }
        if self.receipt.ready.load(Ordering::Acquire) == 1 && self.sent.as_ref() != Some(next) {
            self.revision = self.revision.checked_add(1).expect("scoreboard revision limit");
            self.send(Input::Present { commands: vec![Command::Call { path: API.into(),
                function: "present".into(), args: payload(next, local, self.revision) }], revision: self.revision });
            self.sent = Some(next.clone());
            self.waiting_since = Some(Instant::now());
        }
        let pending = self.admitted.as_ref() != Some(next);
        if pending && self.waiting_since.is_none() { self.waiting_since = Some(Instant::now()); }
        self.active(pending);
        loop {
            let event = self.worker.as_ref().unwrap().lock().unwrap().poll_event();
            match event {
                Some(scaleform::Event::Failed(error)) => { warn!("Original scoreboard: {error}"); self.failed = true; break; }
                Some(_) => (),
                None => break,
            }
        }
        let receipt = *self.receipt.result.lock().unwrap();
        if self.receipt.ready.load(Ordering::Acquire) == 2
            || receipt.is_some_and(|(stamp, success, _)| stamp == self.revision && !success)
            || self.waiting_since.is_some_and(|start| start.elapsed().as_secs() >= 10) {
            self.failed = true;
        }
        let frame = self.worker.as_ref().unwrap().lock().unwrap().take_frame();
        if !self.failed {
            if let Some(frame) = frame.filter(|frame| accepts(frame, size, self.revision, receipt)) {
                let rect = crate::combat_hud::Rect { x: 0, y: 0, width: size.0, height: size.1 };
                if let Some(rgba) = crate::combat_hud::crop(&frame, rect).filter(|rgba| rgba.chunks_exact(4).any(|p| p[3] != 0)) {
                    if let Some(mut image) = images.get_mut(&self.image) {
                        image.texture_descriptor.size = Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 };
                        image.data = Some(rgba);
                        self.admitted = self.sent.clone();
                        self.waiting_since = None;
                        self.active(false);
                    }
                }
            }
        }
        if self.failed {
            self.active(false);
            self.worker = None;
            warn!("Original scoreboard unavailable; retaining functional board");
        }
        // A kill/death/money update must not flash the unrelated fallback
        // layout while its replacement raster is in flight. Only admission of
        // the new revision changes the displayed pixels; failure still falls
        // back and viewer/viewport changes cleared the old raster above.
        let visible = !self.failed && self.admitted.is_some();
        self.visible(commands, visible);
        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_score_updates_keep_the_same_view_but_team_changes_do_not() {
        let mut world = sim::World::new(Arc::new(collision::CollisionWorld::new()));
        let local = world.add_player("Local", sim::Team::CounterTerrorist);
        let other = world.add_player("Other", sim::Team::Terrorist);
        let admitted = super::super::board(&world, local, "de_dust2");
        world.player_mut(local).unwrap().kills += 1;
        world.player_mut(other).unwrap().deaths += 1;
        let refreshed = super::super::board(&world, local, "de_dust2");
        assert!(admitted != refreshed);
        assert!(admitted.same_view(&refreshed), "hold the original raster through stat refreshes");
        assert!(!admitted.same_view(&super::super::board(&world, other, "de_dust2")));
        assert!(!admitted.same_view(&super::super::board(&world, local, "de_nuke")));
        world.player_mut(local).unwrap().team = sim::Team::Terrorist;
        assert!(!admitted.same_view(&super::super::board(&world, local, "de_dust2")),
            "old teammate money/equipment cannot survive a team change");
    }

    #[test]
    fn scoreboard_raster_requires_matching_snapshot_viewport_and_post_receipt_capture() {
        let mut frame = scaleform::Frame { width: 2, height: 2, rgba: vec![255; 16], serial: 1,
            presentation_revision: 4, raster_fence: 7 };
        assert!(accepts(&frame, (2, 2), 4, Some((4, true, 7))));
        for receipt in [None, Some((3, true, 7)), Some((4, false, 7)), Some((4, true, 8))] {
            assert!(!accepts(&frame, (2, 2), 4, receipt));
        }
        assert!(!accepts(&frame, (3, 2), 4, Some((4, true, 7))));
        frame.presentation_revision = 3;
        assert!(!accepts(&frame, (2, 2), 4, Some((4, true, 7))));
    }
}
