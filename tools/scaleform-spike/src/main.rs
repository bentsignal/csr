//! Scaleform spike: can Ruffle play CS:GO's own Scaleform menu movies?
//!
//! Everything is read from the player's CS:GO Legacy install at runtime.
//! Output images contain Valve art: write them outside the repository.
//!
//! ```text
//! scaleform-spike dump <movie|all>...         list imports, exports, fonts, text fields
//! scaleform-spike render <out.png> [frames]   boot the main menu and save a 1920x1080 frame
//! scaleform-spike diff <a.png> <b.png> [out]  difference metric between two images
//! ```

mod bridge;
mod diff;
mod dump;
mod host;
mod rewrite;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, bail};
use ruffle_core::external::Value;

/// Engine components the movies call as `_global.CScaleformComponent_<Name>`.
const COMPONENTS: &[&str] = &[
    "Blog", "CompetitiveMatch", "EmbeddedStream", "Fantasy", "FriendsList", "GameTypes",
    "IME", "ImageCache", "Inventory", "ItemData", "Leaderboards", "Loadout", "MatchDraft",
    "MatchInfo", "MatchList", "MatchStats", "Medals", "Missions", "MyPersona", "News",
    "Overwatch", "PartyBrowser", "PartyList", "Predictions", "SteamOverlay", "Store",
    "Streams", "Teammates", "UGC",
];

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,scaleform_spike=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        bail!("usage: scaleform-spike dump <movie|all>... | render <out.png> [frames] | diff <a> <b> [out]");
    };
    match cmd.as_str() {
        "dump" => dump::run(&game_dir()?.join("resource").join("flash"), &args[1..]),
        "render" => {
            let out = args.get(1).context("render <out.png> [frames]")?;
            let frames = args.get(2).map(|f| f.parse()).transpose()?.unwrap_or(90);
            render(Path::new(out), frames)
        }
        "ls" => {
            let mut fs = vfs::Vfs::new();
            fs.mount_game_dir(&game_dir()?)?;
            for p in fs.archive_paths_with_prefix(args.get(1).map_or("resource/flash/", String::as_str)) {
                println!("{p}");
            }
            Ok(())
        }
        "diff" => {
            let (a, b) = (args.get(1).context("diff <a> <b>")?, args.get(2).context("diff <a> <b>")?);
            diff::run(Path::new(a), Path::new(b), args.get(3).map(Path::new))
        }
        other => bail!("unknown command {other}"),
    }
}

fn render(out: &Path, frames: u32) -> Result<()> {
    let game = game_dir()?;
    let loc = Rc::new(rewrite::Localization::load(&game.join("resource").join("csgo_english.txt"))?);
    tracing::info!("localization: {} tokens", loc.len());
    let mut host = host::Host::new(&game, loc, 1920, 1080)?;

    let mut boot_frames = 0;
    while !host.bridge.borrow().root_ready {
        host.step();
        boot_frames += 1;
        if boot_frames > 300 {
            bail!("root movie never became ready (fetched {:?})", host.files.fetched.borrow());
        }
    }
    tracing::info!("root ready after {boot_frames} frames");
    let names = COMPONENTS.iter().map(|c| Value::String(format!("CScaleformComponent_{c}"))).collect();
    host.call_as("csrDefineComponents", vec![Value::List(names)]);
    host.call_as("csrRequestElement", vec![Value::String("MainMenu".into()), Value::String("MainMenu".into())]);

    let start = std::time::Instant::now();
    let mut load_frames = 0;
    while !host.bridge.borrow().calls.contains_key("MainMenu.OnLoadFinished") {
        host.step();
        load_frames += 1;
        if load_frames > 3000 {
            bail!("MainMenu never finished loading");
        }
    }
    tracing::info!("MainMenu loaded after {load_frames} frames ({:?})", start.elapsed());

    // What the engine did once the menu was up: show it, report the game
    // coordinator connection and the store price sheet, and hand the news
    // panel its HTML render target (a page rendered offscreen; here a flat
    // placeholder, since the 2018 blog page is web content, not Scaleform).
    let menu = "_global.MainMenuMovie";
    let call = |host: &mut host::Host, path: &str, f: &str, args: Vec<Value>| {
        let r = host.call_as("csrCall", vec![Value::String(path.into()), Value::String(f.into()), Value::List(args)]);
        tracing::info!("{path}.{f}() -> {r:?}");
        r
    };
    call(&mut host, menu, "onUIShow", vec![]);
    for event in ["ScaleformComponent_Store_PriceSheetChanged"] {
        call(&mut host, menu, event, vec![]);
    }
    host.files.insert("img://blog", host::solid_png(1024, 1024, [27, 31, 36]));
    let blog = "_global.MainMenuMovie.Panel.Blog";
    call(&mut host, blog, "InitChromeHTMLRenderTarget", vec![Value::String("img://blog".into())]);
    call(&mut host, blog, "setPageLoadState", vec![Value::Bool(false)]);
    let start = std::time::Instant::now();
    for _ in 0..frames {
        host.step();
    }
    tracing::info!("{frames} more frames in {:?}", start.elapsed());
    // SPIKE_CLICKS="x,y;x,y": click each point (window pixels), 30 frames apart.
    if let Ok(clicks) = std::env::var("SPIKE_CLICKS") {
        for point in clicks.split(';').filter(|p| !p.is_empty()) {
            let (x, y) = point.split_once(',').context("SPIKE_CLICKS: x,y;x,y")?;
            let (x, y): (f64, f64) = (x.trim().parse()?, y.trim().parse()?);
            tracing::info!("click {x},{y}");
            host.click(x, y);
            for _ in 0..30 {
                host.step();
            }
        }
    }
    // Steady-state cost of one frame: advance, render, read back to the CPU
    // (what a Bevy integration through an offscreen copy would pay).
    let _ = host.capture()?;
    let start = std::time::Instant::now();
    for _ in 0..10 {
        host.step();
        let _ = host.capture()?;
    }
    tracing::info!("tick + render + readback: {:?} per frame", start.elapsed() / 10);
    let image = host.capture()?;
    image.save(out).with_context(|| format!("saving {}", out.display()))?;
    println!("wrote {} ({}x{})", out.display(), image.width(), image.height());

    let bridge = host.bridge.borrow();
    println!("rewrite: {:?}", host.files.stats.borrow());
    let mut fetched = host.files.fetched.borrow().clone();
    fetched.sort();
    fetched.dedup();
    println!("fetched: {}", fetched.join(" "));
    println!("missing: {:?}", host.files.missing.borrow());
    println!("calls: {} distinct, {} handled", bridge.calls.len(), bridge.handled.len());
    for (name, count) in &bridge.calls {
        let mark = if bridge.handled.contains_key(name) { "+" } else { " " };
        println!("  {mark} {count:5} {name}");
    }
    Ok(())
}

/// The CS:GO Legacy `csgo` folder: `CSR_GAME_DIR`, or the usual Steam paths.
fn game_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CSR_GAME_DIR") {
        return Ok(PathBuf::from(dir));
    }
    for candidate in [
        r"C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo",
        r"C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive\csgo",
    ] {
        let path = PathBuf::from(candidate);
        if path.join("resource").join("flash").join("mainmenu.swf").is_file() {
            return Ok(path);
        }
    }
    bail!("CS:GO Legacy not found; set CSR_GAME_DIR to its csgo folder")
}
