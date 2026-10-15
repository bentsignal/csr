//! Persistent local settings shared by menus, input, audio and HUD.
use bevy::prelude::*;

pub const ACTIONS: [&str; 13] = [
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
];
pub const KEYS: [KeyCode; 40] = [
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
    KeyCode::KeyR,
    KeyCode::KeyS,
    KeyCode::KeyT,
    KeyCode::KeyU,
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
];

/// A wheel detent is a one-shot request, never a held jump button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelAction { None, Jump, Previous, Next }
impl WheelAction {
    pub fn command(self) -> &'static str { match self { Self::None => "", Self::Jump => "+jump", Self::Previous => "invprev", Self::Next => "invnext" } }
    pub fn parse(value: &str) -> Option<Self> { Some(match value { "" => Self::None, "+jump" => Self::Jump, "invprev" => Self::Previous, "invnext" => Self::Next, _ => return None }) }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
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
    pub radar_rotate: bool,
    pub close_on_buy: bool,
    pub binds: [KeyCode; 13],
}
pub const RESOLUTIONS: [(u32, u32); 4] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.5,
            audio_options: Default::default(),
            show_fps: false,
            wheel_up: WheelAction::Previous,
            wheel_down: WheelAction::Next,
            menu_blur: true,
            sensitivity: 2.0,
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
            radar_rotate: true,
            // Preserve the existing stay-open behavior; native default unmeasured.
            close_on_buy: false,
            binds: [
                KeyCode::KeyW,
                KeyCode::KeyS,
                KeyCode::KeyA,
                KeyCode::KeyD,
                KeyCode::Space,
                KeyCode::ControlLeft,
                KeyCode::ShiftLeft,
                KeyCode::KeyE,
                KeyCode::KeyR,
                KeyCode::KeyB,
                KeyCode::KeyG,
                KeyCode::Tab,
                KeyCode::KeyF,
            ],
        }
    }
}
/// Relative to the player data folder (the launcher runs inside it).
pub fn path() -> std::path::PathBuf {
    std::path::PathBuf::from("config/user.cfg")
}
impl Settings {
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
                        .filter(|i| *i < 13)
                    {
                        if let Some(key) = KEYS.iter().find(|k| format!("{k:?}") == value) {
                            s.rebind(index, *key);
                        }
                    }
                }
            }
        }
        s
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
        text.push_str(&format!("menu_blur={}\n", self.menu_blur));
        text.push_str(&format!("radar_scale={}\nradar_rotate={}\n", self.radar_scale, self.radar_rotate));
        text.push_str(&format!("closeonbuy={}\n", u8::from(self.close_on_buy)));
        for (i, key) in self.binds.iter().enumerate() {
            text.push_str(&format!("bind{i}={key:?}\n"));
        }
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
        if let Some(other) = self.binds.iter().position(|k| *k == key) {
            self.binds[other] = old;
        }
        self.binds[index] = key;
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
            "cl_showfps" => u8::from(self.show_fps) as f64,
            "cl_crosshairsize" => self.cross_size as f64,
            "cl_crosshairgap" => self.cross_gap as f64,
            "cl_crosshairthickness" => self.cross_thickness as f64,
            "cl_crosshairdot" => u8::from(self.cross_dot) as f64,
            "sensitivity" => self.sensitivity as f64,
            "m_pitch" => if self.invert_y { -0.022 } else { 0.022 },
            "cl_crosshaircolor" => [1., 3., 2., 4., 0.][self.cross_color.min(4)],
            "cl_radar_scale" => self.radar_scale as f64,
            "cl_radar_rotate" => u8::from(self.radar_rotate) as f64,
            "closeonbuy" => u8::from(self.close_on_buy) as f64,
            _ => return self.audio_options.convar_number(name),
        })
    }

    pub(crate) fn set_convar_number(&mut self, name: &str, number: f64) -> bool {
        if !number.is_finite() { return false; }
        match name {
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
        .replace("Left", " L")
        .replace("Right", " R")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_and_rebind_swap() {
        let mut s = Settings::default();
        s.rebind(0, KeyCode::KeyS);
        s.volume = 0.7;
        s.cross_dot = true;
        s.menu_blur = false;
        s.radar_scale = 0.8;
        s.radar_rotate = false;
        s.close_on_buy = true;
        assert_eq!(s.binds[1], KeyCode::KeyW);
        assert_eq!(Settings::parse(&s.encode()), s);
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
