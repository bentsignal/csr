//! Content packs: `.csrpack` files, the first cut of the connection layer.
//!
//! A pack is a zip:
//!
//! ```text
//! pack.toml                 manifest (see [`Manifest`])
//! defs/weapons/*.toml       weapon definitions (see [`WeaponDef`])
//! materials/... models/...  assets, at their game paths (asset packs only)
//! ```
//!
//! Installed packs live in `<data>\packs\<id>.csrpack` ([`Installed`]).
//! [`Installed::mount`] puts each one on the file system ahead of the game
//! (so an asset pack can add or replace files), and
//! [`Installed::apply_weapons`] adds their weapon definitions to the
//! catalog. Everything a pack adds has a [`ContentId`] and is listed in its
//! manifest's `provides`, which feeds the [`Registry`].
//!
//! Format and plans: `docs/MODDING.md`.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use content::{ContentId, ContentKind, GameOrigin};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const EXTENSION: &str = "csrpack";
pub const MANIFEST: &str = "pack.toml";
/// Subfolder of the data folder holding installed packs.
pub const FOLDER: &str = "packs";
/// Version of this game, compared with a pack's `min_game_version`.
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What a pack carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackKind {
    /// Data only: definitions and instructions for reading content from
    /// the player's own installs. Holds no assets, so it is safe to share
    /// publicly.
    Recipe,
    /// Carries extracted assets. Private: never committed, never on public
    /// releases.
    Assets,
}

impl PackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PackKind::Recipe => "recipe",
            PackKind::Assets => "assets",
        }
    }
}

/// `pack.toml`.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    /// Stable id, `[a-z0-9_]`; also the installed file name.
    pub id: String,
    pub name: String,
    pub version: String,
    /// Game the content comes from (`csgo`, `iw4`, `mod`, ...).
    pub origin: GameOrigin,
    pub kind: PackKind,
    /// Everything the pack adds.
    pub provides: Vec<ContentId>,
    /// `sha256:<hex>` over every file except `pack.toml` ([`content_hash`]).
    pub content_hash: String,
    /// Oldest game version that can load the pack.
    pub min_game_version: String,
    pub description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    id: String,
    name: String,
    version: String,
    origin: String,
    kind: String,
    #[serde(default)]
    provides: Vec<String>,
    content_hash: String,
    #[serde(default = "zero_version")]
    min_game_version: String,
    #[serde(default)]
    description: String,
}

fn zero_version() -> String {
    "0.0.0".into()
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let raw: RawManifest = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        if raw.id.is_empty() || content::sanitize(&raw.id) != raw.id {
            return Err(format!("{MANIFEST}: id \"{}\" must be lowercase letters, digits and single underscores", raw.id));
        }
        let kind = match raw.kind.as_str() {
            "recipe" => PackKind::Recipe,
            "assets" => PackKind::Assets,
            k => return Err(format!("{MANIFEST}: kind must be \"recipe\" or \"assets\", not \"{k}\"")),
        };
        if raw.origin.is_empty() || content::sanitize(&raw.origin) != raw.origin {
            return Err(format!("{MANIFEST}: bad origin \"{}\"", raw.origin));
        }
        let provides = raw
            .provides
            .iter()
            .map(|p| p.parse::<ContentId>().map_err(|e| format!("{MANIFEST}: provides: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        parse_version(&raw.version).ok_or_else(|| format!("{MANIFEST}: bad version \"{}\"", raw.version))?;
        parse_version(&raw.min_game_version).ok_or_else(|| format!("{MANIFEST}: bad min_game_version \"{}\"", raw.min_game_version))?;
        Ok(Manifest {
            id: raw.id,
            name: raw.name,
            version: raw.version,
            origin: GameOrigin::from_prefix(&raw.origin),
            kind,
            provides,
            content_hash: raw.content_hash,
            min_game_version: raw.min_game_version,
            description: raw.description,
        })
    }

    /// The manifest as `pack.toml` text.
    pub fn to_toml(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let provides: Vec<String> = self.provides.iter().map(|p| q(&p.to_string())).collect();
        let mut out = format!(
            "id = {}\nname = {}\nversion = {}\norigin = {}\nkind = {}\nmin_game_version = {}\nprovides = [{}]\ncontent_hash = {}\n",
            q(&self.id),
            q(&self.name),
            q(&self.version),
            q(self.origin.prefix()),
            q(self.kind.as_str()),
            q(&self.min_game_version),
            provides.join(", "),
            q(&self.content_hash)
        );
        if !self.description.is_empty() {
            out += &format!("description = {}\n", q(&self.description));
        }
        out
    }
}

/// `1.2.3` → `[1, 2, 3]`.
pub fn parse_version(v: &str) -> Option<Vec<u64>> {
    v.trim().split('.').map(|p| p.parse().ok()).collect()
}

/// `a >= b` for dotted versions (missing parts count as 0).
pub fn version_at_least(a: &str, b: &str) -> bool {
    let (Some(mut a), Some(mut b)) = (parse_version(a), parse_version(b)) else { return false };
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a >= b
}

/// A weapon a pack adds: a variant of an existing weapon with its own id,
/// name and stats. `defs/weapons/*.toml`:
///
/// ```toml
/// [[weapon]]
/// id = "mod_weapon_ak47_gold"
/// base = "csgo_weapon_ak47"
/// name = "Golden AK-47"
/// [weapon.attributes]          # items_game.txt attribute names
/// damage = 40
/// "in game price" = 3500
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponDef {
    pub id: ContentId,
    pub base: ContentId,
    pub name: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub view_model: Option<String>,
    pub world_model: Option<String>,
    pub fire_sound: Option<String>,
}

#[derive(Deserialize)]
struct RawWeapons {
    #[serde(default)]
    weapon: Vec<RawWeapon>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWeapon {
    id: String,
    base: String,
    name: Option<String>,
    #[serde(default)]
    attributes: BTreeMap<String, toml::Value>,
    view_model: Option<String>,
    world_model: Option<String>,
    fire_sound: Option<String>,
}

fn parse_weapons(file: &str, text: &str) -> Result<Vec<WeaponDef>, String> {
    let raw: RawWeapons = toml::from_str(text).map_err(|e| format!("{file}: {e}"))?;
    raw.weapon
        .into_iter()
        .map(|w| {
            let id: ContentId = w.id.parse().map_err(|e| format!("{file}: {e}"))?;
            let base: ContentId = w.base.parse().map_err(|e| format!("{file}: {e}"))?;
            if id.kind != ContentKind::Weapon || base.kind != ContentKind::Weapon {
                return Err(format!("{file}: {id} and {base} must both be weapon ids"));
            }
            let attributes = w
                .attributes
                .into_iter()
                .map(|(k, v)| {
                    let v = match v {
                        toml::Value::String(s) => s,
                        toml::Value::Integer(i) => i.to_string(),
                        toml::Value::Float(f) => f.to_string(),
                        toml::Value::Boolean(b) => if b { "1" } else { "0" }.to_string(),
                        other => return Err(format!("{file}: attribute \"{k}\" must be a string, number or boolean, not {other}")),
                    };
                    Ok((k.to_ascii_lowercase(), v))
                })
                .collect::<Result<_, String>>()?;
            Ok(WeaponDef {
                id,
                base,
                name: w.name,
                attributes,
                view_model: w.view_model,
                world_model: w.world_model,
                fire_sound: w.fire_sound,
            })
        })
        .collect()
}

/// Content hash of a pack's files (everything but `pack.toml`): SHA-256
/// over, for each file sorted by its normalised path, the path, a zero
/// byte, the length as 8 little-endian bytes, and the data.
pub fn content_hash<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    let mut files: Vec<(String, &[u8])> = files.into_iter().map(|(p, d)| (vfs_path(p), d)).filter(|(p, _)| p != MANIFEST).collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = Sha256::new();
    for (path, data) in files {
        h.update(path.as_bytes());
        h.update([0u8]);
        h.update((data.len() as u64).to_le_bytes());
        h.update(data);
    }
    let digest = h.finalize();
    format!("sha256:{}", digest.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn vfs_path(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

/// Whether a zip entry name is a safe relative path.
fn safe_path(p: &str) -> bool {
    let p = p.replace('\\', "/");
    !p.is_empty() && !p.starts_with('/') && !p.contains(':') && !p.split('/').any(|c| c == "..")
}

/// A pack file, read and checked.
#[derive(Clone, Debug)]
pub struct Pack {
    pub manifest: Manifest,
    pub path: PathBuf,
    pub weapons: Vec<WeaponDef>,
    /// Asset paths (everything outside `pack.toml` and `defs/`).
    pub assets: Vec<String>,
}

impl Pack {
    /// Reads and validates a `.csrpack`: manifest, content hash, game
    /// version, definitions, and the recipe/asset rule.
    pub fn open(path: &Path) -> Result<Pack, String> {
        let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
        let mut zip = zip::ZipArchive::new(File::open(path).map_err(|e| err(&e))?).map_err(|_| err(&"not a content pack (not a zip file)"))?;
        let mut files: Vec<(String, Vec<u8>)> = Vec::with_capacity(zip.len());
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| err(&e))?;
            if f.is_dir() {
                continue;
            }
            let name = f.name().to_string();
            if !safe_path(&name) {
                return Err(err(&format!("unsafe path {name}")));
            }
            let mut data = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut data).map_err(|e| err(&format!("{name}: {e}")))?;
            files.push((vfs_path(&name), data));
        }
        Pack::from_files(path, files).map_err(|e| err(&e))
    }

    fn from_files(path: &Path, files: Vec<(String, Vec<u8>)>) -> Result<Pack, String> {
        let manifest = files
            .iter()
            .find(|(p, _)| p == MANIFEST)
            .ok_or("not a content pack (no pack.toml)")?;
        let manifest = Manifest::parse(&String::from_utf8_lossy(&manifest.1))?;
        if !version_at_least(GAME_VERSION, &manifest.min_game_version) {
            return Err(format!(
                "{} needs CSGO Rewrite {} or newer (this is {GAME_VERSION}); update the game",
                manifest.name, manifest.min_game_version
            ));
        }
        let hash = content_hash(files.iter().map(|(p, d)| (p.as_str(), d.as_slice())));
        if hash != manifest.content_hash {
            return Err(format!("content hash mismatch: the pack is damaged or was edited (expected {}, got {hash})", manifest.content_hash));
        }
        let mut weapons = Vec::new();
        let mut assets = Vec::new();
        for (p, data) in &files {
            if p == MANIFEST {
                continue;
            }
            if let Some(rest) = p.strip_prefix("defs/") {
                match rest.split_once('/') {
                    Some(("weapons", f)) if f.ends_with(".toml") => weapons.extend(parse_weapons(p, &String::from_utf8_lossy(data))?),
                    _ => return Err(format!("{p}: unknown definition file (supported: defs/weapons/*.toml)")),
                }
            } else {
                assets.push(p.clone());
            }
        }
        if manifest.kind == PackKind::Recipe && !assets.is_empty() {
            return Err(format!("a recipe pack holds no assets, but this one has {} (e.g. {}); make it kind = \"assets\"", assets.len(), assets[0]));
        }
        for w in &weapons {
            if !manifest.provides.contains(&w.id) {
                return Err(format!("{} is defined but not listed in provides", w.id));
            }
        }
        Ok(Pack {
            manifest,
            path: path.to_path_buf(),
            weapons,
            assets,
        })
    }

    /// The pack's files as a file-system mount (assets at their game paths).
    pub fn mount(&self) -> io::Result<vfs::Mount> {
        Ok(vfs::Mount::Zip(vfs::ZipArchive::from_bytes(fs::read(&self.path)?)?))
    }
}

/// Writes a `.csrpack` (zip) from a manifest and files, filling in the
/// content hash. For tools and tests.
pub fn build(manifest: &Manifest, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut manifest = manifest.clone();
    manifest.content_hash = content_hash(files.iter().copied());
    let mut out = io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(MANIFEST, opts).expect("zip");
    zip.write_all(manifest.to_toml().as_bytes()).expect("zip");
    for (name, data) in files {
        zip.start_file(*name, opts).expect("zip");
        zip.write_all(data).expect("zip");
    }
    zip.finish().expect("zip");
    out.into_inner()
}

/// The player's installed packs: `<data>\packs\*.csrpack`.
#[derive(Clone, Debug)]
pub struct Installed {
    dir: PathBuf,
}

impl Installed {
    pub fn new(data_dir: &Path) -> Installed {
        Installed { dir: data_dir.join(FOLDER) }
    }

    /// Packs in the current data folder: `CSR_DATA_DIR`, or the working
    /// directory (the launcher runs inside the data folder).
    pub fn current() -> Installed {
        let data = std::env::var_os("CSR_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        Installed::new(&data)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Checks a pack file and copies it in as `<id>.csrpack`, replacing an
    /// installed pack with the same id. The copy appears atomically.
    pub fn install(&self, file: &Path) -> Result<Pack, String> {
        let pack = Pack::open(file)?;
        fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let target = self.dir.join(format!("{}.{EXTENSION}", pack.manifest.id));
        let same = fs::canonicalize(file).ok().zip(fs::canonicalize(&target).ok()).is_some_and(|(a, b)| a == b);
        if !same {
            let tmp = target.with_extension(format!("{EXTENSION}.part"));
            fs::copy(file, &tmp)
                .and_then(|_| fs::rename(&tmp, &target))
                .map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    format!("{}: {e}", target.display())
                })?;
        }
        Ok(Pack { path: target, ..pack })
    }

    /// Every installed pack, sorted by id, plus the files that failed to
    /// load (with the reason).
    pub fn load(&self) -> (Vec<Pack>, Vec<String>) {
        let mut packs = Vec::new();
        let mut errors = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else { return (packs, errors) };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION)))
            .collect();
        paths.sort();
        for path in paths {
            match Pack::open(&path) {
                Ok(p) => packs.push(p),
                Err(e) => errors.push(e),
            }
        }
        (packs, errors)
    }

    /// Deletes an installed pack by id.
    pub fn remove(&self, id: &str) -> io::Result<bool> {
        let (packs, _) = self.load();
        let mut removed = false;
        for p in packs.iter().filter(|p| p.manifest.id == id) {
            fs::remove_file(&p.path)?;
            removed = true;
        }
        let file = self.dir.join(format!("{id}.{EXTENSION}"));
        if !removed && file.is_file() {
            fs::remove_file(file)?;
            removed = true;
        }
        Ok(removed)
    }

    /// Mounts every valid installed pack ahead of everything already on
    /// `fs`, labelled `pack:<id>`. Returns the packs.
    pub fn mount(&self, fs: &mut vfs::Vfs) -> Vec<Pack> {
        let (packs, errors) = self.load();
        for e in errors {
            eprintln!("warning: content pack skipped: {e}");
        }
        // Reverse so the first pack (by id) ends up in front.
        for p in packs.iter().rev() {
            match p.mount() {
                Ok(m) => fs.push_front(format!("pack:{}", p.manifest.id), m),
                Err(e) => eprintln!("warning: content pack {}: {e}", p.manifest.id),
            }
        }
        packs
    }
}

/// Adds the packs' weapon definitions to a catalog. Each one starts as a
/// copy of its base weapon, then takes the pack's name, models, sound and
/// attributes. Returns warnings (unknown base weapons, duplicate ids).
pub fn apply_weapons(packs: &[Pack], catalog: &mut weapons::Catalog) -> Vec<String> {
    let mut warnings = Vec::new();
    for pack in packs {
        for def in &pack.weapons {
            if catalog.by_id(&def.id).is_some() {
                warnings.push(format!("{}: {} already exists; skipped", pack.manifest.id, def.id));
                continue;
            }
            let Some(base) = catalog.by_id(&def.base) else {
                warnings.push(format!("{}: {} is based on unknown weapon {}; skipped", pack.manifest.id, def.id, def.base));
                continue;
            };
            let overrides: HashMap<String, String> = def.attributes.clone().into_iter().collect();
            let mut w = base.variant(def.id.clone(), &overrides);
            w.def_index = variant_index(&def.id, catalog);
            if let Some(name) = &def.name {
                w.name_token = name.clone();
            }
            if let Some(m) = &def.view_model {
                w.view_model = m.clone();
            }
            if let Some(m) = &def.world_model {
                w.world_model = m.clone();
            }
            if let Some(s) = &def.fire_sound {
                w.fire_sound = s.clone();
            }
            catalog.weapons.push(w);
        }
    }
    warnings
}

/// A stable item index for a pack weapon, outside the range items_game.txt
/// uses, so buying and dropping it never confuses it with its base.
fn variant_index(id: &ContentId, catalog: &weapons::Catalog) -> u32 {
    // FNV-1a over the id; probe on the rare collision.
    let mut h: u32 = 0x811c_9dc5;
    for b in id.to_string().bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    let mut index = 1_000_000 + h % 1_000_000;
    while catalog.weapons.iter().any(|w| w.def_index == index) {
        index += 1;
    }
    index
}

/// Where a piece of content comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The player's CS:GO Legacy install.
    Install,
    /// An old map version from Valve's Workshop (item id).
    Workshop(u64),
    /// An installed content pack (pack id).
    Pack(String),
}

/// Every known piece of content, keyed by [`ContentId`].
#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub entries: BTreeMap<ContentId, Source>,
}

impl Registry {
    /// Records `id` unless something already provides it (first wins).
    pub fn add(&mut self, id: ContentId, source: Source) {
        self.entries.entry(id).or_insert(source);
    }

    /// Everything the packs provide.
    pub fn add_packs(&mut self, packs: &[Pack]) {
        for p in packs {
            for id in &p.manifest.provides {
                self.add(id.clone(), Source::Pack(p.manifest.id.clone()));
            }
        }
    }

    pub fn get(&self, id: &ContentId) -> Option<&Source> {
        self.entries.get(id)
    }

    pub fn of_kind(&self, kind: ContentKind) -> impl Iterator<Item = (&ContentId, &Source)> {
        self.entries.iter().filter(move |(id, _)| id.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("csr-packs-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn manifest(kind: PackKind, provides: &[&str]) -> Manifest {
        Manifest {
            id: "test_pack".into(),
            name: "Test pack".into(),
            version: "1.0.0".into(),
            origin: GameOrigin::Mod,
            kind,
            provides: provides.iter().map(|p| p.parse().unwrap()).collect(),
            content_hash: String::new(),
            min_game_version: "0.1.0".into(),
            description: "Made in a test".into(),
        }
    }

    const GOLD: &str = r#"
[[weapon]]
id = "mod_weapon_ak47_gold"
base = "csgo_weapon_ak47"
name = "Golden AK-47"
[weapon.attributes]
damage = 40
"In Game Price" = 3500
"#;

    #[test]
    fn manifest_round_trips_and_rejects_bad_ids() {
        let m = manifest(PackKind::Recipe, &["mod_weapon_ak47_gold"]);
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
        assert!(Manifest::parse(&m.to_toml().replace("test_pack", "Test Pack")).is_err());
        assert!(Manifest::parse(&m.to_toml().replace("\"recipe\"", "\"code\"")).is_err());
        assert!(Manifest::parse(&format!("{}surprise = 1\n", m.to_toml())).is_err());
        assert!(version_at_least("0.2.0", "0.2") && version_at_least("1.0", "0.9.9") && !version_at_least("0.2.0", "0.10.0"));
    }

    #[test]
    fn opens_valid_packs_and_rejects_broken_ones() {
        let dir = temp("open");
        let write = |name: &str, bytes: &[u8]| {
            let p = dir.join(name);
            fs::write(&p, bytes).unwrap();
            p
        };
        let good = build(&manifest(PackKind::Recipe, &["mod_weapon_ak47_gold"]), &[("defs/weapons/gold.toml", GOLD.as_bytes())]);
        let pack = Pack::open(&write("good.csrpack", &good)).unwrap();
        assert_eq!(pack.weapons.len(), 1);
        assert_eq!(pack.weapons[0].attributes["in game price"], "3500");

        // A recipe pack may not carry assets.
        let assets = build(&manifest(PackKind::Recipe, &[]), &[("materials/x.vmt", b"x")]);
        assert!(Pack::open(&write("assets.csrpack", &assets)).unwrap_err().contains("recipe pack holds no assets"));
        let ok = build(&manifest(PackKind::Assets, &[]), &[("materials/x.vmt", b"x")]);
        assert_eq!(Pack::open(&write("ok.csrpack", &ok)).unwrap().assets, vec!["materials/x.vmt"]);

        // Definitions must be declared.
        let undeclared = build(&manifest(PackKind::Recipe, &[]), &[("defs/weapons/gold.toml", GOLD.as_bytes())]);
        assert!(Pack::open(&write("undeclared.csrpack", &undeclared)).unwrap_err().contains("provides"));

        // Too new for this game.
        let mut future = manifest(PackKind::Recipe, &[]);
        future.min_game_version = "999.0.0".into();
        assert!(Pack::open(&write("future.csrpack", &build(&future, &[]))).unwrap_err().contains("or newer"));

        // Edited after it was built.
        let mut m = manifest(PackKind::Assets, &[]);
        m.content_hash = content_hash([("materials/x.vmt", &b"x"[..])]);
        let mut out = io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file(MANIFEST, opts).unwrap();
        zip.write_all(m.to_toml().as_bytes()).unwrap();
        zip.start_file("materials/x.vmt", opts).unwrap();
        zip.write_all(b"y").unwrap();
        zip.finish().unwrap();
        assert!(Pack::open(&write("edited.csrpack", &out.into_inner())).unwrap_err().contains("hash"));

        assert!(Pack::open(&write("junk.csrpack", b"not a zip")).unwrap_err().contains("not a content pack"));
    }

    #[test]
    fn installs_mounts_lists_and_removes() {
        let data = temp("install");
        let src = data.join("Downloads/Golden guns.csrpack");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        let mut m = manifest(PackKind::Assets, &["mod_weapon_ak47_gold"]);
        m.id = "golden_guns".into();
        fs::write(&src, build(&m, &[("defs/weapons/gold.toml", GOLD.as_bytes()), ("materials/models/gold.vmt", b"\"VertexLitGeneric\" {}")])).unwrap();

        let installed = Installed::new(&data);
        let pack = installed.install(&src).unwrap();
        assert_eq!(pack.path, data.join(r"packs\golden_guns.csrpack"));
        // Installing again (or a newer version) replaces it.
        installed.install(&src).unwrap();
        let (packs, errors) = installed.load();
        assert_eq!((packs.len(), errors.len()), (1, 0));

        let mut fs_ = vfs::Vfs::new();
        installed.mount(&mut fs_);
        assert_eq!(fs_.labels(), vec!["pack:golden_guns"]);
        assert!(fs_.read_string("materials/models/gold.vmt").unwrap().contains("VertexLitGeneric"));

        let mut reg = Registry::default();
        reg.add_packs(&packs);
        assert_eq!(reg.get(&"mod_weapon_ak47_gold".parse().unwrap()), Some(&Source::Pack("golden_guns".into())));

        assert!(installed.remove("golden_guns").unwrap());
        assert!(!installed.remove("golden_guns").unwrap());
        assert!(installed.load().0.is_empty());
    }

    #[test]
    fn weapon_variants_join_the_catalog() {
        let items = r#""items_game" {
            "prefabs" { "weapon_ak47_prefab" { "item_class" "weapon_ak47" "item_name" "#SFUI_WPNHUD_AK47" "item_type_name" "#CSGO_Type_Rifle"
                "attributes" { "damage" "36" "in game price" "2700" "cycletime" "0.1" "is full auto" "1" } } }
            "items" { "7" { "name" "weapon_ak47" "prefab" "weapon_ak47_prefab" "baseitem" "1" } } }"#;
        let mut catalog = weapons::Catalog::from_items_game(items).unwrap();
        let dir = temp("variants");
        let file = dir.join("p.csrpack");
        fs::write(&file, build(&manifest(PackKind::Recipe, &["mod_weapon_ak47_gold"]), &[("defs/weapons/gold.toml", GOLD.as_bytes())])).unwrap();
        let pack = Pack::open(&file).unwrap();
        assert!(apply_weapons(std::slice::from_ref(&pack), &mut catalog).is_empty());
        let gold = catalog.by_id(&"mod_weapon_ak47_gold".parse().unwrap()).unwrap();
        let ak = catalog.get("weapon_ak47").unwrap();
        assert_eq!(ak.def_index, 7, "the base weapon is still found by class name");
        assert_eq!((gold.damage, gold.price, gold.name_token.as_str()), (40.0, 3500, "Golden AK-47"));
        assert_eq!((gold.cycle_time, gold.full_auto, gold.class_name.as_str()), (ak.cycle_time, true, "weapon_ak47"));
        assert!(gold.def_index >= 1_000_000);
        // Applying twice does not duplicate it.
        assert_eq!(apply_weapons(&[pack], &mut catalog).len(), 1);
    }
}
