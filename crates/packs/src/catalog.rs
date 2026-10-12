//! Validated metadata snapshots for menus. No store paths or asset buffers are
//! retained. Entries cannot be edited into trusted definitions by UI callers.
use crate::{Manifest, Pack};
use std::ffi::{OsStr, OsString};

#[derive(Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    manifest: Manifest,
    archive_hash: String,
    archive_size: u64,
    // Exact basename stays private (including extension case on Unix). Never
    // expose/reconstruct an arbitrary storage path in a frontend definition.
    file_name: OsString,
}
impl std::fmt::Debug for CatalogEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogEntry")
            .field("manifest", &self.manifest)
            .field("archive_hash", &self.archive_hash)
            .field("archive_size", &self.archive_size)
            .finish_non_exhaustive()
    }
}
impl CatalogEntry {
    pub(crate) fn from_pack(pack: &Pack, file_name: &OsStr) -> Self {
        Self {
            manifest: pack.manifest().clone(),
            archive_hash: pack.archive_hash().into(),
            archive_size: pack.archive_size(),
            file_name: file_name.into(),
        }
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn archive_hash(&self) -> &str {
        &self.archive_hash
    }
    pub fn archive_size(&self) -> u64 {
        self.archive_size
    }
    pub(crate) fn file_name(&self) -> &OsStr {
        &self.file_name
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    /// Sorted by installed pack filename; invalid/conflicting files are absent.
    pub entries: Vec<CatalogEntry>,
    /// Deterministic diagnostics for skipped files or unreadable storage.
    pub errors: Vec<String>,
}
