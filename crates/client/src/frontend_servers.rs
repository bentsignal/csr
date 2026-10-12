//! View-only adapter for the installed native browser layouts.
//! The caller supplies the camera, fonts, snapshots and input ownership.
use super::frontend_servers_data::{
    self as data, Control, Definition, Edit, Intent, Model, Rect, ServerKey, Snapshot, Tab,
};
use bevy::{
    input::{ButtonState, keyboard::KeyboardInput},
    prelude::*,
    ui::FocusPolicy,
};
use std::collections::HashMap;

// The fullscreen movie image occupies global layer100. Bevy globally sorts
// detached UI subtrees, so the browser and its overlays need distinct layers.
const BROWSER_LAYER: i32 = 110;
const ADD_LAYER: i32 = 120;
const CONTEXT_LAYER: i32 = 130;

#[derive(Clone, Default)]
pub struct Fonts(pub HashMap<String, FontSource>);
impl Fonts {
    pub fn insert(&mut self, role: &str, source: impl Into<FontSource>) {
        self.0.insert(role.to_ascii_lowercase(), source.into());
    }
}
#[derive(Component, Clone, Debug)]
pub enum UiAction {
    Command(String, u64),
    Address,
    Select(ServerKey, u64),
    Connect(ServerKey, u64),
    RemoveFavorite(ServerKey, u64),
    Tab(Tab),
}
pub struct View {
    pub root: Entity,
}
impl View {
    pub fn despawn(self, commands: &mut Commands) {
        commands.entity(self.root).try_despawn();
    }
}
fn color(def: &Definition, role: &str) -> Color {
    def.scheme
        .color(role)
        .map(|[r, g, b, a]| Color::srgba_u8(r, g, b, a))
        .unwrap_or(Color::NONE)
}
fn node(rect: Rect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(rect.x),
        top: px(rect.y),
        width: px(rect.width),
        height: px(rect.height),
        ..default()
    }
}
fn label(
    commands: &mut Commands,
    parent: Entity,
    text: &str,
    rect: Rect,
    role: &str,
    color_role: &str,
    def: &Definition,
    fonts: &Fonts,
) -> Result<Entity, String> {
    let info = def
        .scheme
        .font(role)
        .ok_or_else(|| format!("Missing installed font role {role}"))?;
    let source = fonts
        .0
        .get(&role.to_ascii_lowercase())
        .or_else(|| fonts.0.get("default"))
        .ok_or_else(|| format!("No supplied font for {role}"))?;
    let entity = commands
        .spawn(Node {
            overflow: Overflow::clip(),
            ..node(rect)
        })
        .id();
    commands.entity(parent).add_children(&[entity]);
    let text_entity = commands
        .spawn((
            Text::new(data::bounded(text)),
            TextLayout::no_wrap(),
            TextFont {
                font: source.clone(),
                font_size: FontSize::Px(info.size),
                weight: FontWeight(info.weight),
                ..default()
            },
            TextColor(color(def, color_role)),
            Node {
                width: percent(100.0),
                height: percent(100.0),
                ..default()
            },
        ))
        .id();
    commands.entity(entity).add_children(&[text_entity]);
    Ok(entity)
}
fn control(
    commands: &mut Commands,
    parent: Entity,
    c: &Control,
    rect: Rect,
    action: Option<UiAction>,
    enabled: bool,
    def: &Definition,
    fonts: &Fonts,
) -> Result<Entity, String> {
    let class = match c.kind.as_str() {
        "Button" => "Button",
        "TextEntry" => "TextEntry",
        "ListPanel" => "ListPanel",
        _ => "Panel",
    };
    let mut layout = node(rect);
    let border = def.scheme.border_colors(match class {
        "Button" => "ButtonBorder",
        "TextEntry" => "BaseBorder",
        "ListPanel" => "BrowserBorder",
        _ => "FrameBorder",
    });
    let colors = border.map(|rgba| {
        rgba.map(|[r, g, b, a]| Color::srgba_u8(r, g, b, a))
            .unwrap_or(Color::NONE)
    });
    if border.iter().any(Option::is_some) {
        layout.border = UiRect::all(px(1.0));
    }
    let entity = commands
        .spawn((
            layout,
            BackgroundColor(color(def, &format!("{class}.BgColor"))),
            BorderColor {
                top: colors[0],
                right: colors[1],
                bottom: colors[2],
                left: colors[3],
            },
        ))
        .id();
    commands.entity(parent).add_children(&[entity]);
    if enabled {
        if let Some(action) = action {
            commands
                .entity(entity)
                .insert((Button, FocusPolicy::Block, action));
        }
    }
    let text_color = if !enabled {
        "Label.DisabledFgColor1"
    } else if class == "Button" {
        "Button.TextColor"
    } else {
        "Label.TextColor"
    };
    if !c.text.is_empty() {
        label(
            commands,
            entity,
            &c.text,
            Rect {
                width: rect.width,
                height: rect.height,
                ..default()
            },
            &c.font,
            text_color,
            def,
            fonts,
        )?;
    }
    Ok(entity)
}

/// Builds one owned tree. Rebuild only after model or snapshot changes; the caller
/// removes the previous tree and gates its interaction queries by browser ownership.
pub fn spawn(
    commands: &mut Commands,
    camera: Entity,
    viewport: Vec2,
    def: &Definition,
    fonts: &Fonts,
    model: &Model,
    snapshot: &Snapshot,
) -> Result<View, String> {
    let frame = def
        .browser
        .control("CServerBrowserDialog")
        .ok_or("Missing installed browser frame")?;
    let tabs = def
        .browser
        .control("GameTabs")
        .ok_or("Missing installed browser tabs")?;
    let page = def
        .page
        .control("InternetGames")
        .ok_or("Missing installed browser page")?;
    let root = commands
        .spawn((
            Node {
                width: px(viewport.x),
                height: px(viewport.y),
                ..default()
            },
            UiTargetCamera(camera),
            GlobalZIndex(BROWSER_LAYER),
        ))
        .id();
    let frame_entity = commands
        .spawn((
            node(frame.rect),
            BackgroundColor(color(def, "Frame.BgColor")),
        ))
        .id();
    commands.entity(root).add_children(&[frame_entity]);
    let result = (|| {
        label(
            commands,
            frame_entity,
            def.text("ServerBrowser_Servers"),
            Rect {
                x: tabs.rect.x,
                width: tabs.rect.width,
                height: tabs.rect.y,
                ..default()
            },
            "UiBold",
            "FrameTitleBar.TextColor",
            def,
            fonts,
        )?;
        let tabs_entity = commands.spawn(node(tabs.rect)).id();
        commands.entity(frame_entity).add_children(&[tabs_entity]);
        // The page's declared y offset reserves its property-sheet tab strip.
        for (index, (tab, token)) in [
            (Tab::Internet, "ServerBrowser_InternetTab"),
            (Tab::Favorites, "ServerBrowser_FavoritesTab"),
        ]
        .iter()
        .enumerate()
        {
            let r = Rect {
                x: index as f32 * tabs.rect.width / 2.0,
                width: tabs.rect.width / 2.0,
                height: page.rect.y,
                ..default()
            };
            let button = commands
                .spawn((
                    node(r),
                    BackgroundColor(color(
                        def,
                        if model.tab == *tab {
                            "Panel.BgColor"
                        } else {
                            "ListPanel.BgColor"
                        },
                    )),
                ))
                .id();
            commands.entity(tabs_entity).add_children(&[button]);
            if !snapshot.busy && !model.add_open {
                commands
                    .entity(button)
                    .insert((Button, FocusPolicy::Block, UiAction::Tab(*tab)));
            }
            label(
                commands,
                button,
                def.text(token),
                Rect {
                    width: r.width,
                    height: r.height,
                    ..default()
                },
                "Default",
                if snapshot.busy || model.add_open {
                    "Label.DisabledFgColor1"
                } else {
                    "PropertySheet.TextColor"
                },
                def,
                fonts,
            )?;
        }
        let page_entity = commands.spawn(node(page.rect)).id();
        commands.entity(tabs_entity).add_children(&[page_entity]);
        for c in &def.page.controls {
            if !c.visible
                || c.name == "InternetGames"
                || (model.tab == Tab::Favorites && c.name == "RefreshQuickButton")
            {
                continue;
            }
            let action = c
                .command
                .as_deref()
                .filter(|cmd| {
                    matches!(
                        *cmd,
                        "connect" | "GetNewList" | "refresh" | "AddServerByName"
                    )
                })
                .and_then(|s| {
                    if s == "connect" {
                        model
                            .selected
                            .map(|key| UiAction::Connect(key, snapshot.generation))
                    } else {
                        Some(UiAction::Command(s.into(), snapshot.generation))
                    }
                });
            if c.name == "gamelist" {
                render_rows(commands, page_entity, c.rect, def, fonts, model, snapshot)?;
                continue;
            }
            if !matches!(c.kind.as_str(), "Button" | "Label" | "Divider") {
                continue;
            }
            let enabled = if c.command.as_deref() == Some("connect") {
                model.connect(snapshot).is_some()
            } else {
                c.enabled
                    && (c.kind != "Button" || action.is_some())
                    && !snapshot.busy
                    && !model.add_open
            };
            control(
                commands,
                page_entity,
                c,
                c.rect,
                action,
                enabled,
                def,
                fonts,
            )?;
        }
        // Favorites' Add Server uses the authored hidden control's bounds and label.
        if model.tab == Tab::Favorites {
            if let Some(c) = def.page.control("AddServerButton") {
                control(
                    commands,
                    page_entity,
                    c,
                    c.rect,
                    Some(UiAction::Command(
                        "AddServerByName".into(),
                        snapshot.generation,
                    )),
                    !snapshot.busy && !model.add_open,
                    def,
                    fonts,
                )?;
            }
        }
        if let Some(c) = def.browser.control("StatusLabel") {
            let text = snapshot
                .error
                .as_deref()
                .or(snapshot.status.as_deref())
                .unwrap_or(if snapshot.busy {
                    def.text("ServerBrowser_RefreshingServerList")
                } else {
                    ""
                });
            label(
                commands,
                frame_entity,
                text,
                c.rect,
                &c.font,
                "Label.TextColor",
                def,
                fonts,
            )?;
        }
        if model.add_open {
            render_add(commands, root, def, fonts, model, snapshot)?;
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        commands.entity(root).try_despawn();
        return Err(error);
    }
    Ok(View { root })
}
fn render_rows(
    commands: &mut Commands,
    parent: Entity,
    rect: Rect,
    def: &Definition,
    fonts: &Fonts,
    model: &Model,
    snapshot: &Snapshot,
) -> Result<(), String> {
    let list = commands
        .spawn((node(rect), BackgroundColor(color(def, "ListPanel.BgColor"))))
        .id();
    commands.entity(parent).add_children(&[list]);
    let row_height = def
        .scheme
        .font("Default")
        .ok_or("Missing installed default font")?
        .size;
    // Native engine column widths/row padding are not present in the RES data.
    // This functional adapter keeps those unmeasured dimensions explicit.
    let columns = [
        ("ServerBrowser_Servers", 0.0, 0.52),
        ("ServerBrowser_Map", 0.52, 0.24),
        ("ServerBrowser_Players", 0.76, 0.12),
        ("ServerBrowser_Latency", 0.88, 0.12),
    ];
    for (token, left, width) in columns {
        label(
            commands,
            list,
            def.text(token),
            Rect {
                x: rect.width * left,
                width: rect.width * width,
                height: row_height,
                ..default()
            },
            "Default",
            "ListPanel.TextColor",
            def,
            fonts,
        )?;
    }
    for (index, row) in model
        .rows(snapshot)
        .into_iter()
        .skip(
            model
                .row_offset
                .min(model.rows(snapshot).len().saturating_sub(row_capacity(def))),
        )
        .take(row_capacity(def))
        .enumerate()
    {
        let y = (index + 1) as f32 * row_height;
        let entity = commands
            .spawn((
                Button,
                FocusPolicy::Block,
                UiAction::Select(row.key, snapshot.generation),
                node(Rect {
                    y,
                    width: rect.width,
                    height: row_height,
                    ..default()
                }),
                BackgroundColor(color(
                    def,
                    if model.selected == Some(row.key) {
                        "ListPanel.SelectedBgColor"
                    } else {
                        "ListPanel.BgColor"
                    },
                )),
            ))
            .id();
        commands.entity(list).add_children(&[entity]);
        let values = [
            row.name.as_deref().unwrap_or(&row.endpoint).to_string(),
            row.map.clone().unwrap_or_default(),
            row.players
                .map(|(n, max)| format!("{n}/{max}"))
                .unwrap_or_default(),
            row.ping_ms.map(|n| n.to_string()).unwrap_or_default(),
        ];
        for ((_, left, width), text) in columns.into_iter().zip(values) {
            label(
                commands,
                entity,
                &text,
                Rect {
                    x: rect.width * left,
                    width: rect.width * width,
                    height: row_height,
                    ..default()
                },
                "Default",
                if model.selected == Some(row.key) {
                    "ListPanel.SelectedTextColor"
                } else {
                    "ListPanel.TextColor"
                },
                def,
                fonts,
            )?;
        }
    }
    if let Some(key) = model.context_row {
        let rows = model.rows(snapshot);
        let offset = model
            .row_offset
            .min(rows.len().saturating_sub(row_capacity(def)));
        if let Some(index) = rows.iter().position(|r| r.key == key && r.favorite) {
            if index >= offset && index < offset + row_capacity(def) {
                // Localized functional context action; native context-menu bounds are unmeasured.
                let r = Rect {
                    x: rect.width / 2.0,
                    y: (index - offset + 1) as f32 * row_height,
                    width: rect.width / 2.0,
                    height: row_height,
                };
                let popup = commands
                    .spawn((
                        node(r),
                        GlobalZIndex(CONTEXT_LAYER),
                        BackgroundColor(color(def, "Menu.BgColor")),
                    ))
                    .id();
                commands.entity(list).add_children(&[popup]);
                if !snapshot.busy && !model.add_open {
                    commands.entity(popup).insert((
                        Button,
                        FocusPolicy::Block,
                        UiAction::RemoveFavorite(key, snapshot.generation),
                    ));
                }
                label(
                    commands,
                    popup,
                    def.text("ServerBrowser_RemoveServerFromFavorites"),
                    Rect {
                        width: r.width,
                        height: r.height,
                        ..default()
                    },
                    "Default",
                    "Menu.TextColor",
                    def,
                    fonts,
                )?;
            }
        }
    }

    Ok(())
}
fn render_add(
    commands: &mut Commands,
    parent: Entity,
    def: &Definition,
    fonts: &Fonts,
    model: &Model,
    snapshot: &Snapshot,
) -> Result<(), String> {
    let frame = def
        .add
        .control("DialogAddServer")
        .ok_or("Missing installed Add Server frame")?;
    let entity = commands
        .spawn((
            node(frame.rect),
            BackgroundColor(color(def, "Frame.BgColor")),
            GlobalZIndex(ADD_LAYER),
        ))
        .id();
    commands.entity(parent).add_children(&[entity]);
    label(
        commands,
        entity,
        &frame.text,
        Rect {
            width: frame.rect.width,
            height: def.scheme.font("UiBold").ok_or("Missing UiBold")?.size,
            ..default()
        },
        "UiBold",
        "FrameTitleBar.TextColor",
        def,
        fonts,
    )?;
    for c in &def.add.controls {
        if !c.visible || c.name == "DialogAddServer" {
            continue;
        }
        if c.name == "ServerNameText" {
            let field = control(
                commands,
                entity,
                c,
                c.rect,
                Some(UiAction::Address),
                !snapshot.busy,
                def,
                fonts,
            )?;
            render_address(commands, field, c, def, fonts, model)?;
        } else if matches!(c.kind.as_str(), "Button" | "Label") {
            let action = c
                .command
                .as_deref()
                .filter(|cmd| matches!(*cmd, "OK" | "TestServers" | "Close"))
                .map(|s| UiAction::Command(s.into(), snapshot.generation));
            let enabled = c.enabled
                && (c.kind != "Button" || action.is_some())
                && (!snapshot.busy || c.command.as_deref() == Some("Close"));
            control(commands, entity, c, c.rect, action, enabled, def, fonts)?;
        }
    }
    if let Some(area) = def.add.control("GameTabs") {
        if let Some(status) = snapshot.error.as_deref().or(snapshot.status.as_deref()) {
            label(
                commands,
                entity,
                status,
                area.rect,
                "Default",
                "Label.TextColor",
                def,
                fonts,
            )?;
        }
    }
    Ok(())
}
fn render_address(
    commands: &mut Commands,
    parent: Entity,
    c: &Control,
    def: &Definition,
    fonts: &Fonts,
    model: &Model,
) -> Result<(), String> {
    let info = def
        .scheme
        .font(&c.font)
        .ok_or("Missing installed input font")?;
    let source = fonts
        .0
        .get(&c.font.to_ascii_lowercase())
        .or_else(|| fonts.0.get("default"))
        .ok_or("Missing supplied input font")?;
    let container = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
            ..default()
        })
        .id();
    commands.entity(parent).add_children(&[container]);
    let text = model.address.text();
    let cursor = model.address.cursor();
    let selection = model.address.selection();
    let mut cuts = vec![0, text.len(), cursor, selection.start, selection.end];
    cuts.sort_unstable();
    cuts.dedup();
    for (index, at) in cuts.iter().enumerate() {
        if *at == cursor && model.address_focused {
            let wrapper = commands
                .spawn(Node {
                    width: px(0.0),
                    height: px(info.size),
                    flex_shrink: 0.0,
                    ..default()
                })
                .id();
            commands.entity(container).add_children(&[wrapper]);
            let caret = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(1.0),
                        height: px(info.size),
                        ..default()
                    },
                    BackgroundColor(color(def, "TextEntry.CursorColor")),
                ))
                .id();
            commands.entity(wrapper).add_children(&[caret]);
        }
        let Some(end) = cuts.get(index + 1) else {
            continue;
        };
        if at == end {
            continue;
        }
        let selected = *at >= selection.start && *at < selection.end;
        let entity = commands
            .spawn((
                Text::new(&text[*at..*end]),
                TextFont {
                    font: source.clone(),
                    font_size: FontSize::Px(info.size),
                    weight: FontWeight(info.weight),
                    ..default()
                },
                TextColor(color(
                    def,
                    if selected {
                        "TextEntry.SelectedTextColor"
                    } else {
                        "TextEntry.TextColor"
                    },
                )),
                TextLayout::no_wrap(),
                Node {
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(if selected {
                    color(def, "TextEntry.SelectedBgColor")
                } else {
                    Color::NONE
                }),
            ))
            .id();
        commands.entity(container).add_children(&[entity]);
    }
    Ok(())
}

/// Processes a root-owned interaction edge, never retaining held mouse state.
/// Opens the localized favorite action from an owned, currently rendered row.
/// The caller supplies its explicit context gesture; native activation is unmeasured.
pub fn context(
    action: &UiAction,
    owns_focus: bool,
    model: &mut Model,
    snapshot: &Snapshot,
) -> bool {
    let UiAction::Select(key, generation) = action else {
        return false;
    };
    if !owns_focus
        || snapshot.busy
        || model.add_open
        || *generation != snapshot.generation
        || !model
            .rows(snapshot)
            .iter()
            .any(|row| row.key == *key && row.favorite)
    {
        return false;
    }
    let changed = model.context_row != Some(*key) || model.selected != Some(*key);
    model.selected = Some(*key);
    model.context_row = Some(*key);
    changed
}
pub fn interact(
    action: &UiAction,
    owns_focus: bool,
    model: &mut Model,
    snapshot: &Snapshot,
) -> Option<Intent> {
    if !owns_focus {
        model.blur();
        return None;
    }
    match action {
        UiAction::Address if model.add_open => {
            model.address_focused = true;
            None
        }
        UiAction::Select(key, generation)
            if !model.add_open && *generation == snapshot.generation =>
        {
            model.address_focused = false;
            model.context_row = None;
            model.select(*key, snapshot);
            None
        }
        UiAction::Tab(tab) if !model.add_open && !snapshot.busy && model.tab != *tab => {
            model.address_focused = false;
            model.tab = *tab;
            model.row_offset = 0;
            model.context_row = None;
            model.reconcile(snapshot);
            Some(Intent::Refresh(*tab))
        }
        UiAction::Connect(key, generation)
            if *generation == snapshot.generation && model.selected == Some(*key) =>
        {
            model.connect(snapshot)
        }
        UiAction::RemoveFavorite(key, generation) if *generation == snapshot.generation => {
            let result = model.remove_favorite(*key, snapshot);
            if result.is_some() {
                model.context_row = None;
            }
            result
        }
        UiAction::Command(command, generation) if *generation == snapshot.generation => {
            model.command(command, snapshot)
        }
        _ => None,
    }
}
fn row_capacity(def: &Definition) -> usize {
    let Some(list) = def.page.control("gamelist") else {
        return 1;
    };
    let Some(font) = def.scheme.font("Default") else {
        return 1;
    };
    ((list.rect.height / font.size) as usize)
        .saturating_sub(1)
        .clamp(1, data::MAX_ROWS)
}
/// Positive delta scrolls toward later rows. The caller gates wheel input by its
/// owned window/browser; this adapter also rejects modal or busy transitions.
pub fn scroll(
    delta: i32,
    owns_focus: bool,
    model: &mut Model,
    snapshot: &Snapshot,
    def: &Definition,
) -> bool {
    if !owns_focus || model.add_open || snapshot.busy || delta == 0 {
        return false;
    }
    let closed_context = model.context_row.take().is_some();
    let maximum = model.rows(snapshot).len().saturating_sub(row_capacity(def));
    let next = model
        .row_offset
        .saturating_add_signed(delta as isize)
        .min(maximum);
    let changed = next != model.row_offset;
    model.row_offset = next;
    changed || closed_context
}
/// Adds bounded list navigation to the same address-input path. Selection uses
/// stable keys and is revealed using the installed list/font dimensions.
pub fn keyboard_with_layout(
    event: &KeyboardInput,
    control: bool,
    owns_focus: bool,
    model: &mut Model,
    snapshot: &Snapshot,
    def: &Definition,
) -> (bool, Option<Intent>) {
    if owns_focus
        && !control
        && !model.add_open
        && !snapshot.busy
        && event.state == ButtonState::Pressed
        && matches!(event.key_code, KeyCode::ArrowUp | KeyCode::ArrowDown)
    {
        let rows = model.rows(snapshot);
        if rows.is_empty() {
            return (false, None);
        }
        let old = model.selected;
        let index = rows.iter().position(|row| Some(row.key) == old);
        let next = match (index, event.key_code) {
            (Some(i), KeyCode::ArrowUp) => i.saturating_sub(1),
            (Some(i), _) => (i + 1).min(rows.len() - 1),
            (None, KeyCode::ArrowUp) => rows.len() - 1,
            (None, _) => 0,
        };
        let closed_context = model.context_row.take().is_some();
        model.selected = Some(rows[next].key);
        let capacity = row_capacity(def);
        let previous = model.row_offset;
        model.row_offset = model.row_offset.min(rows.len().saturating_sub(capacity));
        if next < model.row_offset {
            model.row_offset = next;
        } else if next >= model.row_offset + capacity {
            model.row_offset = next + 1 - capacity;
        }
        return (
            closed_context || old != model.selected || previous != model.row_offset,
            None,
        );
    }
    keyboard(event, control, owns_focus, model, snapshot)
}
/// Called only for the window which owns the browser. Flash/native input routing
/// and release of previously held gameplay keys remain the root's responsibility.
pub fn keyboard(
    event: &KeyboardInput,
    control: bool,
    owns_focus: bool,
    model: &mut Model,
    snapshot: &Snapshot,
) -> (bool, Option<Intent>) {
    keyboard_with_clipboard(event, control, owns_focus, model, snapshot, clipboard_text)
}
fn keyboard_with_clipboard(
    event: &KeyboardInput,
    control: bool,
    owns_focus: bool,
    model: &mut Model,
    snapshot: &Snapshot,
    mut clipboard: impl FnMut() -> Option<String>,
) -> (bool, Option<Intent>) {
    if !owns_focus {
        model.blur();
        return (false, None);
    }
    if event.state != ButtonState::Pressed {
        return (false, None);
    }
    match event.key_code {
        KeyCode::Escape if !event.repeat => {
            if model.context_row.take().is_some() {
                return (true, None);
            }
            let intent = model.command("Close", snapshot);
            return (true, intent);
        }
        KeyCode::Enter | KeyCode::NumpadEnter if !event.repeat => {
            let command = if model.add_open { "OK" } else { "connect" };
            return (true, model.command(command, snapshot));
        }
        _ => {}
    }
    if !model.address_focused || !model.add_open || snapshot.busy {
        return (false, None);
    }
    let edit = match (event.key_code, control) {
        (KeyCode::KeyA, true) => Some(Edit::SelectAll),
        (KeyCode::KeyV, true) if !event.repeat => clipboard().map(Edit::Insert),
        (_, true) => None,
        (KeyCode::Backspace, false) => Some(Edit::Backspace),
        (KeyCode::Delete, false) => Some(Edit::Delete),
        (KeyCode::ArrowLeft, false) => Some(Edit::Left),
        (KeyCode::ArrowRight, false) => Some(Edit::Right),
        (KeyCode::Home, false) => Some(Edit::Home),
        (KeyCode::End, false) => Some(Edit::End),
        (_, false) => event
            .text
            .as_ref()
            .map(|text| Edit::Insert(text.to_string())),
    };
    (edit.is_some_and(|edit| model.address.edit(edit)), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::Key;
    fn event(code: KeyCode, text: Option<&str>) -> KeyboardInput {
        KeyboardInput {
            key_code: code,
            logical_key: Key::Character(text.unwrap_or("").into()),
            state: ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }
    #[test]
    fn clipboard_is_read_only_for_a_focused_nonrepeat_paste() {
        let mut model = Model {
            add_open: true,
            address_focused: true,
            ..default()
        };
        let snapshot = Snapshot::default();
        let mut reads = 0;
        let result = keyboard_with_clipboard(
            &event(KeyCode::KeyV, None),
            true,
            false,
            &mut model,
            &snapshot,
            || {
                reads += 1;
                Some("127.0.0.1:27015".into())
            },
        );
        assert!(!result.0);
        assert_eq!(reads, 0);
        model.address_focused = true;
        let result = keyboard_with_clipboard(
            &event(KeyCode::KeyV, None),
            true,
            true,
            &mut model,
            &snapshot,
            || {
                reads += 1;
                Some("127.0.0.1:27015".into())
            },
        );
        assert!(result.0);
        assert_eq!(reads, 1);
        assert_eq!(model.address.text(), "127.0.0.1:27015");
        let mut repeat = event(KeyCode::KeyV, None);
        repeat.repeat = true;
        keyboard_with_clipboard(&repeat, true, true, &mut model, &snapshot, || {
            reads += 1;
            None
        });
        assert_eq!(reads, 1);
    }
    #[test]
    fn typed_text_and_enter_emit_one_exact_address_without_local_network_effects() {
        let mut model = Model {
            add_open: true,
            address_focused: true,
            ..default()
        };
        let snapshot = Snapshot::default();
        for text in ["[::1]", ":27015"] {
            assert!(
                keyboard(
                    &event(KeyCode::KeyA, Some(text)),
                    false,
                    true,
                    &mut model,
                    &snapshot
                )
                .0
            );
        }
        assert_eq!(
            keyboard(
                &event(KeyCode::Enter, None),
                false,
                true,
                &mut model,
                &snapshot
            )
            .1,
            Some(Intent::AddFavorite("[::1]:27015".into()))
        );
        let mut released = event(KeyCode::Enter, None);
        released.state = ButtonState::Released;
        assert_eq!(
            keyboard(&released, false, true, &mut model, &snapshot).1,
            None
        );
    }
    #[test]
    fn modal_and_focus_guards_block_underlying_connect_actions() {
        let row = data::ServerRow {
            key: ServerKey(1),
            endpoint: "127.0.0.1:27015".into(),
            name: None,
            map: None,
            players: None,
            ping_ms: None,
            favorite: true,
            connectable: true,
        };
        let snapshot = Snapshot {
            rows: vec![row],
            ..default()
        };
        let mut model = Model {
            selected: Some(ServerKey(1)),
            ..default()
        };
        assert_eq!(
            interact(
                &UiAction::Command("connect".into(), 0),
                false,
                &mut model,
                &snapshot
            ),
            None
        );
        model.add_open = true;
        assert_eq!(
            interact(
                &UiAction::Command("connect".into(), 0),
                true,
                &mut model,
                &snapshot
            ),
            None
        );
        assert_eq!(
            interact(
                &UiAction::Command("GetNewList".into(), 0),
                true,
                &mut model,
                &snapshot
            ),
            None
        );
        model.add_open = false;
        assert_eq!(
            interact(
                &UiAction::Command("connect".into(), 0),
                true,
                &mut model,
                &snapshot
            ),
            Some(Intent::Connect {
                key: ServerKey(1),
                generation: 0
            })
        );
    }
    #[test]
    fn buttons_from_an_old_render_or_different_selection_cannot_connect() {
        let make_row = |n| data::ServerRow {
            key: ServerKey(n),
            endpoint: format!("127.0.0.1:{}", 27015 + n),
            name: None,
            map: None,
            players: None,
            ping_ms: None,
            favorite: true,
            connectable: true,
        };
        let snapshot = Snapshot {
            generation: 2,
            rows: vec![make_row(1), make_row(2)],
            ..default()
        };
        let mut model = Model {
            selected: Some(ServerKey(1)),
            ..default()
        };
        assert_eq!(
            interact(
                &UiAction::Connect(ServerKey(1), 1),
                true,
                &mut model,
                &snapshot
            ),
            None
        );
        assert_eq!(
            interact(
                &UiAction::Connect(ServerKey(2), 2),
                true,
                &mut model,
                &snapshot
            ),
            None
        );
        assert_eq!(
            interact(
                &UiAction::Connect(ServerKey(1), 2),
                true,
                &mut model,
                &snapshot
            ),
            Some(Intent::Connect {
                key: ServerKey(1),
                generation: 2
            })
        );
    }
    #[test]
    fn accepted_tab_change_emits_refresh_and_busy_change_is_rejected() {
        let mut model = Model::default();
        let mut snapshot = Snapshot::default();
        snapshot.busy = true;
        assert_eq!(
            interact(&UiAction::Tab(Tab::Favorites), true, &mut model, &snapshot),
            None
        );
        assert_eq!(model.tab, Tab::Internet);
        snapshot.busy = false;
        assert_eq!(
            interact(&UiAction::Tab(Tab::Favorites), true, &mut model, &snapshot),
            Some(Intent::Refresh(Tab::Favorites))
        );
        assert_eq!(
            interact(&UiAction::Tab(Tab::Favorites), true, &mut model, &snapshot),
            None
        );
    }
    #[test]
    fn enabled_control_blocks_underlying_rows_after_node_allocation() {
        use bevy::ecs::world::CommandQueue;
        let mut world = World::new();
        let parent = world.spawn_empty().id();
        let control_data = Control {
            name: "owned-action".into(),
            field_name: "owned-action".into(),
            kind: "Button".into(),
            rect: Rect {
                width: 100.0,
                height: 20.0,
                ..default()
            },
            text: String::new(),
            command: Some("connect".into()),
            font: "Default".into(),
            visible: true,
            enabled: true,
            default_button: false,
        };
        let definition = Definition {
            browser: default(),
            page: default(),
            add: default(),
            info: default(),
            scheme: default(),
            tokens: default(),
        };
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let entity = control(
            &mut commands,
            parent,
            &control_data,
            control_data.rect,
            Some(UiAction::Command("connect".into(), 1)),
            true,
            &definition,
            &Fonts::default(),
        )
        .unwrap();
        queue.apply(&mut world);
        assert!(world.get::<Button>(entity).is_some());
        assert_eq!(world.get::<FocusPolicy>(entity), Some(&FocusPolicy::Block));
    }

    #[test]
    fn context_uses_opaque_menu_roles_instead_of_transparent_button_roles() {
        use bevy::ecs::world::CommandQueue;
        use std::{collections::BTreeMap, sync::Arc};
        // Synthetic runtime scheme: deliberately different Menu/Button roles
        // ensure the production popup reads the declared menu appearance.
        let scheme = br#"Scheme {
            Colors { MenuBackground "12 34 56 255" MenuForeground "70 80 90 255" Blank "0 0 0 0" }
            BaseSettings { Menu.BgColor MenuBackground Menu.TextColor MenuForeground Button.BgColor Blank Button.TextColor "1 2 3 255" }
            Fonts { Default { "1" { name Synthetic tall "16" weight "500" } } }
        }"#;
        let mut game = vfs::Vfs::new();
        game.push_back(
            "synthetic scheme",
            vfs::Mount::Memory(BTreeMap::from([(
                "resource/sourcescheme.res".into(),
                Arc::<[u8]>::from(&scheme[..]),
            )])),
        );
        let mut files = BTreeMap::new();
        files.insert(
            "servers/serverbrowser_english.txt".into(),
            Arc::<[u8]>::from(&b"lang { Tokens {} }"[..]),
        );
        for name in [
            "dialogserverbrowser",
            "customgamespage",
            "dialogaddserver",
            "dialoggameinfo",
        ] {
            files.insert(
                format!("servers/{name}.res"),
                Arc::<[u8]>::from(&b"root {}"[..]),
            );
        }
        let mut platform = vfs::Vfs::new();
        platform.push_back("synthetic layouts", vfs::Mount::Memory(files));
        let def = Definition::load(&game, &platform).unwrap();
        let mut fonts = Fonts::default();
        fonts.insert("Default", Handle::<Font>::default());
        let key = ServerKey(7);
        let model = Model {
            tab: Tab::Favorites,
            selected: Some(key),
            context_row: Some(key),
            ..default()
        };
        let snapshot = Snapshot {
            generation: 3,
            rows: vec![data::ServerRow {
                key,
                endpoint: "localhost:27015".into(),
                map: Some("synthetic_map".into()),
                name: None,
                players: None,
                ping_ms: None,
                favorite: true,
                connectable: true,
            }],
            ..default()
        };
        let mut world = World::new();
        let parent = world.spawn(Node::default()).id();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        render_rows(
            &mut commands,
            parent,
            Rect {
                width: 600.0,
                height: 300.0,
                ..default()
            },
            &def,
            &fonts,
            &model,
            &snapshot,
        )
        .unwrap();
        queue.apply(&mut world);
        let popup = world
            .query::<(Entity, &UiAction)>()
            .iter(&world)
            .find_map(|(entity, action)| {
                matches!(action, UiAction::RemoveFavorite(found, 3) if *found == key)
                    .then_some(entity)
            })
            .unwrap();
        assert_eq!(
            world.get::<BackgroundColor>(popup).unwrap().0,
            Color::srgba_u8(12, 34, 56, 255)
        );
        let wrapper = world.get::<Children>(popup).unwrap()[0];
        let text = world.get::<Children>(wrapper).unwrap()[0];
        assert_eq!(
            world.get::<TextColor>(text).unwrap().0,
            Color::srgba_u8(70, 80, 90, 255)
        );
        assert_eq!(world.get::<FocusPolicy>(popup), Some(&FocusPolicy::Block));
    }

    #[test]
    fn browser_and_detached_overlays_own_a_layer_above_movie_on_the_same_camera() {
        use bevy::{
            app::{HierarchyPropagatePlugin, PropagateSet},
            ecs::world::CommandQueue,
            ui::{ComputedUiRenderTargetInfo, ComputedUiTargetCamera, UiScale},
        };
        let make = |name: &str| Control {
            name: name.into(),
            field_name: name.into(),
            kind: "Panel".into(),
            rect: Rect {
                width: 600.0,
                height: 300.0,
                ..default()
            },
            text: String::new(),
            command: None,
            font: "Default".into(),
            visible: true,
            enabled: true,
            default_button: false,
        };
        let mut scheme = data::Scheme::default();
        for role in ["default", "uibold"] {
            scheme.fonts.insert(
                role.into(),
                data::FontInfo {
                    family: "synthetic".into(),
                    size: 16.0,
                    weight: 500,
                },
            );
        }
        let def = Definition {
            browser: data::Layout {
                controls: vec![make("CServerBrowserDialog"), make("GameTabs")],
            },
            page: data::Layout {
                controls: vec![
                    make("InternetGames"),
                    Control {
                        kind: "ListPanel".into(),
                        ..make("gamelist")
                    },
                ],
            },
            add: data::Layout {
                controls: vec![make("DialogAddServer")],
            },
            info: default(),
            scheme,
            tokens: default(),
        };
        let mut fonts = Fonts::default();
        fonts.insert("Default", Handle::<Font>::default());
        fonts.insert("UiBold", Handle::<Font>::default());
        for (modal, context, expected_layers) in [
            (false, false, vec![110]),
            (true, false, vec![110, 120]),
            (false, true, vec![110, 130]),
        ] {
            let mut app = App::new();
            app.init_resource::<UiScale>()
                .add_plugins(HierarchyPropagatePlugin::<ComputedUiTargetCamera>::new(
                    PostUpdate,
                ))
                .add_plugins(HierarchyPropagatePlugin::<ComputedUiRenderTargetInfo>::new(
                    PostUpdate,
                ))
                .configure_sets(
                    PostUpdate,
                    PropagateSet::<ComputedUiTargetCamera>::default(),
                )
                .configure_sets(
                    PostUpdate,
                    PropagateSet::<ComputedUiRenderTargetInfo>::default(),
                )
                .add_systems(Update, bevy::ui::update::propagate_ui_target_cameras);
            let camera = app.world_mut().spawn(Camera2d).id();
            let snapshot = Snapshot {
                generation: 3,
                rows: vec![data::ServerRow {
                    key: ServerKey(7),
                    endpoint: "localhost:27015".into(),
                    map: None,
                    name: None,
                    players: None,
                    ping_ms: None,
                    favorite: true,
                    connectable: true,
                }],
                ..default()
            };
            let model = Model {
                tab: Tab::Favorites,
                add_open: modal,
                context_row: context.then_some(ServerKey(7)),
                ..default()
            };
            let mut queue = CommandQueue::default();
            let mut commands = Commands::new(&mut queue, app.world());
            let view = spawn(
                &mut commands,
                camera,
                Vec2::new(1920.0, 1080.0),
                &def,
                &fonts,
                &model,
                &snapshot,
            )
            .unwrap();
            queue.apply(app.world_mut());
            // Spawn the opaque fullscreen movie *after* the browser: global
            // ordering must override entity/tree allocation order.
            let movie = app
                .world_mut()
                .spawn((
                    Node::default(),
                    ImageNode::default(),
                    UiTargetCamera(camera),
                    GlobalZIndex(100),
                ))
                .id();
            app.update();
            let world = app.world_mut();
            let mut layers = Vec::new();
            for (entity, layer) in world.query::<(Entity, &GlobalZIndex)>().iter(world) {
                if entity != movie {
                    assert!(layer.0 > world.get::<GlobalZIndex>(movie).unwrap().0);
                    assert_eq!(
                        world.get::<ComputedUiTargetCamera>(entity).unwrap().get(),
                        Some(camera)
                    );
                    layers.push(layer.0);
                }
            }
            layers.sort_unstable();
            assert_eq!(layers, expected_layers);
            assert!(world.get::<Node>(view.root).is_some());
            for (entity, _) in world.query::<(Entity, &UiAction)>().iter(world) {
                assert_eq!(
                    world.get::<ComputedUiTargetCamera>(entity).unwrap().get(),
                    Some(camera)
                );
            }
        }
    }

    #[test]
    fn failed_font_load_removes_every_queued_tree_entity() {
        use bevy::ecs::world::CommandQueue;
        let make = |name: &str| Control {
            name: name.into(),
            field_name: name.into(),
            kind: "Panel".into(),
            rect: Rect {
                width: 600.0,
                height: 400.0,
                ..default()
            },
            text: String::new(),
            command: None,
            font: "Default".into(),
            visible: true,
            enabled: true,
            default_button: false,
        };
        let definition = Definition {
            browser: data::Layout {
                controls: vec![make("CServerBrowserDialog"), make("GameTabs")],
            },
            page: data::Layout {
                controls: vec![make("InternetGames")],
            },
            add: default(),
            info: default(),
            scheme: default(),
            tokens: default(),
        };
        let mut world = World::new();
        let before: Vec<_> = world.query::<Entity>().iter(&world).collect();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let result = spawn(
            &mut commands,
            Entity::PLACEHOLDER,
            Vec2::new(1920.0, 1080.0),
            &definition,
            &Fonts::default(),
            &Model::default(),
            &Snapshot::default(),
        );
        assert!(result.is_err());
        queue.apply(&mut world);
        let remaining: Vec<_> = world.query::<Entity>().iter(&world).collect();
        assert_eq!(remaining, before);
        assert_eq!(world.query::<&Node>().iter(&world).count(), 0);
    }
    #[test]
    fn keyboard_and_wheel_reach_last_row_without_leaking_modal_or_busy_input() {
        let control = Control {
            name: "gamelist".into(),
            field_name: "gamelist".into(),
            kind: "ListPanel".into(),
            rect: Rect {
                height: 60.0,
                width: 600.0,
                ..default()
            },
            text: String::new(),
            command: None,
            font: "Default".into(),
            visible: true,
            enabled: true,
            default_button: false,
        };
        let mut scheme = data::Scheme::default();
        scheme.fonts.insert(
            "default".into(),
            data::FontInfo {
                family: "own test".into(),
                size: 10.0,
                weight: 500,
            },
        );
        let def = Definition {
            browser: default(),
            page: data::Layout {
                controls: vec![control],
            },
            add: default(),
            info: default(),
            scheme,
            tokens: default(),
        };
        let mut snapshot = Snapshot {
            rows: (0..64)
                .map(|n| data::ServerRow {
                    key: ServerKey(n),
                    endpoint: format!("localhost:{}", 27015 + n),
                    name: None,
                    map: None,
                    players: None,
                    ping_ms: None,
                    favorite: true,
                    connectable: true,
                })
                .collect(),
            ..default()
        };
        let mut model = Model::default();
        for _ in 0..70 {
            keyboard_with_layout(
                &event(KeyCode::ArrowDown, None),
                false,
                true,
                &mut model,
                &snapshot,
                &def,
            );
        }
        assert_eq!(model.selected, Some(ServerKey(63)));
        assert_eq!(model.row_offset, 59);
        assert!(scroll(i32::MIN, true, &mut model, &snapshot, &def));
        assert_eq!(model.row_offset, 0);
        assert!(!scroll(i32::MAX, false, &mut model, &snapshot, &def));
        assert!(scroll(i32::MAX, true, &mut model, &snapshot, &def));
        assert_eq!(model.row_offset, 59);
        snapshot.busy = true;
        assert!(!scroll(-1, true, &mut model, &snapshot, &def));
        assert!(
            !keyboard_with_layout(
                &event(KeyCode::ArrowUp, None),
                false,
                true,
                &mut model,
                &snapshot,
                &def
            )
            .0
        );
        snapshot.busy = false;
        model.add_open = true;
        assert!(!scroll(-1, true, &mut model, &snapshot, &def));
        assert!(
            !keyboard_with_layout(
                &event(KeyCode::ArrowUp, None),
                false,
                true,
                &mut model,
                &snapshot,
                &def
            )
            .0
        );
    }
    #[test]
    fn favorite_context_removal_requires_owned_fresh_matching_row() {
        let mut snapshot = Snapshot {
            generation: 7,
            rows: vec![data::ServerRow {
                key: ServerKey(5),
                endpoint: "localhost:27015".into(),
                name: None,
                map: None,
                players: None,
                ping_ms: None,
                favorite: true,
                connectable: false,
            }],
            ..default()
        };
        let mut model = Model::default();
        assert!(!context(
            &UiAction::Select(ServerKey(5), 6),
            true,
            &mut model,
            &snapshot
        ));
        assert!(!context(
            &UiAction::Select(ServerKey(5), 7),
            false,
            &mut model,
            &snapshot
        ));
        assert!(context(
            &UiAction::Select(ServerKey(5), 7),
            true,
            &mut model,
            &snapshot
        ));
        assert_eq!(
            interact(
                &UiAction::RemoveFavorite(ServerKey(5), 6),
                true,
                &mut model,
                &snapshot
            ),
            None
        );
        snapshot.busy = true;
        assert_eq!(
            interact(
                &UiAction::RemoveFavorite(ServerKey(5), 7),
                true,
                &mut model,
                &snapshot
            ),
            None
        );
        snapshot.busy = false;
        assert_eq!(
            interact(
                &UiAction::RemoveFavorite(ServerKey(5), 7),
                true,
                &mut model,
                &snapshot
            ),
            Some(Intent::RemoveFavorite {
                key: ServerKey(5),
                generation: 7
            })
        );
        assert_eq!(model.context_row, None);
        assert!(context(
            &UiAction::Select(ServerKey(5), 7),
            true,
            &mut model,
            &snapshot
        ));
        assert_eq!(
            keyboard(
                &event(KeyCode::Escape, None),
                false,
                true,
                &mut model,
                &snapshot
            ),
            (true, None)
        );
        snapshot.rows[0].favorite = false;
        assert!(!context(
            &UiAction::Select(ServerKey(5), 7),
            true,
            &mut model,
            &snapshot
        ));
    }
}

#[cfg(not(windows))]
fn clipboard_text() -> Option<String> {
    None
}
#[cfg(windows)]
fn clipboard_text() -> Option<String> {
    use std::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(window: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalLock(memory: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(memory: *mut c_void) -> i32;
        fn GlobalSize(memory: *mut c_void) -> usize;
    }
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    struct Locked(*mut c_void);
    impl Drop for Locked {
        fn drop(&mut self) {
            unsafe {
                GlobalUnlock(self.0);
            }
        }
    }
    // Access occurs exclusively for explicit Ctrl+V in the focused address field.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let _open = Open;
        let memory = GetClipboardData(13);
        if memory.is_null() {
            return None;
        }
        let count = (GlobalSize(memory) / 2).min(data::MAX_TEXT + 1);
        if count == 0 {
            return None;
        }
        let pointer = GlobalLock(memory).cast::<u16>();
        if pointer.is_null() {
            return None;
        }
        let _locked = Locked(memory);
        let text = std::slice::from_raw_parts(pointer, count);
        let end = text.iter().position(|n| *n == 0)?;
        let text = String::from_utf16(&text[..end]).ok()?;
        (text.len() <= data::MAX_TEXT && !text.chars().any(char::is_control)).then_some(text)
    }
}
