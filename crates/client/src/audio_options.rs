//! Persistent controls backed by actual audio behavior. Original widget
//! definitions/labels are loaded separately from the player's installed data.

#[derive(Clone, Debug, PartialEq)]
pub struct AudioOptions {
    pub mute_lose_focus: bool,
}

impl Default for AudioOptions {
    fn default() -> Self {
        Self {
            // Observed installed Legacy public console value (archive): 1.
            mute_lose_focus: true,
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
            _ => None,
        }
    }

    pub(crate) fn set_convar_number(&mut self, name: &str, value: f64) -> bool {
        match name {
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
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

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
