//! Presentation snapshots and pixel regions for the shared authored HUD worker.
//! Gameplay continues to change only through simulation inputs.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use scaleform::{Command, Value};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};

const API: &str = "_global.csrCombatHud";

#[derive(Clone, Copy, Component)]
pub(crate) enum Fallback {
    Health,
    Ammo,
}

#[derive(Component)]
pub(crate) struct Surface(usize);

#[derive(Default)]
pub(crate) struct State {
    // 0 pending, 1 original movie ready, 2 component load failed.
    status: [AtomicU8; 2],
    expected: [AtomicU64; 2],
    accepted: [AtomicU64; 2],
    receipt_fence: [AtomicU64; 2],
    rejected: [AtomicBool; 2],
}
impl State {
    pub fn receive(&self, method: &str, args: &[Value], calls: &mut scaleform::Calls) -> bool {
        let part = match args.first().map(Value::str).as_deref() {
            Some("health") => 0,
            Some("ammo") => 1,
            _ => return false,
        };
        match method {
            "CombatHudReady" => self.status[part].store(1, Ordering::Release),
            "CombatHudFailed" => {
                self.status[part].store(2, Ordering::Release);
                warn!(
                    part,
                    ?args,
                    "Original HUD component keeps ordinary fallback"
                );
            }
            "CombatHudResult" => {
                let (Some(Value::String(stamp)), Some(Value::Bool(success))) =
                    (args.get(1), args.get(2))
                else {
                    return false;
                };
                let Ok(stamp) = stamp.parse::<u64>() else {
                    return false;
                };
                if stamp == 0 || stamp != self.expected[part].load(Ordering::Acquire) {
                    return false;
                }
                self.rejected[part].store(!success, Ordering::Release);
                if *success {
                    // An older GPU readback can finish after this callback with
                    // the same presentation revision. Publish the capture floor
                    // before accepting the receipt, and require one fresh render.
                    self.receipt_fence[part].store(calls.require_raster(), Ordering::Relaxed);
                    self.accepted[part].store(stamp, Ordering::Release);
                }
            }
            _ => return false,
        }
        if std::env::var_os("CSR_COMBAT_HUD_TRACE").is_some() {
            info!(method, ?args, "Original combat HUD component callback");
        }
        true
    }
    fn ready(&self, part: usize) -> bool {
        self.status[part].load(Ordering::Acquire) == 1
    }
    pub fn pending(&self) -> bool {
        self.status
            .iter()
            .any(|status| status.load(Ordering::Acquire) == 0)
    }
    pub fn timeout(&self) {
        for status in &self.status {
            let _ = status.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
        }
    }
}

#[derive(Default)]
pub(crate) struct Presentation {
    pub state: Arc<State>,
    pub enabled: bool,
    images: [Handle<Image>; 2],
    sent: [Option<Snapshot>; 2],
    accepted: [Option<Snapshot>; 2],
    viewport: [(u32, u32); 2],
    raster_revision: [u64; 2],
    visible: [bool; 2],
}

fn command(function: &str, args: Vec<Value>) -> Command {
    Command::Call {
        path: API.into(),
        function: function.into(),
        args,
    }
}

impl Presentation {
    pub fn spawn(&mut self, commands: &mut Commands, images: &mut Assets<Image>) {
        self.enabled = std::env::var_os("CSR_NO_CLASSIC_COMBAT_HUD").is_none();
        if !self.enabled {
            return;
        }
        for part in 0..2 {
            let image = images.add(Image::new(
                Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                vec![0; 4],
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ));
            commands.spawn((
                crate::session::Persistent,
                crate::hud::GameHud,
                Surface(part),
                ImageNode::new(image.clone()),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                GlobalZIndex(90),
                Visibility::Hidden,
            ));
            self.images[part] = image;
        }
    }
    pub fn commands(
        &mut self,
        snapshot: Option<&Snapshot>,
        revision: u64,
        resize: bool,
    ) -> Vec<Command> {
        if !self.enabled {
            return vec![];
        }
        let Some(snapshot) = snapshot else {
            let had_snapshot = self.sent.iter().any(Option::is_some);
            self.sent = [None, None];
            self.accepted = [None, None];
            self.visible = [false; 2];
            for expected in &self.state.expected {
                expected.store(revision, Ordering::Release);
            }
            return if had_snapshot {
                vec![command("clear", vec![])]
            } else {
                vec![]
            };
        };
        let mut batch = vec![];
        if resize {
            batch.push(command("resize", vec![]));
        }
        for part in 0..2 {
            if !self.state.ready(part)
                || (!resize
                    && self.sent[part]
                        .as_ref()
                        .is_some_and(|old| old.same_component(snapshot, part)))
            {
                continue;
            }
            let mut args = vec![revision.to_string().into()];
            let function = if part == 0 {
                args.extend([
                    snapshot.health.into(),
                    snapshot.armor.into(),
                    snapshot.helmet.into(),
                    snapshot.alive.into(),
                ]);
                "health"
            } else {
                let weapon = snapshot.weapon.as_ref();
                args.extend([
                    weapon.map_or(0, |w| w.clip).into(),
                    weapon.map_or(0, |w| w.reserve).into(),
                    (snapshot.alive && weapon.is_some_and(|w| w.uses_ammo)).into(),
                    weapon
                        .and_then(|w| w.burst_mode)
                        .map_or(Value::Null, Value::Bool),
                ]);
                "ammo"
            };
            self.state.expected[part].store(revision, Ordering::Release);
            // A newer receipt owns its own pending/rejection state. Otherwise
            // a prior rejection can park this retry before Show settles.
            self.state.rejected[part].store(false, Ordering::Release);
            self.sent[part] = Some(snapshot.clone());
            batch.push(command(function, args));
        }
        batch
    }
    pub fn waiting(&self) -> bool {
        self.enabled
            && (self.state.pending()
                || (0..2).any(|part| {
                    self.state.ready(part)
                        && !self.state.rejected[part].load(Ordering::Acquire)
                        && self.sent[part].is_some()
                        && self.state.expected[part].load(Ordering::Acquire)
                            > self.raster_revision[part]
                }))
    }
    pub fn hide(&mut self) {
        self.visible = [false; 2];
    }
    pub fn admit(
        &mut self,
        frame: &scaleform::Frame,
        snapshot: Option<&Snapshot>,
        images: &mut Assets<Image>,
    ) {
        let Some(snapshot) = snapshot else {
            self.hide();
            return;
        };
        let size = (frame.width, frame.height);
        let rects = regions(frame.width, frame.height);
        for part in 0..2 {
            let expected = self.state.expected[part].load(Ordering::Acquire);
            if expected == 0
                || expected > frame.presentation_revision
                || !self.state.ready(part)
                || self.state.rejected[part].load(Ordering::Acquire)
                || self.state.accepted[part].load(Ordering::Acquire) != expected
                || frame.raster_fence < self.state.receipt_fence[part].load(Ordering::Relaxed)
            {
                continue;
            }
            if self.viewport[part] == size
                && self.accepted[part]
                    .as_ref()
                    .is_some_and(|old| old.same_component(snapshot, part))
            {
                self.raster_revision[part] = expected;
                continue;
            }
            let rect = rects[part + 1];
            let Some(rgba) = crop(frame, rect) else {
                self.visible[part] = false;
                continue;
            };
            let Some(mut image) = images.get_mut(&self.images[part]) else {
                continue;
            };
            image.texture_descriptor.size = Extent3d {
                width: rect.width,
                height: rect.height,
                depth_or_array_layers: 1,
            };
            image.data = Some(rgba);
            self.viewport[part] = size;
            self.accepted[part] = Some(snapshot.clone());
            self.raster_revision[part] = expected;
            if std::env::var_os("CSR_COMBAT_HUD_TRACE").is_some() {
                info!(
                    part,
                    revision = frame.presentation_revision,
                    raster_fence = frame.raster_fence,
                    ?size,
                    ?snapshot,
                    "Original combat HUD raster admitted"
                );
            }
        }
    }
    pub fn visibility(&mut self, snapshot: Option<&Snapshot>, size: (u32, u32)) {
        for part in 0..2 {
            self.visible[part] = self.enabled
                && self.state.ready(part)
                && !self.state.rejected[part].load(Ordering::Acquire)
                && self.viewport[part] == size
                && snapshot.is_some_and(|now| {
                    self.accepted[part]
                        .as_ref()
                        .is_some_and(|old| old.same_life(now))
                });
        }
    }
}

pub(crate) fn display(
    presentation: Res<crate::kill_feed::Presentation>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut surfaces: Query<(&Surface, &mut Visibility, &mut Node), Without<Fallback>>,
    mut fallbacks: Query<(&Fallback, &mut Visibility), Without<Surface>>,
) {
    let hud = &presentation.combat;
    let Ok(window) = windows.single() else { return };
    let rects = regions(
        window.resolution.physical_width().max(1),
        window.resolution.physical_height().max(1),
    );
    let scale = window.scale_factor();
    for (surface, mut visibility, mut node) in &mut surfaces {
        let rect = rects[surface.0 + 1];
        let left = px(rect.x as f32 / scale);
        let top = px(rect.y as f32 / scale);
        let width = px(rect.width as f32 / scale);
        let height = px(rect.height as f32 / scale);
        if node.left != left {
            node.left = left;
        }
        if node.top != top {
            node.top = top;
        }
        if node.width != width {
            node.width = width;
        }
        if node.height != height {
            node.height = height;
        }
        let value = if hud.visible[surface.0] {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != value {
            *visibility = value;
        }
    }
    for (kind, mut visibility) in &mut fallbacks {
        let part = match kind {
            Fallback::Health => 0,
            Fallback::Ammo => 1,
        };
        let value = if hud.visible[part] {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != value {
            *visibility = value;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub player: sim::PlayerId,
    pub life: u64,
    pub alive: bool,
    pub health: i32,
    pub armor: i32,
    pub helmet: bool,
    pub weapon: Option<Ammo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ammo {
    pub content: String,
    pub class_name: String,
    pub name_token: String,
    pub clip: i32,
    pub reserve: i32,
    pub capacity: i32,
    pub uses_ammo: bool,
    /// Only a schema-admitted burst profile owns the original mode indicator.
    pub burst_mode: Option<bool>,
}

impl Snapshot {
    pub fn from_player(player: &sim::Player) -> Self {
        Self {
            player: player.id,
            life: player.life_epoch,
            alive: player.alive,
            health: player.health,
            armor: player.armor,
            helmet: player.helmet,
            weapon: player.active_weapon().map(|weapon| Ammo {
                content: weapon.def.id.to_string(),
                class_name: weapon.def.class_name.clone(),
                name_token: weapon.def.name_token.clone(),
                clip: weapon.clip,
                reserve: weapon.reserve,
                capacity: weapon.def.clip_size,
                uses_ammo: weapon.uses_ammo(),
                burst_mode: weapon.burst_profile().map(|_| weapon.burst_mode),
            }),
        }
    }

    /// Pending ordinary value updates may retain the last valid raster. A new
    /// viewed player/life must never inherit the old player's health or ammo.
    pub fn same_life(&self, other: &Self) -> bool {
        (self.player, self.life, self.alive) == (other.player, other.life, other.alive)
    }

    fn same_component(&self, other: &Self, part: usize) -> bool {
        self.same_life(other)
            && if part == 0 {
                (self.health, self.armor, self.helmet) == (other.health, other.armor, other.helmet)
            } else {
                self.weapon == other.weapon
            }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Partition in physical pixels, never fractional CSS halves. Odd dimensions
/// belong to the top/left region so no source pixel is duplicated or stretched.
pub(crate) fn regions(width: u32, height: u32) -> [Rect; 3] {
    let top = height.div_ceil(2);
    let left = width.div_ceil(2);
    [
        Rect {
            x: 0,
            y: 0,
            width,
            height: top,
        },
        Rect {
            x: 0,
            y: top,
            width: left,
            height: height - top,
        },
        Rect {
            x: left,
            y: top,
            width: width - left,
            height: height - top,
        },
    ]
}

pub(crate) fn crop(frame: &scaleform::Frame, rect: Rect) -> Option<Vec<u8>> {
    let stride = (frame.width as usize).checked_mul(4)?;
    let expected = stride.checked_mul(frame.height as usize)?;
    if frame.rgba.len() != expected
        || rect.width == 0
        || rect.height == 0
        || rect.x.checked_add(rect.width)? > frame.width
        || rect.y.checked_add(rect.height)? > frame.height
    {
        return None;
    }
    let row_bytes = (rect.width as usize).checked_mul(4)?;
    let mut rgba = Vec::with_capacity(row_bytes.checked_mul(rect.height as usize)?);
    for y in rect.y..rect.y + rect.height {
        let start = y as usize * stride + rect.x as usize * 4;
        rgba.extend_from_slice(frame.rgba.get(start..start + row_bytes)?);
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_revision_pixels_submitted_before_show_cannot_freeze_the_hud() {
        let mut p = Presentation::default();
        p.enabled = true;
        p.state.status[0].store(1, Ordering::Release);
        p.state.status[1].store(2, Ordering::Release);
        let mut images = Assets::<Image>::default();
        p.images[0] = images.add(Image::new(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0; 4],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));
        let snapshot = Snapshot {
            player: 1,
            life: 3,
            alive: true,
            health: 73,
            armor: 41,
            helmet: true,
            weapon: None,
        };
        p.commands(Some(&snapshot), 8, false);
        let mut calls = scaleform::Calls::default();
        assert!(p.state.receive(
            "CombatHudResult",
            &["health".into(), "8".into(), true.into()],
            &mut calls
        ));
        let mut frame = scaleform::Frame {
            width: 4,
            height: 4,
            rgba: vec![7; 64],
            serial: 1,
            presentation_revision: 8,
            raster_fence: 0,
        };
        p.admit(&frame, Some(&snapshot), &mut images);
        p.visibility(Some(&snapshot), (4, 4));
        assert!(
            p.accepted[0].is_none(),
            "pre-Show pixels must not enter the snapshot cache"
        );
        assert!(
            !p.visible[0],
            "ordinary fallback stays visible until the settled capture"
        );
        assert!(
            p.waiting(),
            "the worker must not park on a pre-Show readback"
        );

        frame.raster_fence = p.state.receipt_fence[0].load(Ordering::Acquire);
        frame.serial = 2;
        frame.rgba = vec![99; 64];
        p.admit(&frame, Some(&snapshot), &mut images);
        p.visibility(Some(&snapshot), (4, 4));
        assert!(p.visible[0]);
        assert!(!p.waiting());
        assert_eq!(
            images.get(&p.images[0]).unwrap().data.as_ref().unwrap(),
            &vec![99; 16]
        );

        frame.raster_fence = 0;
        frame.rgba = vec![7; 64];
        p.admit(&frame, Some(&snapshot), &mut images);
        assert_eq!(
            images.get(&p.images[0]).unwrap().data.as_ref().unwrap(),
            &vec![99; 16]
        );
    }

    #[test]
    fn mode_only_changes_refresh_ammo_and_unsupported_weapons_clear_the_indicator() {
        fn mode(batch: Vec<Command>) -> (Value, Value) {
            batch
                .into_iter()
                .find_map(|command| match command {
                    Command::Call { function, args, .. } if function == "ammo" => {
                        Some((args[4].clone(), args[3].clone()))
                    }
                    _ => None,
                })
                .expect("an ammo update")
        }
        let mut presentation = Presentation::default();
        presentation.enabled = true;
        assert!(presentation.state.receive(
            "CombatHudReady",
            &["ammo".into()],
            &mut scaleform::Calls::default()
        ));
        let mut snapshot = Snapshot {
            player: 1,
            life: 3,
            alive: true,
            health: 100,
            armor: 0,
            helmet: false,
            weapon: Some(Ammo {
                content: "test_burst".into(),
                class_name: "test_burst".into(),
                name_token: "test".into(),
                clip: 20,
                reserve: 120,
                capacity: 20,
                uses_ammo: true,
                burst_mode: Some(false),
            }),
        };
        assert_eq!(
            mode(presentation.commands(Some(&snapshot), 11, false)),
            (Value::Bool(false), Value::Bool(true))
        );
        assert!(presentation.commands(Some(&snapshot), 12, false).is_empty());
        snapshot.weapon.as_mut().unwrap().burst_mode = Some(true);
        assert_eq!(
            mode(presentation.commands(Some(&snapshot), 13, false)),
            (Value::Bool(true), Value::Bool(true))
        );
        assert!(!presentation.state.receive(
            "CombatHudResult",
            &["ammo".into(), "11".into(), true.into()],
            &mut scaleform::Calls::default()
        ));
        snapshot.weapon.as_mut().unwrap().burst_mode = None;
        assert_eq!(
            mode(presentation.commands(Some(&snapshot), 14, false)),
            (Value::Null, Value::Bool(true))
        );
        snapshot.weapon.as_mut().unwrap().burst_mode = Some(true);
        snapshot.alive = false;
        assert_eq!(
            mode(presentation.commands(Some(&snapshot), 15, false)),
            (Value::Bool(true), Value::Bool(false))
        );
        snapshot.alive = true;
        snapshot.life += 1;
        snapshot.weapon = None;
        assert_eq!(
            mode(presentation.commands(Some(&snapshot), 16, false)),
            (Value::Null, Value::Bool(false))
        );
    }

    #[test]
    fn a_new_component_retry_stays_active_until_its_own_settled_raster() {
        let mut p = Presentation::default();
        p.enabled = true;
        for name in ["health", "ammo"] {
            assert!(p.state.receive(
                "CombatHudReady",
                &[name.into()],
                &mut scaleform::Calls::default()
            ));
        }
        let mut snapshot = Snapshot {
            player: 1,
            life: 3,
            alive: true,
            health: 100,
            armor: 100,
            helmet: true,
            weapon: None,
        };
        p.commands(Some(&snapshot), 1, false);
        assert!(p.state.receive(
            "CombatHudResult",
            &["health".into(), "1".into(), false.into()],
            &mut scaleform::Calls::default()
        ));
        assert!(p.state.receive(
            "CombatHudResult",
            &["ammo".into(), "1".into(), true.into()],
            &mut scaleform::Calls::default()
        ));
        p.raster_revision[1] = 1;
        assert!(!p.waiting(), "A rejected component may park with fallback");
        snapshot.health = 73;
        assert_eq!(p.commands(Some(&snapshot), 2, false).len(), 1);
        assert!(p.waiting(), "The retry must run its deferred Show observer");
        assert!(!p.state.receive(
            "CombatHudResult",
            &["health".into(), "1".into(), false.into()],
            &mut scaleform::Calls::default()
        ));
        assert!(p.waiting(), "An old failure cannot cancel the retry");
        assert!(p.state.receive(
            "CombatHudResult",
            &["health".into(), "2".into(), true.into()],
            &mut scaleform::Calls::default()
        ));
        assert!(p.waiting(), "Success still needs the matching raster");
        p.raster_revision[0] = 2;
        assert!(!p.waiting());
    }

    #[test]
    fn stale_component_results_cannot_hide_or_admit_a_newer_snapshot() {
        let state = State::default();
        state.expected[0].store(12, Ordering::Release);
        state.expected[1].store(13, Ordering::Release);
        assert!(!state.receive(
            "CombatHudResult",
            &["health".into(), "11".into(), true.into()],
            &mut scaleform::Calls::default()
        ));
        assert_eq!(state.accepted[0].load(Ordering::Acquire), 0);
        assert!(state.receive(
            "CombatHudResult",
            &["ammo".into(), "13".into(), true.into()],
            &mut scaleform::Calls::default()
        ));
        assert!(state.receive(
            "CombatHudResult",
            &["health".into(), "12".into(), false.into()],
            &mut scaleform::Calls::default()
        ));
        assert!(state.rejected[0].load(Ordering::Acquire));
        assert!(!state.rejected[1].load(Ordering::Acquire));
        assert_eq!(state.accepted[1].load(Ordering::Acquire), 13);
        assert!(!state.receive(
            "CombatHudResult",
            &["ammo".into(), "12".into(), false.into()],
            &mut scaleform::Calls::default()
        ));
        assert!(!state.rejected[1].load(Ordering::Acquire));
    }

    #[test]
    fn a_success_callback_alone_never_hides_ordinary_gameplay_values() {
        let mut presentation = Presentation::default();
        presentation.enabled = true;
        let snapshot = Snapshot {
            player: 1,
            life: 3,
            alive: true,
            health: 73,
            armor: 41,
            helmet: true,
            weapon: None,
        };
        presentation.state.receive(
            "CombatHudReady",
            &["health".into()],
            &mut scaleform::Calls::default(),
        );
        presentation.commands(Some(&snapshot), 8, false);
        presentation.state.receive(
            "CombatHudResult",
            &["health".into(), "8".into(), true.into()],
            &mut scaleform::Calls::default(),
        );
        presentation.visibility(Some(&snapshot), (1920, 1080));
        assert_eq!(
            presentation.visible, [false; 2],
            "A matching raster is still required"
        );
        assert!(presentation.waiting());
        presentation.commands(None, 9, false);
        assert!(!presentation.state.receive(
            "CombatHudResult",
            &["health".into(), "8".into(), true.into()],
            &mut scaleform::Calls::default()
        ));
        assert_eq!(presentation.visible, [false; 2]);
    }

    #[test]
    fn pending_damage_can_retain_pixels_but_respawn_and_spectating_cannot() {
        let old = Snapshot {
            player: 1,
            life: 3,
            alive: true,
            health: 100,
            armor: 100,
            helmet: true,
            weapon: None,
        };
        let mut next = old.clone();
        next.health = 73;
        assert!(old.same_life(&next));
        next.life += 1;
        assert!(!old.same_life(&next));
        next.life = old.life;
        next.player = 2;
        assert!(!old.same_life(&next));
        next.player = old.player;
        next.alive = false;
        assert!(!old.same_life(&next));
    }

    #[test]
    fn odd_viewport_crops_preserve_every_physical_pixel_once() {
        let frame = scaleform::Frame {
            width: 5,
            height: 3,
            serial: 1,
            presentation_revision: 7,
            raster_fence: 0,
            rgba: (0..15).flat_map(|n| [n, n, n, 255]).collect(),
        };
        let parts = regions(5, 3);
        assert_eq!(
            parts.map(|rect| (rect.width, rect.height)),
            [(5, 2), (3, 1), (2, 1)]
        );
        let pixels: Vec<_> = parts
            .into_iter()
            .flat_map(|rect| {
                crop(&frame, rect)
                    .unwrap()
                    .chunks_exact(4)
                    .map(|pixel| pixel[0])
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(pixels, (0..15).collect::<Vec<_>>());
        assert!(
            crop(
                &frame,
                Rect {
                    x: 4,
                    y: 2,
                    width: 2,
                    height: 1
                }
            )
            .is_none()
        );
        assert!(
            crop(
                &frame,
                Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 0
                }
            )
            .is_none()
        );
    }

    #[test]
    fn truncated_raster_is_rejected_instead_of_hiding_usable_fallback() {
        let frame = scaleform::Frame {
            width: 1920,
            height: 1080,
            rgba: vec![0; 8],
            serial: 1,
            presentation_revision: 3,
            raster_fence: 0,
        };
        assert!(crop(&frame, regions(1920, 1080)[0]).is_none());
    }
}
