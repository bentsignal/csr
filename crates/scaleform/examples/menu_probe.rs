//! Standalone library proof. Captures stay outside the source checkout.
//! cargo run -p scaleform --example menu_probe -- C:\Users\shawn\csr-fidelity\menu-host
use scaleform::{Calls, Config, Engine, Event, Frame, Input, Menu, MouseButton, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Default)]
struct ProbeEngine {
    seen: std::collections::HashSet<(String, String)>,
    fs: Option<vfs::Vfs>,
    tokens: std::collections::HashMap<String, String>,
    roots: std::collections::HashMap<String, String>,
}
impl Engine for ProbeEngine {
    fn call(
        &mut self,
        object: &str,
        method: &str,
        args: &[Value],
        calls: &mut Calls,
    ) -> Option<Value> {
        if self.seen.insert((object.into(), method.into()))
            || std::env::var_os("CSR_PROBE_VERBOSE").is_some()
            || method == "BasePanelRunCommand"
            || method == "OnUpdateValue"
            || object == "csr"
        {
            tracing::info!("{object}.{method} {args:?}");
        }
        if method == "OnLoadFinished" {
            if let Some(path) = args
                .first()
                .map(Value::str)
                .and_then(|path| path.strip_prefix("mc:").map(str::to_string))
            {
                self.roots.insert(object.into(), path);
            }
        }
        match (object, method) {
            ("CScaleformComponent_GameTypes", "GetMapGroupAttribute")
            | ("CScaleformComponent_GameTypes", "GetMapGroupAttributeSubKeys") => {
                let doc = kv::parse(&self.fs.as_ref()?.read_string("gamemodes.txt")?).ok()?;
                let group = doc.block("GameModes.txt")?.block("mapgroups")?.block(&args.first()?.str())?;
                let attribute = args.get(1)?.str();
                if method == "GetMapGroupAttributeSubKeys" {
                    return Some(Value::List(group.block(&attribute)?.iter().map(|(name, _)| name.into()).collect()));
                }
                let text = group.str(&attribute)?;
                return Some(self.tokens.get(text.trim_start_matches('#')).map_or(text, String::as_str).into());
            }
            ("StartSinglePlayer", "OnReady") => {
                if let Some(path) = self.roots.get(object) {
                    calls.call(
                        path,
                        "InitDialogData",
                        vec![false.into(), false.into(), false.into()],
                    );
                    calls.call(path, "showPanel", vec![]);
                }
            }
            ("CScaleformComponent_GameTypes", "GetGameModeFromInts") => {
                let doc = kv::parse(&self.fs.as_ref()?.read_string("gamemodes.txt")?).ok()?;
                let types = doc.block("GameModes.txt")?.block("gameTypes")?;
                let type_id = args.first()?.str();
                let mode_id = args.get(1)?.str();
                let game_type = types
                    .iter()
                    .filter_map(|(_, value)| value.as_block())
                    .find(|block| block.str("value") == Some(type_id.as_str()))?;
                return game_type
                    .block("gameModes")?
                    .iter()
                    .find_map(|(name, value)| {
                        (value.as_block()?.str("value") == Some(mode_id.as_str()))
                            .then(|| name.into())
                    });
            }
            ("GameInterface", "MakeStringSafe") => {
                return Some(args.first().map(Value::str).unwrap_or_default().into());
            }
            ("GameInterface", "LoadKVFile") => {
                let name = args.first()?.str().to_ascii_lowercase();
                if !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.'))
                {
                    return Some(Value::Null);
                }
                let doc = kv::parse(&self.fs.as_ref()?.read_string(&name)?).ok()?;
                return Some(kv_value(
                    doc.entries
                        .first()
                        .and_then(|(_, value)| value.as_block())
                        .unwrap_or(&doc),
                    &self.tokens,
                ));
            }
            ("MainMenu", "OnLoadFinished") => {
                calls.call("_global.MainMenuMovie", "onUIShow", vec![])
            }
            ("MainMenu", "BasePanelRunCommand") => match args.first().map(Value::str).as_deref() {
                Some("OpenCreateSinglePlayerGameDialog") => {
                    calls.request_element("StartSinglePlayer")
                }
                Some("OpenVideoSettingsDialog") => calls.request_element("OptionsMenu"),
                _ => {}
            },
            ("CScaleformComponent_MyPersona", "GetXuid") => return Some("0".into()),
            ("CScaleformComponent_MyPersona", "GetXpPerLevel") => return Some(5000.0.into()),
            ("OptionsMenu", "GetTotalOptionsSlots") => return Some(3.0.into()),
            ("OptionsMenu", "GetSafeZoneXMin") => return Some(0.8.into()),
            ("GameInterface", "GetConvarNumberMin") => return Some(0.8.into()),
            ("GameInterface", "GetConvarNumberMax") => return Some(1.0.into()),
            ("CScaleformComponent_FriendsList", "GetFriendName")
                if args.first().is_some_and(|v| v.str() == "0") =>
            {
                return Some("Player".into());
            }
            _ => {}
        }
        None
    }
}

fn kv_value(block: &kv::Block, tokens: &std::collections::HashMap<String, String>) -> Value {
    Value::Object(
        block
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    match value {
                        kv::Value::Str(text) => tokens
                            .get(text.trim_start_matches('#'))
                            .unwrap_or(text)
                            .clone()
                            .into(),
                        kv::Value::Block(child) => kv_value(child, tokens),
                    },
                )
            })
            .collect(),
    )
}

fn settle(menu: &Menu, latest: &mut Option<Frame>, duration: Duration) -> anyhow::Result<bool> {
    let deadline = Instant::now() + duration;
    let mut loaded = false;
    loop {
        while let Some(event) = menu.poll_event() {
            match event {
                Event::Failed(message) => anyhow::bail!("{message}"),
                Event::Loaded(name) => {
                    loaded |= name == "MainMenu";
                    tracing::info!("loaded {name}");
                }
                Event::Inspected { path, properties } => {
                    tracing::info!("API {path}: {properties:?}")
                }
            }
        }
        if let Some(frame) = menu.take_frame() {
            *latest = Some(frame);
        }
        if Instant::now() >= deadline {
            return Ok(loaded);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn save(frame: &Frame, path: PathBuf) -> anyhow::Result<()> {
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
        .ok_or_else(|| anyhow::anyhow!("invalid frame dimensions"))?;
    image.save(&path)?;
    println!(
        "saved {} ({}x{}, frame {})",
        path.display(),
        frame.width,
        frame.height,
        frame.serial
    );
    Ok(())
}

fn json_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Bool(value),
        serde_json::Value::Number(value) => Value::Number(value.as_f64().unwrap()),
        serde_json::Value::String(value) => Value::String(value),
        serde_json::Value::Array(values) => {
            Value::List(values.into_iter().map(json_value).collect())
        }
        serde_json::Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(name, value)| (name, json_value(value)))
                .collect(),
        ),
    }
}

/// A JSON argument array supports nested widget data without lossy comma splitting.
fn parse_args(text: &str) -> anyhow::Result<Vec<Value>> {
    if text.trim().starts_with('[') {
        let values: Vec<serde_json::Value> = serde_json::from_str(text)?;
        return Ok(values.into_iter().map(json_value).collect());
    }
    if text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(text
        .split(',')
        .map(|arg| match arg {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "undefined" => Value::Undefined,
            "null" => Value::Null,
            _ => arg
                .parse::<f64>()
                .map(Value::Number)
                .unwrap_or_else(|_| Value::String(arg.into())),
        })
        .collect())
}

fn pointer_inputs(action: &str, args: &[Value]) -> anyhow::Result<Vec<Input>> {
    anyhow::ensure!(
        args.len() == 2,
        "pointer action requires exactly two numeric coordinates"
    );
    let number = |value: &Value| match value {
        Value::Number(value) if value.is_finite() => Ok(*value),
        _ => anyhow::bail!("pointer coordinates must be finite numbers"),
    };
    let (x, y) = (number(&args[0])?, number(&args[1])?);
    let down = Input::MouseDown {
        x,
        y,
        button: MouseButton::Left,
    };
    let up = Input::MouseUp {
        x,
        y,
        button: MouseButton::Left,
    };
    Ok(match action {
        "@click" => vec![Input::MouseMove { x, y }, down, up],
        "@down" => vec![down],
        "@move" => vec![Input::MouseMove { x, y }],
        "@up" => vec![up],
        _ => anyhow::bail!("unknown pointer action: {action}"),
    })
}

fn widget_input(path: &str, function: &str, args: Vec<Value>) -> anyhow::Result<Input> {
    if function == "@set" {
        let [Value::String(property), value] = args.as_slice() else {
            anyhow::bail!("property updates require [property,value]");
        };
        Ok(Input::Set {
            path: path.into(),
            property: property.clone(),
            value: value.clone(),
        })
    } else {
        Ok(Input::Call {
            path: path.into(),
            function: function.into(),
            args,
        })
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("warn,scaleform=info,menu_probe=info,avm_warning=info")
        .init();
    let out =
        PathBuf::from(std::env::args().nth(1).ok_or_else(|| {
            anyhow::anyhow!("menu_probe <absolute output folder outside the repo>")
        })?);
    anyhow::ensure!(
        out.is_absolute(),
        "Output must be an absolute path outside the repository"
    );
    std::fs::create_dir_all(&out)?;
    let out = out.canonicalize()?;
    let cwd = std::env::current_dir()?.canonicalize()?;
    let repo = cwd
        .ancestors()
        .find(|p| p.join(".git").exists())
        .unwrap_or(&cwd);
    anyhow::ensure!(
        !out.starts_with(repo),
        "Captures contain game art; choose a folder outside the repository"
    );
    let game_dir =
        vfs::find_csgo_dir().ok_or_else(|| anyhow::anyhow!("CS:GO Legacy is not installed"))?;
    let mut fs = vfs::Vfs::new();
    fs.mount_game_dir(&game_dir)?;
    let doc = kv::parse_escaped(&kv::decode_text(
        &fs.read("resource/csgo_english.txt").unwrap_or_default(),
    ))?;
    let tokens = doc
        .block("lang")
        .and_then(|root| root.block("Tokens"))
        .map(|tokens| {
            tokens
                .iter()
                .filter_map(|(name, value)| Some((name.to_string(), value.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let menu = Menu::spawn(
        Config {
            game_dir,
            width: 1920,
            height: 1080,
            language: "english".into(),
            elements: vec!["MainMenu".into()],
        },
        Box::new(ProbeEngine {
            fs: Some(fs),
            tokens,
            ..Default::default()
        }),
    );
    let mut latest = None;
    let mut ready = false;
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready && Instant::now() < deadline {
        ready |= settle(&menu, &mut latest, Duration::from_millis(100))?;
    }
    anyhow::ensure!(
        ready && latest.is_some(),
        "Main menu did not finish loading"
    );
    settle(&menu, &mut latest, Duration::from_secs(5))?;
    save(latest.as_ref().unwrap(), out.join("home.png"))?;
    if let Some(points) = std::env::args().nth(2) {
        for (i, point) in points.split(';').enumerate() {
            let (x, y) = point
                .split_once(',')
                .ok_or_else(|| anyhow::anyhow!("clicks: x,y;x,y"))?;
            let (x, y) = (x.parse::<f64>()?, y.parse::<f64>()?);
            menu.send(Input::MouseMove { x, y });
            menu.send(Input::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            });
            menu.send(Input::MouseUp {
                x,
                y,
                button: MouseButton::Left,
            });
            settle(&menu, &mut latest, Duration::from_secs(3))?;
            save(latest.as_ref().unwrap(), out.join(format!("click-{i}.png")))?;
        }
    }
    if let Some(commands) = std::env::args().nth(4) {
        for (index, command) in commands.split(';').enumerate() {
            let parts: Vec<_> = command.split('|').collect();
            anyhow::ensure!(parts.len() >= 2, "calls: path|function|arg,arg;...");
            let args = parse_args(parts.get(2).copied().unwrap_or_default())?;
            if parts[0] == "@batch" {
                // Production queues widget init/update/show together. Keeping
                // those calls adjacent exposes load-order bugs hidden by the
                // normal three-second observation after each probe command.
                for call in args {
                    let Value::List(call) = call else {
                        anyhow::bail!("batch calls must be [path,function,args] arrays");
                    };
                    let [Value::String(path), Value::String(function), Value::List(args)] =
                        call.as_slice()
                    else {
                        anyhow::bail!("batch calls must be [path,function,args] arrays");
                    };
                    menu.send(widget_input(path, function, args.clone())?);
                }
            } else if parts[0].starts_with('@') {
                for input in pointer_inputs(parts[0], &args)? {
                    menu.send(input);
                }
            } else {
                menu.send(widget_input(parts[0], parts[1], args)?);
            }
            settle(&menu, &mut latest, Duration::from_secs(3))?;
            save(
                latest.as_ref().unwrap(),
                out.join(format!("called-{index}.png")),
            )?;
        }
    }
    if let Some(paths) = std::env::args().nth(3) {
        for path in paths.split(';') {
            menu.send(Input::Inspect(path.into()));
        }
        settle(&menu, &mut latest, Duration::from_secs(1))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_actions_preserve_press_release_order_and_reject_invalid_coordinates() {
        let inputs = pointer_inputs("@click", &[10.0.into(), 20.0.into()]).unwrap();
        assert_eq!(inputs.len(), 3);
        assert!(matches!(inputs[0], Input::MouseMove { x, y } if x == 10.0 && y == 20.0));
        assert!(
            matches!(inputs[1], Input::MouseDown { x, y, button: MouseButton::Left } if x == 10.0 && y == 20.0)
        );
        assert!(
            matches!(inputs[2], Input::MouseUp { x, y, button: MouseButton::Left } if x == 10.0 && y == 20.0)
        );
        assert!(pointer_inputs("@warp", &[10.0.into(), 20.0.into()]).is_err());
        assert!(pointer_inputs("@click", &[f64::NAN.into(), 20.0.into()]).is_err());
        assert!(pointer_inputs("@click", &["10".into(), 20.0.into()]).is_err());
        assert!(pointer_inputs("@click", &[]).is_err());
    }

    #[test]
    fn structured_arguments_preserve_nested_choices_and_literal_commas() {
        let args =
            parse_args(r#"[0,1,"Master, volume","volume",{"range":[0,1],"enabled":true},null]"#)
                .unwrap();
        assert_eq!(args[2], Value::String("Master, volume".into()));
        let Value::Object(fields) = &args[4] else {
            panic!("widget data must remain an object")
        };
        assert_eq!(fields.len(), 2);
        assert!(fields.contains(&("enabled".into(), Value::Bool(true))));
        assert!(fields.contains(&(
            "range".into(),
            Value::List(vec![Value::Number(0.0), Value::Number(1.0)])
        )));
        assert_eq!(args[5], Value::Null);
        assert!(parse_args("[invalid]").is_err());
    }

    #[test]
    fn primitive_and_empty_arguments_remain_compatible() {
        assert!(parse_args("").unwrap().is_empty());
        assert_eq!(
            parse_args("false,0.5,volume,undefined").unwrap(),
            vec![
                Value::Bool(false),
                Value::Number(0.5),
                Value::String("volume".into()),
                Value::Undefined
            ]
        );
    }
}
