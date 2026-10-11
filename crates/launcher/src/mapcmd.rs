//! `csr maps`, `csr fetch-map`, `csr remove-map`: the old map versions from
//! Valve's Workshop snapshots (see the `workshop` crate). Playing, probing or
//! simulating an old map fetches it by itself; these commands are for
//! development checks, prefetching and freeing disk space. Navigation and
//! fidelity are being validated before archives appear in the match menu.

use std::io::Write;

/// Runs a map command; `None` if `args` is not one.
pub fn run(args: &[String]) -> Option<i32> {
    let rest = args.get(1..).unwrap_or_default();
    Some(match args.first()?.as_str() {
        "maps" => list(),
        "fetch-map" => fetch(rest),
        "remove-map" => remove(rest),
        _ => return None,
    })
}

fn list() -> i32 {
    let (game_dir, fs) = match level::mount_game(None) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    println!("{:<20} {:<22} {:<10} {}", "map", "title", "status", "source");
    for m in level::maps::list(&game_dir, &fs) {
        let status = match m.status {
            level::maps::MapStatus::Ready if m.old.is_some() => "downloaded",
            level::maps::MapStatus::Ready => "installed",
            level::maps::MapStatus::InSteam => "in Steam",
            level::maps::MapStatus::Download => "download",
        };
        let source = match m.old {
            Some(old) => format!("Workshop {} (v{}, {:.0} MB), {}", old.workshop_id, old.version, old.size as f64 / 1e6, old.note),
            None => "CS:GO Legacy".into(),
        };
        println!("{:<20} {:<22} {:<10} {}", m.name, m.title, status, source);
    }
    println!("\nWorkshop archives are experimental; navigation and fidelity need validation.\nThey are not in the match menu yet. Loading an archive fetches it automatically; `csr fetch-map all` prefetches them.");
    0
}

/// The old maps named on the command line (`all` for every one).
fn targets(args: &[String]) -> Result<Vec<&'static workshop::OldMap>, String> {
    if args.is_empty() {
        return Err("name an old map (see `csr maps`) or `all`".into());
    }
    if args.iter().any(|a| a == "all") {
        return Ok(workshop::OLD_MAPS.iter().collect());
    }
    args.iter()
        .map(|a| workshop::find(a).ok_or_else(|| format!("{a} is not an old map version (see `csr maps`)")))
        .collect()
}

fn fetch(args: &[String]) -> i32 {
    let maps = match targets(args) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let store = workshop::Store::current();
    let mut failures = 0;
    for map in maps {
        let mut last = None;
        let result = store.fetch(map, &mut |p| {
            let pct = if p.total > 0 { p.done * 100 / p.total } else { 0 };
            if last != Some((p.stage, pct)) {
                last = Some((p.stage, pct));
                eprint!("\r{:<18} {:<9} {pct:>3}%", map.name, format!("{:?}", p.stage));
                let _ = std::io::stderr().flush();
            }
            true
        });
        eprint!("\r{:<40}\r", "");
        match result {
            Ok(path) => println!("{:<18} ready: {}", map.name, path.display()),
            Err(e) => {
                failures += 1;
                println!("{:<18} failed: {e}", map.name);
            }
        }
    }
    if failures == 0 { 0 } else { 1 }
}

fn remove(args: &[String]) -> i32 {
    let maps = match targets(args) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let store = workshop::Store::current();
    for map in maps {
        match store.remove(map) {
            Ok(true) => println!("{} removed", map.name),
            Ok(false) => {}
            Err(e) => {
                eprintln!("error: {}: {e}", map.name);
                return 1;
            }
        }
    }
    0
}
