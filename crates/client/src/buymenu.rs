//! Buy menu (B): every item the team may buy, laid out at once as a flat grid
//! of category columns (owner direction 2026-10-29), with one page per
//! content source. Weapons come from the item schema, pack tools (the Skate
//! board) from the installed mechanics. Number keys keep CS:GO's two-key
//! scheme (category, then item; 0 goes back) and a click buys a tile.
//!
//! The menu is presentation and input only: availability and prices come from
//! the simulation's own check ([`sim::World::buy_cost`]) and purchases travel
//! through the normal tick input. The UI is spawned once per layout (page,
//! content, window size) and afterwards only restyled when a tile changes.

use crate::audio::SoundLibrary;
use crate::localize::Localization;
use crate::player::{InputEnabled, LocalPlayer, SimWorld};
use crate::ui_assets::{OwnFont, UiAssets};
use crate::LoadedLevel;
use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy::text::FontSource;
use sim::{BuyError, Purchase, Team};
use weapons::WeaponClass;

/// CS:GO's buy categories in hotkey order (fallbacks for the localized titles).
pub const CATEGORIES: [&str; 6] = ["PISTOLS", "HEAVY", "SMGs", "RIFLES", "GEAR", "GRENADES"];
const CATEGORY_TOKENS: [&str; 6] = [
    "SFUI_BuyMenu_Pistols",
    "SFUI_BuyMenu_HeavyWeapons",
    "SFUI_BuyMenu_SMGs",
    "SFUI_BuyMenu_Rifles",
    "SFUI_BuyMenu_Equipment",
    "SFUI_BuyMenu_Grenades",
];

/// One purchasable thing as the menu shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub item: Purchase,
    pub name: String,
    /// UI art keys to try in order (see [`UiAssets`]).
    pub icons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub title: String,
    pub entries: Vec<Entry>,
}

/// One tab of the menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub title: String,
    pub columns: Vec<Column>,
    /// Optional one-line explanation shown under the grid.
    pub note: Option<String>,
}

/// A content source that offers purchasable items. Add a source here to give
/// it pages; each returned page becomes a tab after WEAPONS.
pub type Source = fn(&sim::World, &Localization, Team) -> Vec<Page>;
pub const SOURCES: &[Source] = &[weapons_page, pack_pages];

/// Every page the team sees, WEAPONS first. Empty pages are skipped.
pub fn pages(world: &sim::World, loc: &Localization, team: Team) -> Vec<Page> {
    let mut pages: Vec<Page> = SOURCES.iter().flat_map(|source| source(world, loc, team)).collect();
    pages.retain(|p| p.columns.iter().any(|c| !c.entries.is_empty()));
    if pages.is_empty() {
        pages.push(Page { title: "WEAPONS".into(), columns: Vec::new(), note: None });
    }
    pages
}

fn in_category(class: WeaponClass, cat: usize) -> bool {
    match cat {
        0 => class == WeaponClass::Pistol,
        1 => matches!(class, WeaponClass::Shotgun | WeaponClass::MachineGun),
        2 => class == WeaponClass::Smg,
        3 => matches!(class, WeaponClass::Rifle | WeaponClass::Sniper),
        5 => class == WeaponClass::Grenade,
        _ => false,
    }
}

fn token(loc: &Localization, token: &str, fallback: &str) -> String {
    loc.get(token).map_or_else(|| fallback.to_string(), str::to_string)
}

fn weapon_icons(def: &weapons::WeaponDefinition) -> Vec<String> {
    let name = def.class_name.trim_start_matches("weapon_");
    vec![format!("buy-{name}"), name.to_string()]
}

/// The items a team sees in a category, cheapest first.
pub fn entries(world: &sim::World, loc: &Localization, team: Team, cat: usize) -> Vec<Entry> {
    let side_ok = |side: weapons::Side| match side {
        weapons::Side::Both => true,
        weapons::Side::Terrorist => team == Team::Terrorist,
        weapons::Side::CounterTerrorist => team == Team::CounterTerrorist,
    };
    if cat == 4 {
        let gear = |item, tok: &str, fallback: &str, icons: &[&str]| Entry {
            item,
            name: token(loc, tok, fallback),
            icons: icons.iter().map(|s| s.to_string()).collect(),
        };
        let mut out = vec![
            gear(Purchase::Kevlar, "SFUI_WPNHUD_KEVLAR", "Kevlar Vest", &["buy-kevlar", "armor"]),
            gear(Purchase::KevlarHelmet, "SFUI_WPNHUD_ASSAULTSUIT", "Kevlar + Helmet", &["buy-assaultsuit", "helmet"]),
        ];
        if let Some(zeus) = world.catalog.get("weapon_taser").filter(|z| z.price > 0) {
            out.push(Entry { item: Purchase::Weapon(zeus.def_index), name: loc.weapon_name(zeus), icons: weapon_icons(zeus) });
        }
        if team == Team::CounterTerrorist {
            out.push(gear(Purchase::DefuseKit, "SFUI_WPNHUD_DEFUSER", "Defuse Kit", &["buy-defuser", "defuser"]));
        }
        return out;
    }
    let mut defs: Vec<&weapons::WeaponDefinition> = world
        .catalog
        .weapons
        .iter()
        .filter(|w| w.price > 0 && in_category(w.class, cat) && side_ok(w.side))
        .collect();
    defs.sort_by_key(|w| (w.price, w.def_index));
    defs.into_iter()
        .map(|w| Entry { item: Purchase::Weapon(w.def_index), name: loc.weapon_name(w), icons: weapon_icons(w) })
        .collect()
}

/// WEAPONS: CS:GO's six buy categories as columns, in hotkey order.
pub fn weapons_page(world: &sim::World, loc: &Localization, team: Team) -> Vec<Page> {
    let columns = (0..CATEGORIES.len())
        .map(|cat| Column { title: token(loc, CATEGORY_TOKENS[cat], CATEGORIES[cat]), entries: entries(world, loc, team, cat) })
        .collect();
    vec![Page { title: "WEAPONS".into(), columns, note: None }]
}

/// One page per installed content pack that provides inventory tools. The
/// tool is equipped through the authoritative [`Purchase::Mechanic`].
pub fn pack_pages(world: &sim::World, _loc: &Localization, _team: Team) -> Vec<Page> {
    let mut packs: Vec<(&str, Page)> = Vec::new();
    for module in &world.mechanics {
        let entry = Entry {
            item: Purchase::Mechanic(sim::rules::mechanic_key(&module.id)),
            // Same display name as the inventory HUD.
            name: if module.external.is_some() { "Skateboard".into() } else { module.name.clone() },
            icons: vec!["skateboard".into()],
        };
        match packs.iter_mut().find(|(hash, _)| *hash == module.archive_hash) {
            Some((_, page)) => page.columns[0].entries.push(entry),
            None => packs.push((&module.archive_hash, Page {
                title: module.name.to_uppercase(),
                columns: vec![Column { title: "GEAR".into(), entries: vec![entry] }],
                note: Some("Goes into your inventory beside your weapons: fire to ride, switch weapons to put it away.".into()),
            })),
        }
    }
    packs.into_iter().map(|(_, page)| page).collect()
}

/// Display price of an entry (what it costs at full price).
pub fn list_price(world: &sim::World, item: Purchase) -> i32 {
    match item {
        Purchase::Weapon(i) => world.catalog.weapons.iter().find(|w| w.def_index == i).map_or(0, |w| w.price),
        Purchase::Kevlar => sim::rules::KEVLAR_PRICE,
        Purchase::KevlarHelmet => sim::rules::KEVLAR_PRICE + sim::rules::HELMET_PRICE,
        Purchase::DefuseKit => sim::rules::DEFUSE_KIT_PRICE,
        Purchase::Mechanic(_) | Purchase::None => 0,
    }
}

/// How a tile reads right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Buy,
    Poor,
    Owned,
    Limit,
    Blocked,
}

/// The sim's verdict on an item, plus the price to show.
pub fn status(world: &sim::World, id: sim::PlayerId, item: Purchase) -> (Status, i32) {
    let Some(p) = world.player(id) else { return (Status::Blocked, list_price(world, item)) };
    // Kevlar + Helmet over existing armor costs only the missing part, as in CS:GO.
    let shown = match item {
        Purchase::KevlarHelmet if p.armor >= 100 => sim::rules::HELMET_PRICE,
        _ => list_price(world, item),
    };
    match world.buy_cost(id, item) {
        Ok(cost) => (Status::Buy, cost),
        Err(BuyError::NotEnoughMoney) => (Status::Poor, shown),
        Err(BuyError::AlreadyOwned) => {
            let limit = matches!(item, Purchase::Weapon(i) if world.catalog.weapons.iter()
                .find(|w| w.def_index == i).is_some_and(|w| w.class == WeaponClass::Grenade && !p.has(&w.class_name)));
            (if limit { Status::Limit } else { Status::Owned }, shown)
        }
        Err(_) => (Status::Blocked, shown),
    }
}

fn can_shop(world: &sim::World, id: sim::PlayerId) -> bool {
    world.player(id).is_some_and(|p| p.alive && (world.mode == sim::GameMode::Special || (world.competitive() && p.in_buy_zone && world.round.can_buy(&world.rules, world.time()))))
}

/// A grid tile: column and row on the current page.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyTile(pub usize, pub usize);
/// A page tab.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyTab(pub usize);

#[derive(Clone, PartialEq)]
struct Look {
    status: Status,
    price: String,
    hovered: bool,
    armed: bool,
}

struct TileUi {
    tile: Entity,
    icon: Option<Entity>,
    key: Entity,
    name: Entity,
    price: Entity,
    look: Option<Look>,
}

#[derive(Default)]
struct Ui {
    tiles: Vec<TileUi>,
    /// Column header (text, key badge) per column.
    headers: Vec<(Entity, Entity)>,
    money: Option<Entity>,
    timer: Option<Entity>,
    header: Option<(i32, Option<i32>, Option<usize>)>,
}

type ContentKey = (Team, usize, u64);

#[derive(Resource, Default)]
pub struct BuyMenu {
    pub open: bool,
    /// True for the frame the menu closed (to recapture the mouse).
    pub just_closed: bool,
    close_after_purchase: bool,
    /// Column armed by the first number key on the current page.
    pub category: Option<usize>,
    /// Current tab; 0 is WEAPONS.
    pub page: usize,
    pages: Vec<Page>,
    content: Option<ContentKey>,
    generation: u64,
    layout: Option<(u64, usize, u32, u32)>,
    root: Option<Entity>,
    ui: Ui,
    /// Entities spawned by the last layout build (for tests and diagnostics).
    pub spawned: usize,
}

impl BuyMenu {
    /// Only the successful authoritative purchase event can request this close.
    pub(crate) fn confirmed_purchase(&mut self, player: sim::PlayerId, local: sim::PlayerId, enabled: bool) {
        self.close_after_purchase |= self.open && enabled && player == local;
    }

    fn begin_input(&mut self) -> bool {
        self.just_closed = false;
        if std::mem::take(&mut self.close_after_purchase) && self.open {
            self.open = false;
            self.just_closed = true;
            return true;
        }
        false
    }

    /// Rebuilds the page list only when the team or offered content changes.
    fn refresh(&mut self, world: &sim::World, loc: &Localization, team: Team) {
        let mechanics = world.mechanics.iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, m| (h ^ sim::rules::mechanic_key(&m.id) as u64).wrapping_mul(0x100_0000_01b3));
        let key = (team, world.catalog.weapons.len(), mechanics ^ world.mechanics.len() as u64);
        if self.content != Some(key) {
            self.content = Some(key);
            self.pages = pages(world, loc, team);
            self.generation += 1;
        }
        self.page = self.page.min(self.pages.len().saturating_sub(1));
    }

    fn current(&self) -> Option<&Page> {
        self.pages.get(self.page)
    }

    /// The entry under a (column, row) hotkey or tile on the current page.
    fn entry(&self, column: usize, row: usize) -> Option<&Entry> {
        self.current()?.columns.get(column)?.entries.get(row)
    }
}

#[allow(clippy::too_many_arguments)]
fn request(
    item: Purchase,
    world: &sim::World,
    local: &mut LocalPlayer,
    commands: &mut Commands,
    sounds: &mut SoundLibrary,
    level: &LoadedLevel,
    sources: &mut Assets<AudioSource>,
) {
    match world.buy_cost(local.id, item) {
        Ok(_) => local.pending_buys.push(item),
        Err(_) => sounds.play(commands, &level.0.vfs, sources, "uipanorama.buymenu_failure", None),
    }
}

const DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Keyboard and mouse handling; runs before mouse look.
#[allow(clippy::too_many_arguments)]
pub fn input(
    mut commands: Commands,
    mut menu: ResMut<BuyMenu>,
    mut local: ResMut<LocalPlayer>,
    world: Res<SimWorld>,
    keys: Res<ButtonInput<KeyCode>>,
    enabled: Res<InputEnabled>,
    loc: Res<Localization>,
    level: Res<LoadedLevel>,
    mut sounds: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    mouse: Res<ButtonInput<MouseButton>>,
    tiles: Query<(&Interaction, &BuyTile)>,
    tabs: Query<(&Interaction, &BuyTab)>,
    front: Res<crate::menu::Menu>,
    settings: Res<crate::settings::Settings>,
) {
    // Effects arrive after this frame's cursor handling. Consume the confirmed
    // close here so just_closed survives until the following grab_cursor call.
    let purchase_closed = menu.begin_input();
    if front.open() {menu.open=false;return;}
    if !enabled.0 || purchase_closed {
        return;
    }
    let shop = can_shop(&world.0, local.id);
    if settings.action_just_pressed(9, &keys, &mouse, local.capture_mouse) {
        if menu.open {
            menu.open = false;
            menu.just_closed = true;
        } else if shop {
            menu.open = true;
            menu.category = None;
            menu.page = 0;
            local.latched = 0;
            local.pointer_attacks = 0;
            local.keyboard_attacks = 0;
            local.pending_slot = 0;
            local.clear_utility_requests();
            local.pending_drop = false;
        } else {
            sounds.play(&mut commands, &level.0.vfs, &mut sources, "buymenu_cant_buy.click", None);
        }
    }
    if !menu.open {
        return;
    }
    if !shop || keys.just_pressed(KeyCode::Escape) {
        menu.open = false;
        menu.just_closed = true;
        return;
    }
    let team = world.0.player(local.id).map_or(Team::Spectator, |p| p.team);
    menu.refresh(&world.0, &loc, team);
    if keys.just_pressed(KeyCode::Digit0) || keys.just_pressed(KeyCode::Backspace) || mouse.just_pressed(MouseButton::Right) {
        menu.category = None;
    }
    for (n, key) in DIGITS.iter().enumerate() {
        if !keys.just_pressed(*key) {
            continue;
        }
        match menu.category {
            None if menu.current().is_some_and(|p| n < p.columns.len()) => menu.category = Some(n),
            Some(cat) => {
                if let Some(item) = menu.entry(cat, n).map(|e| e.item) {
                    request(item, &world.0, &mut local, &mut commands, &mut sounds, &level, &mut sources);
                }
            }
            None => {}
        }
    }
    if mouse.just_pressed(MouseButton::Left) {
        let clicked = |i: &Interaction| *i != Interaction::None;
        if let Some(tab) = tabs.iter().find(|(i, _)| clicked(i)).map(|(_, t)| t.0) {
            if tab != menu.page && tab < menu.pages.len() {
                menu.page = tab;
                menu.category = None;
            }
        } else if let Some(item) = tiles.iter().find(|(i, _)| clicked(i)).and_then(|(_, t)| menu.entry(t.0, t.1)).map(|e| e.item) {
            request(item, &world.0, &mut local, &mut commands, &mut sounds, &level, &mut sources);
        }
    }
}

// ---- Presentation -------------------------------------------------------

/// Layout in 1920x1080 design units, scaled uniformly to the window.
const DESIGN: Vec2 = Vec2::new(1920.0, 1080.0);
const CONTENT_WIDTH: f32 = 1760.0;
const COLUMN_GAP: f32 = 14.0;
const TILE_HEIGHT: f32 = 84.0;
const TILE_GAP: f32 = 8.0;
const ICON_BOX: Vec2 = Vec2::new(196.0, 44.0);

fn accent(team: Team) -> Color {
    match team {
        Team::Terrorist => Color::srgb(0.89, 0.67, 0.30),
        _ => Color::srgb(0.45, 0.63, 0.88),
    }
}
fn grey(v: f32) -> Color {
    Color::srgb(v, v, v)
}
// UI blends in linear space, so panels are dark sRGB colors, never faint whites.
const BACKDROP: Color = Color::srgba(0.016, 0.02, 0.024, 0.94);
const SEPARATOR: Color = Color::srgb(0.26, 0.27, 0.28);
const TILE: Color = Color::srgba(0.115, 0.122, 0.13, 0.94);
const TILE_EDGE: Color = Color::srgb(0.19, 0.2, 0.21);
const MONEY: Color = Color::srgb(0.64, 0.85, 0.45);
const POOR: Color = Color::srgb(0.82, 0.35, 0.31);

/// Colors of one tile in one state.
struct Style {
    bg: Color,
    border: Color,
    name: Color,
    price: Color,
    key: Color,
    icon: Color,
}

fn style(look: &Look, team: Team) -> Style {
    let team_color = accent(team);
    let (bg, border) = match (look.hovered, look.status) {
        (true, Status::Buy) => (Color::srgb(0.2, 0.21, 0.22), team_color),
        (true, _) => (Color::srgb(0.15, 0.155, 0.16), grey(0.3)),
        (false, Status::Owned) => (Color::srgba(0.085, 0.09, 0.095, 0.94), Color::srgb(0.15, 0.155, 0.16)),
        _ => (TILE, TILE_EDGE),
    };
    let (name, price, icon) = match look.status {
        Status::Buy => (grey(0.95), grey(0.92), Color::srgba(1.0, 1.0, 1.0, if look.hovered { 1.0 } else { 0.9 })),
        Status::Poor => (grey(0.52), POOR, Color::srgba(1.0, 1.0, 1.0, 0.3)),
        Status::Owned => (grey(0.6), team_color, Color::srgba(1.0, 1.0, 1.0, 0.42)),
        Status::Limit | Status::Blocked => (grey(0.46), grey(0.46), Color::srgba(1.0, 1.0, 1.0, 0.24)),
    };
    let key = if look.armed { team_color } else { grey(0.56) };
    Style { bg, border, name, price, key, icon }
}

struct Builder<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    art: &'a UiAssets,
    scale: f32,
    spawned: usize,
}

impl Builder<'_, '_, '_> {
    fn px(&self, v: f32) -> Val {
        Val::Px((v * self.scale).round())
    }
    fn node(&mut self, parent: Option<Entity>, bundle: impl Bundle) -> Entity {
        self.spawned += 1;
        let mut e = self.commands.spawn(bundle);
        if let Some(parent) = parent {
            e.insert(ChildOf(parent));
        }
        e.id()
    }
    fn text(&mut self, parent: Entity, text: impl Into<String>, size: f32, color: Color, bold: bool) -> Entity {
        let font = if bold { self.art.bold.clone() } else { self.art.regular.clone() };
        let font_size = FontSize::Px(size * self.scale);
        self.node(Some(parent), (
            Text::new(text),
            TextFont { font: FontSource::Handle(font), font_size, ..default() },
            TextColor(color),
            TextLayout::no_wrap(),
            OwnFont,
        ))
    }
    /// An image fitted into `bounds` (design units), keeping its aspect.
    fn image(&mut self, parent: Entity, image: Handle<Image>, images: &Assets<Image>, bounds: Vec2, color: Color) -> Entity {
        let size = images.get(&image).map_or(bounds, |i| i.size().as_vec2());
        let fit = (bounds.x / size.x.max(1.0)).min(bounds.y / size.y.max(1.0));
        let node = Node { width: self.px(size.x * fit), height: self.px(size.y * fit), ..default() };
        self.node(Some(parent), (ImageNode { color, ..ImageNode::new(image) }, node))
    }
}

fn header_timer(world: &sim::World) -> Option<i32> {
    (world.competitive()).then(|| (world.round.live_start + world.rules.buy_time - world.time()).max(0.0).ceil() as i32)
}

/// The header's live values: money, buy time left, armed column.
type Header = (i32, Option<i32>, Option<usize>);

fn timer_text(loc: &Localization, left: i32) -> String {
    let label = token(loc, "SFUI_BuyMenu_TimerText", "Buy Time Left :");
    format!("{}  {}:{:02}", label.trim_end_matches([' ', ':']), left / 60, left % 60)
}

fn column_colors(armed: bool, team: Team) -> (Color, Color) {
    if armed { (accent(team), accent(team)) } else { (Color::WHITE, grey(0.5)) }
}

#[allow(clippy::too_many_arguments)]
fn build(menu: &mut BuyMenu, b: &mut Builder, loc: &Localization, team: Team, images: &Assets<Image>, header: Header, looks: &[Look]) -> Entity {
    let team_color = accent(team);
    let root = b.node(None, (
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
        BackgroundColor(BACKDROP),
        // Above the classic combat HUD (90), below the pause menu (100).
        GlobalZIndex(95),
    ));
    let content = b.node(Some(root), Node { width: b.px(CONTENT_WIDTH), flex_direction: FlexDirection::Column, padding: UiRect::top(b.px(44.0)), ..default() });

    // Header: title and team on the left, money and buy time on the right.
    let head = b.node(Some(content), Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::End, height: b.px(90.0), ..default() });
    let left = b.node(Some(head), Node { flex_direction: FlexDirection::Column, ..default() });
    b.text(left, token(loc, "SFUI_BuyMenu_Header", "BUY MENU"), 42.0, Color::WHITE, true);
    let team_name = match team {
        Team::Terrorist => token(loc, "SFUI_T_Label", "TERRORISTS"),
        _ => token(loc, "SFUI_CT_Label", "COUNTER-TERRORISTS"),
    };
    let team_row = b.node(Some(left), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: b.px(9.0), margin: UiRect::top(b.px(2.0)), ..default() });
    b.text(team_row, team_name, 18.0, team_color, true);
    let right = b.node(Some(head), Node { flex_direction: FlexDirection::Column, align_items: AlignItems::End, ..default() });
    menu.ui.money = Some(b.text(right, format!("${}", header.0), 46.0, MONEY, true));
    menu.ui.timer = header.1.map(|left| b.text(right, timer_text(loc, left), 18.0, grey(0.74), false));

    // Tabs (WEAPONS plus one per content source) and the key hints.
    let bar = b.node(Some(content), Node {
        flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Stretch,
        margin: UiRect::top(b.px(14.0)), height: b.px(44.0), border: UiRect::bottom(b.px(1.0)), ..default()
    });
    b.commands.entity(bar).insert(BorderColor::all(SEPARATOR));
    let tabs = b.node(Some(bar), Node { flex_direction: FlexDirection::Row, ..default() });
    let titles: Vec<String> = menu.pages.iter().map(|p| p.title.clone()).collect();
    for (i, title) in titles.into_iter().enumerate() {
        let active = i == menu.page;
        let tab = b.node(Some(tabs), (
            Button,
            BuyTab(i),
            Node { padding: UiRect::horizontal(b.px(24.0)), align_items: AlignItems::Center, border: UiRect::bottom(b.px(3.0)), margin: UiRect::bottom(b.px(-1.0)), ..default() },
            BackgroundColor(if active { Color::srgba(0.13, 0.14, 0.15, 0.95) } else { Color::NONE }),
            BorderColor::all(if active { team_color } else { Color::NONE }),
        ));
        b.text(tab, title, 18.0, if active { Color::WHITE } else { grey(0.55) }, true);
    }
    let hints = b.node(Some(bar), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: b.px(28.0), ..default() });
    let columns = menu.current().map(|p| p.columns.clone()).unwrap_or_default();
    let keys = if columns.len() > 1 { format!("1-{}", columns.len().min(9)) } else { "1".into() };
    for (key, action) in [(keys.as_str(), "CATEGORY, THEN ITEM"), ("0", "BACK"), ("B / ESC", "CLOSE")] {
        let hint = b.node(Some(hints), Node { flex_direction: FlexDirection::Row, column_gap: b.px(7.0), ..default() });
        b.text(hint, key, 15.0, grey(0.85), true);
        b.text(hint, action, 15.0, grey(0.55), false);
    }

    // The grid: one column per category, every tile visible at once.
    let grid = b.node(Some(content), Node { flex_direction: FlexDirection::Row, column_gap: b.px(COLUMN_GAP), margin: UiRect::top(b.px(22.0)), ..default() });
    let column_width = (CONTENT_WIDTH - COLUMN_GAP * 5.0) / 6.0;
    let mut looks = looks.iter();
    for (c, column) in columns.iter().enumerate() {
        let col = b.node(Some(grid), Node { width: b.px(column_width), flex_direction: FlexDirection::Column, row_gap: b.px(TILE_GAP), ..default() });
        let top = b.node(Some(col), Node {
            flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: b.px(10.0),
            height: b.px(36.0), border: UiRect::bottom(b.px(1.0)), padding: UiRect::left(b.px(2.0)), ..default()
        });
        b.commands.entity(top).insert(BorderColor::all(SEPARATOR));
        let (title_color, badge_color) = column_colors(header.2 == Some(c), team);
        let badge = b.text(top, if c < 9 { (c + 1).to_string() } else { String::new() }, 18.0, badge_color, true);
        let title = b.text(top, column.title.clone(), 21.0, title_color, true);
        menu.ui.headers.push((title, badge));
        for (r, entry) in column.entries.iter().enumerate() {
            let look = looks.next().cloned().unwrap_or(Look { status: Status::Blocked, price: String::new(), hovered: false, armed: false });
            let s = style(&look, team);
            let tile = b.node(Some(col), (
                Button,
                BuyTile(c, r),
                Node { height: b.px(TILE_HEIGHT), border: UiRect::all(b.px(1.0)), border_radius: BorderRadius::all(b.px(2.0)), ..default() },
                BackgroundColor(s.bg),
                BorderColor::all(s.border),
            ));
            let icon = entry.icons.iter().find_map(|key| b.art.image(key)).map(|image| {
                let wrap = b.node(Some(tile), Node {
                    position_type: PositionType::Absolute, left: px(0), right: px(0), top: b.px(9.0), height: b.px(ICON_BOX.y),
                    justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default()
                });
                b.image(wrap, image, images, ICON_BOX, s.icon)
            });
            let key_box = b.node(Some(tile), Node { position_type: PositionType::Absolute, left: b.px(9.0), top: b.px(6.0), ..default() });
            let key = b.text(key_box, if r < 9 { (r + 1).to_string() } else { String::new() }, 15.0, s.key, true);
            let bottom = b.node(Some(tile), Node {
                position_type: PositionType::Absolute, left: b.px(10.0), right: b.px(10.0), bottom: b.px(7.0),
                flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::End, ..default()
            });
            let name = b.text(bottom, entry.name.clone(), 17.0, s.name, false);
            let price = b.text(bottom, look.price.clone(), 17.0, s.price, true);
            menu.ui.tiles.push(TileUi { tile, icon, key, name, price, look: Some(look) });
        }
    }
    if let Some(note) = menu.current().and_then(|p| p.note.clone()) {
        let row = b.node(Some(content), Node { margin: UiRect::top(b.px(18.0)), ..default() });
        b.text(row, note, 16.0, grey(0.62), false);
    }
    menu.ui.header = Some(header);
    root
}

fn label(loc: &Localization, status: Status, item: Purchase, price: i32) -> String {
    match status {
        Status::Owned if matches!(item, Purchase::Mechanic(_)) => "EQUIPPED".into(),
        Status::Owned => token(loc, "SFUI_BuyMenu_Owned_Short", "OWNED"),
        Status::Limit => token(loc, "SFUI_BuyMenu_AtLimit_Short", "AT LIMIT"),
        Status::Blocked => token(loc, "SFUI_BuyMenu_NotAllowed_Short", "NOT ALLOWED"),
        _ if price == 0 => "FREE".into(),
        _ => format!("${price}"),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    mut menu: ResMut<BuyMenu>,
    local: Res<LocalPlayer>,
    world: Res<SimWorld>,
    loc: Res<Localization>,
    art: Res<UiAssets>,
    windows: Query<&Window>,
    images: Res<Assets<Image>>,
    hover: Query<(&Interaction, &BuyTile)>,
    mut backgrounds: Query<&mut BackgroundColor>,
    mut borders: Query<&mut BorderColor>,
    mut colors: Query<&mut TextColor>,
    mut texts: Query<&mut Text>,
    mut icons: Query<&mut ImageNode>,
) {
    let world = &world.0;
    let player = world.player(local.id).filter(|_| menu.open);
    let Some(p) = player else {
        if let Some(root) = menu.root.take() { commands.entity(root).despawn(); }
        menu.layout = None;
        menu.ui = Ui::default();
        return;
    };
    let team = p.team;
    menu.refresh(world, &loc, team);
    let hovered = hover.iter().find(|(i, _)| **i != Interaction::None).map(|(_, t)| *t);
    let category = menu.category;
    let looks: Vec<Look> = menu.current().map(|page| {
        page.columns.iter().enumerate().flat_map(|(c, column)| column.entries.iter().enumerate().map(move |(r, e)| (c, r, e)))
            .map(|(c, r, e)| {
                let (status, price) = status(world, local.id, e.item);
                Look { status, price: label(&loc, status, e.item, price), hovered: hovered == Some(BuyTile(c, r)), armed: category == Some(c) }
            }).collect()
    }).unwrap_or_default();
    let header = (p.money, header_timer(world), category);

    let size = windows.single().map_or(DESIGN, |w| Vec2::new(w.width(), w.height()));
    let layout = (menu.generation, menu.page, size.x as u32, size.y as u32);
    if menu.layout != Some(layout) {
        // Spawn already styled: new entities cannot be restyled until applied.
        if let Some(root) = menu.root.take() { commands.entity(root).despawn(); }
        menu.ui = Ui::default();
        let scale = (size.x / DESIGN.x).min(size.y / DESIGN.y).max(0.1);
        let mut builder = Builder { commands: &mut commands, art: &art, scale, spawned: 0 };
        let root = build(&mut menu, &mut builder, &loc, team, &images, header, &looks);
        menu.spawned = builder.spawned;
        menu.root = Some(root);
        menu.layout = Some(layout);
        return;
    }

    // Restyle only what changed since the last frame.
    if menu.ui.header != Some(header) {
        if let Some(mut text) = menu.ui.money.and_then(|e| texts.get_mut(e).ok()) { text.0 = format!("${}", header.0); }
        if let (Some(mut text), Some(left)) = (menu.ui.timer.and_then(|e| texts.get_mut(e).ok()), header.1) { text.0 = timer_text(&loc, left); }
        for (c, (title, badge)) in menu.ui.headers.iter().enumerate() {
            let (title_color, badge_color) = column_colors(category == Some(c), team);
            if let Ok(mut color) = colors.get_mut(*title) { color.0 = title_color; }
            if let Ok(mut color) = colors.get_mut(*badge) { color.0 = badge_color; }
        }
        menu.ui.header = Some(header);
    }
    for (tile, look) in menu.ui.tiles.iter_mut().zip(looks) {
        if tile.look.as_ref() == Some(&look) { continue; }
        let s = style(&look, team);
        if let Ok(mut c) = backgrounds.get_mut(tile.tile) { c.0 = s.bg; }
        if let Ok(mut c) = borders.get_mut(tile.tile) { *c = BorderColor::all(s.border); }
        if let Ok(mut c) = colors.get_mut(tile.name) { c.0 = s.name; }
        if let Ok(mut c) = colors.get_mut(tile.price) { c.0 = s.price; }
        if let Ok(mut t) = texts.get_mut(tile.price) { if t.0 != look.price { t.0 = look.price.clone(); } }
        if let Ok(mut c) = colors.get_mut(tile.key) { c.0 = s.key; }
        if let Some(mut node) = tile.icon.and_then(|e| icons.get_mut(e).ok()) { node.color = s.icon; }
        tile.look = Some(look);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::CursorOptions;
    #[test]
    fn only_confirmed_local_purchase_closes_once_and_recaptures_cursor() {
        fn deliver(world: &mut sim::World, menu: &mut BuyMenu, local: sim::PlayerId, enabled: bool) {
            for event in world.events.drain(..) {
                if let sim::GameEvent::Purchase { player, .. } = event {
                    menu.confirmed_purchase(player, local, enabled);
                }
            }
        }
        let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        world.mode = sim::GameMode::Special;
        let local = world.add_player("Local", Team::CounterTerrorist);
        let other = world.add_player("Other", Team::CounterTerrorist);
        world.player_mut(other).unwrap().money = 1000;
        let mut menu = BuyMenu { open: true, ..Default::default() };
        assert_eq!(world.buy(local, Purchase::Kevlar), Err(BuyError::NotEnoughMoney));
        deliver(&mut world, &mut menu, local, true);
        assert!(!menu.begin_input() && menu.open, "rejected purchase must leave menu open");
        world.buy(other, Purchase::Kevlar).unwrap();
        deliver(&mut world, &mut menu, local, true);
        assert!(!menu.begin_input() && menu.open, "another player's success must not close it");
        world.player_mut(local).unwrap().money = sim::rules::KEVLAR_PRICE + sim::rules::DEFUSE_KIT_PRICE;
        world.buy(local, Purchase::Kevlar).unwrap();
        deliver(&mut world, &mut menu, local, false);
        assert!(!menu.begin_input() && menu.open, "No must preserve the open menu");
        world.buy(local, Purchase::DefuseKit).unwrap();
        deliver(&mut world, &mut menu, local, true);
        assert!(menu.open && !menu.just_closed, "event must defer closing until input");
        assert!(menu.begin_input() && !menu.open && menu.just_closed);

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(menu)
            .insert_resource(LocalPlayer::new(local, glam::Vec3::ZERO, 0.0))
            .insert_resource(InputEnabled(true))
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::menu::Menu>()
            .init_resource::<crate::settings::Settings>()
            .add_systems(Update, crate::player::grab_cursor);
        let cursor = app.world_mut().spawn((bevy::window::PrimaryWindow, CursorOptions {
            grab_mode: bevy::window::CursorGrabMode::None, visible: true, ..Default::default()
        })).id();
        app.update();
        let cursor_state = app.world().get::<CursorOptions>(cursor).unwrap();
        assert_eq!(cursor_state.grab_mode, bevy::window::CursorGrabMode::Locked);
        assert!(!cursor_state.visible);
        let mut menu = app.world_mut().resource_mut::<BuyMenu>();
        menu.open = true;
        assert!(!menu.begin_input() && menu.open && !menu.just_closed, "close is one-shot across reopening");
        menu.open = false;
        menu.confirmed_purchase(local, local, true);
        menu.open = true;
        assert!(!menu.begin_input() && menu.open, "late receipt while closed must not arm next open");
    }

    const SCHEMA: &str = r#"items_game { prefabs {
        pistol { attributes { "in game price" 200 } }
        rifle { attributes { "in game price" 2700 } }
    } items {
        1 { name weapon_deagle prefab pistol baseitem 1 attributes { "in game price" 700 } }
        4 { name weapon_glock prefab pistol baseitem 1 used_by_classes { terrorists 1 } }
        32 { name weapon_hkp2000 prefab pistol baseitem 1 used_by_classes { counter-terrorists 1 } }
        7 { name weapon_ak47 prefab rifle baseitem 1 used_by_classes { terrorists 1 } }
        16 { name weapon_m4a1 prefab rifle baseitem 1 attributes { "in game price" 3100 } used_by_classes { counter-terrorists 1 } }
    } }"#;

    fn world(team: Team) -> (sim::World, sim::PlayerId) {
        let catalog = weapons::Catalog::from_items_game(SCHEMA).unwrap();
        let mut world = sim::World::with_catalog(std::sync::Arc::new(collision::CollisionWorld::new()), std::sync::Arc::new(catalog));
        world.mode = sim::GameMode::Special;
        let id = world.add_player("Local", team);
        (world, id)
    }

    fn names(page: &Page, column: usize) -> Vec<String> {
        page.columns[column].entries.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn weapons_page_lists_every_team_item_in_hotkey_columns() {
        let loc = Localization::default();
        let (world, id) = world(Team::CounterTerrorist);
        let ct = pages(&world, &loc, Team::CounterTerrorist);
        assert_eq!(ct.len(), 1, "no pack tools installed: WEAPONS only");
        let weapons = &ct[0];
        assert_eq!(weapons.columns.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), CATEGORIES);
        let side = |team| pages(&world, &loc, team)[0].columns.iter().flat_map(|c| c.entries.iter().map(|e| e.item)).collect::<Vec<_>>();
        let (ct_items, t_items) = (side(Team::CounterTerrorist), side(Team::Terrorist));
        for def in &world.catalog.weapons {
            let item = Purchase::Weapon(def.def_index);
            let ct_ok = world.buy_cost(id, item) != Err(BuyError::WrongTeam) && def.price > 0;
            assert_eq!(ct_items.iter().filter(|i| **i == item).count(), ct_ok as usize, "{}", def.class_name);
        }
        assert!(ct_items.contains(&Purchase::DefuseKit) && !t_items.contains(&Purchase::DefuseKit));
        let ak = world.catalog.get("weapon_ak47").unwrap().def_index;
        assert!(t_items.contains(&Purchase::Weapon(ak)) && !ct_items.contains(&Purchase::Weapon(ak)));
        assert_eq!(names(weapons, 4)[..2], ["Kevlar Vest".to_string(), "Kevlar + Helmet".to_string()]);
    }

    #[test]
    fn tile_status_follows_the_simulation_check() {
        let (mut world, id) = world(Team::CounterTerrorist);
        world.player_mut(id).unwrap().money = 800;
        let m4 = Purchase::Weapon(world.catalog.get("weapon_m4a1").unwrap().def_index);
        let p2000 = Purchase::Weapon(world.catalog.get("weapon_hkp2000").unwrap().def_index);
        assert_eq!(status(&world, id, m4), (Status::Poor, 3100));
        assert_eq!(status(&world, id, Purchase::Kevlar), (Status::Buy, 650));
        assert_eq!(status(&world, id, p2000).0, if world.player(id).unwrap().has("weapon_hkp2000") { Status::Owned } else { Status::Buy });
        world.player_mut(id).unwrap().armor = 100;
        world.player_mut(id).unwrap().money = 0;
        assert_eq!(status(&world, id, Purchase::Kevlar).0, Status::Owned);
        assert_eq!(status(&world, id, Purchase::KevlarHelmet), (Status::Poor, sim::rules::HELMET_PRICE));
        world.mode = sim::GameMode::Deathmatch;
        assert_eq!(status(&world, id, Purchase::Kevlar).0, Status::Blocked);
    }

    #[test]
    fn pack_tools_get_their_own_page_and_buy_through_the_sim() {
        let loc = Localization::default();
        let (mut world, id) = world(Team::Terrorist);
        world.mechanics.push(std::sync::Arc::new(sim::mechanics::Module {
            id: "mod_mode_test".into(), name: "Skate".into(), archive_hash: "hash".into(),
            program: None, config: [0.; _], external: None,
        }));
        let all = pages(&world, &loc, Team::Terrorist);
        assert_eq!(all.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["WEAPONS", "SKATE"]);
        let tool = all[1].columns[0].entries[0].item;
        assert_eq!(tool, Purchase::Mechanic(sim::rules::mechanic_key("mod_mode_test")));
        assert_eq!(status(&world, id, tool), (Status::Buy, 0));
        assert_eq!(label(&loc, Status::Buy, tool, 0), "FREE");
        world.buy(id, tool).unwrap();
        assert_eq!(status(&world, id, tool).0, Status::Owned);
        assert_eq!(label(&loc, Status::Owned, tool, 0), "EQUIPPED");
    }

    #[test]
    fn grid_is_built_once_and_only_restyled_on_state_changes() {
        let (mut world, id) = world(Team::CounterTerrorist);
        world.player_mut(id).unwrap().money = 800;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(BuyMenu { open: true, ..Default::default() })
            .insert_resource(LocalPlayer::new(id, glam::Vec3::ZERO, 0.0))
            .insert_resource(SimWorld(world))
            .init_resource::<Localization>()
            .init_resource::<Assets<Image>>()
            .insert_resource(UiAssets { regular: default(), bold: default(), images: default() })
            .add_systems(Update, draw);
        app.update();
        let count = |app: &mut App| { let mut nodes = app.world_mut().query::<&Node>(); nodes.iter(app.world()).count() };
        let built = count(&mut app);
        assert!(app.world().resource::<BuyMenu>().spawned > 20);
        for _ in 0..3 { app.update(); }
        assert_eq!(count(&mut app), built, "steady state spawns nothing");
        let m4 = app.world().resource::<BuyMenu>().ui.tiles.iter().map(|t| t.price).collect::<Vec<_>>();
        let price_texts = |app: &App| m4.iter().map(|e| app.world().get::<Text>(*e).unwrap().0.clone()).collect::<Vec<_>>();
        let before = price_texts(&app);
        assert!(before.iter().any(|t| t == "$3100"));
        app.world_mut().resource_mut::<SimWorld>().0.player_mut(id).unwrap().money = 5000;
        app.update();
        assert_eq!(count(&mut app), built, "money changes restyle in place");
        let money = app.world().resource::<BuyMenu>().ui.money.unwrap();
        assert_eq!(app.world().get::<Text>(money).unwrap().0, "$5000");
        app.world_mut().resource_mut::<BuyMenu>().open = false;
        app.update();
        assert!(app.world().resource::<BuyMenu>().root.is_none());
        assert!(count(&mut app) == 0, "closing despawns the grid");
    }
}
