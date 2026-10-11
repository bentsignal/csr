//! Standalone library proof. Captures stay outside the source checkout.
//! cargo run -p scaleform --example menu_probe -- C:\Users\shawn\csr-fidelity\menu-host
use scaleform::{Calls, Config, Engine, Event, Frame, Input, Menu, MouseButton, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Default)]
struct ProbeEngine { seen: std::collections::HashSet<(String, String)> }
impl Engine for ProbeEngine {
    fn call(&mut self, object: &str, method: &str, args: &[Value], calls: &mut Calls) -> Option<Value> {
        if self.seen.insert((object.into(), method.into())) || method == "BasePanelRunCommand" {
            tracing::info!("{object}.{method} {args:?}");
        }
        match (object, method) {
            ("MainMenu", "OnLoadFinished") => calls.call("_global.MainMenuMovie", "onUIShow", vec![]),
            ("MainMenu", "BasePanelRunCommand") => match args.first().map(Value::str).as_deref() {
                Some("OpenCreateSinglePlayerGameDialog") => calls.request_element("StartSinglePlayer"),
                Some("OpenVideoSettingsDialog") => calls.request_element("OptionsMenu"),
                _ => {}
            },
            ("CScaleformComponent_MyPersona", "GetXuid") => return Some("0".into()),
            ("CScaleformComponent_MyPersona", "GetXpPerLevel") => return Some(5000.0.into()),
            ("CScaleformComponent_FriendsList", "GetFriendName") if args.first().is_some_and(|v| v.str() == "0") => return Some("Player".into()),
            _ => {}
        }
        None
    }
}

fn settle(menu: &Menu, latest: &mut Option<Frame>, duration: Duration) -> anyhow::Result<bool> {
    let deadline = Instant::now() + duration;
    let mut loaded = false;
    loop {
        while let Some(event) = menu.poll_event() {
            match event {
                Event::Failed(message) => anyhow::bail!("{message}"),
                Event::Loaded(name) => { loaded |= name == "MainMenu"; tracing::info!("loaded {name}"); }
            }
        }
        if let Some(frame) = menu.take_frame() { *latest = Some(frame); }
        if Instant::now() >= deadline { return Ok(loaded); }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn save(frame: &Frame, path: PathBuf) -> anyhow::Result<()> {
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone()).ok_or_else(|| anyhow::anyhow!("invalid frame dimensions"))?;
    image.save(&path)?;
    println!("saved {} ({}x{}, frame {})", path.display(), frame.width, frame.height, frame.serial);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn,scaleform=info,menu_probe=info").init();
    let out = PathBuf::from(std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("menu_probe <absolute output folder outside the repo>"))?);
    anyhow::ensure!(out.is_absolute(), "Output must be an absolute path outside the repository");
    std::fs::create_dir_all(&out)?;
    let out = out.canonicalize()?;
    let cwd = std::env::current_dir()?.canonicalize()?;
    let repo = cwd.ancestors().find(|p| p.join(".git").exists()).unwrap_or(&cwd);
    anyhow::ensure!(!out.starts_with(repo), "Captures contain game art; choose a folder outside the repository");
    let game_dir = vfs::find_csgo_dir().ok_or_else(|| anyhow::anyhow!("CS:GO Legacy is not installed"))?;
    let menu = Menu::spawn(Config { game_dir, width: 1920, height: 1080, language: "english".into(), elements: vec!["MainMenu".into()] }, Box::new(ProbeEngine::default()));
    let mut latest = None;
    let mut ready = false;
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready && Instant::now() < deadline { ready |= settle(&menu, &mut latest, Duration::from_millis(100))?; }
    anyhow::ensure!(ready && latest.is_some(), "Main menu did not finish loading");
    settle(&menu, &mut latest, Duration::from_secs(5))?;
    save(latest.as_ref().unwrap(), out.join("home.png"))?;
    if let Some(points) = std::env::args().nth(2) {
        for (i, point) in points.split(';').enumerate() {
            let (x, y) = point.split_once(',').ok_or_else(|| anyhow::anyhow!("clicks: x,y;x,y"))?;
            let (x, y) = (x.parse::<f64>()?, y.parse::<f64>()?);
            menu.send(Input::MouseMove { x, y });
            menu.send(Input::MouseDown { x, y, button: MouseButton::Left });
            menu.send(Input::MouseUp { x, y, button: MouseButton::Left });
            settle(&menu, &mut latest, Duration::from_secs(3))?;
            save(latest.as_ref().unwrap(), out.join(format!("click-{i}.png")))?;
        }
    }
    Ok(())
}
