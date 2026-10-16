//! Configures the installed option widgets for settings the client implements.
//! Labels/ranges come from the player's data; widget lifecycle calls were
//! verified with the offscreen runtime probe rather than copied movie logic.
use crate::{localize::Localization, settings::Settings};
use scaleform::{Calls, Value};

enum Kind {
    Slider {
        min: f64,
        max: f64,
    },
    Choice {
        choices: Vec<(String, f64)>,
        selected: usize,
    },
    Dropdown {
        choices: Vec<(String, f64)>,
        selected: Option<usize>,
    },
}
struct Widget {
    convar: String,
    label: String,
    tooltip: String,
    kind: Kind,
    priority: i64,
}
#[derive(Default)]
pub(super) struct Scene {
    pub mode: u32,
    widgets: Vec<Widget>,
    layout_initialized: bool,
    focused_widget: Option<usize>,
}
impl Scene {
    pub fn load(fs: &vfs::Vfs, command: &str, loc: &Localization) -> Self {
        let (file, mode) = match command {
            "OpenAudioSettingsDialog" => ("audio_options", 8),
            "OpenMouseDialog" | "OpenKeyboardMouseDialog" => ("mouse_keyboard_options", 0),
            "OpenGameSettingsDialog" | "OpenSettingsDialog" => ("game_options", 2),
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
        Self::from_block(root, mode, loc)
    }

    fn from_block(root: &kv::Block, mode: u32, loc: &Localization) -> Self {
        let mut widgets: Vec<_> = root
            .iter()
            .filter_map(|(_, value)| {
                let block = value.as_block()?;
                let convar = block.str("convar")?;
                let kind = match (block.str("type")?, convar) {
                    ("slider", "volume" | "sensitivity" | "cl_radar_scale") => {
                        let min = f64::from(block.f32("minvalue")?);
                        let max = f64::from(block.f32("maxvalue")?);
                        if !min.is_finite() || !max.is_finite() || min >= max {
                            return None;
                        }
                        Kind::Slider { min, max }
                    }
                    (
                        kind @ ("choice" | "dropdown"),
                        "m_pitch" | "cl_radar_rotate" | "cl_crosshaircolor" | "closeonbuy" | "snd_mute_losefocus" | "fullscreen_restart" | "sys_vsync",
                    ) => {
                        let mut choices = block
                            .block(if kind == "dropdown" {
                                "dropdown"
                            } else {
                                "choices"
                            })?
                            .iter()
                            .map(|(label, value)| {
                                let value = value.as_str()?.parse::<f64>().ok()?;
                                value
                                    .is_finite()
                                    .then(|| (loc.get(label).unwrap_or(label).into(), value))
                            })
                            .collect::<Option<Vec<_>>>()?;
                        // Offer only modes with working backends. In particular,
                        // exclusive fullscreen is not borderless fullscreen, and
                        // a triple-buffer preference is not this FIFO toggle.
                        choices.retain(|(_, value)| match convar {
                            "fullscreen_restart" => *value == 0.0 || *value == 2.0,
                            "sys_vsync" => *value == 0.0 || *value == 1.0,
                            _ => true,
                        });
                        if convar == "sys_vsync" {
                            for (label, value) in &mut choices {
                                if *value == 1.0 {
                                    // This backend implements synchronized
                                    // presentation, not Source's buffer-count
                                    // selection. Use the original generic token.
                                    *label = loc.get("#SFUI_Settings_Enabled").unwrap_or("Enabled").into();
                                }
                            }
                        }
                        if choices.len() < 2 || choices.len() > 32 {
                            return None;
                        }
                        if kind == "dropdown" {
                            Kind::Dropdown {
                                choices,
                                selected: None,
                            }
                        } else {
                            Kind::Choice {
                                choices,
                                selected: 0,
                            }
                        }
                    }
                    _ => return None,
                };
                let name = block.str("name").unwrap_or(convar);
                let tooltip = block.str("tooltip").unwrap_or("");
                Some(Widget {
                    convar: convar.into(),
                    label: loc.get(name).unwrap_or(name).into(),
                    tooltip: loc.get(tooltip).unwrap_or(tooltip).into(),
                    kind,
                    priority: block.i64("priority").unwrap_or(0),
                })
            })
            .collect();
        widgets.sort_by_key(|widget| widget.priority);
        Self {
            mode,
            widgets,
            layout_initialized: false,
            focused_widget: None,
        }
    }

    pub fn count(&self) -> u32 {
        self.widgets.len() as u32
    }

    pub fn bounds(&self, name: &str) -> Option<(f64, f64)> {
        self.widgets
            .iter()
            .find(|slider| slider.convar == name)
            .and_then(|widget| match widget.kind {
                Kind::Slider { min, max } => Some((min, max)),
                Kind::Choice { .. } | Kind::Dropdown { .. } => None,
            })
    }

    pub fn value(
        &mut self,
        index: &Value,
        value: &Value,
        path: &str,
        calls: &mut Calls,
    ) -> Option<(String, Value)> {
        let (Value::Number(index), Value::Number(value)) = (index, value) else {
            return None;
        };
        if !index.is_finite() || *index < 0.0 || index.fract() != 0.0 || !value.is_finite() {
            return None;
        }
        let widget = self.widgets.get_mut(*index as usize)?;
        let control = format!("{path}.TopPanel.Panel.arraySlots.{index}.Widget");
        let number = match &mut widget.kind {
            Kind::Slider { min, max } => {
                if !(0.0..=100.0).contains(value) {
                    return None;
                }
                // SetValueScaled also resets the authored drag state. Leave the
                // pointer-driven control active while publishing its new value.
                let number = *min + (*max - *min) * value / 100.0;
                let caption = Value::String(number.to_string());
                calls.set(
                    &format!("{control}.InputField.InputField"),
                    "text",
                    caption.clone(),
                );
                calls.set(&format!("{control}.InputField"), "inputString", caption);
                number
            }
            Kind::Choice { choices, selected } => {
                let delta = match *value {
                    -1.0 => -1,
                    1.0 => 1,
                    _ => return None,
                };
                *selected =
                    (*selected as isize + delta).rem_euclid(choices.len() as isize) as usize;
                calls.call(
                    &control,
                    "SetText",
                    vec![choices[*selected].0.clone().into()],
                );
                choices[*selected].1
            }
            Kind::Dropdown { choices, selected } => {
                // The authored dropdown reports a list index, not a convar
                // value. Preserve arbitrary ordering/noncontiguous schema IDs.
                if *value < 0.0 || value.fract() != 0.0 || *value >= choices.len() as f64 {
                    return None;
                }
                let index = *value as usize;
                *selected = Some(index);
                set_dropdown_caption(&control, &choices[index].0, calls);
                choices[index].1
            }
        };
        self.focused_widget = Some(*index as usize);
        Some((widget.convar.clone(), number.into()))
    }

    /// Numeric entry identifies its row by convar before the native refresh callback.
    pub fn convar_committed(&mut self, name: &str) {
        if let Some(index) = self.widgets.iter().position(|widget| widget.convar == name) {
            self.focused_widget = Some(index);
        }
    }

    /// Preserve the validated row when the authored mouse highlight reports NaN.
    pub fn highlight(&mut self, index: &Value, path: &str, calls: &mut Calls) {
        if let Value::Number(index) = index {
            if index.is_finite() && *index >= 0.0 && index.fract() == 0.0 {
                let index = *index as usize;
                if index < self.widgets.len() {
                    self.focused_widget = Some(index);
                    return;
                }
            }
            // Only the observed NaN mouse callback needs repair. Negative
            // indices can represent normal navigation away from a widget.
            if !index.is_nan() {
                return;
            }
        } else {
            return;
        }
        if self.layout_initialized {
            if let Some(index) = self.focused_widget {
                calls.call(
                    "_global.csrBindings",
                    "focusWidget",
                    vec![path.into(), (index as u32).into()],
                );
            }
        }
    }

    pub fn show(&mut self, path: &str, settings: &Settings, calls: &mut Calls) {
        calls.set(path, "_visible", true.into());
        self.layout_initialized = false;
        self.focused_widget = None;
        calls.call(path, "Init", vec![self.count().into(), self.mode.into()]);
        for (index, widget) in self.widgets.iter_mut().enumerate() {
            let (kind, label, data) = match &mut widget.kind {
                Kind::Slider { min, max } => (
                    0u32,
                    widget.label.clone(),
                    Value::List(vec![(*min).into(), (*max).into()]),
                ),
                Kind::Choice { choices, selected } => {
                    *selected = choices
                        .iter()
                        .position(|(_, value)| {
                            settings
                                .convar_number(&widget.convar)
                                .is_some_and(|current| (current - value).abs() < 1e-7)
                        })
                        .unwrap_or(0);
                    (1u32, choices[*selected].0.clone(), Value::Undefined)
                }
                Kind::Dropdown { choices, selected } => {
                    *selected = choices.iter().position(|(_, value)| {
                        settings
                            .convar_number(&widget.convar)
                            .is_some_and(|current| (current - value).abs() < 1e-7)
                    });
                    (
                        2u32,
                        widget.label.clone(),
                        Value::List(
                            choices
                                .iter()
                                .map(|(label, _)| label.clone().into())
                                .collect(),
                        ),
                    )
                }
            };
            calls.call(
                path,
                "onUpdateWidget",
                vec![
                    (index as u32).into(),
                    kind.into(),
                    label.into(),
                    widget.convar.clone().into(),
                    data,
                    widget.tooltip.clone().into(),
                ],
            );
        }
        calls.call(path, "ShowPanel", vec![]);
        if self.mode == 8 {
            // Observed original controller dead-zone art remains visible in
            // Audio mode. Its public visibility counterfact removes the empty
            // black panel without replacing the authored settings layout.
            calls.set(&format!("{path}.DeadZone"), "_visible", false.into());
        }
    }

    /// Apply values after the movie reports that its asynchronous widgets loaded.
    /// Calling onLoad ourselves races the movie's own load and double-counts it.
    pub fn layout_ready(&mut self, path: &str, settings: &Settings, calls: &mut Calls) {
        if self.layout_initialized {
            return;
        }
        self.layout_initialized = true;
        for (index, widget) in self.widgets.iter().enumerate() {
            if let Kind::Slider { min, max } = &widget.kind {
                let control = format!("{path}.TopPanel.Panel.arraySlots.{index}.Widget");
                calls.call(&control, "Init", vec![widget.convar.clone().into()]);
                calls.call(
                    &control,
                    "DefineScaledValueRange",
                    vec![(*min).into(), (*max).into()],
                );
            }
        }
        self.refresh_values(path, settings, calls);
        for (index, widget) in self.widgets.iter().enumerate() {
            if matches!(widget.kind, Kind::Slider { .. }) {
                calls.call(
                    "_global.csrBindings", "preserveSliderDragAxis",
                    vec![format!("{path}.TopPanel.Panel.arraySlots.{index}.Widget").into()],
                );
            }
        }
    }

    /// Numeric entry commits convars before requesting a refresh; it is no longer dragging.
    pub fn refresh_values(&mut self, path: &str, settings: &Settings, calls: &mut Calls) {
        if !self.layout_initialized {
            return;
        }
        for (index, widget) in self.widgets.iter_mut().enumerate() {
            let control = format!("{path}.TopPanel.Panel.arraySlots.{index}.Widget");
            match &mut widget.kind {
                Kind::Slider { .. } => {
                    let Some(current) = settings.convar_number(&widget.convar) else {
                        continue;
                    };
                    calls.call(&control, "SetValueScaled", vec![current.into()]);
                }
                Kind::Choice { choices, selected } => {
                    *selected = choices
                        .iter()
                        .position(|(_, value)| {
                            settings
                                .convar_number(&widget.convar)
                                .is_some_and(|current| (current - value).abs() < 1e-7)
                        })
                        .unwrap_or(0);
                    calls.call(
                        &control,
                        "SetText",
                        vec![choices[*selected].0.clone().into()],
                    );
                }
                Kind::Dropdown { choices, selected } => {
                    let (current, caption) = dropdown_presentation(
                        &widget.convar,
                        choices,
                        settings.convar_number(&widget.convar),
                    );
                    *selected = current;
                    let native_index = selected.map_or(-1.0, |index| index as f64);
                    calls.call(
                        &control,
                        "SetUpDropDownForOptions",
                        vec![
                            Value::List(
                                choices
                                    .iter()
                                    .map(|(label, _)| label.clone().into())
                                    .collect(),
                            ),
                            native_index.into(),
                        ],
                    );
                    // An older saved value need not be offered by this schema.
                    // Preserve it, show it truthfully and leave no list choice
                    // selected until the player explicitly picks one.
                    calls.set(&control, "m_nSelectedIndex", native_index.into());
                    set_dropdown_caption(&control, &caption, calls);
                }
            }
            calls.set(
                &format!("{path}.TopPanel.Panel.Control_Dummy.Control_{index}.Control_Text"),
                "text",
                widget.label.clone().into(),
            );
        }
        calls.call(path, "RefreshWidgetLayout", vec![]);
    }
}

fn set_dropdown_caption(control: &str, caption: &str, calls: &mut Calls) {
    // Cached authored buttons retain their initial `buttonText` binding.
    // Update that binding and its existing TextField together, as numeric
    // entry does, keeping the same display object across cached selections.
    let button = format!("{control}.Catagory.ButtonText");
    calls.set(&button, "buttonText", caption.into());
    calls.set(&format!("{button}.Text"), "text", caption.into());
}

fn dropdown_presentation(
    convar: &str,
    choices: &[(String, f64)],
    current: Option<f64>,
) -> (Option<usize>, String) {
    let index = choices
        .iter()
        .position(|(_, value)| current.is_some_and(|current| (current - value).abs() < 1e-7));
    let caption = index
        .map(|index| choices[index].0.clone())
        .unwrap_or_else(|| match (convar, current) {
            (_, None) => "Unavailable".into(),
            ("cl_crosshaircolor", Some(0.0)) => "Red".into(),
            ("cl_crosshaircolor", Some(1.0)) => "Green".into(),
            ("cl_crosshaircolor", Some(2.0)) => "Yellow".into(),
            ("cl_crosshaircolor", Some(3.0)) => "Blue".into(),
            ("cl_crosshaircolor", Some(4.0)) => "Light Blue".into(),
            (_, Some(_)) => "Custom".into(),
        });
    (index, caption)
}

#[cfg(test)]
mod tests {
    #[test]
    fn video_dropdowns_offer_working_modes_and_map_list_indices_to_native_values() {
        use super::*;
        let block = kv::parse(r#"
            display { type dropdown convar fullscreen_restart priority 1 dropdown { Windowed 0 Exclusive 1 Borderless 2 } }
            vsync { type dropdown convar sys_vsync priority 2 dropdown { Disabled 0 Double 1 Triple 2 } }
        "#).unwrap();
        let mut scene = Scene::from_block(&block, 6, &Localization::default());
        assert_eq!(scene.count(), 2);
        let mut calls = Calls::default();
        let (name, value) = scene.value(&0u32.into(), &1.0.into(), "_level21", &mut calls).unwrap();
        assert_eq!(name, "fullscreen_restart");
        assert_eq!(value.num(), 2.0, "second displayed choice must select borderless, not exclusive");
        assert!(scene.value(&0u32.into(), &2.0.into(), "_level21", &mut calls).is_none());
        let (name, value) = scene.value(&1u32.into(), &1.0.into(), "_level21", &mut calls).unwrap();
        assert_eq!(name, "sys_vsync");
        assert_eq!(value.num(), 1.0);
        assert!(scene.value(&1u32.into(), &2.0.into(), "_level21", &mut calls).is_none());
    }
    use super::*;
    #[test]
    fn close_on_buy_choice_cycles_from_saved_no_and_reopens_with_saved_yes() {
        let block = kv::parse("close { type choice convar closeonbuy priority 850 choices { Yes 1 No 0 } }").unwrap();
        let mut scene = Scene::from_block(&block, 2, &Localization::default());
        let mut calls = Calls::default();
        let mut settings = Settings::default();
        assert_eq!(scene.count(), 1);
        scene.show("_level21", &settings, &mut calls);
        scene.layout_ready("_level21", &settings, &mut calls);
        let (convar, value) = scene.value(&0u32.into(), &1.0.into(), "_level21", &mut calls).unwrap();
        assert_eq!(convar, "closeonbuy");
        assert_eq!(value.num(), 1.0);
        assert!(settings.set_convar_number(&convar, value.num()));
        let settings = Settings::parse(&settings.encode());
        scene.show("_level21", &settings, &mut calls);
        scene.layout_ready("_level21", &settings, &mut calls);
        assert_eq!(scene.value(&0u32.into(), &1.0.into(), "_level21", &mut calls).unwrap().1.num(), 0.0);
    }

    #[test]
    fn unlisted_saved_color_is_preserved_until_an_explicit_schema_selection() {
        let choices = vec![("Green".into(), 1.0), ("Blue".into(), 3.0)];
        let mut settings = Settings::default();
        settings.cross_color = 4; // Source ID0 is absent from this schema.
        assert_eq!(
            dropdown_presentation(
                "cl_crosshaircolor",
                &choices,
                settings.convar_number("cl_crosshaircolor")
            ),
            (None, "Red".into())
        );
        assert_eq!(
            dropdown_presentation("cl_crosshaircolor", &choices, Some(13.0)),
            (None, "Custom".into())
        );
        assert_eq!(
            dropdown_presentation("cl_crosshaircolor", &choices, None),
            (None, "Unavailable".into())
        );
        let mut scene = Scene {
            layout_initialized: true,
            widgets: vec![Widget {
                convar: "cl_crosshaircolor".into(),
                label: "Color".into(),
                tooltip: String::new(),
                priority: 0,
                kind: Kind::Dropdown {
                    choices,
                    selected: None,
                },
            }],
            ..Default::default()
        };
        let mut calls = Calls::default();
        scene.refresh_values("_level21", &settings, &mut calls);
        assert_eq!(settings.cross_color, 4);
        assert!(
            matches!(&scene.widgets[0].kind, Kind::Dropdown { selected: None, choices } if choices.len() == 2)
        );
        assert!(
            scene
                .value(&0u32.into(), &(-1.0).into(), "_level21", &mut calls)
                .is_none()
        );
        let (convar, value) = scene
            .value(&0u32.into(), &0u32.into(), "_level21", &mut calls)
            .unwrap();
        assert!(settings.set_convar_number(&convar, value.num()));
        assert_eq!(settings.convar_number("cl_crosshaircolor"), Some(1.0));
    }
    #[test]
    fn native_dropdown_maps_schema_ids_and_rejects_malformed_selections() {
        let block = kv::parse(
            r#"
            color { type dropdown convar cl_crosshaircolor dropdown { Blue 3 Green 1 Yellow 2 } }
            unsupported { type dropdown convar pretend dropdown { No 0 Yes 1 } }
        "#,
        )
        .unwrap();
        let mut scene = Scene::from_block(&block, 2, &Localization::default());
        let mut calls = Calls::default();
        scene.show("_level21", &Settings::default(), &mut calls);
        scene.layout_ready("_level21", &Settings::default(), &mut calls);
        assert_eq!(scene.count(), 1);
        let changed = scene
            .value(&0u32.into(), &0u32.into(), "_level21", &mut calls)
            .unwrap();
        assert_eq!(changed.0, "cl_crosshaircolor");
        assert_eq!(changed.1.num(), 3.0); // First list item carries ID3, not0.
        for (index, selection) in [
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.5, 0.0),
            (f64::NAN, 0.0),
            (0.0, -1.0),
            (0.0, 1.5),
            (0.0, f64::NAN),
            (0.0, f64::INFINITY),
            (0.0, 3.0),
        ] {
            assert!(
                scene
                    .value(&index.into(), &selection.into(), "_level21", &mut calls)
                    .is_none()
            );
        }
    }

    fn scene() -> Scene {
        let block = kv::parse(
            r#"reverse { type choice convar m_pitch choices { Off 0.022 On -0.022 } priority 1 }
            sensitivity { type slider convar sensitivity minvalue 1 maxvalue 8 priority 2 }
            unsupported { type slider convar pretend minvalue 0 maxvalue 1 }"#,
        )
        .unwrap();
        Scene::from_block(&block, 0, &Localization::default())
    }
    #[test]
    fn native_percentage_and_signed_choice_updates_use_the_installed_ranges() {
        let mut scene = scene();
        let mut calls = Calls::default();
        assert_eq!(scene.count(), 2);
        let (_, value) = scene
            .value(&1u32.into(), &71.0.into(), "_level21", &mut calls)
            .unwrap();
        assert!((value.num() - 5.97).abs() < 1e-10);
        for (direction, expected) in [(1.0, -0.022), (1.0, 0.022), (-1.0, -0.022), (-1.0, 0.022)] {
            assert_eq!(
                scene
                    .value(&0u32.into(), &direction.into(), "_level21", &mut calls)
                    .unwrap()
                    .1
                    .num(),
                expected
            );
        }
    }
    #[test]
    fn numeric_commit_selects_its_row_without_a_prior_slider_drag() {
        let mut scene = scene();
        let mut calls = Calls::default();
        scene.highlight(&0u32.into(), "_level21", &mut calls);
        scene.convar_committed("sensitivity");
        assert_eq!(scene.focused_widget, Some(1));
        scene.convar_committed("unknown");
        assert_eq!(scene.focused_widget, Some(1));
    }

    #[test]
    fn refreshed_choices_cycle_from_the_latest_saved_value() {
        let mut scene = scene();
        scene.layout_initialized = true;
        let mut calls = Calls::default();
        let mut settings = Settings::default();
        settings.invert_y = true;
        scene.refresh_values("_level21", &settings, &mut calls);
        let (_, value) = scene
            .value(&0u32.into(), &1.0.into(), "_level21", &mut calls)
            .unwrap();
        assert_eq!(value.num(), 0.022);
    }

    #[test]
    fn mouse_updates_retain_the_correct_row_for_nan_focus_repair() {
        let mut scene = scene();
        let mut calls = Calls::default();
        scene.layout_initialized = true;
        scene.highlight(&0u32.into(), "_level21", &mut calls);
        assert_eq!(scene.focused_widget, Some(0));
        scene
            .value(&1u32.into(), &71.0.into(), "_level21", &mut calls)
            .unwrap();
        scene.highlight(&f64::NAN.into(), "_level21", &mut calls);
        assert_eq!(scene.focused_widget, Some(1));
        for index in [-1.0, 0.5, 10.0, f64::INFINITY] {
            scene.highlight(&index.into(), "_level21", &mut calls);
            assert_eq!(scene.focused_widget, Some(1));
        }
        scene.highlight(&0u32.into(), "_level21", &mut calls);
        assert!(
            scene
                .value(&1u32.into(), &101.0.into(), "_level21", &mut calls)
                .is_none()
        );
        assert_eq!(scene.focused_widget, Some(0));
    }

    #[test]
    fn malformed_callbacks_cannot_change_the_selected_choice() {
        let mut scene = scene();
        let mut calls = Calls::default();
        for (index, value) in [
            (-1.0, 1.0),
            (0.5, 1.0),
            (10.0, 1.0),
            (0.0, 0.0),
            (0.0, 2.0),
            (0.0, f64::NAN),
            (1.0, 101.0),
            (1.0, -1.0),
        ] {
            assert!(
                scene
                    .value(&index.into(), &value.into(), "_level21", &mut calls)
                    .is_none()
            );
        }
        assert_eq!(
            scene
                .value(&0u32.into(), &1.0.into(), "_level21", &mut calls)
                .unwrap()
                .1
                .num(),
            -0.022
        );
    }
}
