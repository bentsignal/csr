//! Developer authoring tool. Its source folder and output stay outside Git.
use std::path::{Path, PathBuf};

fn gather(root: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("authoring folder cannot contain links".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if entry
                .metadata()
                .map_err(|e| e.to_string())?
                .file_attributes()
                & 0x400
                != 0
            {
                return Err("authoring folder cannot contain reparse points".into());
            }
        }
        let path = entry.path();
        if kind.is_dir() {
            gather(root, &path, files)?;
        } else if kind.is_file() {
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("non-UTF-8 path")?
                .replace('\\', "/");
            if name != packs::MANIFEST {
                if entry.metadata().map_err(|e| e.to_string())?.len()
                    > packs::Limits::default().entry
                {
                    return Err("authoring file too large".into());
                }
                files.push((name, std::fs::read(&path).map_err(|e| e.to_string())?));
            }
        } else {
            return Err("authoring folder contains a non-regular file".into());
        }
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("build") => {
            let root = PathBuf::from(
                args.get(1)
                    .ok_or("build needs source folder and output file")?,
            );
            let out = PathBuf::from(args.get(2).ok_or("build needs output file")?);
            let mut manifest: packs::Manifest = packs::Manifest::parse(
                &std::fs::read_to_string(root.join(packs::MANIFEST)).map_err(|e| e.to_string())?,
            )?;
            // The builder computes the payload hash; the placeholder in the template is valid hex.
            manifest.content_hash.clear();
            let mut files = Vec::new();
            gather(&root, &root, &mut files)?;
            let borrowed = files
                .iter()
                .map(|(p, b)| (p.as_str(), b.as_slice()))
                .collect::<Vec<_>>();
            let bytes = packs::build(&manifest, &borrowed)?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out)
                .map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut output, &bytes).map_err(|e| e.to_string())?;
            println!("Built {} ({} bytes)", out.display(), bytes.len());
            Ok(())
        }
        Some("check") => {
            let pack = packs::Pack::open(Path::new(args.get(1).ok_or("check needs pack file")?))?;
            println!(
                "{} {}\n{}",
                pack.manifest().name,
                pack.manifest().version,
                pack.archive_hash()
            );
            Ok(())
        }
        _ => Err(
            "Usage: packtool build <source folder> <new output.csrpack> | check <file.csrpack>"
                .into(),
        ),
    }
}

fn main() {
    if let Err(e) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
