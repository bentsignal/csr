//! Configures the installed option widgets for settings the client implements.
//! Labels/ranges come from the player's data; widget lifecycle calls were
//! verified with the offscreen runtime probe rather than copied movie logic.
use crate::{localize::Localization, settings::Settings};
use scaleform::{Calls, Value};

pub(super) struct Slider {
    convar: String,
    label: String,
    tooltip: String,
    min: f64,
    max: f64,
    priority: i64,
}
#[derive(Default)]
pub(super) struct Scene {
    pub mode: u32,
    sliders: Vec<Slider>,
}
impl Scene {
    pub fn load(fs: &vfs::Vfs, command: &str, loc: &Localization) -> Self {
        let (file, mode) = match command {
            "OpenAudioSettingsDialog" => ("audio_options", 8),
            "OpenKeyboardMouseDialog" => ("mouse_keyboard_options", 0),
            "OpenGameSettingsDialog" => ("game_options", 2),
            _ => ("video_options", 6),
        };
        let doc = fs
            .read_string(&format!("scripts/{file}.txt"))
            .and_then(|text| kv::parse(&text).ok())
            .unwrap_or_default();
        let root = doc
            .entries
            .first()
            .and_then(|(_, value)| value.as_block())
            .unwrap_or(&doc);
        let mut sliders: Vec<_> = root
            .iter()
            .filter_map(|(_, value)| {
                let block = value.as_block()?;
                let convar = block.str("convar")?;
                if block.str("type") != Some("slider")
                    || !matches!(convar, "volume" | "sensitivity")
                {
                    return None;
                }
                let min = f64::from(block.f32("minvalue")?);
                let max = f64::from(block.f32("maxvalue")?);
                if !min.is_finite() || !max.is_finite() || min >= max {
                    return None;
                }
                let name = block.str("name").unwrap_or(convar);
                let tooltip = block.str("tooltip").unwrap_or("");
                Some(Slider {
                    convar: convar.into(),
                    label: loc.get(name).unwrap_or(name).into(),
                    tooltip: loc.get(tooltip).unwrap_or(tooltip).into(),
                    min,
                    max,
                    priority: block.i64("priority").unwrap_or(0),
                })
            })
            .collect();
        sliders.sort_by_key(|slider| slider.priority);
        Self { mode, sliders }
    }

    pub fn count(&self) -> u32 {
        self.sliders.len() as u32
    }
    pub fn bounds(&self, name: &str) -> Option<(f64, f64)> {
        self.sliders
            .iter()
            .find(|slider| slider.convar == name)
            .map(|slider| (slider.min, slider.max))
    }

    pub fn show(&self, path: &str, settings: &Settings, calls: &mut Calls) {
        calls.call(path, "Init", vec![self.count().into(), self.mode.into()]);
        for (index, slider) in self.sliders.iter().enumerate() {
            calls.call(
                path,
                "onUpdateWidget",
                vec![
                    (index as u32).into(),
                    0u32.into(),
                    slider.label.clone().into(),
                    slider.convar.clone().into(),
                    Value::List(vec![slider.min.into(), slider.max.into()]),
                    slider.tooltip.clone().into(),
                ],
            );
            let widget = format!("{path}.TopPanel.Panel.arraySlots.{index}.Widget");
            calls.call(&widget, "Init", vec![slider.convar.clone().into()]);
            calls.call(&widget, "onLoad", vec![]);
            calls.call(
                &widget,
                "DefineScaledValueRange",
                vec![slider.min.into(), slider.max.into()],
            );
            let current = match slider.convar.as_str() {
                "volume" => settings.volume,
                "sensitivity" => settings.sensitivity,
                _ => continue,
            };
            calls.call(&widget, "SetValueScaled", vec![(current as f64).into()]);
            calls.set(
                &format!("{path}.TopPanel.Panel.Control_Dummy.Control_{index}.Control_Text"),
                "text",
                slider.label.clone().into(),
            );
        }
        calls.call(path, "RefreshWidgetLayout", vec![]);
        calls.call(path, "ShowPanel", vec![]);
    }
}
