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
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
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
    viewport: (u32, u32),
    entries: Vec<sim::death::DeathKey>,
    revision: u64,
    presented: bool,
    active: bool,
    last_alpha: f32,
    pub(crate) combat: crate::combat_hud::Presentation,
}

struct Bridge {
    ready: Arc<AtomicBool>,
    notice_failed: Arc<AtomicBool>,
    combat: Arc<crate::combat_hud::State>,
    combat_enabled: bool,
    notice_enabled: bool,
    rejected: Arc<Mutex<Vec<sim::death::DeathKey>>>,
}
impl Engine for Bridge {
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
        if self.combat.receive(method, args) {
            return Some(Value::Undefined);
        }
        match method {
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
    effects: Option<Res<Effects>>,
    local: Option<Res<LocalPlayer>>,
    world: Option<Res<SimWorld>>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut surfaces: Query<(&mut Visibility, &mut Node), (With<Surface>, Without<Fallback>)>,
    mut fallback: Query<(&mut Text, &mut Node), With<Fallback>>,
) {
    for (_, mut node) in &mut fallback {
        node.top = px(60.0);
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
    let surface_height = px(size.1.div_ceil(2) as f32 / window.scale_factor());
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
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: surface_height,
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
    let notice_changed = !batch.is_empty();
    batch.extend(
        feed.combat
            .commands(snapshot.as_ref(), next_revision, resized),
    );
    let has_commands = !batch.is_empty();
    if has_commands {
        feed.revision = feed
            .revision
            .checked_add(1)
            .expect("kill-feed revision limit");
        if notice_changed {
            feed.presented = false;
        }
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
    if let Some(frame) = frame.filter(|frame| {
        frame.presentation_revision == feed.revision && (frame.width, frame.height) == size
    }) {
        feed.combat.admit(&frame, snapshot.as_ref(), &mut images);
        if ready && rendered_count == 0 {
            // No visible notice pixels changed with a health/ammo update.
            // Retain the top texture and avoid copying half the viewport.
            feed.presented = true;
        } else if ready
            && let Some(rgba) = crate::combat_hud::crop(
                &frame,
                crate::combat_hud::regions(frame.width, frame.height)[0],
            )
            && let Some(mut image) = images.get_mut(&feed.image)
        {
            if !feed.presented && std::env::var_os("CSR_KILL_FEED_TRACE").is_some() {
                info!(revision = feed.revision, notices = rendered_count, width = frame.width,
                    height = frame.height, keys = ?feed.entries, "Classic kill feed raster admitted");
            }
            image.texture_descriptor.size = Extent3d {
                width: size.0,
                height: size.1.div_ceil(2),
                depth_or_array_layers: 1,
            };
            image.data = Some(rgba);
            feed.presented = true;
        }
    }
    feed.combat.visibility(snapshot.as_ref(), size);
    if feed.failed.load(Ordering::Acquire) {
        feed.combat.hide();
    }
    let visible = matched
        && ready
        && feed.presented
        && !feed.failed.load(Ordering::Acquire)
        && rendered_count > 0;
    for (mut surface, mut node) in &mut surfaces {
        // An odd physical height must not stretch a ceil(height/2) raster into
        // a fractional-height box. Preserve one authored pixel per screen pixel.
        if node.height != surface_height {
            node.height = surface_height;
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
                // Exceptional plain rows sit below the cropped authored HUD
                // rather than covering a healthy notice at the same anchor.
                node.top = px((size.1.div_ceil(2) + 16) as f32 / window.scale_factor());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_content_is_keyed_bounded_and_does_not_disable_other_notices() {
        let mut bridge = Bridge {
            ready: Arc::new(AtomicBool::new(true)),
            notice_failed: Arc::new(AtomicBool::new(false)),
            combat: Arc::default(),
            combat_enabled: true,
            notice_enabled: true,
            rejected: Arc::default(),
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
