//! Classic notices use the player's authored movie, never a copied HUD image.
use crate::{
    GameData,
    effects::{Effects, KillFeedEntry},
    player::{LocalPlayer, SimWorld},
    session::{AppState, Persistent},
};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDataOrder, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
use scaleform::{Calls, Command, Config, Engine, Input, Value};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

const API: &str = "_global.csrDeathNotices";

#[derive(Component)]
pub(crate) struct Fallback;
#[derive(Component)]
struct Surface;

#[derive(Default, Resource)]
pub(crate) struct Presentation {
    movie: Option<Mutex<scaleform::Menu>>,
    ready: Arc<AtomicBool>,
    notice_failed: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    rejected: Arc<Mutex<Vec<sim::death::DeathKey>>>,
    started: Option<std::time::Instant>,
    node: Option<Entity>,
    image: Handle<Image>,
    notice_extent: Option<(u32, u32)>,
    viewport: (u32, u32),
    entries: Vec<sim::death::DeathKey>,
    revision: u64,
    presented: bool,
    active: bool,
    last_alpha: f32,
    pub(crate) combat: crate::combat_hud::Presentation,
}

impl Presentation {
    fn begin_present(&mut self, resized: bool) {
        self.revision = self.revision.checked_add(1).expect("kill-feed revision limit");
        // Retain the last admitted raster until its replacement arrives.
        if resized {
            self.presented = false;
            self.notice_extent = None;
        }
    }

    fn admit_frame(
        &mut self,
        frame: &scaleform::Frame,
        snapshot: Option<&crate::combat_hud::Snapshot>,
        size: (u32, u32),
        ready: bool,
        rendered_count: usize,
        images: &mut Assets<Image>,
    ) {
        if frame.presentation_revision != self.revision || (frame.width, frame.height) != size {
            return;
        }
        self.combat.admit(frame, snapshot, images);
        if !ready {
            return;
        }
        if rendered_count == 0 {
            // No visible notice pixels changed with a health/ammo update.
            self.presented = true;
            return;
        }

        // Validate the full readback before finding notice ink in its top half.
        // Transparent movie margins must not determine the feed's right edge.
        let Some(stride) = (frame.width as usize).checked_mul(4) else {
            return;
        };
        let Some(expected) = stride.checked_mul(frame.height as usize) else {
            return;
        };
        if frame.width == 0 || frame.height == 0 || frame.rgba.len() != expected {
            return;
        }
        let mut bounds = crate::combat_hud::Rect {
            x: frame.width, y: frame.height, width: 0, height: 0,
        };
        let (mut right, mut bottom) = (0, 0);
        for (y, row) in frame.rgba[..stride * frame.height.div_ceil(2) as usize]
            .chunks_exact(stride).enumerate()
        {
            for (x, pixel) in row.chunks_exact(4).enumerate() {
                if pixel[3] == 0 { continue; }
                bounds.x = bounds.x.min(x as u32);
                bounds.y = bounds.y.min(y as u32);
                right = right.max(x as u32 + 1);
                bottom = bottom.max(y as u32 + 1);
            }
        }
        if right == 0 {
            self.notice_extent = None;
            self.presented = true;
            return;
        }
        bounds.width = right - bounds.x;
        bounds.height = bottom - bounds.y;
        let extent = Extent3d {
            width: bounds.width,
            height: bounds.height,
            depth_or_array_layers: 1,
        };
        let Some(image) = images.get(&self.image) else {
            return;
        };
        let identical = image.texture_descriptor.size == extent
            && image.texture_descriptor.dimension == TextureDimension::D2
            && image.texture_descriptor.format == TextureFormat::Rgba8UnormSrgb
            && image.texture_descriptor.mip_level_count == 1
            && image.texture_descriptor.sample_count == 1
            && image.data_order == TextureDataOrder::default()
            && image.texture_view_descriptor.is_none()
            && image.data.as_deref().is_some_and(|data| {
                let row_bytes = bounds.width as usize * 4;
                data.len() == row_bytes * bounds.height as usize
                    && data.chunks_exact(row_bytes).enumerate().all(|(y, row)| {
                        let start = (bounds.y as usize + y) * stride + bounds.x as usize * 4;
                        row == &frame.rgba[start..start + row_bytes]
                    })
            });
        if !identical {
            let Some(rgba) = crate::combat_hud::crop(frame, bounds) else { return };
            let Some(mut image) = images.get_mut(&self.image) else {
                return;
            };
            image.texture_descriptor.size = extent;
            image.data = Some(rgba);
        }
        self.notice_extent = Some((bounds.width, bounds.height));
        // Equal pixels still belong to this accepted frame/revision. Preserve
        // first-admission tracing and fallback/visibility transitions on reuse.
        if !self.presented && std::env::var_os("CSR_KILL_FEED_TRACE").is_some() {
            info!(revision = self.revision, notices = rendered_count, width = frame.width,
                height = frame.height, keys = ?self.entries, "Classic kill feed raster admitted");
        }
        self.presented = true;
    }
}

struct Bridge {
    ready: Arc<AtomicBool>,
    notice_failed: Arc<AtomicBool>,
    combat: Arc<crate::combat_hud::State>,
    combat_enabled: bool,
    notice_enabled: bool,
    rejected: Arc<Mutex<Vec<sim::death::DeathKey>>>,
    icons: std::collections::HashMap<String, Vec<u8>>,
}
impl Engine for Bridge {
    fn image(&mut self, url: &str) -> Option<Vec<u8>> {
        self.icons.get(url.strip_prefix("img://death-icon/")?).cloned()
    }

    fn call(
        &mut self,
        object: &str,
        method: &str,
        args: &[Value],
        calls: &mut Calls,
    ) -> Option<Value> {
        if object != "csr" {
            return None;
        }
        if self.combat.receive(method, args, calls) {
            return Some(Value::Undefined);
        }
        match method {
            // Share the Bevy HUD correction with the authored bottom modules.
            "CompactHudScale" => return Some(f64::from(crate::hud::COMPACT_SCALE).into()),
            "RootReady" => {
                if self.notice_enabled {
                    calls.call(API, "load", vec!["huddeathnotification.swf".into()]);
                }
                if self.combat_enabled {
                    calls.call("_global.csrCombatHud", "load", vec![]);
                }
            }
            "DeathNoticesReady" => {
                self.ready.store(true, Ordering::Release);
            }
            "DeathNoticesFailed" => {
                error!(
                    ?args,
                    "Classic kill feed could not load; preserving text fallback"
                );
                self.notice_failed.store(true, Ordering::Release);
            }
            "DeathNoticeResult" => {
                let (Some(Value::String(raw)), Some(Value::Bool(success))) =
                    (args.first(), args.get(1))
                else {
                    return None;
                };
                let Some(key) = parse_key(raw) else {
                    return None;
                };
                let mut rejected = self.rejected.lock().unwrap();
                rejected.retain(|old| *old != key);
                if !success {
                    // At most seven notices are live; retain a little room for
                    // callbacks from a just-expired presentation revision.
                    if rejected.len() == 16 {
                        rejected.remove(0);
                    }
                    rejected.push(key);
                    warn!(?key, "Classic kill feed receipt uses text fallback");
                }
            }
            _ => return None,
        }
        Some(Value::Undefined)
    }
}

fn key(key: sim::death::DeathKey) -> String {
    format!("{}:{}:{}", key.victim, key.life_epoch, key.tick)
}
fn parse_key(raw: &str) -> Option<sim::death::DeathKey> {
    let mut parts = raw.split(':');
    let key = sim::death::DeathKey {
        victim: parts.next()?.parse().ok()?,
        life_epoch: parts.next()?.parse().ok()?,
        tick: parts.next()?.parse().ok()?,
    };
    parts.next().is_none().then_some(key)
}
fn call(function: &str, args: Vec<Value>) -> Command {
    Command::Call {
        path: API.into(),
        function: function.into(),
        args,
    }
}
fn team_color(team: sim::Team) -> &'static str {
    match team {
        sim::Team::CounterTerrorist => "#6f9ce6",
        sim::Team::Terrorist => "#eabe54",
        _ => "#ffffff",
    }
}
fn weapon_linkage(entry: &KillFeedEntry) -> String {
    let weapon = entry.weapon.trim_start_matches("weapon_");
    let weapon = match weapon {
        // The installed public HUD exports name the two default knife art
        // variants explicitly; their gameplay class names omit this suffix.
        "knife" => "knife_default_ct",
        "knife_t" => "knife_default_t",
        "planted_c4" => "c4",
        _ => weapon,
    };
    let off = entry.suppressed == Some(false) && matches!(weapon, "m4a1_silencer" | "usp_silencer");
    format!("icon-{weapon}{}.png", if off { "_off" } else { "" })
}
fn add(entry: &KillFeedEntry, local: sim::PlayerId) -> Command {
    let attacker = entry.death.attacker.as_ref();
    call(
        "add",
        vec![
            key(entry.death.key).into(),
            attacker
                .map(|actor| actor.name.clone())
                .unwrap_or_default()
                .into(),
            entry.death.victim_name.clone().into(),
            attacker
                .map(|actor| team_color(actor.team))
                .unwrap_or("#ffffff")
                .into(),
            team_color(entry.death.victim_team).into(),
            weapon_linkage(entry).into(),
            entry.headshot.into(),
            attacker.is_some_and(|actor| actor.id == local).into(),
            (entry.death.key.victim == local).into(),
        ],
    )
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Presentation>()
        .add_systems(
            Update,
            update
                .after(crate::hud::update)
                .after(crate::frontend::update),
        )
        .add_systems(Update, crate::combat_hud::display.after(update));
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    mut feed: ResMut<Presentation>,
    data: Res<GameData>,
    state: Res<State<AppState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    effects: Option<Res<Effects>>,
    local: Option<Res<LocalPlayer>>,
    world: Option<Res<SimWorld>>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut surfaces: Query<(&mut Visibility, &mut Node), (With<Surface>, Without<Fallback>)>,
    mut fallback: Query<(&mut Text, &mut Node), With<Fallback>>,
) {
    let _frame_scope = crate::input_latency::frame_scope("classic_hud");
    for (_, mut node) in &mut fallback {
        node.top = crate::hud::unit(crate::hud_layout::NOTICE_TOP);
    }
    let notice_enabled = std::env::var_os("CSR_NO_CLASSIC_KILL_FEED").is_none();
    let matched = *state.get() == AppState::Match
        && local.is_some()
        && effects.is_some()
        && std::env::var_os("CSR_NO_HUD").is_none();
    let Ok(window) = windows.single() else { return };
    let size = (
        window.resolution.physical_width().max(1),
        window.resolution.physical_height().max(1),
    );
    let snapshot = matched
        .then(|| {
            world
                .as_ref()
                .and_then(|world| {
                    local
                        .as_ref()
                        .and_then(|local| world.0.player(local.view_target()))
                })
                .map(crate::combat_hud::Snapshot::from_player)
        })
        .flatten();
    if feed.movie.is_none()
        && matched
        && !feed.failed.load(Ordering::Acquire)
        && (notice_enabled || std::env::var_os("CSR_NO_CLASSIC_COMBAT_HUD").is_none())
    {
        feed.combat.spawn(&mut commands, &mut images);
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
        let node = commands
            .spawn((
                Persistent,
                Surface,
                ImageNode::new(image.clone()),
                Node {
                    position_type: PositionType::Absolute,
                    right: crate::hud::unit(12.0),
                    top: crate::hud::unit(crate::hud_layout::NOTICE_TOP),
                    width: px(0),
                    height: px(0),
                    ..default()
                },
                GlobalZIndex(90),
                Visibility::Hidden,
            ))
            .id();
        let movie = scaleform::Menu::spawn_named(
            Config {
                game_dir: data.game_dir.clone(),
                width: size.0,
                height: size.1,
                language: "english".into(),
                elements: vec![],
            },
            Box::new(Bridge {
                ready: feed.ready.clone(),
                notice_failed: feed.notice_failed.clone(),
                combat: feed.combat.state.clone(),
                combat_enabled: feed.combat.enabled,
                notice_enabled,
                rejected: feed.rejected.clone(),
                icons: crate::ui_assets::death_notice_icons(&data.vfs),
            }),
            "classic-hud".into(),
        );
        feed.movie = Some(Mutex::new(movie));
        feed.image = image;
        feed.node = Some(node);
        feed.viewport = size;
        feed.active = true;
        feed.started = Some(std::time::Instant::now());
    }
    if feed
        .started
        .is_some_and(|started| started.elapsed().as_secs() >= 30)
    {
        if notice_enabled
            && !feed.ready.load(Ordering::Acquire)
            && !feed.notice_failed.swap(true, Ordering::AcqRel)
        {
            warn!("Classic kill feed readiness timed out; preserving text fallback");
        }
        if feed.combat.enabled {
            feed.combat.state.timeout();
        }
    }
    if feed.failed.load(Ordering::Acquire) {
        // Disconnecting input terminates only our dedicated worker. A failed
        // movie must never erase the last usable text feed.
        feed.combat.hide();
        feed.movie = None;
        for (mut surface, _) in &mut surfaces {
            *surface = Visibility::Hidden;
        }
        return;
    }
    if feed.movie.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let entries: Vec<_> = effects
        .as_ref()
        .filter(|_| matched && notice_enabled)
        .map(|effects| effects.kill_feed.iter().collect())
        .unwrap_or_default();
    let keys: Vec<_> = entries.iter().map(|entry| entry.death.key).collect();
    feed.rejected
        .lock()
        .unwrap()
        .retain(|key| keys.contains(key));
    let ready = notice_enabled
        && feed.ready.load(Ordering::Acquire)
        && !feed.notice_failed.load(Ordering::Acquire);
    let next_revision = feed.revision.checked_add(1).expect("HUD revision limit");
    let resized = feed.viewport != size;
    let mut batch = vec![];
    if feed.viewport != size {
        feed.movie
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .send(Input::Resize {
                width: size.0,
                height: size.1,
            });
        feed.viewport = size;
        if ready {
            batch.push(call("resize", vec![]));
        }
    }
    if ready && keys != feed.entries {
        for old in feed.entries.iter().filter(|old| !keys.contains(old)) {
            batch.push(call("remove", vec![key(*old).into()]));
        }
        if let Some(local) = local {
            for entry in entries
                .iter()
                .filter(|entry| !feed.entries.contains(&entry.death.key))
            {
                batch.push(add(entry, local.id));
            }
        }
        feed.entries = keys;
    }
    batch.extend(
        feed.combat
            .commands(snapshot.as_ref(), next_revision, resized),
    );
    let has_commands = !batch.is_empty();
    if has_commands {
        feed.begin_present(resized);
        if std::env::var_os("CSR_KILL_FEED_TRACE").is_some() {
            info!(revision = feed.revision, notices = feed.entries.len(), keys = ?feed.entries,
                "Classic kill feed receipts updated");
        }
        feed.movie
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .send(Input::Present {
                commands: batch,
                revision: feed.revision,
            });
    }
    // Finish an in-flight bare-root load even if the match ends before it
    // announces readiness; otherwise a parked loader could time out on Home.
    let rejected_before = feed.rejected.lock().unwrap().clone();
    let has_drawable_row = entries
        .iter()
        .any(|entry| !rejected_before.contains(&entry.death.key));
    let notice_pending = notice_enabled
        && !feed.ready.load(Ordering::Acquire)
        && !feed.notice_failed.load(Ordering::Acquire);
    let active = has_commands
        || notice_pending
        || feed.combat.waiting()
        || (matched && ((!feed.presented && !feed.entries.is_empty()) || has_drawable_row));
    if active != feed.active {
        feed.movie
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .send(Input::SetActive(active));
        feed.active = active;
        if std::env::var_os("CSR_COMBAT_HUD_TRACE").is_some() {
            info!(
                active,
                revision = feed.revision,
                notices = feed.entries.len(),
                "Classic HUD worker activity"
            );
        }
    }
    if ready && matched && now - feed.last_alpha >= 1.0 / 30.0 {
        for entry in entries
            .iter()
            .filter(|entry| !rejected_before.contains(&entry.death.key))
        {
            let remaining = entry.lifetime() - (now - entry.time);
            feed.movie
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .send(Input::Call {
                    path: API.into(),
                    function: "alpha".into(),
                    args: vec![
                        key(entry.death.key).into(),
                        f64::from(remaining.clamp(0., 1.) * 100.).into(),
                    ],
                });
        }
        feed.last_alpha = now;
    }
    loop {
        let event = feed.movie.as_ref().unwrap().lock().unwrap().poll_event();
        match event {
            Some(scaleform::Event::Failed(error)) => {
                error!("Classic kill feed: {error}");
                feed.failed.store(true, Ordering::Release);
                break;
            }
            Some(_) => (),
            None => break,
        }
    }
    let frame = feed.movie.as_ref().unwrap().lock().unwrap().take_frame();
    // Snapshot after processing worker output: a failed row's keyed callback
    // must be visible before we replace its ordinary text with the raster.
    let rejected = feed.rejected.lock().unwrap().clone();
    let rendered_count = entries
        .iter()
        .filter(|entry| !rejected.contains(&entry.death.key))
        .count();
    if let Some(frame) = frame {
        feed.admit_frame(
            &frame,
            snapshot.as_ref(),
            size,
            ready,
            rendered_count,
            &mut images,
        );
    }
    feed.combat.visibility(snapshot.as_ref(), size);
    if feed.failed.load(Ordering::Acquire) {
        feed.combat.hide();
    }
    let visible = matched
        && ready
        && feed.presented
        && !feed.failed.load(Ordering::Acquire)
        && rendered_count > 0
        && feed.notice_extent.is_some();
    let (width, height) = feed.notice_extent.unwrap_or_default();
    let display_scale = crate::hud::COMPACT_SCALE / (window.scale_factor() * ui_scale.0);
    for (mut surface, mut node) in &mut surfaces {
        // Scale only the tight notice crop, canceling desktop/UI scaling. Its
        // visible right edge stays at the same height-relative inset at any
        // aspect ratio; changing name widths only extends the feed to the left.
        let width = px(width as f32 * display_scale);
        let height = px(height as f32 * display_scale);
        if node.width != width { node.width = width; }
        if node.height != height {
            node.height = height;
        }
        let value = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *surface != value {
            *surface = value;
        }
    }
    if matched && ready && feed.presented && !feed.failed.load(Ordering::Acquire) {
        let plain = entries
            .iter()
            .filter(|entry| rejected.contains(&entry.death.key))
            .map(|entry| {
                if entry.local_involved {
                    format!("> {}", entry.text)
                } else {
                    entry.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        for (mut text, mut node) in &mut fallback {
            text.0.clone_from(&plain);
            if rendered_count > 0 && !plain.is_empty() {
                // Exceptional plain rows follow the actual compact feed.
                let authored_unit = size.1 as f32 * scaleform::HUD_SCALING as f32 / 720.0;
                node.top = px(((crate::hud_layout::NOTICE_TOP + 12.0) * authored_unit
                    + height as f32) * display_scale);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::{AssetApp, AssetEvent, AssetPlugin};

    fn notice_fixture() -> (App, Presentation, scaleform::Frame) {
        // Real asset notification systems, with no renderer, window or worker.
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                watch_for_changes_override: Some(false),
                ..default()
            },
        ))
        .init_asset::<Image>();
        let image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let feed = Presentation {
            image,
            revision: 8,
            ..default()
        };
        let frame = scaleform::Frame {
            width: 3,
            height: 5,
            rgba: (0..60).collect(),
            serial: 1,
            presentation_revision: 8,
            raster_fence: 0,
        };
        (app, feed, frame)
    }

    fn modified_count(app: &mut App, image: &Handle<Image>) -> usize {
        app.update();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<Image>>>()
            .drain()
            .filter(|event| event.is_modified(image.id()))
            .count()
    }

    fn admit_notice(
        app: &mut App,
        feed: &mut Presentation,
        frame: &scaleform::Frame,
        size: (u32, u32),
        ready: bool,
    ) -> usize {
        feed.admit_frame(
            frame,
            None,
            size,
            ready,
            1,
            &mut app.world_mut().resource_mut::<Assets<Image>>(),
        );
        modified_count(app, &feed.image)
    }

    #[test]
    fn pending_receipts_keep_the_admitted_feed_until_replacement_or_resize() {
        let (mut app, mut feed, frame) = notice_fixture();
        admit_notice(&mut app, &mut feed, &frame, (3, 5), true);
        let extent = feed.notice_extent;
        feed.begin_present(false);
        assert!(feed.presented);
        assert_eq!(feed.notice_extent, extent);
        // An old worker result cannot replace the retained raster.
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 0);
        assert!(feed.presented);
        let mut replacement = frame;
        replacement.presentation_revision = feed.revision;
        replacement.rgba[0] ^= 1;
        assert_eq!(admit_notice(&mut app, &mut feed, &replacement, (3, 5), true), 1);
        feed.begin_present(true);
        assert!(!feed.presented);
        assert_eq!(feed.notice_extent, None);
    }

    #[test]
    fn fresh_notice_frames_only_notify_bevy_when_top_pixels_or_layout_change() {
        let (mut app, mut feed, mut frame) = notice_fixture();
        assert_eq!(modified_count(&mut app, &feed.image), 0);
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 1);
        assert!(feed.presented);
        let crop = crate::combat_hud::crop(&frame, crate::combat_hud::regions(3, 5)[0]).unwrap();
        let images = app.world().resource::<Assets<Image>>();
        let image = images.get(&feed.image).unwrap();
        assert_eq!(image.data.as_deref(), Some(crop.as_slice()));
        assert_eq!(image.texture_descriptor.size.width, 3);
        assert_eq!(image.texture_descriptor.size.height, 3);
        let allocation = image.data.as_ref().unwrap().as_ptr();

        // A new revision still needs admission even when all current bytes match.
        feed.revision += 1;
        feed.presented = false;
        frame.presentation_revision = feed.revision;
        frame.serial += 1;
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 0);
        assert!(feed.presented);
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&feed.image)
                .unwrap()
                .data
                .as_ref()
                .unwrap()
                .as_ptr(),
            allocation,
            "reuse must retain the existing owned pixels"
        );

        frame.rgba[59] ^= 1; // Bottom HUD pixels do not belong to this texture.
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 0);
        frame.rgba[35] ^= 1; // Even one alpha bit in the final top pixel matters.
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 1);
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&feed.image)
                .unwrap()
                .data
                .as_ref()
                .unwrap()[35],
            frame.rgba[35]
        );

        // Same bytes and byte length, different physical shape: must upload.
        frame.width = 9;
        frame.height = 2;
        frame.rgba.resize(72, 0);
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (9, 2), true), 1);
        let image = app
            .world()
            .resource::<Assets<Image>>()
            .get(&feed.image)
            .unwrap();
        assert_eq!(image.texture_descriptor.size.width, 9);
        assert_eq!(image.texture_descriptor.size.height, 1);

        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(&feed.image)
            .unwrap()
            .data = None;
        assert_eq!(modified_count(&mut app, &feed.image), 1);
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (9, 2), true), 1);

        // Equal bytes cannot conceal a different texture layout either.
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(&feed.image)
            .unwrap()
            .texture_descriptor
            .size
            .depth_or_array_layers = 2;
        assert_eq!(modified_count(&mut app, &feed.image), 1);
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (9, 2), true), 1);
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&feed.image)
                .unwrap()
                .texture_descriptor
                .size
                .depth_or_array_layers,
            1
        );
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (9, 2), true), 0);
    }

    #[test]
    fn notice_crop_discards_movie_margins_and_never_includes_bottom_hud() {
        let (mut app, mut feed, mut frame) = notice_fixture();
        frame.width = 6;
        frame.height = 6;
        frame.rgba = vec![0; 6 * 6 * 4];
        for (x, y) in [(3, 1), (4, 2), (0, 5)] {
            let start = (y * 6 + x) * 4;
            frame.rgba[start..start + 4].copy_from_slice(&[12, 34, 56, 255]);
        }
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (6, 6), true), 1);
        assert_eq!(feed.notice_extent, Some((2, 2)));
        let expected = crate::combat_hud::crop(&frame, crate::combat_hud::Rect {
            x: 3, y: 1, width: 2, height: 2,
        }).unwrap();
        assert_eq!(app.world().resource::<Assets<Image>>().get(&feed.image)
            .unwrap().data.as_deref(), Some(expected.as_slice()));

        // Moving the same row within a transparent movie margin must not
        // upload new pixels or change the compact surface's size/right anchor.
        frame.rgba.fill(0);
        for (x, y) in [(1, 0), (2, 1)] {
            let start = (y * 6 + x) * 4;
            frame.rgba[start..start + 4].copy_from_slice(&[12, 34, 56, 255]);
        }
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (6, 6), true), 0);
        assert_eq!(feed.notice_extent, Some((2, 2)));
        frame.rgba.fill(0);
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (6, 6), true), 0);
        assert_eq!(feed.notice_extent, None, "blank frames cannot show stale notices");
    }

    #[test]
    fn equal_notice_pixels_cannot_bypass_frame_acceptance_or_readiness() {
        let (mut app, mut feed, mut frame) = notice_fixture();
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 1);
        feed.presented = false;
        feed.revision = 9;
        for revision in [8, 10] {
            frame.presentation_revision = revision;
            assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 0);
            assert!(!feed.presented);
        }
        frame.presentation_revision = feed.revision;
        // Heights 5 and 6 would have the same top crop, but are not the same viewport.
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 6), true), 0);
        assert!(!feed.presented);
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), false), 0);
        assert!(!feed.presented);
        // A valid top prefix never legitimizes a malformed full readback.
        frame.rgba.pop();
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 0);
        assert!(!feed.presented);
        frame.rgba.push(59);
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 0);
        assert!(feed.presented, "fresh equal pixels must finish admission");

        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .remove(&feed.image);
        feed.presented = false;
        assert_eq!(admit_notice(&mut app, &mut feed, &frame, (3, 5), true), 0);
        assert!(!feed.presented, "a missing asset cannot be presented");
        // Keep the existing no-drawable-row completion path, even without an image.
        feed.admit_frame(
            &frame,
            None,
            (3, 5),
            true,
            0,
            &mut app.world_mut().resource_mut::<Assets<Image>>(),
        );
        assert!(feed.presented);
        assert_eq!(modified_count(&mut app, &feed.image), 0);
    }

    #[test]
    fn rejected_content_is_keyed_bounded_and_does_not_disable_other_notices() {
        let mut bridge = Bridge {
            ready: Arc::new(AtomicBool::new(true)),
            notice_failed: Arc::new(AtomicBool::new(false)),
            combat: Arc::default(),
            combat_enabled: true,
            notice_enabled: true,
            rejected: Arc::default(),
            icons: Default::default(),
        };
        let mut calls = Calls::default();
        let rejected = sim::death::DeathKey {
            victim: 2,
            life_epoch: 4,
            tick: 90,
        };
        let healthy = sim::death::DeathKey {
            victim: 3,
            ..rejected
        };
        bridge.call(
            "csr",
            "DeathNoticeResult",
            &[key(rejected).into(), false.into()],
            &mut calls,
        );
        assert_eq!(*bridge.rejected.lock().unwrap(), [rejected]);
        assert!(!bridge.rejected.lock().unwrap().contains(&healthy));
        assert!(!bridge.notice_failed.load(Ordering::Acquire));
        assert!(bridge.ready.load(Ordering::Acquire));
        for invalid in ["", "2:4", "2:4:90:extra", "invalid:4:90", "2:-4:90"] {
            bridge.call(
                "csr",
                "DeathNoticeResult",
                &[invalid.into(), false.into()],
                &mut calls,
            );
        }
        assert_eq!(*bridge.rejected.lock().unwrap(), [rejected]);
        bridge.call(
            "csr",
            "DeathNoticeResult",
            &[key(rejected).into(), true.into()],
            &mut calls,
        );
        assert!(bridge.rejected.lock().unwrap().is_empty());
        for tick in 0..40 {
            let receipt = sim::death::DeathKey { tick, ..rejected };
            bridge.call(
                "csr",
                "DeathNoticeResult",
                &[key(receipt).into(), false.into()],
                &mut calls,
            );
        }
        assert_eq!(bridge.rejected.lock().unwrap().len(), 16);
        assert!(!bridge.notice_failed.load(Ordering::Acquire));
        bridge.call(
            "csr",
            "DeathNoticesFailed",
            &["missing movie".into()],
            &mut calls,
        );
        assert!(bridge.notice_failed.load(Ordering::Acquire));
    }
}
