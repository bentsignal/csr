//! `csr` — CSGO Rewrite launcher.
//!
//! ```text
//! csr [play] [map] [options]   open the main menu (default map de_dust2)
//!     --screenshot <file.png>  render, save a screenshot, exit (smoke test)
//!     --frames <n>             frames to render before the screenshot (120)
//!     --camera x y z pitch yaw start in noclip at a Source position
//!     --exposure <f>           brightness multiplier (2.0)
//!     --size <w> <h>           window size (1600 900)
//!     --bots <n>               enemy bots, teammates get n-1 (5)
//!     --team <t|ct>            side to play (ct)
//!     --deathmatch             respawn deathmatch instead of competitive rounds
//! csr sim [map] [--tickrate N] headless movement, combat and round checks
//! csr probe [map]              load every asset a map references and report
//! csr weapons                  print the weapon catalog from items_game.txt
//! csr cases                    list available local cases
//! csr give-case <name|id> [N]   add local cases, no keys required
//! ```
//! Set `CSR_GAME_DIR` if the CS:GO install is not found automatically.

mod art;
mod dialog;
mod probe;
mod setup;
mod simcheck;
mod steam;
mod update;

use std::path::PathBuf;

fn main() {
    // Installer hooks and pending updates; must run before anything else.
    update::startup();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("CSGO Rewrite\n\n\
            csr play [map] [options]  Open the main menu (default: de_dust2)\n\
            csr sim [map] [--tickrate N]  Check movement, combat and rounds (default 128 tick)\n\
            csr probe [map]           Validate the map's models and textures\n\
            csr setup                 Choose the CS:GO Legacy folder again\n\
            csr find-csgo             Show the CS:GO installs found on this PC\n\
            csr add-to-steam          Add CSGO Rewrite to your Steam library\n\
            csr weapons               List weapons from your CS:GO install\n\n\
            csr cases                 List local cases and their IDs\n\
            csr give-case NAME [N]    Add N local cases (names may contain spaces)\n\n\
            Play options:\n\
              --direct                Skip the menu and enter the match\n\
              --team t|ct             Your side (default: ct)\n\
              --deathmatch            Respawns and a fixed loadout\n\
              --bots N                Enemy bots; N-1 teammates (default: 5)\n\
              --size W H              Window resolution (default: 1600 900)\n\
              --exposure F            Brightness (default: 2)\n\
              --screenshot FILE.png   Render a screenshot and exit\n\
              --frames N              Screenshot frame (default: 120)\n\
              --camera X Y Z P YAW    Noclip camera in Source coordinates\n\n\
            B: buy | Tab: scoreboard | 5 + hold LMB: plant | hold E: defuse\n\
            Set CSR_GAME_DIR to use a specific CS:GO Legacy install.");
        return;
    }
    if let Some(code) = setup::early_command(&args) {
        std::process::exit(code);
    }
    let setup_only = args.first().is_some_and(|a| a == "setup");
    if let Err(e) = setup::prepare(setup_only) {
        setup::report(&e);
        std::process::exit(1);
    }
    if setup_only {
        println!("CS:GO Legacy folder: {}", std::env::var("CSR_GAME_DIR").unwrap_or_default());
        return;
    }
    if args.first().is_some_and(|a| a == "add-to-steam") {
        steam::offer(&std::env::current_dir().unwrap_or_default(), true);
        return;
    }
    update::check_in_background();
    let (cmd, rest) = match args.first().map(String::as_str) {
        Some("sim") | Some("probe") | Some("play") | Some("weapons") | Some("cases") | Some("give-case") | Some("give-skin") | Some("give-sticker") | Some("stickers") | Some("skins") => (args[0].as_str(), &args[1..]),
        _ => ("play", &args[..]),
    };
    let map = rest
        .first()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "de_dust2".into());
    let code = match cmd {
        "probe" => probe::run(&map),
        "sim" => match flag_value(rest, "--tickrate").map(|v| v.parse::<u32>()) {
            None if !rest.iter().any(|a| a == "--tickrate") => simcheck::run(&map, sim::DEFAULT_TICK_RATE),
            Some(Ok(rate)) if sim::TICK_RATE_RANGE.contains(&rate) => simcheck::run(&map, rate),
            Some(_) | None => {
                eprintln!("error: --tickrate takes a number from {} to {}", sim::TICK_RATE_RANGE.start(), sim::TICK_RATE_RANGE.end());
                2
            }
        },
        "weapons" => weapons_cmd(),
        "cases" | "give-case" | "give-skin" | "give-sticker" | "stickers" | "skins" => cosmetics_cmd(cmd,rest),
        _ => match parse_play(&map, rest) {
            Ok(opts) => client::run(opts),
            Err(e) => {
                eprintln!("error: {e}");
                2
            }
        },
    };
    std::process::exit(code);
}

/// The value after `--flag`, if given.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn cosmetics_cmd(cmd:&str,args:&[String])->i32 {
    let result=(||{
        let (_,fs)=level::mount_game(None).map_err(|e|e.to_string())?;
        let catalog=client::cosmetics::Catalog::load(&fs)?;
        let path=client::cosmetics::profile_path();
        let mut inventory=client::cosmetics::Inventory::load(&path)?;
        client::cosmetics::command(&format!("{cmd} {}",args.join(" ")),&catalog,&mut inventory,&path)
    })();
    match result{Ok(s)=>{println!("{s}");0},Err(e)=>{eprintln!("error: {e}");1}}
}

fn weapons_cmd() -> i32 {
    let (_, fs) = match level::mount_game(None) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let Some(text) = fs.read_string("scripts/items/items_game.txt") else {
        eprintln!("error: scripts/items/items_game.txt not found");
        return 1;
    };
    let catalog = match weapons::Catalog::from_items_game(&text) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: items_game.txt: {e}");
            return 1;
        }
    };
    println!(
        "{:<24} {:<5} {:<10} {:>5} {:>6} {:>5} {:>6} {:>5} {:>4} {:>5} {:>6}",
        "id", "def", "class", "price", "damage", "armor", "rpm", "clip", "auto", "speed", "recoil"
    );
    for w in &catalog.weapons {
        println!(
            "{:<24} {:<5} {:<10} {:>5} {:>6.0} {:>5.2} {:>6.0} {:>5} {:>4} {:>5.0} {:>6.0}",
            w.id.to_string(),
            w.def_index,
            format!("{:?}", w.class),
            w.price,
            w.damage,
            w.armor_ratio,
            w.rpm(),
            w.clip_size,
            if w.full_auto { "yes" } else { "" },
            w.max_player_speed,
            w.recoil.magnitude
        );
    }
    println!("{} weapons", catalog.weapons.len());
    0
}

fn parse_play(map: &str, args: &[String]) -> Result<client::ClientOptions, String> {
    let mut opts = client::ClientOptions {
        map: map.to_string(),
        ..Default::default()
    };
    let num = |s: Option<&String>, what: &str| -> Result<f32, String> {
        s.ok_or(format!("{what} needs a value"))?
            .parse::<f32>()
            .map_err(|_| format!("{what}: not a number"))
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" => {
                opts.screenshot = Some(PathBuf::from(args.get(i + 1).ok_or("--screenshot needs a path")?));
                i += 1;
            }
            "--frames" => {
                opts.frames = num(args.get(i + 1), "--frames")? as u32;
                i += 1;
            }
            "--exposure" => {
                opts.exposure = num(args.get(i + 1), "--exposure")?;
                i += 1;
            }
            "--team" => {
                opts.terrorist = args.get(i + 1).is_some_and(|t| t.eq_ignore_ascii_case("t"));
                i += 1;
            }
            "--deathmatch" => opts.deathmatch = true,
            "--direct" => opts.direct = true,
            "--bots" => {
                opts.bots = num(args.get(i + 1), "--bots")? as u32;
                i += 1;
            }
            "--size" => {
                opts.width = num(args.get(i + 1), "--size")? as u32;
                opts.height = num(args.get(i + 2), "--size")? as u32;
                i += 2;
            }
            "--camera" => {
                let v: Result<Vec<f32>, String> = (1..=5).map(|k| num(args.get(i + k), "--camera")).collect();
                let v = v?;
                opts.camera = Some((glam::Vec3::new(v[0], v[1], v[2]), v[3], v[4]));
                i += 5;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(opts)
}
