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

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
    pub volume: f32,
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
    pub binds: [KeyCode; 13],
}
pub const RESOLUTIONS: [(u32, u32); 4] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.5,
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
        text.push_str(&format!("menu_blur={}\n", self.menu_blur));
        for (i, key) in self.binds.iter().enumerate() {
            text.push_str(&format!("bind{i}={key:?}\n"));
        }
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
            Color::srgb(0.2, 0.85, 1.0),
            Color::srgb(1.0, 0.85, 0.2),
            Color::WHITE,
            Color::srgb(1.0, 0.2, 0.7),
        ][self.cross_color.min(4)]
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
        assert_eq!(s.binds[1], KeyCode::KeyW);
        assert_eq!(Settings::parse(&s.encode()), s);
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
