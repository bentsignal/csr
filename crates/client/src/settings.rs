//! Persistent local settings shared by menus, input, audio and HUD.
use bevy::prelude::*;

pub const ACTIONS: [&str; 30] = [
    "Move forward",
    "Move backward",
    "Move left",
    "Move right",
    "Jump",
    "Crouch",
    "Walk",
    "Use",
    "Reload",
    "Buy menu",
    "Drop weapon",
    "Scoreboard",
    "Inspect weapon",
    "Primary attack",
    "Secondary attack",
    "Select HE grenade",
    "Select flashbang",
    "Select smoke grenade",
    "Select Molotov",
    "Select incendiary grenade",
    "Select decoy",
    "Last weapon",
    "Primary weapon",
    "Pistol",
    "Knife / Zeus",
    "Grenades",
    "Bomb",
    "Next weapon",
    "Previous weapon",
    "Select fire grenade",
];
pub const ACTION_COMMANDS: [&str; 30] = [
    "+forward", "+back", "+moveleft", "+moveright", "+jump", "+duck", "+speed",
    "+use", "+reload", "buymenu", "drop", "+showscores", "+lookatweapon",
    "+attack", "+attack2",
    "use weapon_hegrenade", "use weapon_flashbang", "use weapon_smokegrenade",
    "use weapon_molotov", "use weapon_incgrenade", "use weapon_decoy",
    "lastinv", "slot1", "slot2", "slot3", "slot4", "slot5", "invnext", "invprev",
    "slot10",
];
pub const KEY_FIRE_GRENADE: usize = 29;
pub const KEY_ATTACK: usize = 13;
pub const KEY_ATTACK2: usize = 14;
pub const KEY_GRENADE_FIRST: usize = 15;
pub const KEY_SELECTION_FIRST: usize = 21;
pub const SELECTION_SLOTS: [u8; 8] = [sim::SLOT_LAST, 1, 2, 3, 4, 5, sim::SLOT_NEXT, sim::SLOT_PREVIOUS];
pub const GRENADE_NAMES: [&str; 6] = [
    "weapon_hegrenade", "weapon_flashbang", "weapon_smokegrenade",
    "weapon_molotov", "weapon_incgrenade", "weapon_decoy",
];
pub const MOUSE_BUTTONS: [MouseButton; 5] = [
    MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::Back, MouseButton::Forward,
];

/// Supported gameplay commands only; console text is never executed by input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction { None, Attack, Attack2, Action(usize), Previous, Next, Last }
impl MouseAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Unbound", Self::Attack => "Primary attack", Self::Attack2 => "Secondary attack",
            Self::Action(index) => ACTIONS.get(index).copied().unwrap_or("Unbound"),
            Self::Previous => "Previous weapon", Self::Next => "Next weapon", Self::Last => "Last weapon",
        }
    }
    pub fn command(self) -> &'static str {
        match self {
            Self::None => "", Self::Attack => "+attack", Self::Attack2 => "+attack2",
            Self::Action(index) => ACTION_COMMANDS.get(index).copied().unwrap_or(""),
            Self::Previous => "invprev", Self::Next => "invnext", Self::Last => "lastinv",
        }
    }
    pub fn parse(command: &str) -> Option<Self> {
        Some(match command {
            "" => Self::None, "+attack" => Self::Attack, "+attack2" => Self::Attack2,
            "invprev" => Self::Previous, "invnext" => Self::Next, "lastinv" => Self::Last,
            other => Self::Action(ACTION_COMMANDS.iter().position(|value| *value == other)?),
        })
    }
}
pub const KEYS: &[KeyCode] = &[
    KeyCode::KeyA,
    KeyCode::KeyB,
    KeyCode::KeyC,
    KeyCode::KeyD,
    KeyCode::KeyE,
    KeyCode::KeyF,
    KeyCode::KeyG,
    KeyCode::KeyH,
    KeyCode::KeyI,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyM,
    KeyCode::KeyN,
    KeyCode::KeyO,
    KeyCode::KeyP,
    KeyCode::KeyQ,
    KeyCode::KeyR,
    KeyCode::KeyS,
    KeyCode::KeyT,
    KeyCode::KeyU,
    KeyCode::KeyV,
    KeyCode::KeyW,
    KeyCode::KeyX,
    KeyCode::KeyY,
    KeyCode::KeyZ,
    KeyCode::Space,
    KeyCode::ShiftLeft,
    KeyCode::ControlLeft,
    KeyCode::AltLeft,
    KeyCode::Tab,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Insert,
    KeyCode::Delete,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::CapsLock,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit0, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
    KeyCode::ShiftRight, KeyCode::ControlRight, KeyCode::AltRight, KeyCode::Enter,
    KeyCode::Backquote, KeyCode::Backslash, KeyCode::BracketLeft, KeyCode::BracketRight,
    KeyCode::Comma, KeyCode::Period, KeyCode::Quote, KeyCode::Semicolon,
    KeyCode::Slash, KeyCode::Minus, KeyCode::Equal,
    KeyCode::F1, KeyCode::F2, KeyCode::F3, KeyCode::F4, KeyCode::F5, KeyCode::F6,
    KeyCode::F7, KeyCode::F8, KeyCode::F9, KeyCode::F10, KeyCode::F11, KeyCode::F12,
];

/// Physical key names used by Source configs and the console. Escape remains
/// reserved for navigation. The console binding owns its selected physical key.
pub fn source_key(name: &str) -> Option<KeyCode> {
    let name = name.to_ascii_lowercase();
    let alias = match name.as_str() {
        "ctrl" => "controlleft", "rctrl" => "controlright",
        "shift" => "shiftleft", "rshift" => "shiftright",
        "alt" => "altleft", "ralt" => "altright",
        "uparrow" => "arrowup", "downarrow" => "arrowdown",
        "leftarrow" => "arrowleft", "rightarrow" => "arrowright",
        "ins" => "insert", "del" => "delete", "pgup" => "pageup", "pgdn" => "pagedown",
        "capslock" => "capslock", "`" => "backquote", "\\" => "backslash", "[" => "bracketleft", "]" => "bracketright",
        "," => "comma", "." => "period", "'" => "quote", ";" => "semicolon",
        "/" => "slash", "-" => "minus", "=" => "equal", other => other,
    };
    KEYS.iter().copied().find(|key| {
        let debug = format!("{key:?}").to_ascii_lowercase();
        debug == alias || debug.strip_prefix("key") == Some(alias) || debug.strip_prefix("digit") == Some(alias)
    })
}

/// A wheel detent is a one-shot request, never a held jump button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelAction { None, Jump, Previous, Next }
impl WheelAction {
    pub fn label(self) -> &'static str { match self { Self::None => "Unbound", Self::Jump => "Jump", Self::Previous => "Previous weapon", Self::Next => "Next weapon" } }
    pub fn command(self) -> &'static str { match self { Self::None => "", Self::Jump => "+jump", Self::Previous => "invprev", Self::Next => "invnext" } }
    pub fn parse(value: &str) -> Option<Self> { Some(match value { "" => Self::None, "+jump" => Self::Jump, "invprev" => Self::Previous, "invnext" => Self::Next, _ => return None }) }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Human-readable first-run control provenance, retained across saves.
    pub controls_source: String,
    /// Local UI action, deliberately outside the authoritative TickInput model.
    pub console_key: Option<KeyCode>,
    pub volume: f32,
    pub audio_options: crate::audio_options::AudioOptions,
    pub show_fps: bool,
    pub wheel_up: WheelAction,
    pub wheel_down: WheelAction,
    pub menu_blur: bool,
    pub sensitivity: f32,
    pub invert_y: bool,
    pub exposure: f32,
    pub vsync: bool,
    pub fullscreen: bool,
    pub resolution: usize,
    pub msaa: bool,
    pub blood: bool,
    pub hitmarker: bool,
    pub cross_color: usize,
    pub cross_size: f32,
    pub cross_gap: f32,
    pub cross_thickness: f32,
    pub cross_dot: bool,
    pub radar_scale: f32,
    /// Display size is independent of the overview's map zoom.
    pub radar_hud_size: f32,
    pub radar_rotate: bool,
    pub close_on_buy: bool,
    pub binds: [Option<KeyCode>; 30],
    pub mouse_binds: [MouseAction; 5],
}
pub const RESOLUTIONS: [(u32, u32); 4] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
impl Default for Settings {
    fn default() -> Self {
        Self {
            controls_source: String::new(),
            console_key: Some(KeyCode::Backquote),
            volume: 0.5,
            audio_options: Default::default(),
            show_fps: false,
            wheel_up: WheelAction::Previous,
            wheel_down: WheelAction::Next,
            menu_blur: true,
            sensitivity: 1.0,
            invert_y: false,
            exposure: 2.0,
            vsync: false,
            fullscreen: false,
            resolution: 1,
            msaa: true,
            blood: true,
            hitmarker: false,
            cross_color: 0,
            cross_size: 8.0,
            cross_gap: 4.0,
            cross_thickness: 2.0,
            cross_dot: false,
            // Installed game_options sliderhome and ordinary rotating radar.
            radar_scale: 0.5,
            // Neutral migration fallback; the larger retained reference uses
            // unmatched settings and does not establish an ordinary default.
            radar_hud_size: 1.0,
            radar_rotate: true,
            // Preserve the existing stay-open behavior; native default unmeasured.
            close_on_buy: false,
            binds: [
                Some(KeyCode::KeyW),
                Some(KeyCode::KeyS),
                Some(KeyCode::KeyA),
                Some(KeyCode::KeyD),
                Some(KeyCode::Space),
                Some(KeyCode::ControlLeft),
                Some(KeyCode::ShiftLeft),
                Some(KeyCode::KeyE),
                Some(KeyCode::KeyR),
                Some(KeyCode::KeyB),
                Some(KeyCode::KeyG),
                Some(KeyCode::Tab),
                Some(KeyCode::KeyF),
                None, // Mouse attacks remain the defaults for existing configs.
                None,
                None, None, None, None, None, None, // Direct grenade selection is opt-in.
                Some(KeyCode::KeyQ),
                Some(KeyCode::Digit1), Some(KeyCode::Digit2), Some(KeyCode::Digit3),
                Some(KeyCode::Digit4), Some(KeyCode::Digit5),
                None, None, // Keyboard next/previous default to unbound.
                None, // Either team's fire grenade (CS2 slot10).
            ],
            mouse_binds: [MouseAction::Attack, MouseAction::Attack2, MouseAction::None, MouseAction::None, MouseAction::None],
        }
    }
}
/// Relative to the player data folder (the launcher runs inside it).
pub fn path() -> std::path::PathBuf {
    std::path::PathBuf::from("config/user.cfg")
}
impl Settings {
    /// Reset only the keyboard/mouse model, including the local console key.
    /// Raw device input and scoped FOV scaling have no persisted preferences.
    /// Audio, video, crosshair, radar and other gameplay/UI preferences survive.
    pub(crate) fn restore_controls_defaults(&mut self) {
        let defaults = Self::default();
        self.binds = defaults.binds;
        self.console_key = defaults.console_key;
        self.mouse_binds = defaults.mouse_binds;
        self.wheel_up = defaults.wheel_up;
        self.wheel_down = defaults.wheel_down;
        self.sensitivity = defaults.sensitivity;
        self.invert_y = defaults.invert_y;
        self.controls_source = defaults.controls_source;
    }
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let v = value.parse::<f32>().ok().filter(|x| x.is_finite());
            match key {
                "controls_source" => s.controls_source = value.into(),
                "console_key" => {
                    if value.is_empty() { s.console_key = None; }
                    else if let Some(key) = source_key(value) { s.set_console_key(key); }
                }
                "mouse1" | "mouse2" | "mouse3" | "mouse4" | "mouse5" => {
                    if let Some(action) = MouseAction::parse(value) {
                        s.mouse_binds[key.as_bytes()[5] as usize - b'1' as usize] = action;
                    }
                }
                "cl_showfps" => s.show_fps = value == "1",
                "mwheelup" => { if let Some(action) = WheelAction::parse(value) { s.wheel_up = action; } }
                "mwheeldown" => { if let Some(action) = WheelAction::parse(value) { s.wheel_down = action; } }
                "menu_blur" => s.menu_blur=value=="true",
                "volume" => {
                    if let Some(v) = v {
                        s.volume = v.clamp(0.0, 1.0);
                    }
                }
                "sensitivity" => {
                    if let Some(v) = v {
                        s.sensitivity = v.clamp(0.1, 10.0);
                    }
                }
                "exposure" => {
                    if let Some(v) = v {
                        s.exposure = v.clamp(0.5, 4.0);
                    }
                }
                "cross_size" => {
                    if let Some(v) = v {
                        s.cross_size = v.clamp(2.0, 20.0);
                    }
                }
                "cross_gap" => {
                    if let Some(v) = v {
                        s.cross_gap = v.clamp(0.0, 16.0);
                    }
                }
                "cross_thickness" => {
                    if let Some(v) = v {
                        s.cross_thickness = v.clamp(1.0, 6.0);
                    }
                }
                "cross_color" => {
                    if let Some(v) = v {
                        s.cross_color = (v as usize).min(4);
                    }
                }
                "radar_scale" => {
                    if let Some(v) = v { s.radar_scale = v.clamp(0.4, 1.0); }
                }
                "radar_hud_size" => {
                    if let Some(v) = v { s.radar_hud_size = v.clamp(0.8, 1.3); }
                }
                "radar_rotate" => s.radar_rotate = value == "true",
                "closeonbuy" => {
                    if let Ok(number) = value.parse::<f64>() { s.set_convar_number(key, number); }
                }
                "resolution" => {
                    if let Some(v) = v {
                        s.resolution = (v as usize).min(3);
                    }
                }
                "invert_y" => s.invert_y = value == "true",
                "vsync" => s.vsync = value == "true",
                "fullscreen" => s.fullscreen = value == "true",
                "msaa" => s.msaa = value == "true",
                "blood" => s.blood = value == "true",
                "hitmarker" => s.hitmarker = value == "true",
                "cross_dot" => s.cross_dot = value == "true",
                _ => {
                    s.audio_options.parse_entry(key, value);
                    if let Some(index) = key
                        .strip_prefix("bind")
                        .and_then(|i| i.parse::<usize>().ok())
                        .filter(|i| *i < s.binds.len())
                    {
                        if value.is_empty() {
                            s.binds[index] = None;
                        } else if let Some(key) = KEYS.iter().find(|k| format!("{k:?}") == value) {
                            s.rebind(index, *key);
                        }
                    }
                }
            }
        }
        s
    }
    /// The weapon list describes an effective direct selection binding, never
    /// a slot number that may no longer be bound to that key.
    pub(crate) fn selection_binding_label(&self, slot: u8) -> String {
        let Some(index) = (1..=5).contains(&slot).then_some(KEY_SELECTION_FIRST + slot as usize) else { return String::new(); };
        self.action_binding_label(index)
    }
    /// Keyboard and mouse are independent alternatives in the shared model.
    /// Show both, including bindings made through the console or import.
    pub(crate) fn action_binding_label(&self, index: usize) -> String {
        let Some(command) = ACTION_COMMANDS.get(index) else { return String::new(); };
        let mut labels: Vec<_> = self.binds[index].map(key_name).into_iter().collect();
        labels.extend(self.mouse_binds.iter().enumerate()
            .filter(|(_, action)| action.command() == *command)
            .map(|(button, _)| format!("Mouse {}", button + 1)));
        labels.join(" / ")
    }
    pub fn encode(&self) -> String {
        let mut text = format!(
            "volume={}\nsensitivity={}\ninvert_y={}\nexposure={}\nvsync={}\nfullscreen={}\nresolution={}\nmsaa={}\nblood={}\nhitmarker={}\ncross_color={}\ncross_size={}\ncross_gap={}\ncross_thickness={}\ncross_dot={}\n",
            self.volume,
            self.sensitivity,
            self.invert_y,
            self.exposure,
            self.vsync,
            self.fullscreen,
            self.resolution,
            self.msaa,
            self.blood,
            self.hitmarker,
            self.cross_color,
            self.cross_size,
            self.cross_gap,
            self.cross_thickness,
            self.cross_dot
        );
        text.push_str(&format!("cl_showfps={}\nmwheelup={}\nmwheeldown={}\n", u8::from(self.show_fps), self.wheel_up.command(), self.wheel_down.command()));
        for (index, action) in self.mouse_binds.iter().enumerate() {
            text.push_str(&format!("mouse{}={}\n", index + 1, action.command()));
        }
        text.push_str(&format!("menu_blur={}\n", self.menu_blur));
        text.push_str(&format!("controls_source={}\n", self.controls_source.replace(['\r', '\n'], " ")));
        text.push_str(&format!("radar_scale={}\nradar_hud_size={}\nradar_rotate={}\n", self.radar_scale, self.radar_hud_size, self.radar_rotate));
        text.push_str(&format!("closeonbuy={}\n", u8::from(self.close_on_buy)));
        for (i, key) in self.binds.iter().enumerate() {
            let name = key.map(|key| format!("{key:?}")).unwrap_or_default();
            text.push_str(&format!("bind{i}={name}\n"));
        }
        text.push_str(&format!("console_key={}\n", self.console_key.map(|key| format!("{key:?}")).unwrap_or_default()));
        self.audio_options.encode_into(&mut text);
        text
    }
    pub fn save(&self) -> std::io::Result<()> {
        let p = path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let temp = p.with_extension("tmp");
        std::fs::write(&temp, self.encode())?;
        std::fs::rename(temp, p)
    }
    pub fn rebind(&mut self, index: usize, key: KeyCode) {
        if index >= self.binds.len() || !KEYS.contains(&key) {
            return;
        }
        let old = self.binds[index];
        if self.console_key == Some(key) { self.console_key = None; }
        if let Some(other) = self.binds.iter().position(|k| *k == Some(key)) {
            self.binds[other] = old;
        }
        self.binds[index] = Some(key);
    }
    /// Move this action's first mouse binding to the chosen button, swapping
    /// its former command onto the old button. Keyboard alternatives survive.
    /// Use the console's canonical variants (attacks/weapon cycling are special).
    pub(crate) fn rebind_mouse(&mut self, index: usize, button: MouseButton) {
        let Some(command) = ACTION_COMMANDS.get(index) else { return; };
        let Some(target) = MOUSE_BUTTONS.iter().position(|candidate| *candidate == button) else { return; };
        let action = MouseAction::parse(command).unwrap();
        if self.mouse_binds[target] == action { return; }
        if let Some(old) = self.mouse_binds.iter().position(|candidate| *candidate == action) {
            self.mouse_binds[old] = self.mouse_binds[target];
        }
        self.mouse_binds[target] = action;
    }
    pub(crate) fn clear_action_binding(&mut self, index: usize) {
        let Some(command) = ACTION_COMMANDS.get(index) else { return; };
        self.binds[index] = None;
        for action in &mut self.mouse_binds {
            if action.command() == *command { *action = MouseAction::None; }
        }
    }
    pub fn unbind_key(&mut self, key: KeyCode) {
        if self.console_key == Some(key) { self.console_key = None; }
        for binding in &mut self.binds {
            if *binding == Some(key) { *binding = None; }
        }
    }
    pub fn set_console_key(&mut self, key: KeyCode) {
        if !KEYS.contains(&key) { return; }
        self.unbind_key(key);
        self.console_key = Some(key);
    }
    /// The settings page offers only commands the input sampler implements.
    pub fn cycle_mouse_binding(&mut self, index: usize, direction: i32) {
        let Some(binding) = self.mouse_binds.get_mut(index) else { return };
        let actions: Vec<_> = std::iter::once(MouseAction::None)
            .chain(ACTION_COMMANDS.iter().filter_map(|command| MouseAction::parse(command))).collect();
        let current = actions.iter().position(|action| *action == *binding).unwrap_or(0);
        *binding = actions[(current as i32 + direction.signum()).rem_euclid(actions.len() as i32) as usize];
    }
    pub fn cycle_wheel_binding(&mut self, up: bool, direction: i32) {
        let actions = [WheelAction::None, WheelAction::Jump, WheelAction::Previous, WheelAction::Next];
        let binding = if up { &mut self.wheel_up } else { &mut self.wheel_down };
        let current = actions.iter().position(|action| *action == *binding).unwrap_or(0);
        *binding = actions[(current as i32 + direction.signum()).rem_euclid(actions.len() as i32) as usize];
    }
    pub fn mouse_pressed(&self, action: MouseAction, mouse: &ButtonInput<MouseButton>, blocked: u8) -> bool {
        MOUSE_BUTTONS.iter().enumerate().any(|(index, button)|
            blocked & (1 << index) == 0 && self.mouse_binds[index] == action && mouse.pressed(*button))
    }
    pub fn mouse_just_pressed(&self, action: MouseAction, mouse: &ButtonInput<MouseButton>, blocked: u8) -> bool {
        MOUSE_BUTTONS.iter().enumerate().any(|(index, button)|
            blocked & (1 << index) == 0 && self.mouse_binds[index] == action && mouse.just_pressed(*button))
    }
    pub fn action_pressed(&self, index: usize, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>, blocked: u8) -> bool {
        self.binds[index].is_some_and(|key| keys.pressed(key)) || self.mouse_pressed(MouseAction::Action(index), mouse, blocked)
    }
    pub fn action_just_pressed(&self, index: usize, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>, blocked: u8) -> bool {
        self.binds[index].is_some_and(|key| keys.just_pressed(key)) || self.mouse_just_pressed(MouseAction::Action(index), mouse, blocked)
    }
    pub fn crosshair_color(&self) -> Color {
        [
            Color::srgb(0.2, 1.0, 0.2),
            Color::srgb(0.2, 0.2, 1.0),
            Color::srgb(1.0, 0.85, 0.2),
            Color::srgb(0.2, 1.0, 1.0),
            Color::srgb(1.0, 0.2, 0.2),
        ][self.cross_color.min(4)]
    }

    /// Preserve the saved palette index while presenting Source's numeric IDs.
    pub(crate) fn convar_number(&self, name: &str) -> Option<f64> {
        Some(match name {
            "volume" => self.volume as f64,
            "fullscreen_restart" => if self.fullscreen { 2.0 } else { 0.0 },
            "sys_vsync" | "mat_vsync" => u8::from(self.vsync) as f64,
            "mat_antialias" => if self.msaa { 4.0 } else { 0.0 },
            "cl_showfps" => u8::from(self.show_fps) as f64,
            "cl_crosshairsize" => self.cross_size as f64,
            "cl_crosshairgap" => self.cross_gap as f64,
            "cl_crosshairthickness" => self.cross_thickness as f64,
            "cl_crosshairdot" => u8::from(self.cross_dot) as f64,
            "sensitivity" => self.sensitivity as f64,
            "m_pitch" => if self.invert_y { -0.022 } else { 0.022 },
            "cl_crosshaircolor" => [1., 3., 2., 4., 0.][self.cross_color.min(4)],
            "cl_radar_scale" => self.radar_scale as f64,
            "cl_hud_radar_scale" => self.radar_hud_size as f64,
            "cl_radar_rotate" => u8::from(self.radar_rotate) as f64,
            "closeonbuy" => u8::from(self.close_on_buy) as f64,
            _ => return self.audio_options.convar_number(name),
        })
    }

    pub(crate) fn set_convar_number(&mut self, name: &str, number: f64) -> bool {
        if !number.is_finite() { return false; }
        match name {
            "fullscreen_restart" if number == 0.0 || number == 2.0 => self.fullscreen = number == 2.0,
            "sys_vsync" | "mat_vsync" if number == 0.0 || number == 1.0 => self.vsync = number == 1.0,
            "mat_antialias" if number == 0.0 || number == 4.0 => self.msaa = number == 4.0,
            "cl_showfps" if number == 0.0 || number == 1.0 => self.show_fps = number == 1.0,
            "cl_crosshairsize" => self.cross_size = number.clamp(2.0, 20.0) as f32,
            "cl_crosshairgap" => self.cross_gap = number.clamp(0.0, 16.0) as f32,
            "cl_crosshairthickness" => self.cross_thickness = number.clamp(1.0, 6.0) as f32,
            "cl_crosshairdot" if number == 0.0 || number == 1.0 => self.cross_dot = number == 1.0,
            "volume" => self.volume = number.clamp(0.0, 1.0) as f32,
            "sensitivity" => self.sensitivity = number.clamp(0.1, 10.0) as f32,
            "m_pitch" => self.invert_y = number < 0.0,
            "cl_crosshaircolor" if number.fract() == 0.0 && (0.0..=4.0).contains(&number) => {
                self.cross_color = [4, 0, 2, 1, 3][number as usize];
            }
            "cl_radar_scale" => self.radar_scale = number.clamp(0.4, 1.0) as f32,
            "cl_hud_radar_scale" => self.radar_hud_size = number.clamp(0.8, 1.3) as f32,
            "cl_radar_rotate" if number == 0.0 || number == 1.0 => self.radar_rotate = number == 1.0,
            "closeonbuy" if number == 0.0 || number == 1.0 => self.close_on_buy = number == 1.0,
            _ => return self.audio_options.set_convar_number(name, number),
        }
        true
    }
}
pub fn key_name(key: KeyCode) -> String {
    format!("{key:?}")
        .trim_start_matches("Key")
        .trim_start_matches("Digit")
        .replace("Left", " L")
        .replace("Right", " R")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_row_mouse_bindings_use_canonical_commands_for_every_action_and_button() {
        for (index, command) in ACTION_COMMANDS.iter().enumerate() {
            for (button, value) in MOUSE_BUTTONS.iter().copied().enumerate() {
                let mut settings = Settings::default();
                let keyboard = settings.binds;
                settings.rebind_mouse(index, value);
                assert_eq!(settings.mouse_binds[button], MouseAction::parse(command).unwrap());
                assert_eq!(settings.binds, keyboard, "mouse capture preserves keyboard alternatives");
                assert_eq!(Settings::parse(&settings.encode()), settings);
                assert!(settings.action_binding_label(index).contains(&format!("Mouse {}", button + 1)));
            }
        }
    }

    #[test]
    fn mouse_capture_swaps_occupied_buttons_without_changing_keyboard_wheel_or_console() {
        let knife = KEY_SELECTION_FIRST + 3;
        let mut settings = Settings::default();
        settings.rebind(knife, KeyCode::KeyV);
        settings.mouse_binds[3] = MouseAction::Action(knife);
        settings.mouse_binds[4] = MouseAction::Action(4);
        let original = settings.clone();
        settings.rebind_mouse(knife, MouseButton::Forward);
        let mut expected = original.clone();
        expected.mouse_binds.swap(3, 4);
        assert_eq!(settings, expected);
        assert_eq!(settings.selection_binding_label(3), "V / Mouse 5");
        settings.rebind_mouse(knife, MouseButton::Forward);
        assert_eq!(settings, expected, "capturing the same button is idempotent");
        settings.rebind_mouse(knife, MouseButton::Back);
        assert_eq!(settings, original);
        // The existing keyboard swap must still work beside a mouse binding.
        settings.rebind(knife, KeyCode::KeyR);
        assert_eq!(settings.binds[8], Some(KeyCode::KeyV));
        assert_eq!(settings.selection_binding_label(3), "R / Mouse 4");
        settings.clear_action_binding(knife);
        assert_eq!(settings.binds[knife], None);
        assert_eq!(settings.mouse_binds[3], MouseAction::None);
        assert_eq!(settings.mouse_binds[4], MouseAction::Action(4));
        assert_eq!(settings.selection_binding_label(3), "");
        let unchanged = settings.clone();
        settings.rebind_mouse(ACTIONS.len(), MouseButton::Back);
        settings.rebind_mouse(knife, MouseButton::Other(4));
        settings.clear_action_binding(ACTIONS.len());
        assert_eq!(settings, unchanged, "unsupported buttons/local console are not gameplay actions");
    }

    #[test]
    fn weapon_hints_follow_rebinds_unbinds_and_mouse_selection_after_reload() {
        let mut s = Settings::default();
        assert_eq!(s.selection_binding_label(2), "2");
        s.rebind(KEY_SELECTION_FIRST + 2, KeyCode::KeyK);
        s.rebind(KEY_SELECTION_FIRST + 3, KeyCode::KeyV);
        let s = Settings::parse(&s.encode());
        assert_eq!(s.selection_binding_label(2), "K");
        assert_eq!(s.selection_binding_label(3), "V");
        let mut s = s;
        s.binds[KEY_SELECTION_FIRST + 2] = None;
        assert_eq!(s.selection_binding_label(2), "");
        s.mouse_binds[3] = MouseAction::Action(KEY_SELECTION_FIRST + 2);
        assert_eq!(s.selection_binding_label(2), "Mouse 4");
        assert_eq!(s.selection_binding_label(0), "");
        assert_eq!(s.selection_binding_label(6), "");
    }
    #[test]
    fn settings_round_trip_and_rebind_swap() {
        let mut s = Settings::default();
        s.rebind(0, KeyCode::KeyS);
        s.volume = 0.7;
        s.cross_dot = true;
        s.menu_blur = false;
        s.radar_scale = 0.8;
        s.radar_hud_size = 1.1;
        s.radar_rotate = false;
        s.close_on_buy = true;
        s.controls_source = "CS2 controls imported; details: config/cs2-import.txt".into();
        s.sensitivity = 1.375;
        s.mouse_binds[0] = MouseAction::None;
        s.mouse_binds[3] = MouseAction::Action(4);
        s.mouse_binds[4] = MouseAction::Attack;
        assert_eq!(s.binds[1], Some(KeyCode::KeyW));
        assert_eq!(Settings::parse(&s.encode()), s);
    }
    #[test]
    fn new_controls_default_to_one_and_saved_sensitivity_remains_exact() {
        assert_eq!(Settings::default().sensitivity, 1.);
        assert_eq!(Settings::parse("volume=0.5\n").sensitivity, 1.);
        assert_eq!(Settings::parse("sensitivity=2.71828\n").sensitivity, 2.71828);
        for key in ["v", "0", "9", "rctrl", "leftarrow", "F12", "["] {
            let key = source_key(key).unwrap();
            let mut settings = Settings::default();
            settings.rebind(KEY_FIRE_GRENADE, key);
            assert_eq!(Settings::parse(&settings.encode()).binds[KEY_FIRE_GRENADE], Some(key));
        }
    }
    #[test]
    fn radar_display_size_is_persisted_independently_of_map_zoom() {
        assert_eq!(Settings::default().radar_hud_size, 1.0);
        let mut s = Settings::parse("radar_scale=0.7\nradar_rotate=false\nsensitivity=2.25\n");
        assert_eq!(s.radar_hud_size, 1.0);
        for size in [0.8, 1.0, 1.1, 1.24, 1.3] {
            let stored = Settings::parse(&format!("radar_hud_size={size}\nradar_scale=0.7\nradar_rotate=false\nsensitivity=2.25\n"));
            assert_eq!(stored.radar_hud_size, size, "explicit stored size must survive the default change");
            assert_eq!(Settings::parse(&stored.encode()), stored);
            assert_eq!(stored.radar_scale, s.radar_scale);
            assert_eq!(stored.radar_rotate, s.radar_rotate);
            assert_eq!(stored.sensitivity, s.sensitivity);
        }
        assert!(s.set_convar_number("cl_hud_radar_scale", 1.1));
        assert_eq!(s.radar_scale, 0.7);
        assert!(!s.radar_rotate);
        assert_eq!(s.sensitivity, 2.25);
        let saved = Settings::parse(&s.encode());
        assert_eq!(saved, s);
        assert_eq!(saved.convar_number("cl_hud_radar_scale"), Some(1.1f32 as f64));
        assert!(!s.set_convar_number("cl_hud_radar_scale", f64::NAN));
        assert_eq!(s.radar_hud_size, 1.1);
    }
    #[test]
    fn pointer_settings_cycle_only_supported_commands_and_round_trip() {
        let mut settings = Settings::default();
        let initial = settings.mouse_binds[0];
        for _ in 0..=ACTION_COMMANDS.len() {
            settings.cycle_mouse_binding(0, 1);
            let binding = settings.mouse_binds[0];
            assert_eq!(MouseAction::parse(binding.command()), Some(binding));
            assert_eq!(Settings::parse(&settings.encode()).mouse_binds[0], binding);
        }
        assert_eq!(settings.mouse_binds[0], initial);
        for up in [true, false] {
            let initial = if up { settings.wheel_up } else { settings.wheel_down };
            for _ in 0..4 { settings.cycle_wheel_binding(up, -1); }
            assert_eq!(if up { settings.wheel_up } else { settings.wheel_down }, initial);
        }
    }
    #[test]
    fn classic_color_ids_and_radar_choices_are_independent_of_mouse_inversion() {
        let mut s = Settings::default();
        s.invert_y = true;
        for color in 0..=4 {
            assert!(s.set_convar_number("cl_crosshaircolor", color as f64));
            assert_eq!(s.convar_number("cl_crosshaircolor"), Some(color as f64));
        }
        assert!(!s.set_convar_number("cl_crosshaircolor", 1.5));
        assert!(!s.set_convar_number("cl_radar_rotate", 2.0));
        assert!(s.set_convar_number("cl_radar_rotate", 0.0));
        assert_eq!(s.convar_number("cl_radar_rotate"), Some(0.0));
        assert!(s.invert_y);
        assert!(s.set_convar_number("cl_radar_scale", 2.0));
        assert_eq!(s.radar_scale, 1.0);
        assert!(!s.set_convar_number("cl_radar_scale", f64::NAN));
    }
    #[test]
    fn close_on_buy_accepts_only_schema_boolean_ids_and_persists_them() {
        let mut settings = Settings::default();
        assert!(!settings.close_on_buy);
        for number in [1.0, 0.0, 1.0] {
            assert!(settings.set_convar_number("closeonbuy", number));
            assert_eq!(settings.convar_number("closeonbuy"), Some(number));
            assert_eq!(Settings::parse(&settings.encode()), settings);
        }
        for number in [-1.0, 0.5, 2.0, f64::NAN, f64::INFINITY] {
            assert!(!settings.set_convar_number("closeonbuy", number));
            assert!(settings.close_on_buy);
        }
        for value in ["true", "false", "NaN", "2", "0.5", "1.000000001"] {
            assert!(!Settings::parse(&format!("closeonbuy={value}\n")).close_on_buy);
        }
        assert!(!Settings::parse("volume=0.7\n").close_on_buy);
    }

    #[test]
    fn invalid_config_is_bounded() {
        let s = Settings::parse(
            "volume=NaN\nsensitivity=9999\nresolution=99\ncross_color=-1\nexposure=inf\nbind900=KeyA",
        );
        assert_eq!(s.volume, 0.5);
        assert_eq!(s.sensitivity, 10.0);
        assert_eq!(s.resolution, 3);
        assert_eq!(s.exposure, 2.0);
    }
}
