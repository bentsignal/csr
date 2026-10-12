//! Developer commands for the same installer used by future drop/join flows.
use std::path::{Path, PathBuf};

pub fn run(args: &[String]) -> Option<i32> {
    let command = args.first()?.as_str();
    if !matches!(
        command,
        "install-pack" | "packs" | "remove-pack" | "check-pack"
    ) {
        return None;
    }
    let store = packs::Installed::current();
    let result = match command {
        "install-pack" | "check-pack" => args
            .get(1)
            .ok_or("a .csrpack path is required".into())
            .and_then(|path| {
                let pack = if command == "install-pack" {
                    store.install(Path::new(path))?
                } else {
                    packs::Pack::open(Path::new(path))?
                };
                println!(
                    "{} {} ({:?})\n  archive: {}\n  content: {}",
                    pack.manifest().name,
                    pack.manifest().version,
                    pack.manifest().kind,
                    pack.archive_hash(),
                    pack.manifest().content_hash
                );
                for id in &pack.manifest().provides {
                    println!("  {id}");
                }
                if command == "install-pack" {
                    println!(
                        "Installed in {}. Available on the next map load.",
                        store.dir().display()
                    );
                }
                Ok(())
            }),
        "remove-pack" => args
            .get(1)
            .ok_or("a pack id is required".into())
            .and_then(|id| {
                if store.remove(id)? {
                    println!("Removed {id}.");
                } else {
                    println!("{id} is not installed.");
                }
                Ok(())
            }),
        _ => {
            let catalog = store.catalog();
            for entry in catalog.entries {
                let manifest = entry.manifest();
                println!(
                    "{} {} — {}",
                    manifest.id,
                    manifest.version,
                    manifest.name
                );
                for map in &manifest.maps {
                    println!(
                        "  {}: {} ({:?}, maps/{}.bsp)",
                        map.id, map.name, map.source, map.bsp
                    );
                }
                for resource in &manifest.resources {
                    println!("  {}: {}", resource.id, resource.path);
                }
            }
            if catalog.errors.is_empty() {
                Ok(())
            } else {
                Err(catalog.errors.join("\n"))
            }
        }
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    })
}

/// Resolve pack arguments before setup changes the working directory.
pub fn absolutize(args: &mut [String], cwd: &Path) {
    if args
        .first()
        .is_some_and(|s| matches!(s.as_str(), "install-pack" | "check-pack" | "--open-pack"))
        && let Some(path) = args.get_mut(1)
    {
        let parsed = PathBuf::from(&*path);
        if parsed.is_relative() {
            *path = cwd.join(parsed).to_string_lossy().into_owned();
        }
    } else if let Some(path) = args.first_mut().filter(|s| crate::packopen::is_pack(s)) {
        let parsed = PathBuf::from(&*path);
        if parsed.is_relative() {
            *path = cwd.join(parsed).to_string_lossy().into_owned();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn relative_pack_argument_survives_userdata_directory_change() {
        let cwd = std::env::current_dir().unwrap();
        for command in ["install-pack", "check-pack", "--open-pack"] {
            let mut args = vec![command.into(), "friend pack.csrpack".into()];
            super::absolutize(&mut args, &cwd);
            assert_eq!(
                std::path::PathBuf::from(&args[1]),
                cwd.join("friend pack.csrpack")
            );
        }
        let mut bare = vec!["friend pack.CSRPACK".into()];
        super::absolutize(&mut bare, &cwd);
        assert_eq!(
            std::path::PathBuf::from(&bare[0]),
            cwd.join("friend pack.CSRPACK")
        );
    }
}
