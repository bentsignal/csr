//! Data-only `.csrpack` archives. No pack file is extracted or executed.
//! Validation, mounting and installation use one immutable byte snapshot.
//! Private assets stay outside the repository and public releases.

mod install;
#[cfg(test)]
mod tests;
pub use install::Installed;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;

use content::{ContentId, ContentKind, GameOrigin};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const EXTENSION: &str = "csrpack";
pub const MANIFEST: &str = "pack.toml";
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bounds apply before allocating or decompressing data. `open_with_limits`
/// permits smaller budgets for previews, downloads and tests.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub archive: u64,
    pub entry: u64,
    pub expanded: u64,
    pub entries: usize,
    pub manifest: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            archive: 256 << 20,
            entry: 256 << 20,
            expanded: 512 << 20,
            entries: 8192,
            manifest: 256 << 10,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PackKind {
    Recipe,
    Assets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MapSource {
    /// BSP and dependencies come from this private asset pack.
    Pack,
    /// BSP comes from the player's CS:GO Legacy install, read only.
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub id: String,
    /// Safe basename, independent of ContentId. No `de_`/`cs_` requirement.
    pub bsp: String,
    pub name: String,
    pub source: MapSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDef {
    pub id: String,
    pub path: String,
}

/// Version 1 intentionally supports maps and model/material/sound resources.
/// Weapon overrides, importer recipes and sandboxed mechanics are later APIs.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub origin: String,
    pub kind: PackKind,
    pub provides: Vec<String>,
    pub content_hash: String,
    #[serde(default = "zero_version")]
    pub min_game_version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub maps: Vec<MapDef>,
    #[serde(default)]
    pub resources: Vec<ResourceDef>,
}

fn zero_version() -> String {
    "0.0.0".into()
}

fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 96 && content::sanitize(s) == s && !s.contains('_') // origins are one ContentId component
}

pub fn valid_bsp_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !reserved(s)
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
}

pub(crate) fn valid_pack_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 96 && content::sanitize(s) == s && !reserved(s)
}

fn version(s: &str) -> Option<[u32; 3]> {
    let parts = s.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|s| {
            s.is_empty()
                || !s.bytes().all(|b| b.is_ascii_digit())
                || (s.len() > 1 && s.starts_with('0'))
        })
    {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

fn hash_valid(s: &str) -> bool {
    s.strip_prefix("sha256:").is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Self = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        m.validate()?;
        Ok(m)
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }

    fn validate(&self) -> Result<(), String> {
        if self.format_version != 1 {
            return Err("unsupported pack format_version (expected 1)".into());
        }
        if !valid_pack_id(&self.id) {
            return Err("invalid pack id".into());
        }
        if !identifier(&self.origin) {
            return Err("invalid origin: use a single lowercase ContentId component".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 256 || self.description.len() > 4096 {
            return Err("invalid pack name or description length".into());
        }
        if version(&self.version).is_none() || version(&self.min_game_version).is_none() {
            return Err("versions must be three numeric components (e.g. 1.0.0)".into());
        }
        if version(GAME_VERSION) < version(&self.min_game_version) {
            return Err(format!(
                "pack needs CSGO Rewrite {} or newer",
                self.min_game_version
            ));
        }
        if !hash_valid(&self.content_hash) {
            return Err("invalid content_hash: expected sha256:<64 lowercase hex digits>".into());
        }
        if self.provides.len() > 4096 {
            return Err("too many provided ContentIds".into());
        }
        let mut provided = BTreeSet::new();
        for id in &self.provides {
            let parsed: ContentId = id.parse().map_err(|e| format!("provides: {e}"))?;
            if parsed.origin.prefix() != self.origin || !provided.insert(parsed) {
                return Err(format!("duplicate or wrong-origin ContentId: {id}"));
            }
        }
        let mut defined = BTreeSet::new();
        for map in &self.maps {
            let id: ContentId = map.id.parse().map_err(|e| format!("map: {e}"))?;
            if id.kind != ContentKind::Map
                || !valid_bsp_name(&map.bsp)
                || map.name.trim().is_empty()
                || map.name.len() > 256
            {
                return Err(format!("invalid map definition: {}", map.id));
            }
            if (self.kind == PackKind::Recipe) != (map.source == MapSource::Legacy) {
                return Err("recipe maps must read Legacy; asset maps must read their pack".into());
            }
            if !defined.insert(id) {
                return Err(format!("duplicate definition: {}", map.id));
            }
        }
        for resource in &self.resources {
            let id: ContentId = resource.id.parse().map_err(|e| format!("resource: {e}"))?;
            let extension = resource.path.rsplit('.').next().unwrap_or("");
            let valid = match id.kind {
                ContentKind::Model => resource.path.starts_with("models/") && extension == "mdl",
                ContentKind::Material => {
                    resource.path.starts_with("materials/") && extension == "vmt"
                }
                ContentKind::Sound => {
                    resource.path.starts_with("sound/")
                        && matches!(extension, "wav" | "mp3" | "ogg")
                }
                _ => false,
            };
            if !valid
                || canonical_path(&resource.path)? != resource.path
                || self.kind != PackKind::Assets
            {
                return Err(format!("unsupported resource definition: {}", resource.id));
            }
            if !defined.insert(id) {
                return Err(format!("duplicate definition: {}", resource.id));
            }
        }
        if provided != defined {
            return Err("provides must match the map/resource definitions exactly".into());
        }
        Ok(())
    }
}

fn reserved(s: &str) -> bool {
    let base = s.split('.').next().unwrap_or(s).to_ascii_lowercase();
    matches!(base.as_str(), "con" | "prn" | "aux" | "nul")
        || (base.len() == 4
            && (base.starts_with("com") || base.starts_with("lpt"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'))
}

/// Canonical ZIP namespace: ASCII lowercase, slash-separated relative paths.
/// Reject aliases rather than let hash verification and VFS disagree.
fn canonical_path(path: &str) -> Result<String, String> {
    if path.is_empty()
        || path.len() > 240
        || !path.is_ascii()
        || path.contains('\\')
        || path.starts_with('/')
        || path
            .bytes()
            .any(|b| b < 32 || b == 127 || b"<>:\"|?*".contains(&b))
    {
        return Err(format!("unsafe archive path: {path:?}"));
    }
    for part in path.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with('.')
            || part.ends_with(' ')
            || reserved(part)
        {
            return Err(format!("unsafe archive path: {path:?}"));
        }
    }
    Ok(path.to_ascii_lowercase())
}

fn asset_path(path: &str) -> bool {
    let extension = path.rsplit('.').next().unwrap_or("");
    match path.split('/').next().unwrap_or("") {
        "maps" => matches!(extension, "bsp" | "nav"),
        "materials" => matches!(extension, "vmt" | "vtf"),
        "models" => matches!(extension, "mdl" | "vvd" | "vtx" | "phy" | "ani"),
        "sound" => matches!(extension, "wav" | "mp3" | "ogg"),
        "resource" => path.starts_with("resource/overviews/") && extension == "txt",
        _ => false,
    }
}

/// Payload hash, independent of ZIP compression/order/timestamps. Metadata is
/// excluded; use `Pack::archive_hash` to identify bytes advertised by a server.
pub fn content_hash(files: &BTreeMap<String, Arc<[u8]>>) -> String {
    let mut hash = Sha256::new();
    for (path, data) in files.iter().filter(|(p, _)| p.as_str() != MANIFEST) {
        hash.update(path.as_bytes());
        hash.update([0]);
        hash.update((data.len() as u64).to_le_bytes());
        hash.update(data);
    }
    digest(hash.finalize().as_slice())
}

fn digest(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

#[derive(Clone, Debug)]
pub struct Pack {
    manifest: Manifest,
    archive: Arc<[u8]>,
    assets: BTreeMap<String, Arc<[u8]>>,
    archive_hash: String,
}

impl Pack {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn archive_hash(&self) -> &str {
        &self.archive_hash
    }
    pub fn archive_size(&self) -> u64 {
        self.archive.len() as u64
    }
    pub fn origin(&self) -> GameOrigin {
        GameOrigin::from_prefix(&self.manifest.origin)
    }
    pub fn mount(&self) -> vfs::Mount {
        vfs::Mount::Memory(self.assets.clone())
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        Self::open_with_limits(path, Limits::default())
    }

    pub fn open_with_limits(path: &Path, limits: Limits) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if file.metadata().map_err(|e| e.to_string())?.len() > limits.archive {
            return Err("pack archive exceeds size limit".into());
        }
        let mut bytes = Vec::new();
        file.take(limits.archive.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Self::from_bytes_with_limits(bytes, limits)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        Self::from_bytes_with_limits(bytes, Limits::default())
    }

    pub fn from_bytes_with_limits(bytes: Vec<u8>, limits: Limits) -> Result<Self, String> {
        if bytes.len() as u64 > limits.archive {
            return Err("pack archive exceeds size limit".into());
        }
        let mut zip = zip::ZipArchive::new(Cursor::new(&bytes))
            .map_err(|e| format!("not a content pack: {e}"))?;
        if zip.len() > limits.entries {
            return Err("pack exceeds entry count limit".into());
        }
        let mut names = BTreeSet::new();
        let mut files = BTreeMap::new();
        let mut expanded = 0u64;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
            let is_dir = entry.is_dir();
            let name = canonical_path(if is_dir {
                entry.name().trim_end_matches('/')
            } else {
                entry.name()
            })?;
            if !names.insert(name.clone()) {
                return Err(format!("duplicate archive path: {name}"));
            }
            if entry.unix_mode().is_some_and(|mode| {
                (mode & 0o170000) != 0
                    && (mode & 0o170000) != if is_dir { 0o040000 } else { 0o100000 }
            }) {
                return Err(format!("non-regular archive entry: {name}"));
            }
            if !matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ) {
                return Err("unsupported pack compression".into());
            }
            if is_dir {
                if entry.size() != 0 {
                    return Err("directory contains data".into());
                }
                continue;
            }
            let bound = if name == MANIFEST {
                limits.manifest.min(limits.entry)
            } else {
                limits.entry
            };
            expanded = expanded
                .checked_add(entry.size())
                .ok_or("expanded pack size overflow")?;
            if entry.size() > bound || expanded > limits.expanded {
                return Err("expanded pack exceeds size limit".into());
            }
            if name != MANIFEST && !asset_path(&name) {
                return Err(format!(
                    "unsupported data file: {name}; executable code and unsupported definitions are refused"
                ));
            }
            let size = entry.size();
            let mut data = Vec::new();
            entry
                .by_ref()
                .take(bound.saturating_add(1))
                .read_to_end(&mut data)
                .map_err(|e| format!("{name}: {e}"))?;
            if data.len() as u64 != size || data.len() as u64 > bound {
                return Err(format!("invalid expanded length: {name}"));
            }
            files.insert(name, Arc::<[u8]>::from(data));
        }
        let manifest_bytes = files.get(MANIFEST).ok_or("no pack.toml")?;
        let manifest = Manifest::parse(
            std::str::from_utf8(manifest_bytes).map_err(|_| "pack.toml must be UTF-8")?,
        )?;
        if content_hash(&files) != manifest.content_hash {
            return Err("pack content hash mismatch".into());
        }
        files.remove(MANIFEST);
        if manifest.kind == PackKind::Recipe && !files.is_empty() {
            return Err("recipe packs must contain no assets".into());
        }
        for map in &manifest.maps {
            if map.source == MapSource::Pack
                && !files.contains_key(&format!("maps/{}.bsp", map.bsp))
            {
                return Err(format!("{}: declared BSP missing from pack", map.id));
            }
        }
        for resource in &manifest.resources {
            if !files.contains_key(&resource.path) {
                return Err(format!(
                    "{}: declared resource missing from pack",
                    resource.id
                ));
            }
        }
        // Undeclared BSPs cannot enter the map list through generic VFS scans.
        for path in files
            .keys()
            .filter(|p| p.starts_with("maps/") && p.ends_with(".bsp"))
        {
            if !manifest
                .maps
                .iter()
                .any(|m| path == &format!("maps/{}.bsp", m.bsp))
            {
                return Err(format!("undeclared map: {path}"));
            }
        }
        let archive_hash = digest(&Sha256::digest(&bytes));
        Ok(Self {
            manifest,
            archive: bytes.into(),
            assets: files,
            archive_hash,
        })
    }
}

/// Developer builder. Validates the resulting archive with the same consumer
/// rules; names are canonical and duplicate-free before writing.
pub fn build(manifest: &Manifest, inputs: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut files = BTreeMap::new();
    for (path, data) in inputs {
        let name = canonical_path(path)?;
        if name == MANIFEST || files.insert(name, Arc::<[u8]>::from(*data)).is_some() {
            return Err("duplicate or reserved builder path".into());
        }
    }
    let mut manifest = manifest.clone();
    manifest.content_hash = content_hash(&files);
    manifest.validate()?;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(MANIFEST, options)
        .map_err(|e| e.to_string())?;
    zip.write_all(manifest.to_toml()?.as_bytes())
        .map_err(|e| e.to_string())?;
    for (name, bytes) in files {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    let bytes = zip.finish().map_err(|e| e.to_string())?.into_inner();
    Pack::from_bytes(bytes.clone())?;
    Ok(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Install,
    Workshop(u64),
    Pack { id: String, archive_hash: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: ContentId,
    pub source: Source,
    pub map: Option<MapDef>,
    pub resource_path: Option<String>,
}

/// An explicit ContentId registry; conflicts are errors, never first-wins.
#[derive(Default, Debug)]
pub struct Registry {
    entries: BTreeMap<ContentId, Entry>,
}

impl Registry {
    pub fn get(&self, id: &ContentId) -> Option<&Entry> {
        self.entries.get(id)
    }
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }
    pub fn add(&mut self, entry: Entry) -> Result<(), String> {
        if self.entries.contains_key(&entry.id) {
            return Err(format!("ContentId already provided: {}", entry.id));
        }
        self.entries.insert(entry.id.clone(), entry);
        Ok(())
    }
    /// Checks all entries before mutation, so a conflict leaves the registry intact.
    pub fn add_pack(&mut self, pack: &Pack) -> Result<(), String> {
        let mut entries = Vec::new();
        let source = Source::Pack {
            id: pack.manifest.id.clone(),
            archive_hash: pack.archive_hash.clone(),
        };
        for map in &pack.manifest.maps {
            entries.push(Entry {
                id: map.id.parse().map_err(|e| format!("{e}"))?,
                source: source.clone(),
                map: Some(map.clone()),
                resource_path: None,
            });
        }
        for resource in &pack.manifest.resources {
            entries.push(Entry {
                id: resource.id.parse().map_err(|e| format!("{e}"))?,
                source: source.clone(),
                map: None,
                resource_path: Some(resource.path.clone()),
            });
        }
        for entry in &entries {
            if self.entries.contains_key(&entry.id) {
                return Err(format!("ContentId already provided: {}", entry.id));
            }
        }
        self.entries
            .extend(entries.into_iter().map(|e| (e.id.clone(), e)));
        Ok(())
    }
}
