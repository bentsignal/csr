//! The launcher window: a frameless Bevy UI app with its own title bar.
//! It idles in reactive low-power mode (no frames unless something happens)
//! and only polls quickly while an update check, download or pack install
//! is in progress.
//!
//! `CSR_LAUNCHER_FAKE=checking|downloading|ready|running|error|packs|installing|empty`
//! renders a fixture state without network or processes;
//! `CSR_LAUNCHER_SCREENSHOT=<file.png>` saves the window and exits;
//! `CSR_LAUNCHER_SCALE=<f>` overrides the display scale factor and
//! `CSR_LAUNCHER_FONT=system` skips the CS:GO font.

use crate::art;
use crate::desktop::{Act, Launcher, PackRow, TITLE, Track, WINDOW_CLASS};
use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::text::{FontSize, FontSource, FontWeight, LetterSpacing, LineHeight, TextLayout};
use bevy::ui::{FocusPolicy, RelativeCursorPosition, ScrollPosition, UiTransform};
use bevy::window::{EnabledButtons, MonitorSelection, PrimaryWindow, WindowPosition, WindowResolution};
use bevy::winit::{UpdateMode, WinitSettings};
use bevy::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

const WIDTH: u32 = 1120;
const HEIGHT: u32 = 680;
const HEADER: f32 = 48.0;
const FOOTER: f32 = 44.0;

const INK: Color = Color::srgb(0.925, 0.914, 0.894);
const TEXT2: Color = Color::srgb(0.64, 0.625, 0.60);
const TEXT3: Color = Color::srgb(0.43, 0.42, 0.405);
const RUST: Color = Color::srgb(0.769, 0.329, 0.122);
const RUST_HOVER: Color = Color::srgb(0.84, 0.40, 0.17);
const RUST_PRESS: Color = Color::srgb(0.64, 0.26, 0.09);
const LINE: Color = Color::srgba(1.0, 1.0, 1.0, 0.075);
const PANEL: Color = Color::srgba(0.03, 0.031, 0.034, 0.80);
const WARN: Color = Color::srgb(0.80, 0.52, 0.40);

#[derive(Resource)]
struct Backend(Launcher);

#[derive(Resource, Clone)]
struct Fonts {
    regular: Handle<Font>,
    bold: Handle<Font>,
    /// The fallback is a variable font: bold comes from its weight axis.
    variable: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Play,
    Packs,
}

#[derive(Resource, Default)]
struct UiState {
    page: Page,
    /// The pack whose Remove button is waiting for confirmation.
    confirm: Option<String>,
    art_scale: f32,
    frames: u32,
    last_presence: f32,
}

#[derive(Resource)]
struct Capture {
    path: PathBuf,
    requested: bool,
}

#[derive(Component, Clone, Debug, PartialEq)]
enum Btn {
    Page(Page),
    Do(Act),
    Remove(String),
    Minimize,
    Close,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Look {
    Primary,
    Ghost,
    Tab,
    Segment,
    Window,
    Close,
    Link,
}

/// Text that follows the launcher state.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Status,
    Detail,
    PlayLabel,
    Version,
    NotesTag,
    PackStatus,
    PackCount,
    PackErrors,
}

/// Nodes shown only in some states.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum When {
    PlayPage,
    PacksPage,
    Ready,
    Busy,
    Installing,
    NotInstalling,
    NoPacks,
    AnyPacks,
    HasDetail,
    HasPackErrors,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Fill {
    Download,
    Install,
}

#[derive(Component)]
struct StatusDot;
#[derive(Component)]
struct Underline;
#[derive(Component)]
struct DragRegion;
#[derive(Component)]
struct NotesBody;
#[derive(Component)]
struct NotesViewport;
#[derive(Component)]
struct PackList;
#[derive(Component)]
struct Backdrop;
#[derive(Component)]
struct Emblem(f32);

/// Everything the window shows; the UI is touched only when this changes.
#[derive(Clone, Default, PartialEq)]
struct View {
    page: Page,
    status: String,
    detail: String,
    busy: bool,
    progress: i16,
    running: bool,
    ready: Option<String>,
    track: Option<Track>,
    installing: bool,
    install_progress: i16,
    pack_status: String,
    packs: Vec<PackRow>,
    pack_errors: Vec<String>,
    notes: String,
    notes_version: Option<String>,
    confirm: Option<String>,
    version: String,
    build: String,
}

impl View {
    fn of(l: &Launcher, ui: &UiState) -> Self {
        View {
            page: ui.page,
            status: l.status.clone(),
            detail: l.detail.clone(),
            busy: l.busy,
            progress: l.progress,
            running: l.running,
            ready: l.ready.clone(),
            track: Some(l.track),
            installing: l.installing,
            install_progress: l.install_progress,
            pack_status: l.pack_status.clone(),
            packs: l.packs.clone(),
            pack_errors: l.pack_errors.clone(),
            notes: l.notes.clone(),
            notes_version: l.notes_version.clone(),
            confirm: ui.confirm.clone(),
            version: l.version.clone(),
            build: l.build.clone(),
        }
    }
}

/// Stratum 2 from the player's CS:GO Legacy install (as the game reads it),
/// else Bahnschrift, else Segoe UI.
fn font_bytes(root: &std::path::Path) -> (Vec<u8>, Vec<u8>, bool) {
    if std::env::var("CSR_LAUNCHER_FONT").map_or(true, |v| v != "system") {
        if let Some(dir) = crate::setup::known_game_dir(root) {
            let read = |name: &str| {
                std::fs::read(dir.join(format!("panorama/fonts/{name}.vfont"))).ok().and_then(client::ui_assets::decode_font)
            };
            if let (Some(regular), Some(bold)) = (read("stratum2regular"), read("stratum2bold")) {
                return (regular, bold, false);
            }
        }
    }
    let fonts = std::path::Path::new(r"C:\Windows\Fonts");
    if let Ok(variable) = std::fs::read(fonts.join("bahnschrift.ttf")) {
        return (variable.clone(), variable, true);
    }
    match (std::fs::read(fonts.join("segoeui.ttf")), std::fs::read(fonts.join("segoeuib.ttf"))) {
        (Ok(regular), Ok(bold)) => (regular, bold, false),
        // Bevy's built-in font.
        _ => (vec![], vec![], false),
    }
}

pub fn run(launcher: Launcher) -> Result<(), String> {
    let capture = std::env::var_os("CSR_LAUNCHER_SCREENSHOT").map(PathBuf::from);
    let scale = std::env::var("CSR_LAUNCHER_SCALE").ok().and_then(|s| s.parse::<f32>().ok()).filter(|s| *s > 0.0);
    let (regular, bold, variable) = font_bytes(&launcher.root);
    // The OS scale factor applies to this logical size when the window opens;
    // an override (screenshots) needs the physical size spelled out.
    let resolution = match scale {
        Some(s) => WindowResolution::new((WIDTH as f32 * s) as u32, (HEIGHT as f32 * s) as u32).with_scale_factor_override(s),
        None => WindowResolution::new(WIDTH, HEIGHT),
    };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: TITLE.into(),
                    name: Some(WINDOW_CLASS.into()),
                    resolution,
                    position: WindowPosition::Centered(MonitorSelection::Primary),
                    decorations: false,
                    resizable: false,
                    enabled_buttons: EnabledButtons { maximize: false, ..default() },
                    // Shown once the first frames are drawn: no white flash.
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .set(bevy::log::LogPlugin { filter: "warn,wgpu=error,naga=error,bevy_gizmos_render=error".into(), ..default() })
            .disable::<bevy::audio::AudioPlugin>()
            .disable::<bevy::pbr::PbrPlugin>(),
    );
    let fonts = {
        let mut assets = app.world_mut().resource_mut::<Assets<Font>>();
        if regular.is_empty() {
            Fonts { regular: Handle::default(), bold: Handle::default(), variable: false }
        } else {
            Fonts { regular: assets.add(Font::from_bytes(regular)), bold: assets.add(Font::from_bytes(bold)), variable }
        }
    };
    app.insert_resource(ClearColor(Color::srgb(0.03, 0.032, 0.036)))
        // Continuous until the window is revealed: hidden windows get no redraws.
        .insert_resource(WinitSettings::continuous())
        .insert_resource(fonts)
        .insert_resource(UiState { page: if launcher.packs_page_fixture() { Page::Packs } else { Page::Play }, ..default() })
        .insert_resource(Backend(launcher))
        .add_systems(Startup, build)
        .add_systems(
            Update,
            (pump, clicks, drag, scroll_notes, (sync, style_buttons, animate).chain(), art_for_scale, reveal, capture_frame).chain(),
        );
    if let Some(path) = capture {
        app.insert_resource(Capture { path, requested: false });
    }
    match app.run() {
        AppExit::Success => Ok(()),
        AppExit::Error(code) => Err(format!("The launcher window failed ({code}).")),
    }
}

impl Launcher {
    fn packs_page_fixture(&self) -> bool {
        std::env::var("CSR_LAUNCHER_FAKE").is_ok_and(|s| matches!(s.as_str(), "packs" | "installing" | "empty"))
    }
}

// --- Building blocks -------------------------------------------------------------

fn text(f: &Fonts, s: impl Into<String>, size: f32, bold: bool, color: Color, spacing: f32) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font: FontSource::Handle(if bold { f.bold.clone() } else { f.regular.clone() }),
            font_size: FontSize::Px(size),
            weight: if f.variable && bold { FontWeight::SEMIBOLD } else { FontWeight::default() },
            ..default()
        },
        TextColor(color),
        LetterSpacing::Px(spacing),
    )
}

/// A single-line label (buttons, tabs).
fn label(f: &Fonts, s: impl Into<String>, size: f32, bold: bool, color: Color, spacing: f32) -> impl Bundle {
    (text(f, s, size, bold, color, spacing), TextLayout::no_wrap())
}

fn row() -> Node {
    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, ..default() }
}

fn column() -> Node {
    Node { flex_direction: FlexDirection::Column, ..default() }
}

fn rule(width: Val, height: Val, color: Color) -> impl Bundle {
    (Node { width, height, flex_shrink: 0.0, ..default() }, BackgroundColor(color))
}

fn button(btn: Btn, look: Look, node: Node) -> impl Bundle {
    // A faint sheen over every fill; invisible on transparent buttons.
    let sheen = if look == Look::Primary {
        vec![ColorStop::percent(Color::srgba(1.0, 0.9, 0.8, 0.10), 0), ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.12), 100)]
    } else {
        vec![ColorStop::percent(Color::NONE, 0), ColorStop::percent(Color::NONE, 100)]
    };
    (
        Button,
        node,
        btn,
        look,
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
        BackgroundGradient::from(LinearGradient::to_bottom(sheen)),
    )
}

fn image(art: art::Image) -> Image {
    Image::new(
        Extent3d { width: art.width, height: art.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        art.rgba8(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

// --- Layout ----------------------------------------------------------------------

fn build(mut commands: Commands, fonts: Res<Fonts>) {
    let f = fonts.into_inner().clone();
    commands.spawn(Camera2d);
    commands
        .spawn(Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, ..default() })
        .with_children(|root| {
            root.spawn((
                Backdrop,
                ImageNode::default(),
                Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
            ));
            header(root, &f);
            root.spawn(Node { flex_grow: 1.0, min_height: px(0), ..default() }).with_children(|body| {
                play_page(body, &f);
                packs_page(body, &f);
            });
            footer(root, &f);
        });
}

fn header(root: &mut ChildSpawnerCommands, f: &Fonts) {
    root.spawn((
        DragRegion,
        Interaction::default(),
        FocusPolicy::Pass,
        Node {
            height: px(HEADER),
            flex_shrink: 0.0,
            align_items: AlignItems::Stretch,
            padding: UiRect::left(px(18)),
            border: UiRect::bottom(px(1)),
            ..row()
        },
        BorderColor::all(LINE),
        BackgroundGradient::from(LinearGradient::to_bottom(vec![
            ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.55), 0),
            ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.25), 100),
        ])),
    ))
    .with_children(|h| {
        h.spawn(Node { column_gap: px(10), ..row() }).with_children(|brand| {
            brand.spawn((Emblem(26.0), ImageNode::default(), Node { width: px(26), height: px(26), ..default() }));
            brand.spawn(Node { column_gap: px(7), ..row() }).with_children(|w| {
                w.spawn(label(f, "COUNTER-STRIKE", 13.0, true, INK, 2.2));
                w.spawn(text(f, "RUST", 13.0, true, RUST, 2.2));
            });
        });
        h.spawn(Node { width: px(1), margin: UiRect::axes(px(22), px(15)), ..default() }).insert(BackgroundColor(LINE));
        for (page, name) in [(Page::Play, "PLAY"), (Page::Packs, "CONTENT PACKS")] {
            h.spawn(button(
                Btn::Page(page),
                Look::Tab,
                Node { padding: UiRect::horizontal(px(14)), justify_content: JustifyContent::Center, ..row() },
            ))
            .with_children(|t| {
                t.spawn(label(f, name, 12.5, true, TEXT2, 1.8));
                t.spawn((
                    Underline,
                    Node { position_type: PositionType::Absolute, left: px(14), right: px(14), bottom: px(-1), height: px(2), ..default() },
                    BackgroundColor(Color::NONE),
                ));
            });
        }
        h.spawn(Node { flex_grow: 1.0, ..default() });
        h.spawn(button(Btn::Minimize, Look::Window, Node { width: px(46), justify_content: JustifyContent::Center, ..row() }))
            .with_children(|b| {
                b.spawn(rule(px(10), px(1), INK));
            });
        h.spawn(button(Btn::Close, Look::Close, Node { width: px(46), justify_content: JustifyContent::Center, ..row() }))
            .with_children(|b| {
                for angle in [45.0_f32, -45.0] {
                    b.spawn((
                        Node { position_type: PositionType::Absolute, width: px(14), height: px(1), ..default() },
                        BackgroundColor(INK),
                        UiTransform { rotation: Rot2::degrees(angle), ..default() },
                    ));
                }
            });
    });
}

fn play_page(body: &mut ChildSpawnerCommands, f: &Fonts) {
    body.spawn((
        When::PlayPage,
        Node {
            flex_grow: 1.0,
            padding: UiRect { left: px(56), right: px(40), top: px(40), bottom: px(28) },
            column_gap: px(40),
            align_items: AlignItems::Stretch,
            ..row()
        },
    ))
    .with_children(|page| {
        page.spawn(Node { width: px(420), flex_shrink: 0.0, justify_content: JustifyContent::SpaceBetween, ..column() })
            .with_children(|left| {
                // The lockup.
                left.spawn(Node { column_gap: px(18), ..row() }).with_children(|lock| {
                    lock.spawn((Emblem(84.0), ImageNode::default(), Node { width: px(84), height: px(84), ..default() }));
                    lock.spawn(Node { row_gap: px(6), ..column() }).with_children(|words| {
                        words.spawn(label(f, "COUNTER-STRIKE", 38.0, true, INK, 1.2));
                        words.spawn(Node { column_gap: px(12), ..row() }).with_children(|sub| {
                            sub.spawn(rule(px(26), px(2), RUST));
                            sub.spawn(label(f, "RUST", 16.0, true, RUST, 9.0));
                        });
                    });
                });
                // Status, Play and the update controls.
                left.spawn(column()).with_children(|play| {
                    play.spawn(Node { column_gap: px(10), margin: UiRect::bottom(px(4)), ..row() }).with_children(|s| {
                        s.spawn((StatusDot, rule(px(6), px(6), TEXT3)));
                        s.spawn((Slot::Status, text(f, "", 15.0, false, INK, 0.3)));
                    });
                    play.spawn((
                        When::HasDetail,
                        Slot::Detail,
                        text(f, "", 12.5, false, TEXT3, 0.2),
                        Node { margin: UiRect::left(px(16)), max_width: px(400), ..default() },
                    ));
                    play.spawn(button(
                        Btn::Do(Act::Play),
                        Look::Primary,
                        Node {
                            width: px(380),
                            height: px(64),
                            margin: UiRect::top(px(14)),
                            justify_content: JustifyContent::Center,
                            overflow: Overflow::clip(),
                            ..row()
                        },
                    ))
                    .with_children(|b| {
                        b.spawn((Slot::PlayLabel, text(f, "PLAY", 22.0, true, Color::WHITE, 4.0)));
                        b.spawn((
                            When::Busy,
                            Node { position_type: PositionType::Absolute, left: px(0), right: px(0), bottom: px(0), height: px(3), ..default() },
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
                        ))
                        .with_children(|track| {
                            track.spawn((
                                Fill::Download,
                                Node { position_type: PositionType::Absolute, top: px(0), bottom: px(0), left: px(0), width: percent(0), ..default() },
                                BackgroundColor(Color::srgba(1.0, 0.95, 0.9, 0.9)),
                            ));
                        });
                    });
                    play.spawn(Node { margin: UiRect::top(px(14)), column_gap: px(10), ..row() }).with_children(|controls| {
                        controls.spawn(Node { ..row() }).with_children(|seg| {
                            for (track, name) in [(Track::Stable, "STABLE"), (Track::Dev, "DEV")] {
                                seg.spawn(button(
                                    Btn::Do(Act::Track(track)),
                                    Look::Segment,
                                    Node {
                                        width: px(84),
                                        height: px(32),
                                        border: UiRect::all(px(1)),
                                        margin: UiRect::right(px(-1)),
                                        justify_content: JustifyContent::Center,
                                        ..row()
                                    },
                                ))
                                .with_children(|b| {
                                    b.spawn(label(f, name, 11.5, true, TEXT2, 1.8));
                                });
                            }
                        });
                        controls.spawn(button(
                            Btn::Do(Act::Refresh),
                            Look::Ghost,
                            Node { height: px(32), padding: UiRect::horizontal(px(14)), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, ..row() },
                        ))
                        .with_children(|b| {
                            b.spawn(label(f, "REFRESH", 11.5, true, TEXT2, 1.4));
                        });
                        controls
                            .spawn((
                                When::Ready,
                                button(
                                    Btn::Do(Act::UpdateOnly),
                                    Look::Link,
                                    Node { height: px(32), padding: UiRect::horizontal(px(6)), justify_content: JustifyContent::Center, ..row() },
                                ),
                            ))
                            .with_children(|b| {
                                b.spawn(label(f, "UPDATE ONLY", 11.5, true, TEXT2, 1.4));
                            });
                    });
                });
            });
        page.spawn(Node { flex_grow: 1.0, ..default() });
        // Patch notes.
        page.spawn((
            Node { width: px(420), flex_shrink: 0.0, border: UiRect::all(px(1)), ..column() },
            BackgroundColor(PANEL),
            BorderColor::all(LINE),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node { height: px(48), flex_shrink: 0.0, padding: UiRect::horizontal(px(22)), border: UiRect::bottom(px(1)), ..row() })
                .insert(BorderColor::all(LINE))
                .with_children(|h| {
                    h.spawn(rule(px(3), px(14), RUST));
                    h.spawn((text(f, "PATCH NOTES", 12.5, true, INK, 2.4), Node { margin: UiRect::left(px(10)), ..default() }));
                    h.spawn(Node { flex_grow: 1.0, ..default() });
                    h.spawn((Slot::NotesTag, text(f, "", 12.0, true, TEXT3, 1.2)));
                });
            panel
                .spawn((
                    NotesViewport,
                    RelativeCursorPosition::default(),
                    ScrollPosition::default(),
                    Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), ..column() },
                ))
                .with_children(|v| {
                    v.spawn((NotesBody, Node { padding: UiRect { left: px(22), right: px(22), top: px(14), bottom: px(18) }, ..column() }));
                });
        });
    });
}

fn packs_page(body: &mut ChildSpawnerCommands, f: &Fonts) {
    body.spawn((
        When::PacksPage,
        Node { flex_grow: 1.0, padding: UiRect { left: px(56), right: px(56), top: px(40), bottom: px(24) }, ..column() },
        BackgroundColor(Color::srgba(0.02, 0.021, 0.024, 0.55)),
    ))
    .with_children(|page| {
        page.spawn(Node { align_items: AlignItems::FlexEnd, ..row() }).with_children(|top| {
            top.spawn(Node { row_gap: px(8), ..column() }).with_children(|t| {
                t.spawn(Node { column_gap: px(14), ..row() }).with_children(|title| {
                    title.spawn(text(f, "CONTENT PACKS", 30.0, true, INK, 1.4));
                    title
                        .spawn((When::AnyPacks, Node { height: px(22), padding: UiRect::horizontal(px(8)), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, ..row() }, BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.16))))
                        .with_children(|badge| {
                            badge.spawn((Slot::PackCount, label(f, "", 12.0, true, TEXT2, 0.8)));
                        });
                });
                t.spawn((Slot::PackStatus, text(f, "", 13.0, false, TEXT2, 0.3)));
            });
            top.spawn(Node { flex_grow: 1.0, ..default() });
            top.spawn(Node { column_gap: px(10), ..row() }).with_children(|actions| {
                actions
                    .spawn(button(
                        Btn::Do(Act::PacksFolder),
                        Look::Ghost,
                        Node { height: px(40), padding: UiRect::horizontal(px(20)), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, ..row() },
                    ))
                    .with_children(|b| {
                        b.spawn(label(f, "OPEN FOLDER", 12.0, true, TEXT2, 1.6));
                    });
                actions
                    .spawn((
                        When::Installing,
                        button(
                            Btn::Do(Act::Cancel),
                            Look::Ghost,
                            Node { height: px(40), width: px(170), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, ..row() },
                        ),
                    ))
                    .with_children(|b| {
                        b.spawn(label(f, "CANCEL", 12.0, true, TEXT2, 1.6));
                    });
                actions
                    .spawn((
                        When::NotInstalling,
                        button(
                            Btn::Do(Act::PickPack),
                            Look::Primary,
                            Node { height: px(40), width: px(170), justify_content: JustifyContent::Center, ..row() },
                        ),
                    ))
                    .with_children(|b| {
                        b.spawn(label(f, "INSTALL PACK", 12.0, true, Color::WHITE, 1.6));
                    });
            });
        });
        // A rule under the title that doubles as the install progress bar.
        page.spawn((
            Node { height: px(2), flex_shrink: 0.0, margin: UiRect::top(px(18)), overflow: Overflow::clip(), ..default() },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
        ))
        .with_children(|t| {
            t.spawn((
                Fill::Install,
                Node { position_type: PositionType::Absolute, top: px(0), bottom: px(0), left: px(0), width: percent(0), ..default() },
                BackgroundColor(RUST),
            ));
        });
        page.spawn((
            Node { flex_grow: 1.0, min_height: px(0), margin: UiRect::top(px(22)), border: UiRect::all(px(1)), ..column() },
            BackgroundColor(PANEL),
            BorderColor::all(LINE),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node { height: px(38), flex_shrink: 0.0, padding: UiRect::horizontal(px(22)), border: UiRect::bottom(px(1)), ..row() })
                .insert(BorderColor::all(LINE))
                .with_children(|h| {
                    h.spawn((text(f, "NAME", 11.0, true, TEXT3, 2.0), Node { flex_grow: 1.0, ..default() }));
                    h.spawn((text(f, "VERSION", 11.0, true, TEXT3, 2.0), Node { width: px(160), ..default() }));
                    h.spawn(Node { width: px(104), ..default() });
                });
            panel.spawn((PackList, Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), ..column() }, ScrollPosition::default()));
            panel
                .spawn((When::NoPacks, Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(38), bottom: px(0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: px(8), ..column() }))
                .with_children(|e| {
                    e.spawn(text(f, "NO CONTENT PACKS INSTALLED", 13.0, true, TEXT2, 2.0));
                    e.spawn(text(f, "Installed .csrpack files appear here.", 13.0, false, TEXT3, 0.2));
                });
        });
        page.spawn((When::HasPackErrors, Slot::PackErrors, text(f, "", 12.0, false, WARN, 0.2), Node { margin: UiRect::top(px(12)), ..default() }));
    });
}

fn footer(root: &mut ChildSpawnerCommands, f: &Fonts) {
    root.spawn((
        Node { height: px(FOOTER), flex_shrink: 0.0, padding: UiRect::horizontal(px(56)), border: UiRect::top(px(1)), ..row() },
        BorderColor::all(LINE),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
    ))
    .with_children(|bar| {
        bar.spawn((Slot::Version, text(f, "", 11.5, false, TEXT3, 1.0)));
        bar.spawn(Node { flex_grow: 1.0, ..default() });
        bar.spawn(button(Btn::Do(Act::Logs), Look::Link, Node { height: percent(100), padding: UiRect::horizontal(px(4)), ..row() }))
            .with_children(|b| {
                b.spawn(label(f, "OPEN LOGS", 11.5, true, TEXT2, 1.6));
            });
    });
}

// --- Notes ------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Block {
    Heading(String, usize),
    Bullet(String),
    Para(String),
}

/// Strips inline markdown: emphasis, code ticks and link targets.
fn inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('[') {
        let Some(mid) = rest[start..].find("](") else { break };
        let Some(end) = rest[start + mid..].find(')') else { break };
        out.push_str(&rest[..start]);
        out.push_str(&rest[start + 1..start + mid]);
        rest = &rest[start + mid + end + 1..];
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "").replace('`', "").trim().to_string()
}

/// A minimal markdown reader: headings, bullets and paragraphs.
fn parse_notes(md: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para = String::new();
    let flush = |para: &mut String, blocks: &mut Vec<Block>| {
        if !para.trim().is_empty() {
            blocks.push(Block::Para(inline(para)));
        }
        para.clear();
    };
    for line in md.lines() {
        let t = line.trim();
        if t.is_empty() || t.chars().all(|c| c == '-' || c == '=' || c == '*') && t.len() >= 3 {
            flush(&mut para, &mut blocks);
        } else if let Some(h) = t.strip_prefix('#') {
            flush(&mut para, &mut blocks);
            let level = 1 + h.chars().take_while(|c| *c == '#').count();
            blocks.push(Block::Heading(inline(h.trim_start_matches('#')), level));
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Bullet(inline(item)));
        } else if line.starts_with("  ") && matches!(blocks.last(), Some(Block::Bullet(_))) && para.is_empty() {
            // A wrapped bullet continues the previous item.
            if let Some(Block::Bullet(b)) = blocks.last_mut() {
                b.push(' ');
                b.push_str(&inline(t));
            }
        } else {
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(t);
        }
    }
    flush(&mut para, &mut blocks);
    blocks
}

fn spawn_notes(body: &mut ChildSpawnerCommands, f: &Fonts, notes: &str) {
    let blocks = parse_notes(notes);
    if blocks.is_empty() {
        body.spawn(text(f, "No notes for this build.", 13.5, false, TEXT3, 0.2));
        return;
    }
    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Heading(s, level) => {
                let (size, color, spacing) = if *level <= 2 { (17.0, INK, 1.2) } else { (12.5, TEXT2, 2.0) };
                body.spawn((
                    text(f, s.to_uppercase(), size, true, color, spacing),
                    Node { margin: UiRect { top: px(if i == 0 { 4 } else { 18 }), bottom: px(8), ..default() }, ..default() },
                ));
            }
            Block::Bullet(s) => {
                body.spawn(Node { align_items: AlignItems::FlexStart, margin: UiRect::bottom(px(7)), ..row() }).with_children(|r| {
                    r.spawn((Node { width: px(4), height: px(4), margin: UiRect { top: px(7), right: px(12), ..default() }, flex_shrink: 0.0, ..default() }, BackgroundColor(RUST)));
                    r.spawn((text(f, s.clone(), 13.5, false, TEXT2, 0.2), LineHeight::RelativeToFont(1.4), Node { flex_shrink: 1.0, ..default() }));
                });
            }
            Block::Para(s) => {
                body.spawn((
                    text(f, s.clone(), 13.5, false, TEXT2, 0.2),
                    LineHeight::RelativeToFont(1.45),
                    Node { margin: UiRect { top: px(if i == 0 { 0 } else { 6 }), bottom: px(8), ..default() }, ..default() },
                ));
            }
        }
    }
}

fn spawn_packs(list: &mut ChildSpawnerCommands, f: &Fonts, packs: &[PackRow], confirm: Option<&str>) {
    for pack in packs {
        list.spawn((
            Node { height: px(58), flex_shrink: 0.0, padding: UiRect::horizontal(px(22)), border: UiRect::bottom(px(1)), ..row() },
            BorderColor::all(LINE),
        ))
        .with_children(|r| {
            r.spawn(Node { flex_grow: 1.0, row_gap: px(3), ..column() }).with_children(|n| {
                n.spawn(text(f, pack.name.clone(), 15.0, true, INK, 0.6));
                n.spawn(text(f, pack.id.clone(), 11.5, false, TEXT3, 0.4));
            });
            r.spawn((text(f, pack.version.clone(), 13.0, false, TEXT2, 0.6), Node { width: px(160), ..default() }));
            let confirming = confirm == Some(pack.id.as_str());
            r.spawn(button(
                Btn::Remove(pack.id.clone()),
                if confirming { Look::Primary } else { Look::Ghost },
                Node { width: px(104), height: px(30), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, ..row() },
            ))
            .with_children(|b| {
                b.spawn(label(f, if confirming { "CONFIRM" } else { "REMOVE" }, 11.0, true, TEXT2, 1.6));
            });
        });
    }
}

// --- Systems ----------------------------------------------------------------------

fn pump(mut backend: ResMut<Backend>, mut ui: ResMut<UiState>, time: Res<Time>, mut settings: ResMut<WinitSettings>, capture: Option<Res<Capture>>) {
    let now = time.elapsed_secs();
    let presence = now - ui.last_presence >= 1.0;
    if presence {
        ui.last_presence = now;
    }
    let l = &mut backend.0;
    l.pump(presence);
    if capture.is_some() || ui.frames < 3 {
        return;
    }
    let animating = (l.busy && l.progress < 0) || (l.installing && l.install_progress < 0);
    let want = if animating {
        let m = UpdateMode::reactive(Duration::from_millis(16));
        (m, m)
    } else if l.active() {
        let m = UpdateMode::reactive(Duration::from_millis(100));
        (m, m)
    } else if l.running {
        let m = UpdateMode::reactive_low_power(Duration::from_secs(1));
        (m, m)
    } else {
        (UpdateMode::reactive(Duration::from_secs(2)), UpdateMode::reactive_low_power(Duration::from_secs(5)))
    };
    if settings.focused_mode != want.0 || settings.unfocused_mode != want.1 {
        settings.focused_mode = want.0;
        settings.unfocused_mode = want.1;
    }
}

fn enabled(btn: &Btn, l: &Launcher) -> bool {
    match btn {
        Btn::Do(Act::Play) => l.can_play(),
        Btn::Do(Act::UpdateOnly) => l.can_play() && !l.busy && l.ready.is_some(),
        Btn::Do(Act::Refresh) | Btn::Do(Act::Track(_)) => !l.busy,
        Btn::Do(Act::PickPack) | Btn::Remove(_) => l.can_manage_packs(),
        _ => true,
    }
}

fn selected(btn: &Btn, l: &Launcher, ui: &UiState) -> bool {
    match btn {
        Btn::Page(p) => ui.page == *p,
        Btn::Do(Act::Track(t)) => l.track == *t,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn clicks(
    buttons: Query<(Entity, &Interaction, &Btn), Changed<Interaction>>,
    mut pressed: Local<Option<Entity>>,
    mut backend: ResMut<Backend>,
    mut ui: ResMut<UiState>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (entity, interaction, btn) in &buttons {
        match interaction {
            Interaction::Pressed => *pressed = Some(entity),
            Interaction::Hovered if *pressed == Some(entity) => {
                *pressed = None;
                if !enabled(btn, &backend.0) {
                    continue;
                }
                match btn.clone() {
                    Btn::Page(page) => {
                        ui.page = page;
                        ui.confirm = None;
                    }
                    Btn::Do(act) => {
                        ui.confirm = None;
                        if backend.0.act(act) {
                            exit.write(AppExit::Success);
                        }
                    }
                    Btn::Remove(id) => {
                        if ui.confirm.as_deref() == Some(id.as_str()) {
                            ui.confirm = None;
                            backend.0.act(Act::Remove(id));
                        } else {
                            ui.confirm = Some(id);
                        }
                    }
                    Btn::Minimize => window.set_minimized(true),
                    Btn::Close => {
                        exit.write(AppExit::Success);
                    }
                }
            }
            _ => {
                if *pressed == Some(entity) {
                    *pressed = None;
                }
            }
        }
    }
}

/// Dragging the title bar moves the window.
fn drag(
    region: Single<&Interaction, With<DragRegion>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if *region.into_inner() != Interaction::None && mouse.just_pressed(MouseButton::Left) {
        window.start_drag_move();
        // The OS move loop swallows the button release.
        mouse.release(MouseButton::Left);
    }
}

fn scroll_notes(
    mut wheel: MessageReader<MouseWheel>,
    mut views: Query<(&RelativeCursorPosition, &mut ScrollPosition, &ComputedNode), With<NotesViewport>>,
    mut lists: Query<(&mut ScrollPosition, &ComputedNode), (With<PackList>, Without<NotesViewport>)>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui: Res<UiState>,
) {
    let mut dy = 0.0;
    for event in wheel.read() {
        dy += match event.unit {
            MouseScrollUnit::Line => event.y * 48.0,
            MouseScrollUnit::Pixel => event.y / window.scale_factor(),
        };
    }
    if dy == 0.0 {
        return;
    }
    let scrolled = |pos: &mut ScrollPosition, node: &ComputedNode| {
        let s = node.inverse_scale_factor();
        let max = ((node.content_size.y - node.size.y) * s).max(0.0);
        pos.0.y = (pos.0.y - dy).clamp(0.0, max);
    };
    match ui.page {
        Page::Play => {
            for (cursor, mut pos, node) in &mut views {
                if cursor.cursor_over() {
                    scrolled(&mut pos, node);
                }
            }
        }
        Page::Packs => {
            for (mut pos, node) in &mut lists {
                scrolled(&mut pos, node);
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync(
    mut commands: Commands,
    backend: Res<Backend>,
    ui: Res<UiState>,
    fonts: Res<Fonts>,
    mut last: Local<Option<View>>,
    mut slots: Query<(&Slot, &mut Text)>,
    mut shown: Query<(&When, &mut Node), Without<Fill>>,
    mut fills: Query<(&Fill, &mut Node), Without<When>>,
    mut dots: Query<&mut BackgroundColor, With<StatusDot>>,
    notes: Single<Entity, With<NotesBody>>,
    list: Single<Entity, With<PackList>>,
    mut notes_scroll: Query<&mut ScrollPosition, With<NotesViewport>>,
) {
    let l = &backend.0;
    let view = View::of(l, &ui);
    if last.as_ref() == Some(&view) {
        return;
    }
    let old = last.take().unwrap_or_default();
    for (slot, mut text) in &mut slots {
        let value = match slot {
            Slot::Status => {
                if view.busy && view.progress >= 0 {
                    format!("{} · {}%", view.status, view.progress)
                } else {
                    view.status.clone()
                }
            }
            Slot::Detail => view.detail.clone(),
            Slot::PlayLabel => if view.running {
                "GAME RUNNING"
            } else if view.ready.is_some() {
                "UPDATE & PLAY"
            } else {
                "PLAY"
            }
            .into(),
            Slot::Version => format!("VERSION {}  ·  BUILD {}", view.version, view.build.to_uppercase()),
            Slot::NotesTag => view.notes_version.clone().unwrap_or_else(|| view.version.clone()),
            Slot::PackStatus => {
                if view.pack_status.is_empty() {
                    "Private .csrpack content. Kept across game updates.".into()
                } else {
                    view.pack_status.clone()
                }
            }
            Slot::PackCount => {
                if view.packs.is_empty() {
                    String::new()
                } else {
                    view.packs.len().to_string()
                }
            }
            Slot::PackErrors => view.pack_errors.join("\n"),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
    for (when, mut node) in &mut shown {
        let visible = match when {
            When::PlayPage => view.page == Page::Play,
            When::PacksPage => view.page == Page::Packs,
            When::Ready => view.ready.is_some(),
            When::Busy => view.busy,
            When::Installing => view.installing,
            When::NotInstalling => !view.installing,
            When::NoPacks => view.packs.is_empty(),
            When::AnyPacks => !view.packs.is_empty(),
            When::HasDetail => !view.detail.is_empty(),
            When::HasPackErrors => !view.pack_errors.is_empty(),
        };
        let display = if visible { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    for (fill, mut node) in &mut fills {
        let (p, on) = match fill {
            Fill::Download => (view.progress, view.busy),
            Fill::Install => (view.install_progress, view.installing),
        };
        if !on {
            node.width = percent(0);
        } else if p >= 0 {
            node.left = px(0);
            node.width = percent(p as f32);
        }
    }
    for mut dot in &mut dots {
        dot.0 = if view.busy || view.ready.is_some() || view.running { RUST } else { TEXT3 };
    }
    if old.notes != view.notes || last.is_none() && old.notes.is_empty() {
        commands.entity(*notes).despawn_children();
        let f = fonts.clone();
        let md = view.notes.clone();
        commands.entity(*notes).with_children(|b| spawn_notes(b, &f, &md));
        for mut pos in &mut notes_scroll {
            pos.0 = Vec2::ZERO;
        }
    }
    if old.packs != view.packs || old.confirm != view.confirm || old.version.is_empty() {
        commands.entity(*list).despawn_children();
        let f = fonts.clone();
        let (packs, confirm) = (view.packs.clone(), view.confirm.clone());
        commands.entity(*list).with_children(|b| spawn_packs(b, &f, &packs, confirm.as_deref()));
    }
    *last = Some(view);
}

/// Hover, pressed, selected and disabled colours for every button.
#[allow(clippy::type_complexity)]
fn style_buttons(
    backend: Res<Backend>,
    ui: Res<UiState>,
    mut buttons: Query<(&Btn, &Look, &Interaction, &mut BackgroundColor, &mut BorderColor, &Children), Without<Underline>>,
    mut texts: Query<&mut TextColor>,
    mut lines: Query<&mut BackgroundColor, (With<Underline>, Without<Btn>)>,
) {
    let l = &backend.0;
    for (btn, look, interaction, mut bg, mut border, children) in &mut buttons {
        let on = enabled(btn, l);
        let sel = selected(btn, l, &ui);
        let hover = on && *interaction == Interaction::Hovered;
        let press = on && *interaction == Interaction::Pressed;
        let clear = Color::NONE;
        let (b, e, t, u) = match look {
            Look::Primary if !on => (Color::srgb(0.14, 0.14, 0.15), clear, TEXT3, clear),
            Look::Primary => (if press { RUST_PRESS } else if hover { RUST_HOVER } else { RUST }, clear, Color::WHITE, clear),
            Look::Ghost => {
                let b = if press { Color::srgba(1.0, 1.0, 1.0, 0.03) } else if hover { Color::srgba(1.0, 1.0, 1.0, 0.06) } else { Color::srgba(0.0, 0.0, 0.0, 0.25) };
                let e = Color::srgba(1.0, 1.0, 1.0, if !on { 0.06 } else if hover { 0.30 } else { 0.15 });
                (b, e, if !on { TEXT3 } else if hover { INK } else { TEXT2 }, clear)
            }
            Look::Tab => (clear, clear, if sel || hover { INK } else { TEXT2 }, if sel { RUST } else { clear }),
            Look::Segment => {
                let b = if sel { Color::srgba(0.769, 0.329, 0.122, 0.16) } else if hover { Color::srgba(1.0, 1.0, 1.0, 0.05) } else { Color::srgba(0.0, 0.0, 0.0, 0.25) };
                let e = if sel { RUST } else { Color::srgba(1.0, 1.0, 1.0, 0.15) };
                (b, e, if !on && !sel { TEXT3 } else if sel || hover { INK } else { TEXT2 }, clear)
            }
            Look::Window => (if press { Color::srgba(1.0, 1.0, 1.0, 0.05) } else if hover { Color::srgba(1.0, 1.0, 1.0, 0.09) } else { clear }, clear, INK, clear),
            Look::Close => (if press { Color::srgb(0.62, 0.13, 0.08) } else if hover { Color::srgb(0.77, 0.17, 0.11) } else { clear }, clear, INK, clear),
            Look::Link => (clear, clear, if !on { TEXT3 } else if hover { INK } else { TEXT2 }, clear),
        };
        bg.set_if_neq(BackgroundColor(b));
        border.set_if_neq(BorderColor::all(e));
        for child in children.iter() {
            if let Ok(mut c) = texts.get_mut(child) {
                c.set_if_neq(TextColor(t));
            }
            if let Ok(mut c) = lines.get_mut(child) {
                c.set_if_neq(BackgroundColor(u));
            }
        }
    }
}

/// The indeterminate progress sweep.
fn animate(backend: Res<Backend>, time: Res<Time>, mut fills: Query<(&Fill, &mut Node)>) {
    let l = &backend.0;
    for (fill, mut node) in &mut fills {
        let unknown = match fill {
            Fill::Download => l.busy && l.progress < 0,
            Fill::Install => l.installing && l.install_progress < 0,
        };
        if unknown {
            let t = (time.elapsed_secs() * 0.8).fract();
            node.left = percent(t * 130.0 - 30.0);
            node.width = percent(30);
        }
    }
}

/// Draws the backdrop and emblems at the window's physical resolution.
fn art_for_scale(
    window: Single<&Window, With<PrimaryWindow>>,
    mut ui: ResMut<UiState>,
    mut images: ResMut<Assets<Image>>,
    mut backdrop: Query<&mut ImageNode, (With<Backdrop>, Without<Emblem>)>,
    mut emblems: Query<(&Emblem, &mut ImageNode), Without<Backdrop>>,
) {
    let s = window.scale_factor();
    if (s - ui.art_scale).abs() < 0.01 {
        return;
    }
    ui.art_scale = s;
    let (w, h) = ((WIDTH as f32 * s).round() as u32, (HEIGHT as f32 * s).round() as u32);
    let handle = images.add(image(art::launcher_backdrop(w, h, s)));
    for mut node in &mut backdrop {
        node.image = handle.clone();
    }
    let mut cache: Vec<(u32, Handle<Image>)> = Vec::new();
    for (emblem, mut node) in &mut emblems {
        let size = (emblem.0 * s).round() as u32;
        let handle = match cache.iter().find(|(px, _)| *px == size) {
            Some((_, h)) => h.clone(),
            None => {
                let h = images.add(image(art::emblem(size)));
                cache.push((size, h.clone()));
                h
            }
        };
        node.image = handle;
    }
}

fn reveal(mut ui: ResMut<UiState>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    if ui.frames < 3 {
        ui.frames += 1;
        if ui.frames == 3 {
            window.visible = true;
        }
    }
}

fn capture_frame(mut commands: Commands, ui: Res<UiState>, capture: Option<ResMut<Capture>>, mut frames: Local<u32>) {
    let Some(mut capture) = capture else { return };
    *frames += 1;
    if capture.requested || *frames < 12 || ui.art_scale == 0.0 {
        return;
    }
    capture.requested = true;
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(capture.path.clone()))
        .observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            exit.write(AppExit::Success);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_keep_headings_bullets_and_paragraphs() {
        let md = "# Counter-Strike Rust 0.5.1\n\nIntro line one\ncontinues here.\n\n## Fixes\n- **Bold** fix with `code`\n  wrapped onto two lines\n* See [the release](https://example.com)\n---\nLast words.";
        assert_eq!(
            parse_notes(md),
            vec![
                Block::Heading("Counter-Strike Rust 0.5.1".into(), 1),
                Block::Para("Intro line one continues here.".into()),
                Block::Heading("Fixes".into(), 2),
                Block::Bullet("Bold fix with code wrapped onto two lines".into()),
                Block::Bullet("See the release".into()),
                Block::Para("Last words.".into()),
            ]
        );
        assert!(parse_notes("  \n\n").is_empty());
    }
}
