//! Headless developer proof without compiling the renderer. Uses the exact
//! pack installer and shared level loader used by launcher/server sessions.
use std::path::Path;

fn run(args: &[String]) -> Result<(), String> {
    let file = Path::new(args.first().ok_or("Usage: packcheck <file.csrpack> <map ContentId> <private data folder> [Legacy game folder]")?);
    let id = args
        .get(1)
        .ok_or("map ContentId required")?
        .parse()
        .map_err(|e| format!("{e}"))?;
    let data = Path::new(args.get(2).ok_or("private data folder required")?);
    let store = packs::Installed::new(data);
    let installed = store.install(file)?;
    let (pack, map) = store.resolve_map(&id)?;
    if installed.archive_hash() != pack.archive_hash() {
        return Err("installed archive differs from source snapshot".into());
    }
    let (game_dir, fs) = if let Some(game) = args.get(3) {
        level::mount_game(Some(Path::new(game))).map_err(|e| e.to_string())?
    } else {
        if map.source == packs::MapSource::Legacy {
            return Err("recipe proof requires a Legacy game folder".into());
        }
        (data.to_path_buf(), vfs::Vfs::new())
    };
    let level = level::Level::load_pack(&pack, &map, game_dir, fs).map_err(|e| e.to_string())?;
    if level.id != id {
        return Err("loaded map identity changed".into());
    }
    println!(
        "{} -> maps/{}.bsp\narchive: {}\nBSP version: {}\nNAV: {}\nweapons: {}",
        level.id,
        level.name,
        pack.archive_hash(),
        level.bsp.version,
        if level.nav.is_some() {
            "present"
        } else {
            "absent"
        },
        level.catalog.weapons.len()
    );
    Ok(())
}

fn main() {
    if let Err(e) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
