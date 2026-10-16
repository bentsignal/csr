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
    localization: Arc<crate::localize::Localization>,
    game_modes: kv::Block,
    state: AppState,
    menu_open: bool,
    loading: String,
    loading_map: Option<LoadingContext>,
    player_name: String,
    movie_ready: HashSet<String>,
    home_bootstrapped: bool,
    home_retired: bool,
    pause_surface: crate::frontend_pause_surface::Tracker,
    deferred_pause: crate::frontend_pause_surface::DeferredPause,
}

impl Data {
    fn fork_worker(&self) -> Self {
        Self { fs: self.fs.clone(), settings: self.settings.clone(), inventory: self.inventory.clone(),
            localization: self.localization.clone(), game_modes: self.game_modes.clone(),
            state: self.state, menu_open: self.menu_open, loading: self.loading.clone(),
            loading_map: self.loading_map.clone(), player_name: self.player_name.clone(),
            movie_ready: HashSet::new(), home_bootstrapped: false, home_retired: false,
            pause_surface: Default::default(), deferred_pause: Default::default() }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct LoadingContext {
    selected: crate::session::LoadingMap,
    title: String,
    radar_only: bool,
}
impl LoadingContext {
    fn installed(selected: crate::session::LoadingMap, data: &Data) -> Option<Self> {
        let map = data.game_modes.block("maps")?.block(&selected.name)?;
        let title = map.str("nameID")?;
        let title = data.localization.get(title).unwrap_or(title).to_owned();
        // Photo compatibility is deferred. The installed radar and icon are
        // sufficient for the explicit radar surface. Unknown art keeps
        // the engine progress/cancel surface, never an invented map alias.
        if !data.fs.exists(&format!("resource/overviews/{}_radar.dds", selected.name))
            || !data.fs.exists(&format!("resource/flash/images/map_icons/collection_icon_{}.png", selected.name)) { return None; }
        let radar_only = !data.fs.exists(&format!("maps/{}_loading.jpg", selected.name));
        Some(Self { selected, title, radar_only })
    }

    fn arguments(&self) -> Vec<Value> {
        // Observed map/title positions. The other argument domains remain
        // unknown; never label a remote game Competitive by default.
        vec![self.selected.name.clone().into(), Value::Null, self.title.clone().into(), Value::Null, Value::Null, Value::Null]
    }
}
enum Action {
    Sound(String),
    Command(String),
    Convar(String, Value),
    Launch(crate::frontend_game::Selection, bool),
    Notice(String),
    Host(crate::frontend_lobby::Start, String),
    LobbyHome,
}
/// Park immediately rejects in-flight old actions. Worker-ordered resume
/// advances the epoch only after old queued input has been cancelled.
struct ActionQueue {
    pending: Vec<Action>,
    epoch: u64,
    active: bool,
}
impl Default for ActionQueue {
    fn default() -> Self { Self { pending: Vec::new(), epoch: 0, active: true } }
}
impl std::ops::Deref for ActionQueue {
    type Target = Vec<Action>;
    fn deref(&self) -> &Self::Target { &self.pending }
}
impl ActionQueue {
    fn current(&self) -> Option<u64> { self.active.then_some(self.epoch) }
    fn push_at(&mut self, epoch: Option<u64>, action: Action) {
        if self.active && epoch == Some(self.epoch) { self.pending.push(action); }
    }
    fn park(&mut self) { self.active = false; self.pending.clear(); }
    fn resume(&mut self) -> bool {
        self.park();
        let Some(epoch) = self.epoch.checked_add(1) else { return false; };
        self.epoch = epoch;
        self.active = true;
        true
    }
    fn take(&mut self) -> Vec<Action> { std::mem::take(&mut self.pending) }
}

struct Bridge {
    dispatch_epoch: Option<u64>,
    data: Arc<Mutex<Data>>,
    actions: Arc<Mutex<ActionQueue>>,
    seen: HashMap<String, HashSet<String>>,
    roots: HashMap<String, String>,
    requested: HashSet<String>,
    options: crate::frontend_options::Scene,
    offline: crate::frontend_game::OfflineLaunch,
    offline_open: bool,
    quit: QuitConfirmation,
    lobby: Arc<Mutex<crate::frontend_lobby::Scene>>,
}

/// A confirmation belongs to one opening, not to the cached MessageBox movie.
/// Retained callbacks from a cancelled opening must never quit a later scene.
#[derive(Default)]
struct QuitConfirmation { epoch: u64, active: bool, shown: bool }
impl QuitConfirmation {
    fn begin(&mut self) -> bool {
        if self.active { return false; }
        let Some(epoch) = self.epoch.checked_add(1) else { return false; };
        self.epoch = epoch;
        self.active = true;
        true
    }
    fn receiver(&self) -> String { format!("OwnedQuit{}", self.epoch) }
    fn owns(&self, receiver: &str) -> bool { self.active && self.shown && receiver == self.receiver() }
    fn cancel(&mut self) { self.active = false; self.shown = false; }
}

fn show_single_player(path: &str, calls: &mut Calls) {
    calls.call("_global.csrBindings", "releaseOwnedLifecycle", vec![Value::String(path.to_string())]);
    calls.set(path, "_visible", true.into());
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
    // MainMenu.hidePanelImmediate disables its native power button. Keep that
    // original control usable above the owned picker, as requested; its public
    // setter and actual Quit callback were observed together at runtime.
    calls.call("_global.MainMenuMovie.Panel.SelectPanel.MainMenuTopBar.QuitButton", "setDisabled", vec![false.into()]);
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
    settings.set_convar_number(name, number)
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

impl Bridge {
    fn push_action(&self, action: Action) { self.actions.lock().unwrap().push_at(self.dispatch_epoch, action); }

    fn reset_navigation(&mut self, calls: &mut Calls) {
        self.close_quit(calls);
        self.close_options(calls);
        self.offline_open = false;
        self.offline.query(None);
        self.lobby.lock().unwrap().cancel();
        for element in ["OptionsMenu", "StartSinglePlayer", "LobbyBrowser", "LobbyScreen"] {
            if let Some(path) = self.roots.get(element) {
                if element != "OptionsMenu" {
                    calls.call("_global.csrBindings", "releaseOwnedLifecycle", vec![path.clone().into()]);
                }
                if element == "StartSinglePlayer" {
                    calls.call(path, "hidePanel", vec![true.into()]);
                    calls.call(&format!("{path}.Panel.Panel.BotDiffPanel"), "HidePanel", vec![]);
                }
                calls.set(path, "_visible", false.into());
            }
        }
        if let Some(path) = self.roots.get("PauseMenu") {
            calls.call(path, "hidePanel", vec![]);
            calls.set(path, "_visible", false.into());
        }
    }
    /// A navigation command supersedes Options even when its movie is still
    /// loading. Revoke the pending show before hiding the registered root so
    /// a late OnReady cannot put an old settings page over the new destination.
    fn close_options(&mut self, calls: &mut Calls) {
        let mut data = self.data.lock().unwrap();
        data.pause_surface.options_cancelled();
        data.deferred_pause.cancel_pending();
        if let Some(path) = self.roots.get("OptionsMenu") {
            calls.set(path, "_visible", false.into());
        }
    }

    fn close_quit(&mut self, calls: &mut Calls) {
        let was_active = self.quit.active;
        self.quit.cancel();
        if let Some(path) = self.roots.get("MessageBox") {
            if was_active { calls.call(path, "hidePanelImmediate", vec![]); }
            calls.set(path, "_visible", false.into());
            calls.call("_global.csrBindings", "releaseOwnedQuit", vec![path.clone().into()]);
        }
    }

    fn show_quit(&mut self, calls: &mut Calls) {
        let Some(path) = self.roots.get("MessageBox") else { return; };
        let data = self.data.lock().unwrap();
        if !self.quit.active || !matches!(data.state, AppState::Menu | AppState::Browser)
            || self.actions.lock().unwrap().current().is_none() { return; }
        let text = |token: &str| data.localization.get(token).unwrap_or(token).to_owned();
        calls.call("_global.csrBindings", "bindOwnedQuit", vec![path.clone().into(), self.quit.receiver().into()]);
        // These are the installed MessageBox's observed public setters. Keep
        // its native geometry, flags and navigation; do not guess Init's order.
        calls.call(path, "SetTitle", vec![text("#SFUI_MainMenu_ExitGameConfirmation_Title").into()]);
        calls.call(path, "SetMessage", vec![text("#SFUI_MainMenu_ExitGameConfirmation_Message").into()]);
        calls.call(path, "SetButtonLegend", vec![text("#SFUI_MainMenu_ExitGameConfirmation_Navigation").into()]);
        calls.call(path, "SetFlagOk", vec![true.into()]);
        calls.call(path, "SetFlagCancel", vec![true.into()]);
        calls.call(path, "SetFlagTertiary", vec![false.into()]);
        calls.set(path, "_visible", true.into());
        calls.call(path, "showPanel", vec![]);
        self.quit.shown = true;
    }

    fn open_quit(&mut self, calls: &mut Calls) {
        if !matches!(self.data.lock().unwrap().state, AppState::Menu | AppState::Browser)
            || self.dispatch_epoch.is_none() || !self.quit.begin() { return; }
        let ready = self.data.lock().unwrap().movie_ready.contains("MessageBox");
        if ready {
            self.show_quit(calls);
        } else if self.requested.insert("MessageBox".into()) {
            calls.request_element("MessageBox");
        }
    }

    fn close_lobby(&mut self, calls: &mut Calls) {
        self.lobby.lock().unwrap().cancel();
        for element in ["LobbyBrowser", "LobbyScreen", "StartSinglePlayer"] {
            if let Some(path) = self.roots.get(element) {
                calls.call("_global.csrBindings", "releaseOwnedLifecycle", vec![Value::String(path.to_string())]);
                calls.set(path, "_visible", false.into());
            }
        }
        calls.call("_global.MainMenuMovie", "showPanel", vec![]);
        self.push_action(Action::LobbyHome);
    }
}

impl Engine for Bridge {
    fn lifecycle(&mut self, active: bool, calls: &mut Calls) -> bool {
        self.dispatch_epoch = None;
        self.actions.lock().unwrap().park();
        self.reset_navigation(calls);
        let show_home = active && matches!(self.data.lock().unwrap().state, AppState::Menu | AppState::Browser);
        if let Some(path) = self.roots.get("MainMenu") {
            calls.call(path, if show_home { "onUIShow" } else { "onUIHide" }, vec![]);
            calls.set(path, "_visible", show_home.into());
        }
        !active || self.actions.lock().unwrap().resume()
    }
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
        self.dispatch_epoch = self.actions.lock().unwrap().current();
        if object == "MainMenu" && self.data.lock().unwrap().home_retired { return Some(Value::Undefined); }
        let seen_object = if object.starts_with("OwnedLobby") { "OwnedLobby" }
            else if object.starts_with("OwnedPicker") { "OwnedPicker" }
            else if object.starts_with("OwnedQuit") { "OwnedQuit" } else { object };
        // Repeated native getters are the common path. Borrow their existing
        // names instead of allocating two Strings only to discard them when
        // the diagnostic pair was already seen. This does not cache results.
        let first = self.seen.get(seen_object).is_none_or(|methods| !methods.contains(method));
        if first {
            self.seen.entry(seen_object.to_owned()).or_default().insert(method.to_owned());
        }
        if first
            || matches!(
                method,
                "BasePanelRunCommand" | "OnUpdateValue" | "SendUIEvent" | "OnButtonPress"
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
        if object.starts_with("OwnedQuit") {
            if !self.quit.owns(object) || self.dispatch_epoch.is_none()
                || !matches!(self.data.lock().unwrap().state, AppState::Menu | AppState::Browser) { return None; }
            match (method, args) {
                ("OnButtonPress", [Value::String(key)]) if key == "KEY_ENTER" => {
                    self.close_quit(calls);
                    self.push_action(Action::Command("quit".into()));
                }
                ("OnButtonPress", [Value::String(key)]) if key == "KEY_ESCAPE" => self.close_quit(calls),
                _ => (),
            }
            return None;
        }
        // The current opening's self-bound API is the only exit authority.
        if object == "MessageBox" && method == "OnButtonPress" { return None; }
        // Only our self-bound proxy labels carry epoch bookkeeping. Reject a
        // stale label before dispatch, including zero-argument Start/getters.
        if crate::frontend_lobby::Scene::owned_receiver(object) {
            use crate::frontend_lobby::Api;
            let data = self.data.lock().unwrap();
            let mut lobby = self.lobby.lock().unwrap();
            let Some((api, epoch)) = lobby.receiver(object) else { return None; };
            if data.state != AppState::Menu { lobby.cancel(); return None; }
            match (api, method) {
                (api, "OnUnload") => {
                    let path = match args { [Value::String(handle)] => handle.strip_prefix("mc:"), _ => None };
                    let element = match (api, path) {
                        (Api::Picker, Some(path)) if self.roots.get("StartSinglePlayer").is_some_and(|root| root == path) => Some("StartSinglePlayer"),
                        (Api::Lobby, Some(path)) => ["LobbyBrowser", "LobbyScreen"].into_iter().find(|element| self.roots.get(*element).is_some_and(|root| root == path)),
                        _ => None,
                    };
                    if let (Some(element), Some(path)) = (element, path) {
                        calls.set(path, "_visible", false.into());
                        if api == Api::Picker {
                            lobby.chooser_unloaded();
                            if lobby.phase() == crate::frontend_lobby::Phase::Lobby {
                                if let Some(path) = self.roots.get("LobbyScreen") { lobby.show(path, &data.player_name, calls); }
                            }
                        } else if element == "LobbyScreen" || lobby.phase() == crate::frontend_lobby::Phase::Browser {
                            drop(lobby); drop(data); self.close_lobby(calls);
                        }
                        // Browser unload after observed ShowLobbyUI is the
                        // expected handoff; it must retain the current lobby.
                    }
                }
                (Api::Lobby, "ChooseGameSettings") if args.is_empty() => {
                    if lobby.choose(epoch) {
                        if let Some(path) = self.roots.get("LobbyScreen") {
                            calls.call(path, "hidePanelNoDestroy", vec![]);
                        }
                        if let Some(path) = self.roots.get("StartSinglePlayer") {
                            lobby.show_picker(path, calls);
                        } else { if self.requested.insert("StartSinglePlayer".into()) { calls.request_element("StartSinglePlayer"); } }
                    }
                }
                (Api::Picker, "SetCustomBotDifficulty") => lobby.difficulty(epoch, args),
                (Api::Picker, "CheckGameSettingsRequirements") => {
                    let text = match args { [Value::String(text)] => Some(text.as_str()), _ => None };
                    let result = lobby.check(epoch, &data.game_modes, text);
                    if let Err(error) = &result { self.push_action(Action::Notice(error.clone())); }
                    return Some(result.is_ok().into());
                }
                (Api::Picker, "SetMatchmakingQuery") => {
                    lobby.query(epoch, match args { [Value::String(text)] => Some(text.as_str()), _ => None });
                }
                (Api::Picker, "UpdatedSelections") => {
                    if lobby.updated(epoch, args) {
                        if let Some(path) = self.roots.get("LobbyScreen") { lobby.show(path, &data.player_name, calls); }
                    }
                }
                (Api::Lobby, "LaunchGame") => {
                    if let Some(start) = lobby.start(epoch, args) {
                        self.push_action(Action::Host(start, data.player_name.clone()));
                    }
                }
                (Api::Lobby, "ConfirmExit") if args.is_empty() => {
                    drop(lobby); drop(data); self.close_lobby(calls);
                }
                _ => (),
            }
            return None;
        }
        // Host cancellation must use a self-issued epoch receiver. A raw
        // no-argument callback cannot establish which cached opening owns it.
        if object == "LobbyScreen" && method == "ConfirmExit" { return None; }
        if matches!(object, "LobbyBrowser" | "LobbyScreen" | "StartSinglePlayer") && method == "OnUnload" {
            if self.lobby.lock().unwrap().active() { return None; }
            if let Some(path) = arg(0).strip_prefix("mc:") { calls.set(path, "_visible", false.into()); }
            let player_name = self.data.lock().unwrap().player_name.clone();
            let mut lobby = self.lobby.lock().unwrap();
            match object {
                "StartSinglePlayer" => {
                    self.offline_open = false;
                    self.offline.query(None);
                    lobby.chooser_unloaded();
                    if lobby.phase() == crate::frontend_lobby::Phase::Lobby {
                        if let Some(path) = self.roots.get("LobbyScreen") {
                            lobby.show(path, &player_name, calls);
                        }
                    }
                }
                "LobbyScreen" => lobby.cancel(),
                "LobbyBrowser" if lobby.phase() == crate::frontend_lobby::Phase::Browser => lobby.cancel(),
                _ => (),
            }
            return None;
        }
        if object == "MainMenu" && method == "IsMultiplayerPrivilegeEnabled" {
            // Community connections use our local transport without Steam.
            return Some(Value::Bool(matches!(self.data.lock().unwrap().state, AppState::Menu | AppState::Browser)));
        }
        if object == "StartSinglePlayer" {
            if (!self.offline_open || self.lobby.lock().unwrap().active()) && method != "OnLoadFinished" && method != "OnReady" { return None; }
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
                        self.push_action(Action::Notice(error));
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
                            self.push_action(Action::Launch(selection, no_bots));
                        }
                    }
                    return None;
                }
                _ => {}
            }
        }
        if object == crate::frontend_pause_surface::RECEIVER && method == crate::frontend_pause_surface::CALLBACK {
            self.data.lock().unwrap().pause_surface.accept(args);
            return Some(Value::Undefined);
        }
        if object == "OptionsMenu" && method == "OnCancel" {
            let mut data = self.data.lock().unwrap();
            if data.pause_surface.options_requested() {
                data.pause_surface.options_cancelled();
                data.deferred_pause.cancel_pending();
                if let Some(path) = self.roots.get("OptionsMenu") { calls.set(path, "_visible", false.into()); }
                let base = if data.state == AppState::Match && data.menu_open { "PauseMenu" } else { "MainMenu" };
                if let Some(path) = self.roots.get(base) {
                    calls.set(path, "_visible", true.into());
                    calls.call(path, "showPanel", vec![]);
                }
            }
            return Some(Value::Undefined);
        }
        if method == "OnReady" {
            let mut data = self.data.lock().unwrap();
            if object == "MainMenu" && data.home_retired { return Some(Value::Undefined); }
            data.movie_ready.insert(object.into());
            if object == "MainMenu" { data.home_bootstrapped = true; }
        }
        if method == "OnLoadFinished" && !object.starts_with("CScaleformComponent_") {
            self.data.lock().unwrap().movie_ready.remove(object);
            if let Some(path) = arg(0).strip_prefix("mc:") {
                self.roots.insert(object.into(), path.into());
                match object {
                    "PauseMenu" => self.data.lock().unwrap().pause_surface.root(crate::frontend_pause_surface::Root::Pause, path),
                    "OptionsMenu" => self.data.lock().unwrap().pause_surface.root(crate::frontend_pause_surface::Root::Options, path),
                    _ => (),
                }
                self.requested.insert(object.into());
                if object == "MainMenu" {
                    calls.call(path, "onUIShow", vec![]);
                    if !matches!(
                        self.data.lock().unwrap().state,
                        AppState::Menu | AppState::Browser
                    ) {
                        calls.call(path, "hidePanelImmediate", vec![]);
                        calls.set(path, "_visible", false.into());
                    }
                } else if matches!(object, "LobbyBrowser" | "LobbyScreen") {
                    if self.data.lock().unwrap().state != AppState::Menu || !self.lobby.lock().unwrap().active() {
                        calls.set(path, "_visible", false.into());
                    }
                } else if matches!(object, "OptionsMenu" | "StartSinglePlayer" | "MessageBox") {
                    // Loader completion precedes authored Ready. Keep a cold
                    // root hidden until the still-current destination owns its
                    // initialization; superseded loads must never flash open.
                    calls.set(path, "_visible", false.into());
                } else if object == "PauseMenu" {
                    // Loaded precedes authored Ready. The deferred owner shows
                    // this exact cached root only after both movies are ready.
                    calls.set(path, "_visible", false.into());
                    calls.call(path, "SetPlayerName", vec!["You".into()]);
                    let data = self.data.lock().unwrap();
                    if data.state != AppState::Match || !data.menu_open || data.pause_surface.options_requested() {
                        calls.call(path, "hidePanel", vec![]);
                    }
                }
            }
            return None;
        }
        if method == "OnReady" && object == "PauseMenu" {
            let data = self.data.lock().unwrap();
            if data.state != AppState::Match || !data.menu_open || data.pause_surface.options_requested() {
                if let Some(path) = self.roots.get(object) { calls.set(path, "_visible", false.into()); }
            }
            return None;
        }
        if method == "OnReady" && object == "MessageBox" {
            let current = self.quit.active && matches!(self.data.lock().unwrap().state, AppState::Menu | AppState::Browser)
                && self.dispatch_epoch.is_some();
            if current { self.show_quit(calls); }
            else { self.close_quit(calls); }
            return None;
        }
        if method == "OnReady" && matches!(object, "LobbyBrowser" | "LobbyScreen") {
            let data = self.data.lock().unwrap();
            let lobby = self.lobby.lock().unwrap();
            if let Some(path) = self.roots.get(object) {
                if data.state == AppState::Menu && lobby.active() {
                    if object == "LobbyScreen" && lobby.phase() == crate::frontend_lobby::Phase::Lobby {
                        lobby.show(path, &data.player_name, calls);
                    } else if object == "LobbyBrowser" && lobby.phase() == crate::frontend_lobby::Phase::Browser {
                        lobby.show_browser(path, &data.player_name, calls);
                    } else { calls.set(path, "_visible", false.into()); }
                } else { calls.set(path, "_visible", false.into()); }
            }
            return None;
        }
        if method == "OnReady" && object == "StartSinglePlayer" {
            let lobby = self.lobby.lock().unwrap();
            if lobby.picker() {
                if let Some(path) = self.roots.get(object) { lobby.show_picker(path, calls); }
                return None;
            }
            if lobby.active() {
                if let Some(path) = self.roots.get(object) { calls.set(path, "_visible", false.into()); }
                return None;
            }
            drop(lobby);
            if let Some(path) = self.roots.get(object) {
                if self.offline_open && matches!(
                    self.data.lock().unwrap().state,
                    AppState::Menu | AppState::Browser
                ) {
                    show_single_player(path, calls);
                } else {
                    calls.set(path, "_visible", false.into());
                }
            }
            return None;
        }
        if method == "OnReady" && object == "OptionsMenu" {
            if let Some(path) = self.roots.get(object) {
                let data = self.data.lock().unwrap();
                if data.pause_surface.options_requested()
                    && (matches!(data.state, AppState::Menu | AppState::Browser)
                        || (data.state == AppState::Match && data.menu_open))
                {
                    self.options.show(path, &data.settings, calls);
                } else {
                    calls.set(path, "_visible", false.into());
                }
            }
            return None;
        }
        if method == "OnReady" && object == "LoadingScreen" {
            if let Some(path) = self.roots.get(object) {
                let data = self.data.lock().unwrap();
                if data.state == AppState::Loading {
                    calls.call(path, "setLoadingText", vec![data.loading.clone().into()]);
                    if let Some(context) = &data.loading_map {
                        calls.call("_global.csrBindings", "configureLoadingOverview", vec![path.clone().into(), context.selected.name.clone().into(), context.selected.serial.to_string().into(), context.radar_only.into()]);
                        calls.call(path, "setMapInfo", context.arguments());
                        calls.call(path, "setHintText", vec!["".into()]);
                        calls.call(path, "showPanel", vec![]);
                        calls.call(path, "showLoadinginfo", vec![]);
                        for label in ["ModeText", "ScenarioText"] {
                            calls.set(&format!("{path}.Panel.MainText.{label}"), "_visible", false.into());
                        }
                    } else {
                        calls.call("_global.csrBindings", "configureLoadingOverview", vec![path.clone().into(), Value::Null]);
                        calls.call(path, "hidePanel", vec![]);
                    }
                } else {
                    calls.call("_global.csrBindings", "configureLoadingOverview", vec![path.clone().into(), Value::Null]);
                    calls.call(path, "hidePanel", vec![]);
                }
            }
            return None;
        }
        if method == "BasePanelRunCommand" {
            let command = arg(0);
            // The installed Home power button emits Quit. Confirmation must
            // precede generic navigation teardown so Cancel keeps the picker.
            if object == "MainMenu" && command == "Quit" {
                self.open_quit(calls);
                return None;
            }
            self.close_quit(calls);
            self.close_options(calls);
            self.offline_open = false;
            self.offline.query(None);
            if arg(1) == "bHidePanel" {
                calls.call("_global.MainMenuMovie", "hidePanelImmediate", vec![]);
            }
            if !matches!(command.as_str(), "ShowJoinPartyUI:competitive" | "ShowLobbyUI") {
                self.lobby.lock().unwrap().cancel();
                for element in ["LobbyBrowser", "LobbyScreen", "StartSinglePlayer"] {
                    if let Some(path) = self.roots.get(element) {
                        calls.call("_global.csrBindings", "releaseOwnedLifecycle", vec![Value::String(path.to_string())]);
                        calls.set(path, "_visible", false.into());
                    }
                }
            }
            match command.as_str() {
                "ShowJoinPartyUI:competitive" => {
                    let data = self.data.lock().unwrap();
                    if data.state != AppState::Menu { return None; }
                    let mut lobby = self.lobby.lock().unwrap();
                    if lobby.begin(&data.game_modes) {
                        self.push_action(Action::Notice(crate::frontend_lobby::ADDRESS_NOTICE.into()));
                        if let Some(path) = self.roots.get("LobbyBrowser") {
                            lobby.show_browser(path, &data.player_name, calls);
                        } else { if self.requested.insert("LobbyBrowser".into()) { calls.request_element("LobbyBrowser"); } }
                    }
                }
                "ShowLobbyUI" => {
                    let data = self.data.lock().unwrap();
                    let mut lobby = self.lobby.lock().unwrap();
                    if data.state == AppState::Menu && lobby.open_lobby() {
                        if let Some(path) = self.roots.get("LobbyScreen") { lobby.show(path, &data.player_name, calls); }
                        else { if self.requested.insert("LobbyScreen".into()) { calls.request_element("LobbyScreen"); } }
                    }
                }
                "OpenCreateSinglePlayerGameDialog" => {
                    self.offline_open = true;
                    self.lobby.lock().unwrap().cancel();
                    for element in ["LobbyBrowser", "LobbyScreen"] {
                        if let Some(path) = self.roots.get(element) { calls.set(path, "_visible", false.into()); }
                    }
                    if self.data.lock().unwrap().movie_ready.contains("StartSinglePlayer") {
                        if let Some(path) = self.roots.get("StartSinglePlayer") { show_single_player(path, calls); }
                    } else if self.requested.insert("StartSinglePlayer".into()) {
                        calls.request_element("StartSinglePlayer");
                    }
                }
                "OpenVideoSettingsDialog"
                | "OpenAudioSettingsDialog"
                | "OpenGameSettingsDialog"
                | "OpenSettingsDialog"
                | "OpenMouseDialog"
                | "OpenKeyboardMouseDialog" => {
                    self.lobby.lock().unwrap().cancel();
                    let mut data = self.data.lock().unwrap();
                    data.pause_surface.options_opened();
                    data.deferred_pause.cancel_pending();
                    if let Some(path) = self.roots.get("PauseMenu") { calls.set(path, "_visible", false.into()); }
                    self.options = crate::frontend_options::Scene::load(
                        &data.fs,
                        &command,
                        &data.localization,
                    );
                    if data.movie_ready.contains("OptionsMenu") {
                        if let Some(path) = self.roots.get("OptionsMenu") { self.options.show(path, &data.settings, calls); }
                    } else if self.requested.insert("OptionsMenu".into()) {
                        calls.request_element("OptionsMenu");
                    }
                }
                _ => {
                    self.lobby.lock().unwrap().cancel();
                    self.push_action(Action::Command(command));
                }
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
        // The original movie's native widget API is shared by dropdowns,
        // choices and sliders. Cached/hidden pages cannot publish changes.
        if object == "OptionsMenu"
            && matches!(method, "OnUpdateValue" | "OnHighlightWidget" | "OnRefreshValues" | "OnLayoutComplete")
            && !(data.pause_surface.options_requested()
                && (matches!(data.state, AppState::Menu | AppState::Browser)
                    || (data.state == AppState::Match && data.menu_open)))
        {
            return None;
        }
        match (object, method) {
            ("PartyList", "GetPartySessionSetting") => self.lobby.lock().unwrap().party_setting(&arg(0)),
            ("PartyList", "GetPartySystemSetting") if arg(0) == "xuidHost" && self.lobby.lock().unwrap().active() => Some("0".into()),
            ("LobbyScreen", "GetVoiceChatEnabled") => Some(false.into()),
            ("GameTypes", "GetGameModeType") => data.game_modes.block("gameTypes")?.iter().find_map(|(kind, value)| {
                value.as_block()?.block("gameModes")?.get(&arg(0)).map(|_| kind.to_string().into())
            }),
            ("GameTypes", "GetGameModeAttribute") => {
                let value = data.game_modes.path(&["gameTypes", &arg(0), "gameModes", &arg(1)])?.as_block()?.str(&arg(2))?;
                Some(data.localization.get(value).unwrap_or(value).into())
            }
            ("OptionsMenu", "OnUpdateValue") => {
                if let Some((name, value)) = self.options.value(
                    args.first()?,
                    args.get(1)?,
                    self.roots.get("OptionsMenu")?,
                    calls,
                ) {
                    apply_setting(&mut data.settings, &name, &value);
                    self.push_action(Action::Convar(name, value));
                }
                None
            }
            ("OptionsMenu", "OnHighlightWidget") => {
                if let (Some(index), Some(path)) = (args.first(), self.roots.get("OptionsMenu")) {
                    self.options.highlight(index, path, calls);
                }
                None
            }
            ("OptionsMenu", "OnRefreshValues") => {
                if let Some(path) = self.roots.get("OptionsMenu") {
                    self.options.refresh_values(path, &data.settings, calls);
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
                self.push_action(Action::Sound(arg(0)));
                None
            }
            ("GameInterface", "SetConvar")
            | ("GameInterface", "SetConvarNumber")
            | ("GameInterface", "SetConvarString")
            | ("GameInterface", "SetConvarBoolean") => {
                let name = arg(0);
                let value = args.get(1).cloned().unwrap_or(Value::Undefined);
                // Authored numeric entry immediately asks the bridge to refresh.
                // Update this snapshot before the following callback reads it.
                if apply_setting(&mut data.settings, &name, &value) {
                    self.options.convar_committed(&name);
                }
                self.push_action(Action::Convar(name, value));
                None
            }
            ("GameInterface", "GetConvarNumber") => Some(Value::Number(match arg(0).as_str() {
                "safezonex" | "safezoney" => 1.0,
                "hud_scaling" => 0.85,
                "game_type" => 0.0,
                "game_mode" => 1.0,
                name => data.settings.convar_number(name)?,
            })),
            ("MyPersona", "GetXuid") => Some("0".into()),
            ("MyPersona", "GetXpPerLevel") => Some(5000.0.into()),
            ("MyPersona", "GetXp") | ("MyPersona", "GetLevel") => Some(0.0.into()),
            ("FriendsList", "GetFriendName") if arg(0) == "0" => Some(data.player_name.clone().into()),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerKind { Home, Match }

struct CachedWorker {
    movie: Option<Mutex<scaleform::Menu>>,
    data: Arc<Mutex<Data>>,
    actions: Arc<Mutex<ActionQueue>>,
    worker_generation: u64,
    active: bool,
    ready: bool,
    failed: Option<String>,
    requested: HashSet<String>,
    loaded: HashSet<String>,
    loading_text: String,
    loading_map: Option<LoadingContext>,
    loading_selection: Option<crate::session::LoadingMap>,
    lobby: Arc<Mutex<crate::frontend_lobby::Scene>>,
}
impl CachedWorker {
    fn new(data: Data, catalog: crate::frontend_lobby::Catalog) -> Self {
        Self { movie: None, data: Arc::new(Mutex::new(data)), actions: Default::default(),
            worker_generation: 0, active: false, ready: false, failed: None,
            requested: Default::default(), loaded: Default::default(), loading_text: String::new(),
            loading_map: None, loading_selection: None,
            lobby: Arc::new(Mutex::new(crate::frontend_lobby::Scene::new(catalog))) }
    }
}

#[derive(Resource)]
pub struct Frontend {
    worker_kind: WorkerKind,
    parked: Option<CachedWorker>,
    next_worker_generation: u64,
    home_show_pending: bool,
    movie: Option<Mutex<scaleform::Menu>>,
    data: Arc<Mutex<Data>>,
    actions: Arc<Mutex<ActionQueue>>,
    image: Handle<Image>,
    presented_serial: u64,
    pause_presentation: crate::frontend_pause_surface::Presentation,
    performance_ack: Option<(u64, std::time::Instant)>,
    worker_generation: u64,
    root: Entity,
    camera: Entity,
    visible: bool,
    active: bool,
    pub ready: bool,
    pub(crate) failed: Option<String>,
    viewport: (u32, u32),
    requested: HashSet<String>,
    loaded: HashSet<String>,
    scene: Option<AppState>,
    loading_text: String,
    loading_map: Option<LoadingContext>,
    loading_selection: Option<crate::session::LoadingMap>,
    lobby: Arc<Mutex<crate::frontend_lobby::Scene>>,
    notice: Entity,
    notice_text: Entity,
    cancel: Entity,
}

impl Frontend {
    fn select_worker(&mut self, kind: WorkerKind, catalog: crate::frontend_lobby::Catalog) -> bool {
        if self.worker_kind == kind { return false; }
        // Reject actions immediately, including an in-flight old native call.
        self.actions.lock().unwrap().park();
        {
            let mut data = self.data.lock().unwrap();
            data.state = AppState::Loading;
            data.menu_open = false;
            data.pause_surface.activate(self.worker_generation, false);
            data.pause_surface.options_cancelled();
            data.deferred_pause.cancel_pending();
        }
        if let Some(movie) = &self.movie { movie.lock().unwrap().park(); }
        self.active = false;
        let fork = self.data.lock().unwrap().fork_worker();
        let mut other = self.parked.take().unwrap_or_else(|| CachedWorker::new(fork, catalog));
        std::mem::swap(&mut self.movie, &mut other.movie);
        std::mem::swap(&mut self.data, &mut other.data);
        std::mem::swap(&mut self.actions, &mut other.actions);
        std::mem::swap(&mut self.worker_generation, &mut other.worker_generation);
        std::mem::swap(&mut self.active, &mut other.active);
        std::mem::swap(&mut self.ready, &mut other.ready);
        std::mem::swap(&mut self.failed, &mut other.failed);
        std::mem::swap(&mut self.requested, &mut other.requested);
        std::mem::swap(&mut self.loaded, &mut other.loaded);
        std::mem::swap(&mut self.loading_text, &mut other.loading_text);
        std::mem::swap(&mut self.loading_map, &mut other.loading_map);
        std::mem::swap(&mut self.loading_selection, &mut other.loading_selection);
        std::mem::swap(&mut self.lobby, &mut other.lobby);
        self.parked = Some(other);
        self.worker_kind = kind;
        self.home_show_pending = kind == WorkerKind::Home;
        self.performance_ack = None;
        self.pause_presentation.open();
        info!(worker = ?kind, generation = self.worker_generation, "Classic UI selected cached domain");
        true
    }

    fn resume_worker(&mut self, focused: bool, width: u32, height: u32) {
        if let Some(movie) = &self.movie {
            let movie = movie.lock().unwrap();
            movie.resume(focused);
            movie.send(Input::Resize { width, height });
        } else if !self.actions.lock().unwrap().resume() {
            self.failed = Some("The menu session limit was reached. Restart the game.".into());
        }
    }

    pub(crate) fn home_bootstrapped(&self) -> bool { self.data.lock().unwrap().home_bootstrapped }
    pub(crate) fn pause_presented(&self) -> bool {
        self.worker_kind == WorkerKind::Match && self.pause_surface_visible()
            && self.pause_presentation.ready()
    }
    pub(crate) fn home_presented(&self) -> bool {
        self.worker_kind == WorkerKind::Home && !self.home_show_pending
            && self.movie_ready("MainMenu") && self.pause_presentation.ready()
    }
    pub(crate) fn take_performance_ack(&mut self) -> Option<(u64, std::time::Instant)> { self.performance_ack.take() }
    pub(crate) fn camera(&self) -> Entity { self.camera }
    pub(crate) fn pause_surface_visible(&self) -> bool { self.data.lock().unwrap().pause_surface.visible() }
    pub(crate) fn options_requested(&self) -> bool { self.data.lock().unwrap().pause_surface.options_requested() }
    pub(crate) fn pause_root(&self) -> Option<String> { self.data.lock().unwrap().pause_surface.pause_root().map(str::to_owned) }
    pub(crate) fn loaded(&self, name: &str) -> bool { self.loaded.contains(name) }
    /// Authored OnReady received for the current cached movie; Loaded alone is earlier.
    pub(crate) fn movie_ready(&self, name: &str) -> bool {
        self.data.lock().unwrap().movie_ready.contains(name)
    }
    pub(crate) fn lobby_staged(&self) -> bool { self.lobby.lock().unwrap().staged() }
    pub(crate) fn lobby_staged_map(&self) -> Option<String> { self.lobby.lock().unwrap().staged_map() }
    pub(crate) fn lobby_phase(&self) -> crate::frontend_lobby::Phase { self.lobby.lock().unwrap().phase() }
    /// Counts completed frame assignments to the Bevy image, not render loops.
    pub(crate) fn presented_serial(&self) -> u64 { self.presented_serial }
    /// Requested worker viewport and the last assigned raster, for opt-in diagnostics.
    pub(crate) fn raster_dimensions(&self, images: &Assets<Image>) -> Option<((u32, u32), (u32, u32))> {
        let size = images.get(&self.image)?.texture_descriptor.size;
        Some((self.viewport, (size.width, size.height)))
    }
    pub(crate) fn send(&self, input: Input) {
        if let Some(movie) = &self.movie {
            movie.lock().unwrap().send(input);
        }
    }
    fn request(&mut self, element: &str) {
        // RequestElement mutates an already-loaded movie's stage transform.
        // Reopen the cached movie instead of applying that transform twice.
        if self.requested.insert(element.into()) {
            self.send(Input::RequestElement(element.into()));
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
        localization: Arc::new(crate::localize::Localization::load(&data.vfs)),
        game_modes: crate::frontend_game::load(&data.vfs, &installed),
        state: AppState::Menu,
        menu_open: menu.open(),
        loading: String::new(),
        loading_map: None,
        player_name: "You".into(),
        movie_ready: HashSet::new(), home_bootstrapped: false, home_retired: false,
        pause_surface: Default::default(),
        deferred_pause: Default::default(),
    }));
    let lobby = Arc::new(Mutex::new(crate::frontend_lobby::Scene::new(
        crate::frontend_lobby::Catalog::installed(&options.0, &menu.maps),
    )));
    let actions = Arc::new(Mutex::new(ActionQueue::default()));
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
            BackgroundColor(Color::NONE),
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
    let mut presentation = crate::frontend_pause_surface::Presentation::default();
    presentation.open();
    commands.insert_resource(Frontend {
        worker_kind: WorkerKind::Home,
        parked: None,
        next_worker_generation: 0,
        home_show_pending: true,
        movie: None,
        data: shared,
        actions,
        image,
        presented_serial: 0,
        pause_presentation: presentation,
        performance_ack: None,
        worker_generation: 0,
        root,
        camera,
        visible: false,
        active: false,
        ready: false,
        failed: None,
        viewport: (options.0.width, options.0.height),
        requested: HashSet::new(),
        loaded: HashSet::new(),
        scene: None,
        loading_text: String::new(),
        loading_map: None,
        loading_selection: None,
        lobby,
        notice,
        notice_text,
        cancel,
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    mut front: ResMut<Frontend>,
    mut images: ResMut<Assets<Image>>,
    (mut nodes, mut cameras, mut texts, mut backgrounds): (
        Query<&mut Node>,
        Query<(&mut Camera, &mut Msaa)>,
        Query<&mut Text>,
        Query<&mut BackgroundColor>,
    ),
    mut menu: ResMut<crate::menu::Menu>,
    state: Res<State<AppState>>,
    mut settings: ResMut<Settings>,
    (data, mut loader, options, request, remote, host_request): (
        Res<GameData>,
        ResMut<crate::session::Loader>,
        Res<Options>,
        Res<crate::menu::LoadRequest>,
        Option<Res<crate::remote::RemoteClient>>,
        Res<crate::hosting::HostRequest>,
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
    let _frame_scope = crate::input_latency::frame_scope("frontend");
    let remote_loading = crate::remote_join::loading(remote.as_deref());
    let scene = if remote_loading { AppState::Loading } else { *state.get() };
    let kind = if matches!(scene, AppState::Loading | AppState::Match) { WorkerKind::Match } else { WorkerKind::Home };
    let switched = if front.worker_kind != kind {
        let catalog = crate::frontend_lobby::Catalog::installed(&options.0, &menu.maps);
        front.select_worker(kind, catalog)
    } else { false };
    let loading_selection = if scene == AppState::Loading { loader.loading_map() } else { None };
    let loading_selection_changed = front.loading_selection != loading_selection;
    front.loading_selection = loading_selection.clone();
    {
        let mut snapshot = front.data.lock().unwrap();
        snapshot.state = scene;
        snapshot.menu_open = menu.open();
        let options_opening = snapshot.pause_surface.options_requested();
        snapshot.deferred_pause.observe(scene == AppState::Match && menu.open(), options_opening);
        snapshot.loading = loader.status.clone();
        if loading_selection_changed {
            snapshot.loading_map = loading_selection.and_then(|selected| LoadingContext::installed(selected, &snapshot));
        }
        snapshot.pause_surface.activate(front.worker_generation,
            scene == AppState::Match && matches!(menu.page, Some(crate::menu::Page::Main | crate::menu::Page::Pause)));
    }
    if settings.is_changed() || switched {
        front.data.lock().unwrap().settings = settings.clone();
    }
    if inventory.is_changed() || catalog.is_changed() || practice.is_changed() || switched {
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
                .find(|finish| finish.id == skin.paint && finish.weapon == skin.weapon && crate::classic_cosmetics::finish_available(finish))
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
            if let Some(case) = catalog.cases.iter().find(|case| case.id == definition && crate::classic_cosmetics::case_available(case)) {
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
    if switched {
        front.resume_worker(window.focused, window.physical_width(), window.physical_height());
        // An unbooted Loading domain must not show the parked Home raster.
        if scene == AppState::Loading {
            if let Some(mut image) = images.get_mut(&front.image) {
                image.texture_descriptor.size = Extent3d { width: 1, height: 1, depth_or_array_layers: 1 };
                image.data = Some(vec![0, 0, 0, 0]);
            }
        }
    }
    let visible = matches!(scene, AppState::Menu | AppState::Browser)
        || (scene == AppState::Match && menu.open())
        || (scene == AppState::Loading
            && (front.movie.is_some() || (!options.0.direct && options.0.screenshot.is_none())));
    // A UI-only camera must not overwrite a match's resolved color target.
    // During play the existing viewmodel camera continues drawing the HUD.
    if let Ok((mut camera, mut msaa)) = cameras.get_mut(front.camera) {
        camera.is_active = scene != AppState::Match || menu.open();
        if front.scene != Some(scene) {
            // A new/cold Home must clear the former match target while its
            // genuine Ready/presentation fence is pending. Pause composites.
            camera.clear_color = if scene == AppState::Match { ClearColorConfig::None }
                else { ClearColorConfig::Custom(Color::BLACK) };
        }
        *msaa = if scene == AppState::Match && settings.msaa {
            Msaa::Sample4
        } else {
            Msaa::Off
        };
    }
    let background_start = options.0.screenshot.is_none()
        && matches!(scene, AppState::Loading | AppState::Match);
    if (visible || background_start) && front.movie.is_none() && front.failed.is_none() {
        let Some(generation) = front.next_worker_generation.checked_add(1) else {
            front.failed = Some("The menu session limit was reached. Restart the game.".into());
            return;
        };
        front.next_worker_generation = generation;
        front.worker_generation = generation;
        info!(worker = ?front.worker_kind, generation, "Classic UI starting isolated worker");
        front.data.lock().unwrap().pause_surface.activate(generation,
            scene == AppState::Match && matches!(menu.page, Some(crate::menu::Page::Main | crate::menu::Page::Pause)));
        let mut elements = vec!["MainMenu".into()];
        if matches!(scene, AppState::Loading | AppState::Match) {
            elements.push("PauseMenu".into());
        }
        if scene == AppState::Loading { elements.push("LoadingScreen".into()); }
        front.requested.extend(elements.iter().cloned());
        front.movie = Some(Mutex::new(scaleform::Menu::spawn_named(
            scaleform::Config {
                game_dir: data.game_dir.clone(),
                width: window.physical_width(),
                height: window.physical_height(),
                language: "english".into(),
                elements,
            },
            Box::new(Bridge {
                dispatch_epoch: None,
                data: front.data.clone(),
                actions: front.actions.clone(),
                seen: HashMap::new(),
                roots: HashMap::new(),
                requested: HashSet::new(),
                options: Default::default(),
                offline: Default::default(),
                offline_open: false, quit: Default::default(),
                lobby: front.lobby.clone(),
            }),
            format!("{:?}-{generation}", front.worker_kind),
        )));
    }
    // Only the match player's duplicate Home is disposable. The original
    // parsed Home is safely parked in its own inactive callback domain.
    let retire_match_home = {
        let mut snapshot = front.data.lock().unwrap();
        let retire = front.worker_kind == WorkerKind::Match && scene == AppState::Match
            && !snapshot.home_retired && snapshot.movie_ready.contains("MainMenu")
            && snapshot.movie_ready.contains("PauseMenu");
        if retire { snapshot.home_retired = true; snapshot.movie_ready.remove("MainMenu"); }
        retire
    };
    if retire_match_home {
        front.ready = true;
        front.loaded.remove("MainMenu");
        front.send(Input::Call { path: "_global.MainMenuMovie".into(), function: "unloadMovie".into(), args: vec![] });
        info!(generation = front.worker_generation, "Classic UI retired duplicate match Home after bootstrap");
    }
    if front.worker_kind == WorkerKind::Home && front.home_show_pending && front.movie_ready("MainMenu") {
        front.home_show_pending = false;
        front.send(Input::Present { commands: vec![
            scaleform::Command::Set { path: "_global.MainMenuMovie".into(), property: "_visible".into(), value: true.into() },
            scaleform::Command::Call { path: "_global.MainMenuMovie".into(), function: "showPanel".into(), args: vec![] },
        ], revision: front.pause_presentation.revision() });
        info!(generation = front.worker_generation, "Classic UI Home Ready; awaiting fresh presentation");
    }
    // Load the first Pause while the map loads, rather than making Escape pay
    // for its movies/fonts. A closed late load stays hidden in the bridge.
    if matches!(scene, AppState::Loading | AppState::Match) {
        front.request("PauseMenu");
    }
    let active = visible || (front.movie.is_some() && matches!(scene, AppState::Loading | AppState::Match)
        && front.failed.is_none() && (!front.home_bootstrapped() || !front.movie_ready("PauseMenu")));
    if front.active != active {
        front.send(Input::SetActive(active));
        front.active = active;
    }
    if front.visible != visible {
        front.visible = visible;
        if visible && scene == AppState::Match {
            // Never flash a cached closed/previous menu raster while the worker
            // is still preparing this Escape transition.
            front.pause_presentation.open();
        } else if !switched && !front.home_show_pending {
            front.pause_presentation.close();
        }
        if visible && scene == AppState::Menu && front.ready {
            front.send(Input::Call {
                path: "_global.MainMenuMovie".into(),
                function: "showPanel".into(),
                args: vec![],
            });
        }
        if visible && scene == AppState::Match && front.ready {
            let options_root = {
                let mut data = front.data.lock().unwrap();
                data.pause_surface.options_cancelled();
                data.pause_surface.options_root().map(str::to_owned)
            };
            if let Some(path) = options_root { front.send(Input::Set { path, property: "_visible".into(), value: false.into() }); }
            front.request("PauseMenu");
        } else if scene == AppState::Match && front.ready {
            front.send(Input::Call {
                path: "_level100".into(),
                function: "hidePanel".into(),
                args: vec![],
            });
        }
    }
    let (pause_show, options_to_hide) = {
        let mut data = front.data.lock().unwrap();
        let main_ready = data.home_bootstrapped;
        let pause_ready = data.movie_ready.contains("PauseMenu");
        let path = data.pause_surface.pause_root().map(str::to_owned);
        let options_path = data.pause_surface.options_root().map(str::to_owned);
        let show = if data.deferred_pause.take_ready(main_ready, pause_ready, path.is_some()) { path } else { None };
        (show, options_path)
    };
    if let Some(path) = pause_show {
        // The previous show consumed this open cycle. Existing native OnCancel
        // restores Pause itself; it never rearms this deferred request.
        let mut commands: Vec<_> = ["_global.MainMenuMovie", "_global.SinglePlayerMovie", "_global.LobbyMovie", "_level17"]
            .into_iter().map(|root| scaleform::Command::Set {
                path: root.into(), property: "_visible".into(), value: false.into(),
            }).collect();
        if let Some(path) = options_to_hide {
            commands.push(scaleform::Command::Set { path, property: "_visible".into(), value: false.into() });
        }
        commands.push(scaleform::Command::Set { path: path.clone(), property: "_visible".into(), value: true.into() });
        commands.push(scaleform::Command::Call { path, function: "showPanel".into(), args: vec![] });
        front.send(Input::Present { commands, revision: front.pause_presentation.revision() });
    }
    let sample = front.data.lock().unwrap().pause_surface.request(front.presented_serial);
    if let Some(sample) = sample {
        front.send(Input::ObservePauseSurface { args: sample.args() });
    }
    if scene != AppState::Menu && front.lobby.lock().unwrap().active() { front.lobby.lock().unwrap().cancel(); }
    if front.scene != Some(scene) {
        // Return-to-Home must not expose the last in-match Pause raster while
        // this transaction is still in the worker queue/GPU readback pipeline.
        if visible && scene == AppState::Menu && front.scene.is_some() {
            front.pause_presentation.open();
        }
        let mut scene_commands = Vec::new();
        let options_root = {
            let mut data = front.data.lock().unwrap();
            data.pause_surface.options_cancelled();
            data.pause_surface.options_root().map(str::to_owned)
        };
        if let Some(path) = options_root { scene_commands.push(scaleform::Command::Set { path, property: "_visible".into(), value: false.into() }); }
        for path in ["_global.LobbyMovie", "_level17"] {
            scene_commands.push(scaleform::Command::Set { path: path.into(), property: "_visible".into(), value: false.into() });
        }
        // Movie animations and late callbacks can leave cached launch panels
        // visible. Own the root visibility across scenes, including descendants.
        for path in ["_global.MainMenuMovie", "_global.SinglePlayerMovie"] {
            scene_commands.push(scaleform::Command::Set {
                path: path.into(),
                property: "_visible".into(),
                value: (matches!(scene, AppState::Menu | AppState::Browser)
                    && path == "_global.MainMenuMovie")
                    .into(),
            });
        }
        if scene != AppState::Match {
            scene_commands.push(scaleform::Command::Call {
                path: "_level100".into(),
                function: "hidePanel".into(),
                args: vec![],
            });
            scene_commands.push(scaleform::Command::Set {
                path: "_level100".into(), property: "_visible".into(), value: false.into(),
            });
        }
        if scene == AppState::Loading {
            scene_commands.push(scaleform::Command::Call {
                path: "_global.MainMenuMovie".into(),
                function: "hidePanelImmediate".into(),
                args: vec![],
            });
            scene_commands.push(scaleform::Command::Call {
                path: "_global.SinglePlayerMovie".into(),
                function: "hidePanel".into(),
                args: vec![true.into()],
            });
            scene_commands.push(scaleform::Command::Call {
                path: "_global.SinglePlayerMovie.Panel.Panel.BotDiffPanel".into(),
                function: "HidePanel".into(),
                args: vec![],
            });
        } else {
            scene_commands.push(scaleform::Command::Call {
                path: "_level1000".into(),
                function: "hidePanel".into(),
                args: vec![],
            });
            if scene == AppState::Menu {
                scene_commands.push(scaleform::Command::Call {
                    path: "_global.MainMenuMovie".into(),
                    function: "showPanel".into(),
                    args: vec![],
                });
            }
        }
        front.send(Input::Present {
            commands: scene_commands,
            revision: if front.home_show_pending { 0 } else { front.pause_presentation.revision() },
        });
        if scene == AppState::Loading { front.request("LoadingScreen"); }
        front.scene = Some(scene);
    }
    let loading_map = front.data.lock().unwrap().loading_map.clone();
    if front.loading_map != loading_map {
        front.loading_map = loading_map.clone();
        if front.movie_ready("LoadingScreen") {
            if let Some(context) = loading_map {
                front.send(Input::Call { path: "_global.csrBindings".into(), function: "configureLoadingOverview".into(), args: vec!["_level1000".into(), context.selected.name.clone().into(), context.selected.serial.to_string().into(), context.radar_only.into()] });
                for (function, args) in [
                    ("setMapInfo", context.arguments()),
                    ("setHintText", vec!["".into()]),
                    ("setLoadingText", vec![loader.status.clone().into()]),
                    ("showPanel", vec![]),
                    ("showLoadinginfo", vec![]),
                ] {
                    front.send(Input::Call { path: "_level1000".into(), function: function.into(), args });
                }
                for label in ["ModeText", "ScenarioText"] {
                    front.send(Input::Set { path: format!("_level1000.Panel.MainText.{label}"), property: "_visible".into(), value: false.into() });
                }
            } else {
                front.send(Input::Call { path: "_global.csrBindings".into(), function: "configureLoadingOverview".into(), args: vec!["_level1000".into(), Value::Null] });
                front.send(Input::Call { path: "_level1000".into(), function: "hidePanel".into(), args: vec![] });
                front.send(Input::Call { path: "_level1000".into(), function: "setMapInfo".into(), args: vec![Value::Null; 6] });
            }
        }
    }
    let loading = scene == AppState::Loading;
    if loading && front.loading_text != loader.status {
        front.send(Input::Call {
            path: "_level1000".into(),
            function: "setLoadingText".into(),
            args: vec![loader.status.clone().into()],
        });
        front.loading_text = loader.status.clone();
    } else if !loading {
        front.loading_text.clear();
    }
    let owned_lobby_notice = scene == AppState::Menu && front.lobby.lock().unwrap().active();
    if let Ok(mut node) = nodes.get_mut(front.notice) {
        // The installed lobby/picker owns its footer controls. The clear margin
        // between the 55px navigation bar and 155px panels holds our notice.
        node.top = if owned_lobby_notice { percent(7) } else { Val::Auto };
        node.bottom = if owned_lobby_notice { Val::Auto } else { px(24.0) };
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
        root.display = if (visible && front.failed.is_none() && front.pause_presentation.ready()) || loading {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut background) = backgrounds.get_mut(front.root) {
        background.0 = if loading { Color::BLACK } else { Color::NONE };
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
                if name == "MainMenu" && front.data.lock().unwrap().home_retired { continue; }
                front.ready |= name == "MainMenu";
                front.loaded.insert(name.clone());
                info!("Classic UI loaded {name}");
            }
            Some(scaleform::Event::Failed(error)) => {
                error!("Classic UI: {error}");
                menu.status = error.clone();
                front.failed = Some(error);
            }
            Some(scaleform::Event::Inspected { path, properties }) => {
                if let Some(id) = path.strip_prefix("_global.csrPerformanceProbe.").and_then(|id| id.parse().ok()) {
                    front.performance_ack = Some((id, std::time::Instant::now()));
                }
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
            front.pause_presentation.presented(frame.presentation_revision);
            front.presented_serial = front.presented_serial.saturating_add(1);
        }
    }
    let actions = front.actions.lock().unwrap().take();
    // Home/quit in this batch wins over a preceding queued Start.
    if actions.iter().any(|action| matches!(action, Action::LobbyHome) ||
        matches!(action, Action::Command(command) if matches!(command.to_ascii_lowercase().as_str(), "quit" | "quitgame" | "disconnect" | "openserverbrowser"))) {
        front.lobby.lock().unwrap().cancel();
        *host_request.0.lock().unwrap() = None;
    }
    for action in actions {
        match action {
            Action::Host(start, name) => {
                if scene == AppState::Menu {
                    let mut map = start.map;
                    map.options.terrorist = menu.terrorist;
                    let selected = crate::hosting::HostSelection {
                        team: if map.options.terrorist { sim::Team::Terrorist } else { sim::Team::CounterTerrorist },
                        map, config: netcode::host::HostConfig::default(), tick_rate: 128, name,
                    };
                    match selected.validate() {
                        Ok(()) if front.lobby.lock().unwrap().consume(start.epoch) => { menu.status.clear(); *host_request.0.lock().unwrap() = Some(selected); }
                        Ok(()) => (),
                        Err(error) => {
                            front.lobby.lock().unwrap().reject_start(start.epoch);
                            front.send(Input::Call { path: "_global.LobbyMovie".into(), function: "SetNavOptions".into(), args: vec![false.into()] });
                            menu.status = error;
                        }
                    }
                }
            }
            Action::LobbyHome => {
                // Home can race the queued HostRequest before Loading enters.
                // Invalidate that serial even when Menu -> Menu is no transition.
                loader.invalidate();
                *host_request.0.lock().unwrap() = None;
                menu.status.clear(); next.set(AppState::Menu);
            }
            Action::Launch(selection, no_bots) => {
                if matches!(scene, AppState::Menu | AppState::Browser) {
                    let mut selected = options.0.clone();
                    selected.map = selection.map;
                    selected.deathmatch = selection.deathmatch;
                    if no_bots {
                        selected.bots = 0;
                    }
                    menu.status.clear();
                    *request.0.lock().unwrap() = Some(selected.into());
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
    mut loader: ResMut<crate::session::Loader>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    if (*state.get() == AppState::Loading || crate::remote_join::loading(remote.as_deref()))
        && (keys.just_pressed(KeyCode::Escape)
            || interactions.get(front.cancel) == Ok(&Interaction::Pressed))
    {
        loader.invalidate();
        menu.status.clear();
        next.set(AppState::Menu);
    }
}

pub(crate) fn input(
    front: Res<Frontend>,
    console: Option<Res<crate::console::DeveloperConsole>>,
    mut ownership: Local<crate::frontend_input::Ownership>,
    packs: Option<Res<crate::pack_ui::PackUi>>,
    sharing: Option<Res<crate::host_sharing_ui::Ui>>,
    enabled: Res<crate::player::InputEnabled>,
    state: Res<State<AppState>>,
    menu: Res<crate::menu::Menu>,
    window: Single<(Entity, &Window), With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut keys: MessageReader<KeyboardInput>,
    mut wheel: MessageReader<MouseWheel>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    buttons: Res<ButtonInput<KeyCode>>,
    mut focus: MessageReader<WindowFocused>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    let (window_entity, window) = *window;
    let blocked = console.is_some_and(|console| console.blocks_input()) || *state.get() == AppState::Browser || crate::remote_join::loading(remote.as_deref()) || packs.is_some_and(|packs| packs.blocks_movie()) || sharing.is_some_and(|sharing| sharing.blocks_movie()) || crate::frontend_flow::enabled() || !enabled.0 || !front.visible || front.home_show_pending || front.failed.is_some() || !window.focused;
    for input in ownership.transition(blocked, window.focused) { front.send(input); }
    if blocked {
        keys.clear();
        wheel.clear();
        focus.clear();
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
    let position = window.cursor_position().map(|position| {
        let scale = window.scale_factor() as f64;
        (position.x as f64 * scale, position.y as f64 * scale)
    });
    let movement = position.and_then(|(x, y)| ownership.mouse_move(x, y));
    let mut send = |input: Input| { ownership.record(&input); movie.send(input); };
    if let Some((x, y)) = position {
        if let Some(movement) = movement { send(movement); }
        for (button, mapped) in [
            (MouseButton::Left, scaleform::MouseButton::Left),
            (MouseButton::Right, scaleform::MouseButton::Right),
            (MouseButton::Middle, scaleform::MouseButton::Middle),
        ] {
            if mouse.just_pressed(button) {
                send(Input::MouseDown {
                    x,
                    y,
                    button: mapped,
                });
            }
            if mouse.just_released(button) {
                send(Input::MouseUp {
                    x,
                    y,
                    button: mapped,
                });
            }
        }
    }
    for event in focus.read() {
        if event.window == window_entity { send(Input::Focus(event.focused)); }
    }
    for event in wheel.read() {
        send(Input::MouseWheel {
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
                send(Input::Edit {
                    name: name.clone(),
                    control,
                    shift,
                });
            }
            send(Input::Key {
                name,
                vkey,
                down: event.state == ButtonState::Pressed,
            });
        }
        if event.state == ButtonState::Pressed && !control {
            if let Some(text) = &event.text {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    send(Input::Char(ch));
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
    if crate::browser_flow::enabled() || crate::pack_ui::proof_enabled() || crate::frontend_flow::enabled() {
        return;
    }
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
    fn cached_domain_actions_discard_parked_and_inflight_prior_epoch_calls() {
        let mut queue = ActionQueue::default();
        let old_dispatch = queue.current();
        queue.push_at(old_dispatch, Action::Command("old-click".into()));
        queue.park();
        assert!(queue.is_empty());
        queue.push_at(old_dispatch, Action::Command("inflight-after-park".into()));
        assert!(queue.is_empty());
        assert!(queue.resume());
        queue.push_at(old_dispatch, Action::Command("inflight-after-resume".into()));
        assert!(queue.is_empty());
        queue.push_at(queue.current(), Action::Command("new-click".into()));
        assert!(matches!(queue.take().as_slice(), [Action::Command(command)] if command == "new-click"));
        queue.epoch = u64::MAX;
        assert!(!queue.resume());
        assert!(queue.current().is_none());
    }

    #[test]
    fn owned_proxy_epoch_rejects_stale_query_completion_and_duplicate_zeroarg_start() {
        use crate::frontend_lobby::{Api, Catalog, Scene};
        let modes = kv::parse("gameTypes { classic { value 0 gameModes { competitive { value 1 mapgroupsSP { mg_de_dust2 0 } } } } } mapgroups { mg_de_dust2 { maps { de_dust2 0 } } } maps { de_dust2 {} }").unwrap();
        let fs = Arc::new(vfs::Vfs::new());
        let data = Arc::new(Mutex::new(Data {
            inventory: InventoryView::new(&fs), fs, settings: Settings::default(),
            localization: Default::default(), game_modes: modes.clone(),
            state: AppState::Menu, menu_open: true, loading: String::new(), loading_map: None, player_name: "You".into(), movie_ready: HashSet::new(), home_bootstrapped: false, home_retired: false, pause_surface: Default::default(),
        deferred_pause: Default::default(),
        }));
        let mut scene = Scene::new(Catalog::installed(&crate::ClientOptions::default(), &["de_dust2".into()]));
        assert!(scene.begin(&modes)); assert!(scene.open_lobby());
        let old_lobby = scene.proxy(Api::Lobby);
        let actions = Arc::new(Mutex::new(ActionQueue::default()));
        let mut bridge = Bridge { dispatch_epoch: None, data, actions: actions.clone(), seen: Default::default(), roots: Default::default(), requested: Default::default(),
            options: Default::default(), offline: Default::default(), offline_open: false, quit: Default::default(), lobby: Arc::new(Mutex::new(scene)) };
        let mut calls = Calls::default();
        bridge.call(&old_lobby, "ChooseGameSettings", &[], &mut calls);
        let old_picker = bridge.lobby.lock().unwrap().proxy(Api::Picker);
        bridge.lobby.lock().unwrap().cancel();
        { let mut scene = bridge.lobby.lock().unwrap(); assert!(scene.begin(&modes)); assert!(scene.open_lobby()); }
        bridge.roots.insert("LobbyBrowser".into(), "_level17".into());
        let current_lobby = bridge.lobby.lock().unwrap().proxy(Api::Lobby);
        bridge.call(&old_lobby, "OnUnload", &["mc:_level17".into()], &mut calls);
        bridge.call("LobbyBrowser", "OnUnload", &["mc:_level17".into()], &mut calls);
        bridge.call(&current_lobby, "OnUnload", &["mc:_level17".into()], &mut calls);
        assert_eq!(bridge.lobby.lock().unwrap().phase(), crate::frontend_lobby::Phase::Lobby);
        bridge.call(&current_lobby, "ChooseGameSettings", &[], &mut calls);
        let picker = bridge.lobby.lock().unwrap().proxy(Api::Picker);
        assert!(!bridge.data.lock().unwrap().movie_ready.contains("LobbyScreen"));
        bridge.call("LobbyScreen", "OnReady", &[], &mut calls);
        assert!(bridge.data.lock().unwrap().movie_ready.contains("LobbyScreen"));
        bridge.call("LobbyScreen", "OnLoadFinished", &["mc:_level16".into()], &mut calls);
        assert!(!bridge.data.lock().unwrap().movie_ready.contains("LobbyScreen"));
        bridge.call("StartSinglePlayer", "OnLoadFinished", &["mc:_level15".into()], &mut calls);
        let query = "System { network offline } Game { type classic mode competitive mapgroupname mg_de_dust2 skirmishmode undefined } Options { action create anytypemode 0 }";
        assert_eq!(bridge.call(&old_picker, "CheckGameSettingsRequirements", &[query.into()], &mut calls), None);
        bridge.call(&picker, "SetCustomBotDifficulty", &[0.0.into()], &mut calls);
        assert_eq!(bridge.call(&picker, "CheckGameSettingsRequirements", &[query.into()], &mut calls), Some(true.into()));
        bridge.call(&picker, "SetMatchmakingQuery", &[query.into()], &mut calls);
        let selection = ["0".into(), "1".into(), Value::Number(f64::NAN), "mg_de_dust2".into(), 0.0.into()];
        // Identical cached root handles do not give raw or retained-old
        // lifecycle callbacks permission to cancel the reopened picker.
        for object in [&old_picker, "StartSinglePlayer"] {
            bridge.call(object, "OnUnload", &["mc:_level15".into()], &mut calls);
        }
        bridge.call("LobbyScreen", "ConfirmExit", &[], &mut calls);
        bridge.call("LobbyScreen", "OnUnload", &["mc:_level16".into()], &mut calls);
        bridge.call(&old_lobby, "OnUnload", &["mc:_level16".into()], &mut calls);
        bridge.call(&picker, "OnUnload", &["mc:_level16".into()], &mut calls);
        assert!(bridge.lobby.lock().unwrap().picker());
        bridge.call(&old_picker, "UpdatedSelections", &selection, &mut calls);
        bridge.call(&old_lobby, "LaunchGame", &[], &mut calls);
        assert!(actions.lock().unwrap().is_empty());
        bridge.call(&picker, "UpdatedSelections", &selection, &mut calls);
        bridge.call("StartSinglePlayer", "OnUnload", &["mc:_level15".into()], &mut calls);
        bridge.call(&old_picker, "OnUnload", &["mc:_level15".into()], &mut calls);
        assert!(bridge.lobby.lock().unwrap().staged());
        bridge.call(&picker, "OnUnload", &["mc:_level15".into()], &mut calls);
        assert!(bridge.lobby.lock().unwrap().staged());
        let lobby = bridge.lobby.lock().unwrap().proxy(Api::Lobby);
        bridge.call(&lobby, "LaunchGame", &[], &mut calls);
        bridge.call(&lobby, "LaunchGame", &[], &mut calls);
        let actions = actions.lock().unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Host(start, name) if start.map.options.map == "de_dust2" && start.map.options.bots == 0 && name == "You"));
        drop(actions);
        bridge.call(&old_lobby, "ConfirmExit", &[], &mut calls);
        assert!(bridge.lobby.lock().unwrap().active());
        bridge.call(&lobby, "ConfirmExit", &[], &mut calls);
        assert!(!bridge.lobby.lock().unwrap().active());
        assert!(matches!(bridge.actions.lock().unwrap().last(), Some(Action::LobbyHome)));
        { let mut scene = bridge.lobby.lock().unwrap(); assert!(scene.begin(&modes)); assert!(scene.open_lobby()); }
        let reopened = bridge.lobby.lock().unwrap().proxy(Api::Lobby);
        bridge.call(&reopened, "ChooseGameSettings", &[], &mut calls);
        let current_picker = bridge.lobby.lock().unwrap().proxy(Api::Picker);
        bridge.call(&current_picker, "OnUnload", &["mc:_level15".into()], &mut calls);
        assert_eq!(bridge.lobby.lock().unwrap().phase(), crate::frontend_lobby::Phase::Lobby);
        assert!(!bridge.lobby.lock().unwrap().staged());
        assert!(bridge.lobby.lock().unwrap().receiver(&current_picker).is_none());
        let prior = bridge.lobby.lock().unwrap().proxy(Api::Lobby);
        { let mut scene = bridge.lobby.lock().unwrap(); scene.cancel(); assert!(scene.begin(&modes)); }
        bridge.call(&prior, "OnUnload", &["mc:_level17".into()], &mut calls);
        bridge.call("LobbyBrowser", "OnUnload", &["mc:_level17".into()], &mut calls);
        assert_eq!(bridge.lobby.lock().unwrap().phase(), crate::frontend_lobby::Phase::Browser);
        let browser = bridge.lobby.lock().unwrap().proxy(Api::Lobby);
        bridge.call(&browser, "OnUnload", &["mc:_level17".into()], &mut calls);
        assert!(!bridge.lobby.lock().unwrap().active());
    }

    #[test]
    fn offline_navigation_revokes_audio_even_if_options_becomes_ready_late() {
        let fs = Arc::new(vfs::Vfs::new());
        let data = Arc::new(Mutex::new(Data {
            inventory: InventoryView::new(&fs), fs, settings: Settings::default(),
            localization: Default::default(), game_modes: Default::default(),
            state: AppState::Menu, menu_open: true, loading: String::new(),
            loading_map: None, player_name: "You".into(), movie_ready: Default::default(), home_bootstrapped: false, home_retired: false,
            pause_surface: Default::default(), deferred_pause: Default::default(),
        }));
        let mut bridge = Bridge { dispatch_epoch: None,
            data: data.clone(), actions: Default::default(), seen: Default::default(),
            roots: HashMap::from([("OptionsMenu".into(), "_level12".into()),
                ("StartSinglePlayer".into(), "_level15".into())]),
            requested: Default::default(), options: Default::default(),
            offline: Default::default(), offline_open: false, quit: Default::default(),
            lobby: Arc::new(Mutex::new(crate::frontend_lobby::Scene::new(
                crate::frontend_lobby::Catalog::installed(&crate::ClientOptions::default(), &[])))),
        };
        let mut calls = Calls::default();
        bridge.call("MainMenu", "BasePanelRunCommand", &["OpenAudioSettingsDialog".into()], &mut calls);
        assert!(data.lock().unwrap().pause_surface.options_requested());
        bridge.call("MainMenu", "BasePanelRunCommand", &["OpenCreateSinglePlayerGameDialog".into()], &mut calls);
        assert!(!data.lock().unwrap().pause_surface.options_requested());
        assert!(bridge.offline_open);
        bridge.call("OptionsMenu", "OnReady", &[], &mut calls);
        bridge.call("OptionsMenu", "OnCancel", &[], &mut calls);
        assert!(!data.lock().unwrap().pause_surface.options_requested());
        assert!(bridge.offline_open, "late Audio cancellation must not replace the picker");
        bridge.call("MainMenu", "BasePanelRunCommand", &["OpenAudioSettingsDialog".into()], &mut calls);
        assert!(!bridge.offline_open);
        bridge.call("StartSinglePlayer", "OnReady", &[], &mut calls);
        assert!(data.lock().unwrap().pause_surface.options_requested());
        assert!(!bridge.offline_open, "late picker readiness must not replace Audio");
    }

    #[test]
    fn quit_confirmation_preserves_picker_and_rejects_cancelled_or_parked_callbacks() {
        let fs = Arc::new(vfs::Vfs::new());
        let data = Arc::new(Mutex::new(Data {
            inventory: InventoryView::new(&fs), fs, settings: Settings::default(),
            localization: Default::default(), game_modes: Default::default(),
            state: AppState::Menu, menu_open: true, loading: String::new(),
            loading_map: None, player_name: "You".into(), movie_ready: Default::default(),
            home_bootstrapped: false, home_retired: false,
            pause_surface: Default::default(), deferred_pause: Default::default(),
        }));
        let mut bridge = Bridge {
            dispatch_epoch: None, data: data.clone(), actions: Default::default(),
            seen: Default::default(), roots: HashMap::from([("MessageBox".into(), "_level500".into())]),
            requested: Default::default(), options: Default::default(),
            offline: Default::default(), offline_open: false, quit: Default::default(),
            lobby: Arc::new(Mutex::new(crate::frontend_lobby::Scene::new(
                crate::frontend_lobby::Catalog::installed(&crate::ClientOptions::default(), &[])))),
        };
        let mut calls = Calls::default();
        bridge.call("MainMenu", "BasePanelRunCommand", &["OpenCreateSinglePlayerGameDialog".into()], &mut calls);
        bridge.call("MainMenu", "BasePanelRunCommand", &["Quit".into(), "false".into()], &mut calls);
        let first = bridge.quit.receiver();
        assert!(bridge.offline_open && bridge.quit.active);
        assert!(bridge.actions.lock().unwrap().is_empty(), "power must not exit before confirmation");
        bridge.call("MainMenu", "BasePanelRunCommand", &["Quit".into()], &mut calls);
        assert_eq!(bridge.quit.receiver(), first, "repeat power during loading must keep one opening");
        bridge.call(&first, "OnButtonPress", &["KEY_ENTER".into()], &mut calls);
        assert!(bridge.actions.lock().unwrap().is_empty(), "pending dialog cannot confirm before it is shown");
        bridge.call("MessageBox", "OnReady", &[], &mut calls);
        assert!(bridge.quit.shown);
        bridge.call("MessageBox", "OnButtonPress", &["KEY_ENTER".into()], &mut calls);
        bridge.call(&first, "OnButtonPress", &[48.0.into()], &mut calls);
        assert!(bridge.actions.lock().unwrap().is_empty(), "unowned/malformed confirmation must not exit");
        bridge.call(&first, "OnButtonPress", &["KEY_ESCAPE".into()], &mut calls);
        assert!(!bridge.quit.active && bridge.offline_open, "Cancel must retain the picker");
        bridge.call("MainMenu", "BasePanelRunCommand", &["Quit".into()], &mut calls);
        assert_ne!(bridge.quit.receiver(), first);
        bridge.call(&first, "OnButtonPress", &["KEY_ENTER".into()], &mut calls);
        assert!(bridge.actions.lock().unwrap().is_empty());
        let second = bridge.quit.receiver();
        assert!(bridge.lifecycle(false, &mut calls));
        bridge.call("MessageBox", "OnReady", &[], &mut calls);
        bridge.call(&second, "OnButtonPress", &["KEY_ENTER".into()], &mut calls);
        assert!(!bridge.quit.active && bridge.actions.lock().unwrap().is_empty());
        assert!(bridge.lifecycle(true, &mut calls));
        bridge.call("MainMenu", "BasePanelRunCommand", &["Quit".into()], &mut calls);
        let current = bridge.quit.receiver();
        bridge.call(&current, "OnButtonPress", &["KEY_ENTER".into()], &mut calls);
        bridge.call(&current, "OnButtonPress", &["KEY_ENTER".into()], &mut calls);
        let actions = bridge.actions.lock().unwrap();
        assert_eq!(actions.len(), 1, "current confirmation must exit exactly once");
        assert!(matches!(&actions[0], Action::Command(command) if command == "quit"));
    }

    #[test]
    fn numeric_entry_updates_the_snapshot_before_the_following_native_getter() {
        let fs = Arc::new(vfs::Vfs::new());
        let data = Arc::new(Mutex::new(Data {
            inventory: InventoryView::new(&fs),
            fs,
            settings: Settings::default(),
            localization: crate::localize::Localization::default().into(),
            game_modes: kv::Block::new(),
            state: AppState::Menu,
            menu_open: false,
            loading: String::new(),
            loading_map: None,
            player_name: "You".into(),
        movie_ready: HashSet::new(), home_bootstrapped: false, home_retired: false,
        pause_surface: Default::default(),
        deferred_pause: Default::default(),
        }));
        let actions = Arc::new(Mutex::new(ActionQueue::default()));
        let mut bridge = Bridge { dispatch_epoch: None,
            data,
            actions: actions.clone(),
            seen: HashMap::new(),
            roots: HashMap::new(),
            requested: HashSet::new(),
            options: crate::frontend_options::Scene::default(),
            offline: crate::frontend_game::OfflineLaunch::default(),
            offline_open: false, quit: Default::default(),
            lobby: Arc::new(Mutex::new(crate::frontend_lobby::Scene::new(
                crate::frontend_lobby::Catalog::installed(&crate::ClientOptions::default(), &[]),
            ))),
        };
        let mut calls = Calls::default();
        bridge.call("GameInterface", "SetConvar", &["volume".into(), "0.33".into()], &mut calls);
        let value = bridge.call("GameInterface", "GetConvarNumber", &["volume".into()], &mut calls).unwrap();
        assert!((value.num() - 0.33).abs() < 1e-6);
        let actions = actions.lock().unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Convar(name, Value::String(value)) if name == "volume" && value == "0.33"));
    }

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
