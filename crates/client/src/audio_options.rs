//! Persistent controls backed by actual audio behavior. Original widget
//! definitions/labels are loaded separately from the player's installed data.

#[derive(Clone, Debug, PartialEq)]
pub struct AudioOptions {
    pub mute_lose_focus: bool,
    pub menu_music_volume: f32,
    pub headshot_feedback_volume: f32,
}

impl Default for AudioOptions {
    fn default() -> Self {
        Self {
            // Observed installed Legacy public console value (archive): 1.
            mute_lose_focus: true,
            // Legacy's public help reports default 1.0; a player's archived
            // current value is not the engine default.
            menu_music_volume: 1.0,
            headshot_feedback_volume: crate::effects::ATTACKER_HEAD_GAIN,
        }
    }
}

impl AudioOptions {
    pub(crate) fn effective_volume(&self, master: f32, focused: bool) -> f32 {
        if self.mute_lose_focus && !focused {
            0.0
        } else {
            master
        }
    }

    pub(crate) fn convar_number(&self, name: &str) -> Option<f64> {
        match name {
            "snd_mute_losefocus" => Some(u8::from(self.mute_lose_focus) as f64),
            "snd_menumusic_volume" => Some(self.menu_music_volume as f64),
            "csr_headshot_volume" => Some(self.headshot_feedback_volume as f64),
            _ => None,
        }
    }

    pub(crate) fn set_convar_number(&mut self, name: &str, value: f64) -> bool {
        match name {
            "csr_headshot_volume" if value.is_finite() => {
                self.headshot_feedback_volume = value.clamp(0.0, 2.0) as f32;
                true
            }
            "snd_menumusic_volume" if value.is_finite() => {
                self.menu_music_volume = value.clamp(0.0, 1.0) as f32;
                true
            }
            "snd_mute_losefocus" if value == 0.0 || value == 1.0 => {
                self.mute_lose_focus = value == 1.0;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn parse_entry(&mut self, name: &str, value: &str) {
        if let Ok(number) = value.parse::<f64>() {
            self.set_convar_number(name, number);
        }
    }

    pub(crate) fn encode_into(&self, text: &mut String) {
        text.push_str(&format!(
            "snd_mute_losefocus={}\n",
            u8::from(self.mute_lose_focus)
        ));
        text.push_str(&format!("snd_menumusic_volume={}\n", self.menu_music_volume));
        text.push_str(&format!("csr_headshot_volume={}\n", self.headshot_feedback_volume));
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn window_focus_changes_existing_playback_without_changing_saved_volume() {
        use bevy::{
            audio::{AudioSink, AudioSinkPlayback},
            prelude::*,
            window::PrimaryWindow,
        };
        let mut app = App::new();
        app.insert_resource(Settings::parse("volume=0.8\nsnd_mute_losefocus=1\n"))
            .insert_resource(crate::Options(crate::ClientOptions {
                screenshot: Some("unused-focus-test.png".into()),
                ..default()
            }))
            .init_resource::<crate::audio::SoundLibrary>()
            .init_resource::<Assets<crate::material::WorldMaterial>>()
            .add_systems(Update, crate::menu::apply);
        let window = app
            .world_mut()
            .spawn((
                PrimaryWindow,
                Window {
                    focused: true,
                    ..default()
                },
            ))
            .id();
        // A real rodio queue/sink, without opening an output device.
        let (player, _queue) = rodio::Player::new();
        let sound = app
            .world_mut()
            .spawn((crate::audio::SoundGain(0.25), AudioSink::new(player)))
            .id();
        let volume = |app: &App| {
            app.world()
                .get::<AudioSink>(sound)
                .unwrap()
                .volume()
                .to_linear()
        };
        app.update();
        assert!((volume(&app) - 0.2).abs() < 1e-6);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert_eq!(volume(&app), 0.0);
        assert_eq!(
            app.world()
                .resource::<crate::audio::SoundLibrary>()
                .master_volume,
            0.0
        );
        assert_eq!(app.world().resource::<Settings>().volume, 0.8);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.update();
        assert!((volume(&app) - 0.2).abs() < 1e-6);
        // The original Yes choice is zero: playback remains audible unfocused.
        app.world_mut()
            .resource_mut::<Settings>()
            .set_convar_number("snd_mute_losefocus", 0.0);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert!((volume(&app) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn focus_mute_keeps_saved_master_and_shared_ui_console_value() {
        let mut settings = Settings::parse("volume=0.73\nsnd_mute_losefocus=1\n");
        assert_eq!(
            settings
                .audio_options
                .effective_volume(settings.volume, false),
            0.0
        );
        assert_eq!(
            settings
                .audio_options
                .effective_volume(settings.volume, true),
            0.73
        );
        assert_eq!(settings.volume, 0.73);
        // The original "Play Audio No Focus" Yes choice carries zero.
        assert!(settings.set_convar_number("snd_mute_losefocus", 0.0));
        assert_eq!(
            settings
                .audio_options
                .effective_volume(settings.volume, false),
            0.73
        );
        assert_eq!(settings.convar_number("snd_mute_losefocus"), Some(0.0));
        assert_eq!(Settings::parse(&settings.encode()), settings);
        for invalid in [f64::NAN, f64::INFINITY, -1.0, 0.5, 2.0] {
            assert!(!settings.set_convar_number("snd_mute_losefocus", invalid));
        }
        assert_eq!(
            Settings::parse("snd_mute_losefocus=NaN\n").audio_options,
            Default::default()
        );
    }
}

#[cfg(test)]
mod headshot_tests {
    use crate::settings::Settings;
    #[test]
    fn headshot_gain_round_trips_and_rejects_nonfinite_values() {
        let mut settings = Settings::parse("csr_headshot_volume=0.73\n");
        assert_eq!(settings.audio_options.headshot_feedback_volume, 0.73);
        assert_eq!(Settings::parse(&settings.encode()), settings);
        assert!(!settings.set_convar_number("csr_headshot_volume", f64::NAN));
        assert!(settings.set_convar_number("csr_headshot_volume", 4.0));
        assert_eq!(settings.audio_options.headshot_feedback_volume, 2.0);
    }
}
