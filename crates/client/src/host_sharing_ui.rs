//! An owned address receipt beside the original Pause panel. Its buttons never
//! mutate gameplay, and Copy uses the live host generation and UI-thread window.
use crate::{
    host_sharing::{CopyIntent, CopyResult, Receipt, Sharing},
    session::{AppState, Persistent},
};
use bevy::{
    ecs::system::NonSendMarker,
    prelude::*,
    window::{PrimaryWindow, RawHandleWrapper},
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

const PAGE_ROWS: usize = 3;

#[derive(Component, Clone, Copy)]
pub(crate) enum Action {
    Copy(CopyIntent),
    Toggle(u64),
    Refresh(u64),
    Previous(u64),
    Next(u64),
}
impl Action {
    fn generation(self) -> u64 {
        match self {
            Self::Copy(intent) => intent.generation,
            Self::Toggle(generation)
            | Self::Refresh(generation)
            | Self::Previous(generation)
            | Self::Next(generation) => generation,
        }
    }
}
struct Row {
    root: Entity,
    text: Entity,
    button: Entity,
}
#[derive(Resource)]
pub(crate) struct Ui {
    manager: Mutex<Sharing>,
    root: Entity,
    heading: Entity,
    toggle: Entity,
    controls: Entity,
    expanded: bool,
    status: Entity,
    rows: Vec<Row>,
    refresh: Entity,
    previous: Entity,
    next: Entity,
    page_text: Entity,
    page: usize,
    drawn: String,
    visible: bool,
    hover: bool,
    feedback: Option<(String, Instant)>,
    exit: Option<AppExit>,
}
impl Default for Ui {
    fn default() -> Self {
        Self {
            manager: Mutex::new(Sharing::default()),
            root: Entity::PLACEHOLDER,
            heading: Entity::PLACEHOLDER,
            toggle: Entity::PLACEHOLDER,
            controls: Entity::PLACEHOLDER,
            expanded: false,
            status: Entity::PLACEHOLDER,
            rows: Vec::new(),
            refresh: Entity::PLACEHOLDER,
            previous: Entity::PLACEHOLDER,
            next: Entity::PLACEHOLDER,
            page_text: Entity::PLACEHOLDER,
            page: 0,
            drawn: String::new(),
            visible: false,
            hover: false,
            feedback: None,
            exit: None,
        }
    }
}
impl Ui {
    pub(crate) fn blocks_movie(&self) -> bool {
        self.exit.is_some() || (self.visible && self.hover)
    }
    pub(crate) fn receipt(&self) -> Receipt {
        self.manager.lock().unwrap().receipt()
    }
}

#[derive(Component)]
pub(crate) struct ShareText;

fn text(commands: &mut Commands, art: &crate::ui_assets::UiAssets, value: &str) -> Entity {
    commands
        .spawn((
            ShareText,
            Text::new(value),
            TextFont {
                font: bevy::text::FontSource::Handle(art.regular.clone()),
                font_size: bevy::text::FontSize::Px(17.),
                ..default()
            },
            TextColor(Color::WHITE),
        ))
        .id()
}
fn button(commands: &mut Commands, art: &crate::ui_assets::UiAssets, caption: &str) -> Entity {
    let child = text(commands, art, caption);
    let entity = commands
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(px(12.), px(8.)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.14, 0.23, 0.29)),
        ))
        .id();
    commands.entity(entity).add_child(child);
    entity
}
pub(crate) fn setup(
    mut commands: Commands,
    front: Res<crate::frontend::Frontend>,
    art: Res<crate::ui_assets::UiAssets>,
) {
    if proof_enabled() {
        commands.insert_resource(ProofSink::default());
    }
    let root = commands
        .spawn((
            Persistent,
            Node {
                position_type: PositionType::Absolute,
                left: percent(56),
                top: percent(41),
                width: percent(31),
                max_height: percent(56),
                padding: UiRect::all(px(16.)),
                row_gap: px(12.),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.06, 0.08, 0.96)),
            UiTargetCamera(front.camera()),
            GlobalZIndex(103),
        ))
        .id();
    let heading = text(&mut commands, &art, "Invite friends");
    let toggle = commands
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(px(12.), px(8.)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.14, 0.23, 0.29)),
        ))
        .id();
    commands.entity(toggle).add_child(heading);
    let status = text(&mut commands, &art, "");
    let mut rows = Vec::new();
    commands.entity(root).add_child(toggle);
    for _ in 0..PAGE_ROWS {
        let row = commands
            .spawn(Node {
                column_gap: px(12.),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .id();
        let label = text(&mut commands, &art, "");
        commands.entity(label).insert(Node {
            flex_grow: 1.,
            flex_basis: px(0.),
            ..default()
        });
        let copy = button(&mut commands, &art, "Copy");
        commands.entity(row).add_children(&[label, copy]);
        commands.entity(root).add_child(row);
        rows.push(Row {
            root: row,
            text: label,
            button: copy,
        });
    }
    let controls = commands
        .spawn(Node {
            column_gap: px(10.),
            align_items: AlignItems::Center,
            ..default()
        })
        .id();
    let refresh = button(&mut commands, &art, "Refresh");
    let previous = button(&mut commands, &art, "Previous");
    let next = button(&mut commands, &art, "Next");
    let page_text = text(&mut commands, &art, "");
    commands
        .entity(controls)
        .add_children(&[refresh, previous, page_text, next]);
    commands.entity(root).add_children(&[controls, status]);
    commands.insert_resource(Ui {
        root,
        heading,
        toggle,
        controls,
        status,
        rows,
        refresh,
        previous,
        next,
        page_text,
        ..default()
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    mut ui: ResMut<Ui>,
    hosting: Res<crate::hosting::Hosting>,
    front: Res<crate::frontend::Frontend>,
    scene: Res<State<AppState>>,
    menu: Res<crate::menu::Menu>,
    window: Single<(&Window, Option<&RawHandleWrapper>), With<PrimaryWindow>>,
    pressed: Query<(&Interaction, &Action), Changed<Interaction>>,
    computed: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
    mut fonts: Query<&mut TextFont, With<ShareText>>,
    mut commands: Commands,
    mut proof_sink: Option<ResMut<ProofSink>>,
    _ui_thread: NonSendMarker,
) {
    let ui = &mut *ui;
    let manager = ui.manager.get_mut().unwrap();
    let binding = if ui.exit.is_none() {
        hosting.share_binding()
    } else {
        None
    };
    let old = manager.receipt();
    match binding {
        Some((generation, address)) => manager.set_host(generation, address),
        None if old.host_generation.is_some() => manager.clear(),
        None => (),
    }
    manager.poll();
    let (window, owner) = *window;
    ui.visible = ui.exit.is_none()
        && binding.is_some()
        && *scene.get() == AppState::Match
        && menu.open()
        && front.pause_surface_visible();
    let receipt = manager.receipt();
    if old.generation != receipt.generation {
        ui.feedback = None;
        ui.page = 0;
        ui.expanded = false;
    }
    if ui
        .feedback
        .as_ref()
        .is_some_and(|(_, until)| Instant::now() >= *until)
    {
        ui.feedback = None;
    }
    ui.hover = ui.visible
        && window.cursor_position().is_some_and(|position| {
            computed.get(ui.root).is_ok_and(|(node, transform)| {
                Rect::from_center_size(transform.affine().translation, node.size)
                    .contains(position * window.scale_factor())
            })
        });
    if !ui.visible || !window.focused || (ui.expanded && !ui.hover) {
        ui.expanded = false;
    }
    // An intent belongs to its previously rendered receipt, not whatever row
    // happens to occupy that slot after a refresh or a new host admission.
    if ui.visible && window.focused {
        for (interaction, action) in &pressed {
            if *interaction != Interaction::Pressed
                || action.generation() != manager.receipt().generation
            {
                continue;
            }
            match *action {
                Action::Toggle(_) => ui.expanded = !ui.expanded,
                _ if !ui.expanded => continue,
                Action::Refresh(_) => {
                    manager.refresh();
                    ui.page = 0;
                    ui.feedback = None;
                }
                Action::Previous(_) => ui.page = ui.page.saturating_sub(1),
                Action::Next(_) => ui.page += 1,
                Action::Copy(intent) => {
                    let result = if let Some(sink) = proof_sink.as_deref_mut() {
                        // Explicit engine fixture: exercise the real receipt and
                        // click path without touching the user's OS clipboard.
                        manager.copy_with(intent, |address| {
                            sink.copied = Some(address.to_string());
                            Ok(())
                        })
                    } else {
                        owner.map_or_else(
                            || {
                                CopyResult::Failed(
                                    "The game window is unavailable. Try again.".into(),
                                )
                            },
                            |owner| manager.copy(intent, owner),
                        )
                    };
                    let message = match result {
                        CopyResult::Copied(address) => format!("Copied {address}"),
                        CopyResult::Failed(message) => message,
                    };
                    ui.feedback = Some((message, Instant::now() + Duration::from_secs(5)));
                }
            }
        }
    }
    let receipt = manager.receipt();
    let pages = receipt.candidates.len().div_ceil(PAGE_ROWS).max(1);
    ui.page = ui.page.min(pages - 1);
    if let Ok(mut node) = nodes.get_mut(ui.root) {
        node.display = if ui.visible {
            Display::Flex
        } else {
            Display::None
        };
    }
    ui.hover = ui.visible
        && window.cursor_position().is_some_and(|position| {
            computed.get(ui.root).is_ok_and(|(node, transform)| {
                let position = position * window.scale_factor();
                let center = transform.affine().translation;
                Rect::from_center_size(center, node.size).contains(position)
            })
        });
    let paint = format!(
        "{:?}|{}|{}|{:?}|{}",
        receipt,
        ui.page,
        ui.expanded,
        ui.feedback.as_ref().map(|v| &v.0),
        window.height().to_bits()
    );
    if paint == ui.drawn {
        return;
    }
    ui.drawn = paint;
    // Match the source movie's viewport scaling while retaining our own clear
    // right-hand space beside the measured Pause panel (ends x=1008 at 1920).
    let scale = (window.height() / 1080.).clamp(0.5, 2.);
    for mut font in &mut fonts {
        font.font_size = bevy::text::FontSize::Px(17. * scale);
    }
    let title = if ui.expanded {
        "Hosting addresses ? Close"
    } else {
        "Invite friends"
    };
    if let Ok(mut value) = texts.get_mut(ui.heading) {
        value.0 = title.into();
    }
    commands
        .entity(ui.toggle)
        .insert(Action::Toggle(receipt.generation));
    for entity in [ui.controls, ui.status] {
        if let Ok(mut node) = nodes.get_mut(entity) {
            node.display = if ui.expanded {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    for (index, row) in ui.rows.iter().enumerate() {
        let candidate = receipt.candidates.get(ui.page * PAGE_ROWS + index);
        if let Ok(mut node) = nodes.get_mut(row.root) {
            node.display = if ui.expanded && candidate.is_some() {
                Display::Flex
            } else {
                Display::None
            };
        }
        if let Some(candidate) = candidate {
            let label = candidate.interface.as_deref().map_or_else(
                || candidate.scope.label().to_string(),
                |interface| format!("{} · {interface}", candidate.scope.label()),
            );
            if let Ok(mut value) = texts.get_mut(row.text) {
                value.0 = format!("{label}\n{}", candidate.endpoint);
            }
            commands.entity(row.button).insert(Action::Copy(CopyIntent {
                generation: receipt.generation,
                key: candidate.key,
            }));
        } else {
            commands.entity(row.button).remove::<Action>();
        }
        if let Ok(mut font) = fonts.get_mut(row.text) {
            font.font_size = bevy::text::FontSize::Px(17. * scale);
        }
    }
    commands
        .entity(ui.refresh)
        .insert(Action::Refresh(receipt.generation));
    commands
        .entity(ui.previous)
        .insert(Action::Previous(receipt.generation));
    commands
        .entity(ui.next)
        .insert(Action::Next(receipt.generation));
    if let Ok(mut node) = nodes.get_mut(ui.previous) {
        node.display = if ui.page > 0 {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut node) = nodes.get_mut(ui.next) {
        node.display = if ui.page + 1 < pages {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut value) = texts.get_mut(ui.page_text) {
        value.0 = if pages > 1 {
            format!("{} / {pages}", ui.page + 1)
        } else {
            String::new()
        };
    }
    if let Ok(mut value) = texts.get_mut(ui.status) {
        let message = ui
            .feedback
            .as_ref()
            .map(|v| v.0.clone())
            .or_else(|| receipt.error.clone())
            .unwrap_or_else(|| {
                if receipt.busy {
                    "Finding addresses…".into()
                } else {
                    String::new()
                }
            });
        value.0 = display_text(&message, 96);
    }
    if let Ok(mut node) = nodes.get_mut(ui.root) {
        node.padding = UiRect::all(px(16. * scale));
        node.row_gap = px(12. * scale);
    }
    for entity in
        ui.rows
            .iter()
            .map(|row| row.button)
            .chain([ui.toggle, ui.refresh, ui.previous, ui.next])
    {
        if let Ok(mut node) = nodes.get_mut(entity) {
            node.padding = UiRect::axes(px(12. * scale), px(8. * scale));
        }
    }
    for entity in [ui.heading, ui.status, ui.page_text] {
        if let Ok(mut font) = fonts.get_mut(entity) {
            font.font_size = bevy::text::FontSize::Px(17. * scale);
        }
    }
}

/// Elide presentation only; receipt identity and the copied endpoint stay exact.
fn display_text(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let mut out: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        out.push('?');
    }
    out
}

pub(crate) fn gate_exit(world: &mut World) {
    let exits: Vec<_> = world.resource_mut::<Messages<AppExit>>().drain().collect();
    let mut ui = world.resource_mut::<Ui>();
    let first = ui.exit.is_none();
    for exit in exits {
        if ui.exit.as_ref().is_none_or(|old| !old.is_error()) {
            ui.exit = Some(exit);
        }
    }
    let Some(exit) = ui.exit.clone() else {
        return;
    };
    if first {
        ui.manager.get_mut().unwrap().clear();
        ui.visible = false;
        ui.hover = false;
    }
    ui.manager.get_mut().unwrap().poll();
    if !ui.manager.get_mut().unwrap().busy() {
        world.write_message(exit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn exit_waits_for_cancelled_address_worker_and_retains_first_error() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let mut sharing = Sharing::with_test_enumerator(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(Vec::new())
        });
        sharing.set_host(23, "0.0.0.0:27422".parse().unwrap());
        sharing.poll();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut app = App::new();
        app.add_message::<AppExit>()
            .insert_resource(Ui {
                manager: Mutex::new(sharing),
                ..default()
            })
            .init_resource::<crate::server_browser::Browser>()
            .init_resource::<crate::pack_ui::PackUi>()
            .add_systems(
                Last,
                (
                    gate_exit,
                    crate::server_browser::gate_exit,
                    crate::pack_ui::gate_exit,
                )
                    .chain(),
            );
        app.world_mut().write_message(AppExit::from_code(7));
        app.update();
        assert!(app.should_exit().is_none());
        let ui = app.world().resource::<Ui>();
        assert!(ui.blocks_movie());
        assert!(ui.receipt().host_generation.is_none());
        assert!(ui.receipt().candidates.is_empty());
        assert!(ui.manager.lock().unwrap().busy());
        // Later successful/error exits cannot replace the original failure.
        app.world_mut().write_message(AppExit::Success);
        app.world_mut().write_message(AppExit::from_code(9));
        app.update();
        assert!(app.should_exit().is_none());
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            app.update();
            if app.should_exit().is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "cancelled worker was not reaped");
            std::thread::yield_now();
        }
        assert_eq!(app.should_exit(), Some(AppExit::from_code(7)));
        assert!(!app.world().resource::<Ui>().manager.lock().unwrap().busy());
        assert!(
            app.world()
                .resource::<crate::pack_ui::PackUi>()
                .blocks_movie()
        );
    }

    #[test]
    fn long_unicode_labels_are_bounded_without_changing_receipt_or_endpoint() {
        let label = "?".repeat(128);
        assert_eq!(display_text(&label, 24).chars().count(), 25);
        let endpoint = "[2001:db8:abcd:1234:ffff:eeee:dddd:cccc]:65535";
        assert_eq!(display_text(endpoint, 96), endpoint);
        assert_eq!(label.chars().count(), 128);
    }

    #[test]
    fn downstream_exit_retention_keeps_sharing_closed() {
        fn hold(mut exits: ResMut<Messages<AppExit>>) {
            exits.drain().for_each(drop);
        }
        let mut app = App::new();
        app.add_message::<AppExit>()
            .init_resource::<Ui>()
            .add_systems(Last, (gate_exit, hold).chain());
        app.world_mut().write_message(AppExit::from_code(7));
        app.update();
        app.update();
        assert!(app.should_exit().is_none());
        assert!(app.world().resource::<Ui>().blocks_movie());
        assert_eq!(
            app.world().resource::<Ui>().exit,
            Some(AppExit::from_code(7))
        );
    }
}

/// Private opt-in engine proof. This sink is never an OS clipboard claim.
#[derive(Resource, Default)]
pub(crate) struct ProofSink {
    copied: Option<String>,
    pointer: Option<Entity>,
    stage: u8,
}
pub(crate) fn proof_enabled() -> bool {
    std::env::var_os("CSR_TEST_HOST_SHARING").is_some()
        && std::env::var_os("CSR_TEST_HOST").is_some()
}
pub(crate) fn prove_copy(world: &mut World) -> Result<bool, String> {
    use bevy::input::{ButtonState, mouse::MouseButtonInput};
    let (stage, pointer, copied) = {
        let sink = world
            .get_resource::<ProofSink>()
            .ok_or("Missing address proof sink")?;
        (sink.stage, sink.pointer, sink.copied.clone())
    };
    if stage == 3 {
        return Ok(true);
    }
    if stage == 2 && copied.is_none() && world.resource::<Ui>().expanded {
        use bevy::input::{ButtonState, mouse::MouseButtonInput};
        let window = world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(world)
            .map_err(|_| "Missing proof window")?;
        world.write_message(MouseButtonInput {
            window,
            button: MouseButton::Left,
            state: ButtonState::Released,
        });
        world.resource_mut::<ProofSink>().stage = 0;
        return Ok(false);
    }
    let (visible, _expanded, receipt, button) = {
        let ui = world.resource::<Ui>();
        (
            ui.visible,
            ui.expanded,
            ui.receipt(),
            if ui.expanded {
                ui.rows.first().map(|row| row.button)
            } else {
                Some(ui.toggle)
            },
        )
    };
    if !visible || receipt.busy {
        return Ok(false);
    }
    let button = button.ok_or("Address proof has no rendered row")?;
    let Some(action) = world.get::<Action>(button).copied() else {
        return Ok(false);
    };
    if action.generation() != receipt.generation {
        return Ok(false);
    }
    let expected = match action {
        Action::Copy(intent) => Some(
            receipt
                .candidates
                .iter()
                .find(|value| value.key == intent.key)
                .ok_or("Rendered address no longer belongs to the receipt")?
                .endpoint
                .to_string(),
        ),
        Action::Toggle(_) => None,
        _ => return Err("Address proof targeted an unexpected control".into()),
    };
    let (window_id, mut window) = world
        .query_filtered::<(Entity, &mut Window), With<PrimaryWindow>>()
        .single_mut(world)
        .map_err(|_| "Address proof has no owned window")?;
    window.focused = true;
    drop(window);
    match stage {
        0 => {
            let node = world
                .get::<ComputedNode>(button)
                .ok_or("Address button has no layout")?;
            if node.size.min_element() <= 0. {
                return Ok(false);
            }
            let position = world
                .get::<UiGlobalTransform>(button)
                .ok_or("Address button has no transform")?
                .affine()
                .translation;
            world
                .get_mut::<Window>(window_id)
                .unwrap()
                .set_physical_cursor_position(Some(position.as_dvec2()));
            let mut sink = world.resource_mut::<ProofSink>();
            sink.pointer = Some(button);
            sink.stage = 1;
        }
        1 => {
            if pointer != Some(button) {
                return Err("Address proof button changed during click".into());
            }
            if world.get::<Interaction>(button) != Some(&Interaction::Hovered) {
                return Ok(false);
            }
            world.write_message(MouseButtonInput {
                window: window_id,
                button: MouseButton::Left,
                state: ButtonState::Pressed,
            });
            world.resource_mut::<ProofSink>().stage = 2;
        }
        2 => {
            world.write_message(MouseButtonInput {
                window: window_id,
                button: MouseButton::Left,
                state: ButtonState::Released,
            });
            let copied =
                copied.ok_or("Rendered Copy did not deliver its address to the proof sink")?;
            let expected = expected.ok_or("Invite button did not reveal addresses")?;
            if copied != expected || crate::remote_servers::normalize_address(&copied)? != copied {
                return Err("Copied address did not round-trip through the browser parser".into());
            }
            info!(
                "host sharing proof: real rendered Copy, receipt {} -> {copied}, browser parser exact; fake clipboard sink, no OS clipboard/reachability claim",
                receipt.generation
            );
            world.resource_mut::<ProofSink>().stage = 3;
        }
        _ => return Err("Invalid address proof phase".into()),
    }
    Ok(false)
}
