//! HUD: crosshair, health/armour, money, ammo and inventory, the round
//! timer and score, bomb status, plant/defuse progress, kill feed, round
//! banners, spectating, hit marker and a debug line.

use crate::effects::Effects;
use crate::localize::Localization;
use crate::player::{LocalPlayer, SimWorld};
use crate::hud_layout as layout;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::text::LineBreak;
use sim::{Phase, Team};

// Owner's compact-HUD correction. Keep art, text, marker offsets and their
// corner insets in one space; crosshair settings remain independent.
pub(crate) const COMPACT_SCALE: f32 = 0.8;
pub(crate) const fn unit(value: f32) -> Val { layout::unit(value * COMPACT_SCALE) }
pub(crate) const fn font(value: f32) -> FontSize { layout::font(value * COMPACT_SCALE) }
pub(crate) const fn legacy(value: f32) -> Val { layout::legacy(value * COMPACT_SCALE) }
use self::{font as hf, legacy as hp, unit as hu};

/// Which piece of HUD text a node shows.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum HudText {
    Speed,
    Debug,
    Health,
    Armor,
    Money,
    Ammo,
    Inventory,
    Feed,
    Center,
    Hint,
    ScoreCt,
    Timer,
    ScoreT,
    Phase,
    Progress,
}

#[derive(Component)]
pub struct HitMarker;

#[derive(Component)]
pub struct Crosshair(pub u8);

#[derive(Component)]
pub struct ProgressBar;

#[derive(Component)]
pub struct TimerPlate;

#[derive(Component)]
pub struct ProgressFill;

const HUD_YELLOW: Color = Color::srgb(0.88, 0.89, 0.87);
const MONEY_GREEN: Color = Color::srgb(0.88, 0.89, 0.87);
const CT_BLUE: Color = Color::srgb(0.45, 0.65, 1.0);
const T_GOLD: Color = Color::srgb(0.95, 0.75, 0.3);
const BOMB_RED: Color = Color::srgb(1.0, 0.25, 0.2);

type BurstIdentity = (sim::PlayerId, u64, Team, usize, content::ContentId);
type BurstSample<'a> = ((sim::PlayerId, u64, Team, usize, &'a content::ContentId), bool, bool);

/// Reuses the existing center notification's approximate placement and lifetime;
/// neither is measured native burst-message parity. No authored toggle clip is known.
#[derive(Default)]
pub struct BurstNotice {
    baseline: Option<(BurstIdentity, bool)>,
    message: Option<(&'static str, f32)>,
    sim_time: f32,
}

impl BurstNotice {
    fn observe(&mut self, me: Option<&sim::Player>, hidden: bool, now: f32, sim_time: f32) {
        let sample = me.filter(|p| p.alive && !hidden).and_then(|p| {
            let weapon = p.active_weapon()?;
            weapon.burst_profile()?;
            Some(((p.id, p.life_epoch, p.team, p.active, &weapon.def.id), weapon.burst_mode, weapon.def.full_auto))
        });
        self.observe_mode(sample, now, sim_time);
    }

    fn observe_mode(
        &mut self,
        sample: Option<BurstSample<'_>>,
        now: f32,
        sim_time: f32,
    ) {
        if sim_time < self.sim_time { self.baseline = None; self.message = None; }
        self.sim_time = sim_time;
        // Expire while a round banner is visible, rather than queueing behind it.
        if self.message.is_some_and(|(_, started)| now - started >= 7.0) { self.message = None; }
        let Some(((id, life, team, slot, weapon), mode, full_auto)) = sample else {
            self.baseline = None;
            self.message = None;
            return;
        };
        let same_equip = self.baseline.as_ref().is_some_and(|((old_id, old_life, old_team, old_slot, old_weapon), _)| {
            (*old_id, *old_life, *old_team, *old_slot, old_weapon) == (id, life, team, slot, weapon)
        });
        if same_equip {
            let (_, previous) = self.baseline.as_mut().unwrap();
            if *previous != mode {
                let token = if mode { "Cstrike_TitlesTXT_Switch_To_BurstFire" }
                    else if full_auto { "Cstrike_TitlesTXT_Switch_To_FullAuto" }
                    else { "Cstrike_TitlesTXT_Switch_To_SemiAuto" };
                self.message = Some((token, now));
                *previous = mode;
            }
        } else {
            // Equip/respawn establishes a quiet baseline. The protocol has no
            // instance ID for an unobserved same-class, same-slot replacement.
            self.baseline = Some(((id, life, team, slot, weapon.clone()), mode));
            self.message = None;
        }
    }
}

fn text(commands: &mut Commands, kind: HudText, size: f32, color: Color, node: Node) -> Entity {
    let entity = commands
        .spawn((
            Text::new(""),
            TextFont {
                font_size: layout::legacy_font(size * COMPACT_SCALE),
                ..default()
            },
            TextColor(color),
            node,
            kind,
        ))
        .id();
    match kind {
        HudText::Health | HudText::Armor => { commands.entity(entity).insert(crate::combat_hud::Fallback::Health); }
        HudText::Ammo => { commands.entity(entity).insert(crate::combat_hud::Fallback::Ammo); }
        HudText::Money => { commands.entity(entity).insert(crate::hud_art::RadarLayout::Money); }
        _ => (),
    }
    entity
}

fn centered_row(top: Val, bottom: Val) -> Node {
    Node {
        position_type: PositionType::Absolute,
        top,
        bottom,
        width: percent(100),
        justify_content: JustifyContent::Center,
        ..default()
    }
}

pub fn setup(mut commands: Commands) {
    // Skip the entire tree, including opaque/background-only nodes. Skipping
    // update alone leaves the timer plate painted over HUD-free map captures.
    if std::env::var_os("CSR_NO_HUD").is_some() { return; }
    // Crosshair: four green bars with a gap, CS:GO "classic static" style,
    // plus a hit marker (hidden until a hit).
    let color = Color::srgb(0.2, 1.0, 0.2);
    let (len, thick, gap) = (8.0, 2.0, 4.0);
    let bars = [
        (-(gap + len), -thick / 2.0, len, thick),
        (gap, -thick / 2.0, len, thick),
        (-thick / 2.0, -(gap + len), thick, len),
        (-thick / 2.0, gap, thick, len),
        (-thick / 2.0, -thick / 2.0, thick, thick),
    ];
    commands
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|root| {
            root.spawn(Node {
                width: px(0.0),
                height: px(0.0),
                ..default()
            })
            .with_children(|c| {
                for (i, (x, y, w, h)) in bars.into_iter().enumerate() {
                    c.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(x),
                            top: px(y),
                            width: px(w),
                            height: px(h),
                            ..default()
                        },
                        BackgroundColor(color),
                        Crosshair(i as u8),
                    ));
                }
                for (x, y) in [(-12.0, -12.0), (7.0, -12.0), (-12.0, 7.0), (7.0, 7.0)] {
                    c.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(x),
                            top: px(y),
                            width: px(5.0),
                            height: px(5.0),
                            ..default()
                        },
                        BackgroundColor(Color::WHITE),
                        Visibility::Hidden,
                        HitMarker,
                    ));
                }
            });
        });

    let speed = text(&mut commands, HudText::Speed, 20.0, Color::srgba(1.0, 1.0, 1.0, 0.8), centered_row(Val::Auto, hp(90.0)));
    commands.entity(speed).insert(TextLayout::justify(Justify::Center));
    text(
        &mut commands,
        HudText::Debug,
        13.0,
        Color::srgba(1.0, 1.0, 1.0, 0.75),
        Node {
            position_type: PositionType::Absolute,
            top: hp(6.0),
            left: hp(8.0),
            ..default()
        },
    );
    text(
        &mut commands,
        HudText::Health,
        34.0,
        HUD_YELLOW,
        Node {
            position_type: PositionType::Absolute,
            bottom: hp(16.0),
            left: hp(66.0),
            ..default()
        },
    );
    text(&mut commands, HudText::Armor, 34.0, HUD_YELLOW, Node { position_type: PositionType::Absolute, bottom: hp(16.0), left: hp(217.0), ..default() });
    text(
        &mut commands,
        HudText::Money,
        28.0,
        MONEY_GREEN,
        Node {
            position_type: PositionType::Absolute,
            top: hu(layout::RADAR_TOP + layout::RADAR_DIAMETER + 32.0),
            left: hu(60.0),
            ..default()
        },
    );
    let ammo = text(
        &mut commands,
        HudText::Ammo,
        34.0,
        HUD_YELLOW,
        Node {
            position_type: PositionType::Absolute,
            bottom: hp(16.0),
            right: hp(24.0),
            ..default()
        },
    );
    commands.entity(ammo).insert(TextLayout::justify(Justify::Right));
    let inv = text(
        &mut commands,
        HudText::Inventory,
        17.0,
        Color::srgba(1.0, 1.0, 1.0, 0.8),
        Node {
            position_type: PositionType::Absolute,
            bottom: hp(110.0),
            right: hp(24.0),
            ..default()
        },
    );
    commands.entity(inv).insert(TextLayout::justify(Justify::Right));
    let feed = text(
        &mut commands,
        HudText::Feed,
        16.0,
        Color::WHITE,
        Node {
            position_type: PositionType::Absolute,
            top: hu(layout::NOTICE_TOP),
            right: hu(12.0),
            // Intrinsic-width fallback stays against the same right edge even
            // when NoWrap text layout does not expand its justification box.
            width: Val::Auto,
            ..default()
        },
    );
    commands.entity(feed).insert((TextLayout::new(Justify::Right, LineBreak::NoWrap), crate::kill_feed::Fallback));
    let center = text(&mut commands, HudText::Center, 32.0, Color::WHITE, centered_row(percent(24), Val::Auto));
    commands.entity(center).insert(TextLayout::justify(Justify::Center));
    let hint = text(&mut commands, HudText::Hint, 20.0, Color::WHITE, centered_row(Val::Auto, hp(150.0)));
    commands.entity(hint).insert(TextLayout::justify(Justify::Center));

    // Classic top centre: timer ABOVE the two scores, inside the same strip
    // used to anchor both teams. Reference: 2015 tutorial nominal254s.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: hu(0.0),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|top| {
            top.spawn((
                Node {
                    width: hu(layout::CENTER_WIDTH),
                    height: hu(layout::PLAYER_HEIGHT),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                TimerPlate,
            ))
            .with_children(|row| {
                row.spawn((Text::new(""), TextFont { font_size: hf(20.0), ..default() }, TextColor(Color::WHITE), HudText::Timer));
                row.spawn(Node { width: percent(100), justify_content: JustifyContent::SpaceAround, ..default() }).with_children(|scores| {
                    for (kind, color) in [(HudText::ScoreCt, CT_BLUE), (HudText::ScoreT, T_GOLD)] {
                        scores.spawn((Text::new(""), TextFont { font_size: hf(14.0), ..default() }, TextColor(color), kind));
                    }
                });
            });
            top.spawn((
                Text::new(""),
                TextFont {
                    font_size: hf(10.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
                HudText::Phase,
            ));
        });

    // Plant / defuse progress bar.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: percent(62),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: hp(4.0),
                ..default()
            },
            Visibility::Hidden,
            ProgressBar,
        ))
        .with_children(|bar| {
            bar.spawn((
                Text::new(""),
                TextFont {
                    font_size: hf(12.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                HudText::Progress,
            ));
            bar.spawn((
                Node {
                    width: hp(300.0),
                    height: hp(10.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ))
            .with_children(|track| {
                track.spawn((
                    Node {
                        width: percent(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.95, 0.8, 0.3)),
                    ProgressFill,
                ));
            });
        });
}

fn clock(seconds: f32) -> String {
    let s = seconds.ceil().max(0.0) as i32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update(
    local: Res<LocalPlayer>,
    world: Res<SimWorld>,
    effects: Res<Effects>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    level_name: Res<crate::LevelInfo>,
    loc: Res<Localization>,
    mut burst_notice: Local<BurstNotice>,
    mut texts: Query<(&mut Text, &mut TextColor, &HudText)>,
    mut markers: Query<&mut Visibility, (With<HitMarker>, Without<ProgressBar>)>,
    mut bar: Query<&mut Visibility, (With<ProgressBar>, Without<HitMarker>)>,
    mut fill: Query<&mut Node, With<ProgressFill>>,
    mut timer_plates: Query<&mut BackgroundColor, With<TimerPlate>>,
) {
    let _frame_scope = crate::input_latency::frame_scope("hud");
    static DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let debug = *DEBUG.get_or_init(|| std::env::var("CSR_HUD_DEBUG").is_ok_and(|value| value == "1"));
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed()).unwrap_or(0.0);
    let w = &world.0;
    let now = time.elapsed_secs();
    let sim_now = w.time();
    let me = w.player(local.id);
    burst_notice.observe(me, local.noclip || local.spectating.is_some(), now, sim_now);
    let Some(me) = me else { return };
    // The player whose HUD we show (a spectated teammate's while dead).
    let p = local.spectating.and_then(|s| w.player(s)).unwrap_or(me);
    let competitive = w.competitive();
    // Deathmatch has no score/timer text in this strip. Its otherwise empty
    // plate looked like a square painted into the sky on every map.
    for mut plate in &mut timer_plates {
        let color = if competitive { Color::srgba(0.0, 0.0, 0.0, 0.45) } else { Color::NONE };
        if plate.0 != color { plate.0 = color; }
    }

    // Plant / defuse progress for the viewed player.
    let progress: Option<(String, f32)> = if let Some(start) = p.plant_start {
        Some(("Planting the bomb".into(), (sim_now - start) / w.rules.plant_time))
    } else {
        w.bomb.and_then(|b| b.defuser).filter(|(d, _)| *d == p.id).map(|(_, done)| {
            let total = w.rules.defuse_duration(p.defuse_kit);
            (if p.defuse_kit { "Defusing the bomb (kit)".into() } else { "Defusing the bomb".into() }, 1.0 - (done - sim_now) / total)
        })
    };
    for mut v in &mut bar {
        *v = if progress.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    if let Some((_, k)) = &progress {
        for mut n in &mut fill {
            n.width = percent((k.clamp(0.0, 1.0) * 100.0) as i32);
        }
    }

    let round = &w.round;

    for (mut t, mut color, kind) in &mut texts {
        let value: String = match kind {
            HudText::Speed if !debug => String::new(),
            HudText::Debug if !debug && settings.show_fps => format!("{fps:.0} fps | {frame_ms:.1} ms"),
            HudText::Debug if !debug => String::new(),
            HudText::Speed => format!("{:.0}", p.movement.velocity.truncate().length()),
            HudText::Debug => format!(
                "{} | {:.0} fps\npos {:.0} {:.0} {:.0}\nK {} D {}{}",
                level_name.name,
                fps,
                local.curr_eye.x,
                local.curr_eye.y,
                local.curr_eye.z,
                me.kills,
                me.deaths,
                if local.noclip { " | NOCLIP" } else { "" },
            ),
            HudText::Health => if p.alive { p.health.to_string() } else { String::new() },
            HudText::Armor => if p.alive { p.armor.to_string() } else { String::new() },
            HudText::Money => if competitive || w.mode == sim::GameMode::Special { format!("${}", p.money) } else { String::new() },
            HudText::Ammo => match p.active_weapon() {
                Some(wp) if p.alive && wp.uses_ammo() => format!("{} / {}", wp.clip, wp.reserve),
                _ => String::new(),
            },
            HudText::Inventory => String::new(),
            HudText::Feed => effects
                .kill_feed
                .iter()
                .map(|k| if k.local_involved { format!("> {}", k.text) } else { k.text.clone() })
                .collect::<Vec<_>>()
                .join("\n"),
            HudText::Center => {
                let banner = effects.banner.as_ref().filter(|b| now - b.2 < 7.0);
                if let Some((text, c, _)) = banner {
                    color.0 = *c;
                    text.clone()
                } else if !me.alive && !local.noclip && !competitive {
                    color.0 = Color::srgb(1.0, 0.35, 0.3);
                    format!("YOU DIED - respawning in {:.1}s", (me.respawn_at - sim_now).max(0.0))
                } else if let Some((token, _)) = burst_notice.message {
                    color.0 = Color::WHITE;
                    // Missing installed localization suppresses the notice;
                    // do not synthesize or translate the game's wording.
                    loc.get(token).unwrap_or_default().to_owned()
                } else {
                    String::new()
                }
            }
            HudText::Hint => {
                if let Some(state) = me.mechanic.as_ref().filter(|_| me.alive) {
                    let mode = w.mechanics.iter().find(|m| m.id == state.id).map_or("Movement pack", |m| m.name.as_str());
                    if let Some(native)=&state.external{
                        let key=settings.action_binding_label(crate::settings::KEY_MOVEMENT_MODE);
                        let switch=if key.is_empty(){"View".into()}else{format!("View / {key}")};
                        format!("{mode}   {switch}: CS/skate   A/X: push   B: brake   Left stick: steer   Right stick: tricks   LT/RT: grab   Y: on/off board\n{}  {}   D-pad right: fire   left: reload   up/down: weapons   Start: pause",native.pose.trick,native.pose.score)
                    }else{format!("{mode}   {}: movement mode   {}: push   {} / {}: steer   {}: jump   {}: brake",
                        settings.action_binding_label(crate::settings::KEY_MOVEMENT_MODE),
                        settings.action_binding_label(0), settings.action_binding_label(2), settings.action_binding_label(3),
                        settings.action_binding_label(4), settings.action_binding_label(6))}
                } else if !me.alive && !local.noclip {
                    match local.spectating.and_then(|s| w.player(s)) {
                        Some(s) => format!("Spectating {}   (LMB: next player)", s.name),
                        None if competitive => "Waiting for the next round".into(),
                        None => String::new(),
                    }
                } else if !w.mechanics.is_empty() && me.alive && !competitive {
                    let key=settings.action_binding_label(crate::settings::KEY_MOVEMENT_MODE);
                    if key.is_empty(){"View: switch movement mode (set a keyboard binding in Settings)".into()}else{format!("View / {key}: switch movement mode")}
                } else if competitive && round.phase == Phase::Live && me.alive {
                    let (mn, mx) = me.bounds();
                    let in_site = w.objectives.site_at(mn, mx);
                    if me.has_bomb() && in_site.is_some() && me.plant_start.is_none() {
                        "In the bomb site: hold LMB with the C4 (5) to plant".into()
                    } else if me.team == Team::CounterTerrorist
                        && w.bomb.is_some_and(|b| b.defuser.is_none() && (b.pos - me.movement.origin).truncate().length() < sim::DEFUSE_RANGE)
                    {
                        "Look at the bomb and hold E to defuse".into()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            }
            HudText::ScoreCt => {
                if competitive { round.score(Team::CounterTerrorist).to_string() } else { String::new() }
            }
            HudText::ScoreT => {
                if competitive { round.score(Team::Terrorist).to_string() } else { String::new() }
            }
            HudText::Timer => {
                if !competitive {
                    String::new()
                } else if w.bomb.is_some() && round.phase == Phase::Live {
                    // The game hides the bomb timer; show a pulsing icon.
                    color.0 = if (now * 2.0).fract() < 0.5 { BOMB_RED } else { Color::srgb(0.6, 0.1, 0.1) };
                    "C4".into()
                } else {
                    color.0 = if round.phase == Phase::Live && round.time_left(&w.rules, sim_now) <= 10.0 { BOMB_RED } else { Color::WHITE };
                    clock(round.time_left(&w.rules, sim_now))
                }
            }
            HudText::Phase => {
                if !competitive {
                    String::new()
                } else {
                    let r = round.round;
                    match round.phase {
                        Phase::Freeze => format!("Round {r} - freeze time"),
                        Phase::Live if w.bomb.is_some() => format!("Round {r} - BOMB PLANTED"),
                        Phase::Live => format!("Round {r}{}", if me.has_bomb() { " - you have the bomb" } else { "" }),
                        Phase::Over => format!("Round {r} over"),
                        Phase::MatchOver => "Match over - new match soon".into(),
                    }
                }
            }
            HudText::Progress => progress.as_ref().map(|p| p.0.clone()).unwrap_or_default(),
        };
        if t.0 != value {
            t.0 = value;
        }
    }

    let show = settings.hitmarker && effects.last_hit > 0.0 && now - effects.last_hit < 0.18;
    for mut v in &mut markers {
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// Hide gameplay UI during transparent menus without disturbing per-widget visibility.
#[derive(Component)]
pub struct GameHud;
pub fn mark_roots(mut commands:Commands,roots:Query<Entity,(With<Node>,Without<ChildOf>,Without<crate::session::Persistent>)>) {
    for e in &roots {commands.entity(e).insert(GameHud);}
}
pub fn menu_visibility(menu:Res<crate::menu::Menu>,remote:Option<Res<crate::remote::RemoteClient>>,mut roots:Query<&mut Node,With<GameHud>>) {
    let display=if menu.open() || crate::remote_join::loading(remote.as_deref()){Display::None}else{Display::Flex};
    for mut node in &mut roots {if node.display!=display {node.display=display;}}
}

#[cfg(test)]
mod burst_notice_tests {
    use super::*;

    #[test]
    fn mode_feedback_is_quiet_on_equip_and_life_changes() {
        let glock = content::ContentId::csgo(content::ContentKind::Weapon, "glock");
        let famas = content::ContentId::csgo(content::ContentKind::Weapon, "famas");
        let mut notice = BurstNotice::default();
        let sample = |weapon, life, mode, full_auto| Some(((1, life, Team::Terrorist, 0, weapon), mode, full_auto));
        notice.observe_mode(sample(&glock, 1, false, false), 0., 0.);
        assert!(notice.message.is_none());
        notice.observe_mode(sample(&glock, 1, true, false), 1., 1.);
        assert_eq!(notice.message.unwrap().0, "Cstrike_TitlesTXT_Switch_To_BurstFire");
        notice.observe_mode(sample(&glock, 1, false, false), 2., 2.);
        assert_eq!(notice.message.unwrap().0, "Cstrike_TitlesTXT_Switch_To_SemiAuto");
        notice.observe_mode(sample(&glock, 2, true, false), 3., 3.);
        assert!(notice.message.is_none(), "respawn is a baseline, not a toggle");
        notice.observe_mode(sample(&famas, 2, true, true), 4., 4.);
        assert!(notice.message.is_none(), "equipping a weapon is quiet");
        notice.observe_mode(sample(&famas, 2, false, true), 5., 5.);
        assert_eq!(notice.message.unwrap().0, "Cstrike_TitlesTXT_Switch_To_FullAuto");
    }

    #[test]
    fn hidden_notices_expire_and_ineligible_or_restarted_sessions_baseline_quietly() {
        let glock = content::ContentId::csgo(content::ContentKind::Weapon, "glock");
        let sample = |mode| Some(((1, 1, Team::Terrorist, 0, &glock), mode, false));
        let mut notice = BurstNotice::default();
        notice.observe_mode(sample(false), 0., 0.);
        notice.observe_mode(sample(true), 1., 1.);
        // No Center-node rendering is necessary for expiration behind a banner.
        notice.observe_mode(sample(true), 8., 8.);
        assert!(notice.message.is_none());
        notice.observe_mode(None, 9., 9.);
        notice.observe_mode(sample(false), 10., 10.);
        assert!(notice.message.is_none(), "holster/death/spectating resets the baseline");
        notice.observe_mode(sample(true), 11., 11.);
        assert!(notice.message.is_some());
        notice.observe_mode(sample(false), 12., 0.);
        assert!(notice.message.is_none(), "restarted world does not emit a stale transition");
    }
}
