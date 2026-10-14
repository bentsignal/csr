//! Local content preparation for remote admission. Run resolve and prepare in
//! the loader worker: both perform disk I/O. No current-directory stores,
//! downloads, advertised paths, GPU work or tick-rate policy live here.

use content::{ContentId, GameOrigin};
use netcode::protocol::ContentManifest;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrepareError {
    InvalidAdvertisement(String),
    UnsupportedOrigin,
    GameUnavailable,
    MissingLocalContent,
    AmbiguousLocalContent,
    /// A catalog with skipped invalid/conflicting archives is not admissible.
    CatalogUnavailable,
    SelectionChanged,
    LoadFailed(String),
    MissingCatalogDigest,
    ContentMismatch,
}
impl std::fmt::Display for PrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAdvertisement(e) => write!(f, "Invalid server content: {e}"),
            Self::UnsupportedOrigin => write!(f, "This content has no supported local map reader"),
            Self::GameUnavailable => write!(f, "The selected CS:GO Legacy install is unavailable"),
            Self::MissingLocalContent => write!(f, "The server map is not installed locally"),
            Self::AmbiguousLocalContent => {
                write!(f, "More than one local definition matches the server map")
            }
            Self::CatalogUnavailable => write!(
                f,
                "The local content catalog contains invalid or conflicting packs"
            ),
            Self::SelectionChanged => write!(
                f,
                "The selected local content changed; refresh before joining"
            ),
            Self::LoadFailed(e) => write!(f, "Could not prepare the selected map: {e}"),
            Self::MissingCatalogDigest => {
                write!(f, "The selected map has no verified weapon catalog")
            }
            Self::ContentMismatch => write!(
                f,
                "The installed map or weapon catalog differs from the server"
            ),
        }
    }
}
impl std::error::Error for PrepareError {}

#[derive(Clone)]
enum Selection {
    Local(level::maps::MapEntry),
    Private(packs::CatalogEntry),
}

/// An owned selection suitable for moving to the existing loader worker.
/// Private definitions retain their complete immutable archive binding.
#[derive(Clone)]
pub struct LocalMapRequest {
    advertised: ContentManifest,
    id: ContentId,
    game_dir: PathBuf,
    packs: packs::Installed,
    archives: workshop::Store,
    selection: Selection,
}

pub struct PreparedMap {
    pub level: level::Level,
    /// Map-only world; the caller applies the negotiated rate and admission.
    pub world: sim::World,
    pub manifest: ContentManifest,
}

fn mounted(game_dir: &Path, archives: &workshop::Store) -> Result<vfs::Vfs, PrepareError> {
    let mut fs = vfs::Vfs::new();
    fs.mount_game_dir(game_dir)
        .map_err(|_| PrepareError::GameUnavailable)?;
    archives.mount(&mut fs);
    Ok(fs)
}

fn local_definition(
    id: &ContentId,
    game_dir: &Path,
    fs: &vfs::Vfs,
    archives: &workshop::Store,
) -> Result<level::maps::MapEntry, PrepareError> {
    let mut matches = level::maps::list_with(game_dir, fs, archives)
        .into_iter()
        .filter(|entry| entry.id == *id && packs::valid_bsp_name(&entry.name));
    let entry = matches.next().ok_or(PrepareError::MissingLocalContent)?;
    if matches.next().is_some() {
        return Err(PrepareError::AmbiguousLocalContent);
    }
    if entry.status != level::maps::MapStatus::Ready {
        return Err(PrepareError::MissingLocalContent);
    }
    Ok(entry)
}

impl LocalMapRequest {
    pub fn resolve(
        advertised: &ContentManifest,
        game_dir: &Path,
        packs: &packs::Installed,
        archives: &workshop::Store,
    ) -> Result<Self, PrepareError> {
        let id = advertised
            .validate()
            .map_err(PrepareError::InvalidAdvertisement)?;
        let selection = match &id.origin {
            GameOrigin::Csgo => {
                let fs = mounted(game_dir, archives)?;
                Selection::Local(local_definition(&id, game_dir, &fs, archives)?)
            }
            GameOrigin::Mod | GameOrigin::Other(_) => {
                let catalog = packs.catalog();
                // The scanner intentionally skips conflicting archives. Do not
                // treat its retained first definition as unambiguous admission.
                if !catalog.errors.is_empty() {
                    return Err(PrepareError::CatalogUnavailable);
                }
                let mut matches = catalog.entries.into_iter().filter(|entry| {
                    entry
                        .manifest()
                        .maps
                        .iter()
                        .any(|map| map.id == id.to_string())
                });
                let entry = matches.next().ok_or(PrepareError::MissingLocalContent)?;
                if matches.next().is_some() {
                    return Err(PrepareError::AmbiguousLocalContent);
                }
                Selection::Private(entry)
            }
            _ => return Err(PrepareError::UnsupportedOrigin),
        };
        Ok(Self {
            advertised: advertised.clone(),
            id,
            game_dir: game_dir.to_owned(),
            packs: packs.clone(),
            archives: archives.clone(),
            selection,
        })
    }

    pub fn id(&self) -> &ContentId {
        &self.id
    }

    pub fn prepare(&self) -> Result<PreparedMap, PrepareError> {
        // No advertisement is mutable after resolve. Reopen only explicit local
        // sources, and let Level revalidate the selected pack during loading.
        let fs = mounted(&self.game_dir, &self.archives)?;
        let level = match &self.selection {
            Selection::Local(selected) => {
                let current = local_definition(&self.id, &self.game_dir, &fs, &self.archives)?;
                if &current != selected {
                    return Err(PrepareError::SelectionChanged);
                }
                level::Level::load_local_id_from_vfs(
                    &self.id,
                    self.game_dir.clone(),
                    fs,
                    &self.archives,
                )
                .map_err(|e| PrepareError::LoadFailed(e.to_string()))
            }
            Selection::Private(entry) => {
                level::Level::load_catalog_map_from_vfs(
                    &self.packs,
                    entry,
                    &self.id,
                    self.game_dir.clone(),
                    fs,
                )
                .map_err(|e| {
                    // Successful preparation opens the archive only once. On
                    // failure, distinguish stale selection from map parse errors
                    // using the same trusted entry, never by parsing error text.
                    if self.packs.resolve_catalog_map(entry, &self.id).is_err() {
                        PrepareError::SelectionChanged
                    } else {
                        PrepareError::LoadFailed(e.to_string())
                    }
                })
            }
        }?;
        let manifest = manifest_for_level(&level)?;
        if manifest != self.advertised {
            return Err(PrepareError::ContentMismatch);
        }
        let world = map_world_for_level(&level);
        Ok(PreparedMap {
            level,
            world,
            manifest,
        })
    }
}

/// Construct one replay-safe map world from the retained parsed level.
pub(crate) fn map_world_for_level(level: &level::Level) -> sim::World {
    let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    world.set_character_profiles(level.character_profiles.clone());
    world.load_map(&level.bsp, level.overview().as_deref());
    world
}

/// Identity of the exact parsed bytes, never a post-mount VFS reread.
pub fn manifest_for_level(level: &level::Level) -> Result<ContentManifest, PrepareError> {
    fn hex(bytes: &[u8; 20]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
    let catalog = level
        .catalog_sha1()
        .ok_or(PrepareError::MissingCatalogDigest)?;
    let manifest = ContentManifest {
        map_id: level.id.to_string(),
        map_name: level.name.clone(),
        map_hash: hex(level.bsp_sha1()),
        catalog_hash: hex(catalog),
    };
    manifest
        .validate()
        .map_err(PrepareError::InvalidAdvertisement)?;
    Ok(manifest)
}
