//! Classic movies rendered by Ruffle; game mutations are queued for Bevy.
//! The movie thread sees a snapshot of local data and never touches the sim.
use crate::frontend_inventory::{InventoryView, Item};
use crate::{
    GameData, Options,
    session::{AppState, Persistent},
    settings::Settings,
};
use bevy::{
    asset::RenderAssetUsages,
    input::{
        ButtonState,
        keyboard::KeyboardInput,
        mouse::{MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowFocused},
};
use scaleform::{Calls, Engine, Input, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

struct Data {
    fs: Arc<vfs::Vfs>,
    settings: Settings,
    inventory: InventoryView,
    localization: crate::localize::Localization,
    game_modes: kv::Block,
    state: AppState,
    menu_open: bool,
    loading: String,
}
enum Action {
    Sound(String),
    Command(String),
    Convar(String, Value),
    Launch(crate::frontend_game::Selection, bool),
    Notice(String),
}
struct Bridge {
    data: Arc<Mutex<Data>>,
    actions: Arc<Mutex<Vec<Action>>>,
    seen: HashSet<(String, String)>,
    roots: HashMap<String, String>,
    options: crate::frontend_options::Scene,
    offline: crate::frontend_game::OfflineLaunch,
}

fn show_single_player(path: &str, calls: &mut Calls) {
    calls.call(
        "_global.csrBindings",
        "bindApi",
        vec!["SinglePlayerAPI".into(), "StartSinglePlayer".into()],
    );
    calls.call(
        path,
        "InitDialogData",
        vec![false.into(), false.into(), false.into()],
    );
    calls.call(path, "showPanel", vec![]);
    let modes = format!("{path}.Panel.Panel.ModeChooser.TabDefault");
    calls.call(&format!("{modes}.ModeCompetitive"), "Action", vec![]);
    for mode in ["ModeCasual", "ModeWingman", "ModeWargames"] {
        calls.call(&format!("{modes}.{mode}"), "setDisabled", vec![true.into()]);
    }
}

/// Update the menu's pending snapshot and Bevy's saved settings identically.
/// Invalid values must not turn an unanswered native getter into zero volume.
fn apply_setting(settings: &mut Settings, name: &str, value: &Value) -> bool {
    let number = match value {
        Value::Number(number) => Some(*number),
        Value::String(text) => text.parse::<f64>().ok(),
        Value::Bool(value) => Some(u8::from(*value) as f64),
        _ => None,
    };
    let Some(number) = number.filter(|number| number.is_finite()) else {
        return false;
    };
    match name {
        "volume" => settings.volume = number.clamp(0.0, 1.0) as f32,
        "sensitivity" => settings.sensitivity = number.clamp(0.1, 10.0) as f32,
        "m_pitch" => settings.invert_y = number < 0.0,
        _ => return false,
    }
    true
}

fn keyvalues(block: &kv::Block, localization: &crate::localize::Localization) -> Value {
    Value::Object(
        block
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    match value {
                        kv::Value::Str(text) => Value::String(if text.starts_with('#') {
                            localization.get(text).unwrap_or(text).to_string()
                        } else {
                            text.clone()
                        }),
                        kv::Value::Block(child) => keyvalues(child, localization),
                    },
                )
            })
            .collect(),
    )
}

impl Engine for Bridge {
    fn image(&mut self, url: &str) -> Option<Vec<u8>> {
        let id = url.strip_prefix("img://inventory_")?;
        let data = self.data.lock().ok()?;
        let path = data.inventory.image_path(id)?;
        data.fs.read(&path)
    }

    fn call(
        &mut self,
        object: &str,
        method: &str,
        args: &[Value],
        calls: &mut Calls,
    ) -> Option<Value> {
        if self.seen.insert((object.into(), method.into()))
            || matches!(
                method,
                "BasePanelRunCommand" | "OnUpdateValue" | "SendUIEvent"
            )
            || method.starts_with("SetConvar")
        {
            if std::env::var_os("CSR_UI_TRACE").is_some() {
                info!("UI {object}.{method} {args:?}");
            } else {
                debug!("UI {object}.{method} {args:?}");
            }
        }
        let arg = |index: usize| args.get(index).map(Value::str).unwrap_or_default();
        if object == "StartSinglePlayer" {
            let text = match args {
                [Value::String(text)] => Some(text.as_str()),
                _ => None,
            };
            match method {
                "SetCustomBotDifficulty" => {
                    self.offline.difficulty(match args {
                        [Value::Number(value)] => Some(*value),
                        _ => None,
                    });
                    return None;
                }
                "CheckGameSettingsRequirements" => {
                    let data = self.data.lock().unwrap();
                    let result = self.offline.check(&data.game_modes, text);
                    let valid =
                        result.is_ok() && matches!(data.state, AppState::Menu | AppState::Browser);
                    if let Err(error) = result {
                        self.actions.lock().unwrap().push(Action::Notice(error));
                    }
                    if !valid {
                        self.offline.query(None);
                    }
                    return Some(Value::Bool(valid));
                }
                "SetMatchmakingQuery" => {
                    self.offline.query(text);
                    return None;
                }
                "OnOk" => {
                    let selection = self.offline.commit();
                    if args.is_empty()
                        && matches!(
                            self.data.lock().unwrap().state,
                            AppState::Menu | AppState::Browser
                        )
                    {
                        if let Some((selection, no_bots)) = selection {
                            self.actions
                                .lock()
                                .unwrap()
                                .push(Action::Launch(selection, no_bots));
                        }
                    }
                    return None;
                }
                _ => {}
            }
        }
        if method == "OnLoadFinished" && !object.starts_with("CScaleformComponent_") {
            if let Some(path) = arg(0).strip_prefix("mc:") {
                self.roots.insert(object.into(), path.into());
                if object == "MainMenu" {
                    calls.call(path, "onUIShow", vec![]);
                    if !matches!(
                        self.data.lock().unwrap().state,
                        AppState::Menu | AppState::Browser
                    ) {
                        calls.call(path, "hidePanelImmediate", vec![]);
                    }
                } else if object == "PauseMenu" {
                    let data = self.data.lock().unwrap();
                    if data.state == AppState::Match && data.menu_open {
                        calls.call(path, "SetPlayerName", vec!["You".into()]);
                        calls.call(path, "showPanel", vec![]);
                    } else {
                        calls.call(path, "hidePanel", vec![]);
                    }
                }
            }
            return None;
        }
        if method == "OnReady" && object == "StartSinglePlayer" {
            if let Some(path) = self.roots.get(object) {
                show_single_player(path, calls);
            }
            return None;
        }
        if method == "OnReady" && object == "OptionsMenu" {
            if let Some(path) = self.roots.get(object) {
                self.options
                    .show(path, &self.data.lock().unwrap().settings, calls);
            }
            return None;
        }
        if method == "OnReady" && object == "LoadingScreen" {
            if let Some(path) = self.roots.get(object) {
                let data = self.data.lock().unwrap();
                if data.state == AppState::Loading {
                    calls.call(path, "setLoadingText", vec![data.loading.clone().into()]);
                    calls.call(path, "showPanel", vec![]);
                    calls.call(path, "showLoadinginfo", vec![]);
                } else {
                    calls.call(path, "hidePanel", vec![]);
                }
            }
            return None;
        }
        if method == "BasePanelRunCommand" {
            let command = arg(0);
            if arg(1) == "bHidePanel" {
                calls.call("_global.MainMenuMovie", "hidePanelImmediate", vec![]);
            }
            match command.as_str() {
                "OpenCreateSinglePlayerGameDialog" => {
                    if let Some(path) = self.roots.get("StartSinglePlayer") {
                        show_single_player(path, calls);
                    } else {
                        calls.request_element("StartSinglePlayer");
                    }
                }
                "OpenVideoSettingsDialog"
                | "OpenAudioSettingsDialog"
                | "OpenGameSettingsDialog"
                | "OpenMouseDialog"
                | "OpenKeyboardMouseDialog" => {
                    let data = self.data.lock().unwrap();
                    self.options = crate::frontend_options::Scene::load(
                        &data.fs,
                        &command,
                        &data.localization,
                    );
                    if let Some(path) = self.roots.get("OptionsMenu") {
                        self.options.show(path, &data.settings, calls);
                    } else {
                        calls.request_element("OptionsMenu");
                    }
                }
                _ => self.actions.lock().unwrap().push(Action::Command(command)),
            }
            return None;
        }
        let object = object
            .strip_prefix("CScaleformComponent_")
            .unwrap_or(object);
        let mut data = self.data.lock().unwrap();
        if matches!(object, "Inventory" | "ItemData") {
            if let Some(value) = data.inventory.call(method, args) {
                return Some(value);
            }
        }
        match (object, method) {
            ("OptionsMenu", "OnUpdateValue") => {
                if let Some((name, value)) = self.options.value(
                    args.first()?,
                    args.get(1)?,
                    self.roots.get("OptionsMenu")?,
                    calls,
                ) {
                    apply_setting(&mut data.settings, &name, &value);
                    self.actions
                        .lock()
                        .unwrap()
                        .push(Action::Convar(name, value));
                }
                None
            }
            ("OptionsMenu", "OnLayoutComplete") => {
                if let Some(path) = self.roots.get("OptionsMenu") {
                    self.options.layout_ready(path, &data.settings, calls);
                }
                None
            }
            ("OptionsMenu", "GetTotalOptionsSlots") => Some(self.options.count().into()),
            ("OptionsMenu", "GetSafeZoneXMin") => Some(0.8.into()),
            ("GameInterface", "GetConvarNumberMin" | "GetConvarNumberMax") => {
                let (min, max) = self.options.bounds(&arg(0)).or_else(|| {
                    matches!(arg(0).as_str(), "safezonex" | "safezoney").then_some((0.8, 1.0))
                })?;
                Some(if method.ends_with("Min") { min } else { max }.into())
            }
            ("GameInterface", "MakeStringSafe") => Some(arg(0).into()),
            ("GameInterface", "LoadKVFile") => {
                let name = arg(0).to_ascii_lowercase();
                if name == "gamemodes.txt" {
                    return Some(keyvalues(&data.game_modes, &data.localization));
                }
                if !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.'))
                {
                    return Some(Value::Null);
                }
                let text = data.fs.read_string(&name)?;
                let doc = kv::parse(&text).ok()?;
                Some(keyvalues(
                    doc.entries
                        .first()
                        .and_then(|(_, value)| value.as_block())
                        .unwrap_or(&doc),
                    &data.localization,
                ))
            }
            ("GameInterface", "PlaySound") => {
                self.actions.lock().unwrap().push(Action::Sound(arg(0)));
                None
            }
            ("GameInterface", "SetConvar")
            | ("GameInterface", "SetConvarNumber")
            | ("GameInterface", "SetConvarString")
            | ("GameInterface", "SetConvarBoolean") => {
                self.actions.lock().unwrap().push(Action::Convar(
                    arg(0),
                    args.get(1).cloned().unwrap_or(Value::Undefined),
                ));
                None
            }
            ("GameInterface", "GetConvarNumber") => Some(Value::Number(match arg(0).as_str() {
                "volume" => data.settings.volume as f64,
                "sensitivity" => data.settings.sensitivity as f64,
                "m_pitch" => {
                    if data.settings.invert_y {
                        -0.022
                    } else {
                        0.022
                    }
                }
                "safezonex" | "safezoney" => 1.0,
                "hud_scaling" => 0.85,
                "game_type" => 0.0,
                "game_mode" => 1.0,
                _ => return None,
            })),
            ("MyPersona", "GetXuid") => Some("0".into()),
            ("MyPersona", "GetXpPerLevel") => Some(5000.0.into()),
            ("MyPersona", "GetXp") | ("MyPersona", "GetLevel") => Some(0.0.into()),
            ("FriendsList", "GetFriendName") if arg(0) == "0" => Some("You".into()),
            ("GameTypes", "GetGameModeFromInts") => Some(
                crate::frontend_game::mode_name(
                    &data.game_modes,
                    args.first().map_or(-1.0, Value::num),
                    args.get(1).map_or(-1.0, Value::num),
                )
                .into(),
            ),
            ("GameTypes", "GetMapGroupAttribute") | ("GameTypes", "GetMapAttribute") => {
                let section = if method == "GetMapGroupAttribute" {
                    "mapgroups"
                } else {
                    "maps"
                };
                let value = data
                    .game_modes
                    .block(section)
                    .and_then(|groups| groups.block(&arg(0)))
                    .and_then(|group| group.str(&arg(1)))
                    .unwrap_or("");
                Some(data.localization.get(value).unwrap_or(value).into())
            }
            ("GameTypes", "GetMapGroupAttributeSubKeys") => Some(
                crate::frontend_game::map_group_subkeys(&data.game_modes, &arg(0), &arg(1)).into(),
            ),
            _ => None,
        }
    }
}

#[derive(Resource)]
pub struct Frontend {
    movie: Option<Mutex<scaleform::Menu>>,
    data: Arc<Mutex<Data>>,
    actions: Arc<Mutex<Vec<Action>>>,
    image: Handle<Image>,
    root: Entity,
    camera: Entity,
    visible: bool,
    pub ready: bool,
    failed: Option<String>,
    viewport: (u32, u32),
    scene: Option<AppState>,
    notice: Entity,
    notice_text: Entity,
    cancel: Entity,
}

impl Frontend {
    fn send(&self, input: Input) {
        if let Some(movie) = &self.movie {
            movie.lock().unwrap().send(input);
        }
    }
    fn poll(&self) -> Option<scaleform::Event> {
        self.movie.as_ref()?.lock().unwrap().poll_event()
    }
    fn frame(&self) -> Option<scaleform::Frame> {
        self.movie.as_ref()?.lock().unwrap().take_frame()
    }
}

pub fn native_enabled(front: Option<Res<Frontend>>) -> bool {
    front.is_none_or(|front| front.failed.is_some())
}

pub(crate) fn setup(
    mut commands: Commands,
    data: Res<GameData>,
    settings: Res<Settings>,
    options: Res<Options>,
    menu: Res<crate::menu::Menu>,
    mut images: ResMut<Assets<Image>>,
    art: Res<crate::ui_assets::UiAssets>,
) {
    let installed = menu.maps.iter().cloned().collect();
    let shared = Arc::new(Mutex::new(Data {
        fs: data.vfs.clone(),
        settings: settings.clone(),
        inventory: InventoryView::new(&data.vfs),
        localization: crate::localize::Localization::load(&data.vfs),
        game_modes: crate::frontend_game::load(&data.vfs, &installed),
        state: AppState::Menu,
        menu_open: menu.open(),
        loading: String::new(),
    }));
    let actions = Arc::new(Mutex::new(vec![]));
    let image = images.add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    let camera = commands
        .spawn((
            Persistent,
            Camera2d,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Msaa::Off,
        ))
        .id();
    let root = commands
        .spawn((
            Persistent,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode::new(image.clone()),
            UiTargetCamera(camera),
            GlobalZIndex(100),
        ))
        .id();
    let notice = commands
        .spawn((
            Persistent,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(24.0),
                left: percent(15),
                width: percent(70),
                padding: UiRect::all(px(12.0)),
                column_gap: px(20.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.06, 0.08, 0.96)),
            UiTargetCamera(camera),
            GlobalZIndex(102),
        ))
        .id();
    let notice_text = commands
        .spawn((
            Persistent,
            Text::new(""),
            TextFont {
                font: bevy::text::FontSource::Handle(art.regular.clone()),
                font_size: bevy::text::FontSize::Px(20.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ))
        .id();
    let cancel = commands
        .spawn((
            Persistent,
            Button,
            Node {
                padding: UiRect::axes(px(18.0), px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.15, 0.23, 0.3)),
        ))
        .id();
    let cancel_text = commands
        .spawn((
            Persistent,
            Text::new("Cancel"),
            TextFont {
                font: bevy::text::FontSource::Handle(art.regular.clone()),
                font_size: bevy::text::FontSize::Px(20.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(cancel).add_child(cancel_text);
    commands.entity(notice).add_children(&[notice_text, cancel]);
    commands.insert_resource(Frontend {
        movie: None,
        data: shared,
        actions,
        image,
        root,
        camera,
        visible: false,
        ready: false,
        failed: None,
        viewport: (options.0.width, options.0.height),
        scene: None,
        notice,
        notice_text,
        cancel,
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    mut front: ResMut<Frontend>,
    mut images: ResMut<Assets<Image>>,
    (mut nodes, mut cameras, mut texts): (
        Query<&mut Node>,
        Query<(&mut Camera, &mut Msaa)>,
        Query<&mut Text>,
    ),
    mut menu: ResMut<crate::menu::Menu>,
    state: Res<State<AppState>>,
    mut settings: ResMut<Settings>,
    (data, loader, options, request): (
        Res<GameData>,
        Res<crate::session::Loader>,
        Res<Options>,
        Res<crate::menu::LoadRequest>,
    ),
    loc: Res<crate::localize::Localization>,
    (inventory, practice): (
        Res<crate::cosmetics::Inventory>,
        Res<crate::inventory::Inventory>,
    ),
    catalog: Res<crate::cosmetics::Catalog>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    mut sounds: ResMut<crate::audio::SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
) {
    {
        let mut snapshot = front.data.lock().unwrap();
        snapshot.state = *state.get();
        snapshot.menu_open = menu.open();
        snapshot.loading = loader.status.clone();
    }
    if settings.is_changed() {
        front.data.lock().unwrap().settings = settings.clone();
    }
    if inventory.is_changed() || catalog.is_changed() || practice.is_changed() {
        let mut items = Vec::new();
        for weapon in &data.catalog.weapons {
            if !crate::inventory::visible(weapon, 0) {
                continue;
            }
            items.push(Item {
                id: (0x8000_0000_0000_0000u64 | weapon.def_index as u64).to_string(),
                name: loc.weapon_name(weapon),
                icon: format!("econ/weapons/base_weapons/{}", weapon.class_name),
                definition: weapon.def_index,
                rarity: 0,
                category: category(weapon.class),
                base: true,
                equipped: practice.loadouts.contains(&weapon.class_name),
                stickers: 0,
            });
        }
        for skin in &inventory.skins {
            if let Some(finish) = catalog
                .finishes
                .iter()
                .find(|finish| finish.id == skin.paint && finish.weapon == skin.weapon)
            {
                let definition = data
                    .catalog
                    .get(&skin.weapon)
                    .map_or(0, |weapon| weapon.def_index);
                items.push(Item {
                    id: skin.serial.to_string(),
                    name: skin.display_name(finish),
                    icon: finish.icon.trim_end_matches(".png").to_string(),
                    definition,
                    rarity: finish.rarity,
                    category: data
                        .catalog
                        .get(&skin.weapon)
                        .map_or("melee", |weapon| category(weapon.class)),
                    base: false,
                    equipped: inventory
                        .equipped
                        .values()
                        .any(|serial| *serial == skin.serial),
                    stickers: skin.stickers.iter().flatten().count() as u32,
                });
            }
        }
        for (&definition, &count) in &inventory.cases {
            if let Some(case) = catalog.cases.iter().find(|case| case.id == definition) {
                for copy in 0..count {
                    items.push(Item {
                        id: format!("case:{definition}:{copy}"),
                        name: case.name.clone(),
                        icon: case.icon.trim_end_matches(".png").to_string(),
                        definition,
                        rarity: 0,
                        category: "not_equipment",
                        base: false,
                        equipped: false,
                        stickers: 0,
                    });
                }
            }
        }
        front.data.lock().unwrap().inventory.replace(items);
    }
    let visible = matches!(state.get(), AppState::Menu | AppState::Browser)
        || (*state.get() == AppState::Match && menu.open())
        || (*state.get() == AppState::Loading
            && (front.movie.is_some() || (!options.0.direct && options.0.screenshot.is_none())));
    // A UI-only camera must not overwrite a match's resolved color target.
    // During play the existing viewmodel camera continues drawing the HUD.
    if let Ok((mut camera, mut msaa)) = cameras.get_mut(front.camera) {
        camera.is_active = *state.get() != AppState::Match || menu.open();
        *msaa = if *state.get() == AppState::Match && settings.msaa {
            Msaa::Sample4
        } else {
            Msaa::Off
        };
    }
    if visible && front.movie.is_none() && front.failed.is_none() {
        let mut elements = vec!["MainMenu".into()];
        if *state.get() == AppState::Match {
            elements.push("PauseMenu".into());
        }
        front.movie = Some(Mutex::new(scaleform::Menu::spawn(
            scaleform::Config {
                game_dir: data.game_dir.clone(),
                width: window.physical_width(),
                height: window.physical_height(),
                language: "english".into(),
                elements,
            },
            Box::new(Bridge {
                data: front.data.clone(),
                actions: front.actions.clone(),
                seen: HashSet::new(),
                roots: HashMap::new(),
                options: Default::default(),
                offline: Default::default(),
            }),
        )));
    }
    if front.visible != visible {
        front.send(Input::SetActive(visible));
        front.visible = visible;
        if visible && *state.get() == AppState::Menu && front.ready {
            front.send(Input::Call {
                path: "_global.MainMenuMovie".into(),
                function: "showPanel".into(),
                args: vec![],
            });
        }
        if visible && *state.get() == AppState::Match && front.ready {
            front.send(Input::RequestElement("PauseMenu".into()));
            front.send(Input::Call {
                path: "_level100".into(),
                function: "showPanel".into(),
                args: vec![],
            });
        } else if *state.get() == AppState::Match && front.ready {
            front.send(Input::Call {
                path: "_level100".into(),
                function: "hidePanel".into(),
                args: vec![],
            });
        }
    }
    if front.scene != Some(*state.get()) {
        if *state.get() != AppState::Match {
            front.send(Input::Call {
                path: "_level100".into(),
                function: "hidePanel".into(),
                args: vec![],
            });
        }
        if *state.get() == AppState::Loading {
            front.send(Input::Call {
                path: "_global.MainMenuMovie".into(),
                function: "hidePanelImmediate".into(),
                args: vec![],
            });
            front.send(Input::Call {
                path: "_global.SinglePlayerMovie".into(),
                function: "hidePanel".into(),
                args: vec![true.into()],
            });
            front.send(Input::Call {
                path: "_level21".into(),
                function: "HidePanel".into(),
                args: vec![],
            });
            front.send(Input::RequestElement("LoadingScreen".into()));
            front.send(Input::Call {
                path: "_level1000".into(),
                function: "setLoadingText".into(),
                args: vec![loader.status.clone().into()],
            });
            front.send(Input::Call {
                path: "_level1000".into(),
                function: "showPanel".into(),
                args: vec![],
            });
        } else {
            front.send(Input::Call {
                path: "_level1000".into(),
                function: "hidePanel".into(),
                args: vec![],
            });
            if *state.get() == AppState::Menu {
                front.send(Input::Call {
                    path: "_global.MainMenuMovie".into(),
                    function: "showPanel".into(),
                    args: vec![],
                });
            }
        }
        front.scene = Some(*state.get());
    }
    let loading = *state.get() == AppState::Loading;
    if let Ok(mut node) = nodes.get_mut(front.notice) {
        node.display = if loading || (visible && !menu.status.is_empty()) {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut node) = nodes.get_mut(front.cancel) {
        node.display = if loading {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut text) = texts.get_mut(front.notice_text) {
        text.0 = if loading {
            loader.status.clone()
        } else {
            menu.status.clone()
        };
    }
    if let Ok(mut root) = nodes.get_mut(front.root) {
        root.display = if visible && front.failed.is_none() {
            Display::Flex
        } else {
            Display::None
        };
    }
    let size = (window.physical_width(), window.physical_height());
    if size != front.viewport {
        front.send(Input::Resize {
            width: size.0,
            height: size.1,
        });
        front.viewport = size;
    }
    loop {
        let event = front.poll();
        match event {
            Some(scaleform::Event::Loaded(name)) => {
                front.ready |= name == "MainMenu";
                info!("Classic UI loaded {name}");
            }
            Some(scaleform::Event::Failed(error)) => {
                error!("Classic UI: {error}");
                menu.status = error.clone();
                front.failed = Some(error);
            }
            Some(scaleform::Event::Inspected { path, properties }) => {
                debug!("UI API {path}: {properties:?}")
            }
            None => break,
        }
    }
    let frame = front.frame();
    if let Some(frame) = frame {
        if let Some(mut image) = images.get_mut(&front.image) {
            image.texture_descriptor.size = Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            };
            image.data = Some(frame.rgba);
        }
    }
    let actions = std::mem::take(&mut *front.actions.lock().unwrap());
    for action in actions {
        match action {
            Action::Launch(selection, no_bots) => {
                if matches!(state.get(), AppState::Menu | AppState::Browser) {
                    let mut selected = options.0.clone();
                    selected.map = selection.map;
                    selected.deathmatch = selection.deathmatch;
                    if no_bots {
                        selected.bots = 0;
                    }
                    menu.status.clear();
                    *request.0.lock().unwrap() = Some(selected);
                }
            }
            Action::Notice(message) => menu.status = message,
            Action::Sound(path) => {
                if let Some(entity) =
                    sounds.play_ui_wave(&mut commands, &data.vfs, &mut sources, &path)
                {
                    commands.entity(entity).insert(Persistent);
                }
            }
            Action::Command(command) => match command.to_ascii_lowercase().as_str() {
                "quit" | "quitgame" => {
                    exit.write(AppExit::Success);
                }
                "disconnect" => {
                    next.set(AppState::Menu);
                }
                "resume" | "resumegame" => menu.page = None,
                "openserverbrowser" => {
                    next.set(AppState::Browser);
                }
                _ => info!("Classic UI command: {command}"),
            },
            Action::Convar(name, value) => {
                if !apply_setting(&mut settings, &name, &value) {
                    info!("Classic UI setting is not implemented or invalid: {name}={value:?}");
                    continue;
                }
                if let Err(error) = settings.save() {
                    menu.status = format!("Could not save settings: {error}");
                }
            }
        }
    }
}

/// Loading cancellation is available even before the movie has finished loading.
pub(crate) fn cancel_loading(
    front: Res<Frontend>,
    state: Res<State<AppState>>,
    keys: Res<ButtonInput<KeyCode>>,
    interactions: Query<&Interaction>,
    mut next: ResMut<NextState<AppState>>,
    mut menu: ResMut<crate::menu::Menu>,
) {
    if *state.get() == AppState::Loading
        && (keys.just_pressed(KeyCode::Escape)
            || interactions.get(front.cancel) == Ok(&Interaction::Pressed))
    {
        menu.status.clear();
        next.set(AppState::Menu);
    }
}

pub(crate) fn input(
    front: Res<Frontend>,
    enabled: Res<crate::player::InputEnabled>,
    state: Res<State<AppState>>,
    menu: Res<crate::menu::Menu>,
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut keys: MessageReader<KeyboardInput>,
    mut wheel: MessageReader<MouseWheel>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    buttons: Res<ButtonInput<KeyCode>>,
    mut focus: MessageReader<WindowFocused>,
) {
    if !enabled.0 || !front.visible || front.failed.is_some() {
        keys.clear();
        wheel.clear();
        return;
    }
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
    let Some(movie) = &front.movie else {
        keys.clear();
        wheel.clear();
        return;
    };
    let movie = movie.lock().unwrap();
    if let Some(position) = window.cursor_position() {
        let scale = window.scale_factor() as f64;
        let (x, y) = (position.x as f64 * scale, position.y as f64 * scale);
        movie.send(Input::MouseMove { x, y });
        for (button, mapped) in [
            (MouseButton::Left, scaleform::MouseButton::Left),
            (MouseButton::Right, scaleform::MouseButton::Right),
            (MouseButton::Middle, scaleform::MouseButton::Middle),
        ] {
            if mouse.just_pressed(button) {
                movie.send(Input::MouseDown {
                    x,
                    y,
                    button: mapped,
                });
            }
            if mouse.just_released(button) {
                movie.send(Input::MouseUp {
                    x,
                    y,
                    button: mapped,
                });
            }
        }
    }
    for event in focus.read() {
        movie.send(Input::Focus(event.focused));
    }
    for event in wheel.read() {
        movie.send(Input::MouseWheel {
            lines: event.y as f64
                / if event.unit == MouseScrollUnit::Pixel {
                    40.0
                } else {
                    1.0
                },
        });
    }
    let control = buttons.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = buttons.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for event in keys.read() {
        if event.key_code == KeyCode::Escape && *state.get() == AppState::Match && !menu.open() {
            continue;
        }
        if let Some((name, vkey)) = key(event.key_code) {
            if event.state == ButtonState::Pressed {
                movie.send(Input::Edit {
                    name: name.clone(),
                    control,
                    shift,
                });
            }
            movie.send(Input::Key {
                name,
                vkey,
                down: event.state == ButtonState::Pressed,
            });
        }
        if event.state == ButtonState::Pressed && !control {
            if let Some(text) = &event.text {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    movie.send(Input::Char(ch));
                }
            }
        }
    }
}

fn category(class: weapons::WeaponClass) -> &'static str {
    use weapons::WeaponClass::*;
    match class {
        Pistol => "secondary",
        Smg => "smg",
        Rifle | Sniper => "rifle",
        Shotgun | MachineGun => "heavy",
        Knife => "melee",
        _ => "not_equipment",
    }
}

fn key(code: KeyCode) -> Option<(String, u32)> {
    let debug = format!("{code:?}");
    if let Some(letter) = debug.strip_prefix("Key").filter(|s| s.len() == 1) {
        return Some((format!("KEY_{letter}"), letter.as_bytes()[0] as u32));
    }
    if let Some(digit) = debug.strip_prefix("Digit").filter(|s| s.len() == 1) {
        return Some((format!("KEY_{digit}"), digit.as_bytes()[0] as u32));
    }
    let (name, vk) = match code {
        KeyCode::Escape => ("ESCAPE", 27),
        KeyCode::Enter => ("ENTER", 13),
        KeyCode::Space => ("SPACE", 32),
        KeyCode::Backspace => ("BACKSPACE", 8),
        KeyCode::Tab => ("TAB", 9),
        KeyCode::ArrowLeft => ("LEFT", 37),
        KeyCode::ArrowUp => ("UP", 38),
        KeyCode::ArrowRight => ("RIGHT", 39),
        KeyCode::ArrowDown => ("DOWN", 40),
        KeyCode::ShiftLeft => ("LSHIFT", 16),
        KeyCode::ShiftRight => ("RSHIFT", 16),
        KeyCode::ControlLeft => ("LCONTROL", 17),
        KeyCode::ControlRight => ("RCONTROL", 17),
        KeyCode::AltLeft => ("LALT", 18),
        KeyCode::AltRight => ("RALT", 18),
        KeyCode::Home => ("HOME", 36),
        KeyCode::End => ("END", 35),
        KeyCode::Delete => ("DELETE", 46),
        _ => return None,
    };
    Some((format!("KEY_{name}"), vk))
}

pub(crate) fn capture_menu(
    mut frame: Local<u32>,
    mut step: Local<usize>,
    mut loaded_at: Local<Option<std::time::Instant>>,
    mut started_at: Local<Option<std::time::Instant>>,
    front: Res<Frontend>,
    opts: Res<Options>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let started = started_at.get_or_insert_with(std::time::Instant::now);
    if front.failed.is_some() || started.elapsed() > std::time::Duration::from_secs(60) {
        error!(
            "Classic menu capture failed: {}",
            front.failed.as_deref().unwrap_or("load/capture timeout")
        );
        exit.write(AppExit::error());
        return;
    }
    if !front.ready {
        return;
    }
    let ready = loaded_at.get_or_insert_with(std::time::Instant::now);
    // Hidden smoke runs can render hundreds of Bevy frames before Ruffle's
    // next 30 Hz frame. Let dependent fonts/images finish loading first.
    if ready.elapsed() < std::time::Duration::from_secs(5) {
        return;
    }
    let points: Vec<(f64, f64)> = match std::env::var("CSR_MENU_PROBE").as_deref() {
        Ok("inventory") => vec![(724.0, 26.0)],
        Ok("play") => vec![(486.0, 26.0)],
        Ok("offline") => vec![(486.0, 26.0), (503.0, 264.0), (1450.0, 206.0)],
        Ok("video") => vec![(1412.0, 26.0), (1450.0, 222.0)],
        _ => vec![],
    };
    let points: Vec<(String, f64, f64)> = std::env::var("CSR_MENU_CLICKS")
        .ok()
        .map(|text| {
            text.split(';')
                .filter_map(|point| {
                    let (kind, point) = point.split_once(':').unwrap_or(("click", point));
                    if !matches!(kind, "click" | "down" | "move" | "up") {
                        return None;
                    }
                    let (x, y) = point.split_once(',')?;
                    let (x, y): (f64, f64) = (x.parse().ok()?, y.parse().ok()?);
                    (x.is_finite() && y.is_finite()).then(|| (kind.into(), x, y))
                })
                .collect()
        })
        .unwrap_or_else(|| {
            points
                .into_iter()
                .map(|(x, y)| ("click".into(), x, y))
                .collect()
        });
    if *step < points.len() && ready.elapsed().as_secs_f64() >= 5.0 + *step as f64 * 3.0 {
        let (kind, x, y) = &points[*step];
        let (x, y) = (
            x * front.viewport.0 as f64 / 1920.0,
            y * front.viewport.1 as f64 / 1080.0,
        );
        if matches!(kind.as_str(), "click" | "move" | "down") {
            front.send(Input::MouseMove { x, y });
        }
        if matches!(kind.as_str(), "click" | "down") {
            front.send(Input::MouseDown {
                x,
                y,
                button: scaleform::MouseButton::Left,
            });
        }
        if matches!(kind.as_str(), "click" | "up") {
            front.send(Input::MouseUp {
                x,
                y,
                button: scaleform::MouseButton::Left,
            });
        }
        *step += 1;
    }
    if ready.elapsed().as_secs_f64() < 5.0 + points.len() as f64 * 3.0 {
        return;
    }
    *frame += 1;
    if *frame == opts.0.frames {
        if let Some(path) = &opts.0.screenshot {
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
        }
    }
    if *frame == opts.0.frames + 30 {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unanswered_or_malformed_native_values_cannot_reset_saved_settings() {
        let mut settings = Settings::default();
        let original = settings.clone();
        for value in [
            Value::Undefined,
            Value::Null,
            "not a number".into(),
            f64::NAN.into(),
            f64::INFINITY.into(),
        ] {
            assert!(!apply_setting(&mut settings, "volume", &value));
            assert_eq!(settings, original);
        }
        assert!(apply_setting(&mut settings, "volume", &"0.76".into()));
        assert_eq!(settings.volume, 0.76);
        assert!(apply_setting(&mut settings, "m_pitch", &(-0.022).into()));
        assert!(settings.invert_y);
    }
}
