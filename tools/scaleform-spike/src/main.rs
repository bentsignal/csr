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
    let start = std::time::Instant::now();
    for _ in 0..frames {
        host.step();
    }
    tracing::info!("{frames} more frames in {:?}", start.elapsed());
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
