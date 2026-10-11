//! `dump`: list the structure of Scaleform movies (imports, exports, fonts,
//! text fields) so we learn how the movies reference each other. Prints facts
//! only; nothing is written to disk.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use ruffle_core::swf;

pub fn run(flash_dir: &Path, names: &[String]) -> Result<()> {
    let mut names = names.to_vec();
    if names.iter().any(|n| n == "all") {
        names = std::fs::read_dir(flash_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str()?.strip_suffix(".swf").map(str::to_string))
            .collect();
        names.sort();
    }
    for name in &names {
        let path = flash_dir.join(format!("{name}.swf"));
        let data = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let buf = swf::decompress_swf(&data[..])?;
        let parsed = swf::parse_swf(&buf)?;
        let h = &parsed.header;
        println!(
            "== {name}: version {} stage {:?} fps {} frames {}",
            h.version(),
            h.stage_size(),
            h.frame_rate(),
            h.num_frames()
        );
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut fonts: BTreeMap<u16, String> = BTreeMap::new();
        walk(&parsed.tags, 0, &mut counts, &mut fonts);
        println!("   tags: {counts:?}");
    }
    Ok(())
}

fn walk(
    tags: &[swf::Tag],
    depth: usize,
    counts: &mut BTreeMap<String, usize>,
    fonts: &mut BTreeMap<u16, String>,
) {
    let pad = "   ".repeat(depth + 1);
    for tag in tags {
        let kind = format!("{tag:?}");
        let kind = kind
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or("")
            .to_string();
        *counts.entry(kind).or_default() += 1;
        match tag {
            swf::Tag::ImportAssets { url, imports } => {
                let list: Vec<String> =
                    imports.iter().map(|a| format!("{}:{}", a.id, a.name.to_str_lossy(swf::UTF_8))).collect();
                println!("{pad}import {} -> {}", url.to_str_lossy(swf::UTF_8), list.join(", "));
            }
            swf::Tag::ExportAssets(list) => {
                let list: Vec<String> =
                    list.iter().map(|a| format!("{}:{}", a.id, a.name.to_str_lossy(swf::UTF_8))).collect();
                if list.len() > 12 {
                    println!("{pad}export {} symbols: {} ...", list.len(), list[..12].join(", "));
                } else {
                    println!("{pad}export {}", list.join(", "));
                }
            }
            swf::Tag::DefineFont2(f) => {
                let name = f.name.to_str_lossy(swf::UTF_8).to_string();
                println!(
                    "{pad}font{} id {} '{}' glyphs {} flags {:?}",
                    f.version,
                    f.id,
                    name,
                    f.glyphs.len(),
                    f.flags
                );
                fonts.insert(f.id, name);
            }
            swf::Tag::DefineFont4(f) => {
                println!("{pad}font4 id {} '{}'", f.id, f.name.to_str_lossy(swf::UTF_8));
            }
            swf::Tag::DefineEditText(t) => {
                let font = t.font_id().and_then(|id| fonts.get(&id)).cloned().unwrap_or_default();
                if depth == 0 || std::env::var_os("DUMP_ALL_TEXT").is_some() {
                    println!(
                        "{pad}edittext id {} font {} '{}' class '{}' h {} outlines {} html {} var '{}' text '{}'",
                        t.id(),
                        t.font_id().unwrap_or(0),
                        font,
                        t.font_class().map(|c| c.to_str_lossy(swf::UTF_8).to_string()).unwrap_or_default(),
                        t.height().map(|h| h.to_pixels()).unwrap_or(0.0),
                        t.use_outlines(),
                        t.flags().contains(swf::EditTextFlag::HTML),
                        t.variable_name().to_str_lossy(swf::UTF_8),
                        t.initial_text()
                            .map(|s| s.to_str_lossy(swf::UTF_8).chars().take(40).collect::<String>())
                            .unwrap_or_default()
                    );
                }
            }
            swf::Tag::DefineSprite(s) => walk(&s.tags, depth + 1, counts, fonts),
            _ => {}
        }
    }
}
