//! Installed, read-only VHV comparator. No graphics, physics tick, asset
//! export, color decoding or claim of individual record-to-vertex order.
use mdl::vhv::{InstanceKey, ScopedFile};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    time::{Duration, Instant},
};

const EXPECTED_BSP: [u8; 20] = [
    0xd9, 0x05, 0xc6, 0xf7, 0xdd, 0x69, 0x4a, 0x0d, 0xac, 0xe9, 0x38, 0x0a, 0x85, 0x58, 0xca, 0x8a,
    0x17, 0x7c, 0xbd, 0x91,
];
const MAX_READ_BYTES: usize = 512 * 1024 * 1024;
const MAX_ELAPSED: Duration = Duration::from_secs(60);

fn checksum(bytes: &[u8], at: usize) -> Result<u32, Box<dyn Error>> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated model checksum")?
            .try_into()?,
    ))
}
fn model_bytes(fs: &vfs::Vfs, path: &str, total: &mut usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let bytes = fs
        .read(path)
        .ok_or_else(|| format!("missing model data: {path}"))?;
    // Existing VFS performs whole-file reads: post-read admission limits are
    // not a streaming I/O bound or OS watchdog.
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("model file exceeds comparator bound".into());
    }
    *total = total
        .checked_add(bytes.len())
        .ok_or("byte total overflow")?;
    if *total > MAX_READ_BYTES {
        return Err("comparator byte budget exceeded".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let (game_dir, fs) = level::mount_game(None)?;
    let level =
        level::Level::load_from_vfs("csgo_map_de_dust2".parse()?, "de_dust2", game_dir, fs)?;
    if level.bsp_sha1() != &EXPECTED_BSP {
        return Err("Dust2 identity differs from admitted reference".into());
    }
    let props = &level.bsp.static_props;
    if props.props.len() > 8192 || props.models.len() > 4096 {
        return Err("prop/model count exceeds comparator bounds".into());
    }
    let mut by_model: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (ordinal, prop) in props.props.iter().enumerate() {
        let path = props
            .models
            .get(prop.model as usize)
            .ok_or("prop model index past dictionary")?;
        by_model.entry(path).or_default().push(ordinal);
    }
    let mut read_bytes = 0usize;
    let mut instances = 0usize;
    let mut spans = 0usize;
    let mut vertices = 0u64;
    // Load each model once, check its instances, then drop its geometry.
    for (path, ordinals) in &by_model {
        if start.elapsed() > MAX_ELAPSED {
            return Err("comparator elapsed bound exceeded".into());
        }
        let base = path
            .strip_suffix(".mdl")
            .ok_or("model name lacks MDL suffix")?;
        let mdl = model_bytes(&level.vfs, path, &mut read_bytes)?;
        let vvd = model_bytes(&level.vfs, &format!("{base}.vvd"), &mut read_bytes)?;
        let vtx = model_bytes(&level.vfs, &format!("{base}.dx90.vtx"), &mut read_bytes)?;
        let expected_checksum = checksum(&mdl, 8)?;
        if checksum(&vvd, 8)? != expected_checksum || checksum(&vtx, 16)? != expected_checksum {
            return Err("MDL/VVD/VTX checksums differ".into());
        }
        let model = mdl::Model::load(&mdl, &vvd, &vtx)?;
        for &ordinal in ordinals {
            if start.elapsed() > MAX_ELAPSED {
                return Err("comparator elapsed bound exceeded".into());
            }
            let key = InstanceKey::new(level.id.clone(), *level.bsp_sha1(), ordinal.try_into()?)?;
            let name = key.hdr_name();
            let bytes = level
                .vfs
                .read_from("map", &name)
                .ok_or_else(|| format!("selected BSP lacks {name}"))?;
            read_bytes = read_bytes
                .checked_add(bytes.len())
                .ok_or("byte total overflow")?;
            if read_bytes > MAX_READ_BYTES {
                return Err("comparator byte budget exceeded".into());
            }
            let file = ScopedFile::parse(key.clone(), &name, &bytes)?;
            let candidate = file.candidate(&key, &model)?;
            instances += 1;
            spans += candidate.matching_spans().len();
            vertices += u64::from(file.file().vertex_count);
        }
    }
    // Preserve every raw group's checksum checks. Canonical names describe
    // VFS aliases; they must never suppress verification of a raw group.
    let canonical_names: BTreeSet<_> = by_model.keys().map(|name| vpk::normalize(name)).collect();
    println!(
        "vhv_metadata aggregate raw_model_groups={} canonical_model_groups={} scoped_instances={instances} mesh_spans={spans} opaque_records={vertices} admitted_read_bytes={read_bytes}",
        by_model.len(),
        canonical_names.len()
    );
    for canonical in &canonical_names {
        let aliases: Vec<_> = by_model
            .keys()
            .filter(|name| vpk::normalize(name) == *canonical)
            .collect();
        if aliases.len() > 1 {
            println!("vhv_metadata alias canonical={canonical} raw_names={aliases:?}");
        }
    }
    if instances != 3158 || spans != 3749 || by_model.len() != 1258 || canonical_names.len() != 1257
    {
        return Err(format!("installed count sequence differs from admitted reference: raw_models={} canonical_models={} instances={instances} spans={spans}", by_model.len(), canonical_names.len()).into());
    }
    println!(
        "map={} bsp_sha1=d905c6f7dd694a0dace9380a8558ca8a177cbd91 raw_models={} canonical_models={} scoped_instances={instances} mesh_spans={spans} opaque_records={vertices} admitted_read_bytes={read_bytes} checksum_scope_count_failures=0 payload_semantics=OPAQUE vertex_order=NOT_PROVEN failures: 0",
        level.id,
        by_model.len(),
        canonical_names.len()
    );
    Ok(())
}
