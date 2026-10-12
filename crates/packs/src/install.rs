use super::*;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct Installed {
    dir: PathBuf,
}

fn no_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            let link = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let link = metadata.file_type().is_symlink();
            if link {
                Err(format!("pack storage cannot be a link: {}", path.display()))
            } else {
                Ok(())
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

struct Pending(PathBuf);
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
fn publish(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are NUL-terminated buffers alive through the call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn publish(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

impl Installed {
    /// `data_dir` is the launcher-selected runtime userdata folder.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join("packs"),
        }
    }
    pub fn current() -> Self {
        let data = std::env::var_os("CSR_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        Self::new(&data)
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Explicit map selection by stable ContentId. Only the selected pack is
    /// mounted for that level; unrelated private packs cannot alter a match.
    pub fn resolve_map(&self, id: &ContentId) -> Result<(Pack, MapDef), String> {
        let (packs, errors) = self.load();
        for pack in packs {
            if let Some(map) = pack
                .manifest
                .maps
                .iter()
                .find(|m| m.id == id.to_string())
                .cloned()
            {
                return Ok((pack, map));
            }
        }
        Err(format!(
            "map ContentId {id} is not installed{}",
            if errors.is_empty() {
                String::new()
            } else {
                format!(": {}", errors.join("; "))
            }
        ))
    }

    /// Installs the exact already validated bytes. Replacing an existing pack
    /// never removes its old file first; failed publication preserves it.
    pub fn install(&self, source: &Path) -> Result<Pack, String> {
        let pack = Pack::open(source)?;
        self.install_validated(&pack)?;
        Ok(pack)
    }

    pub fn install_validated(&self, pack: &Pack) -> Result<(), String> {
        self.install_inner(pack, &Control::default(), false)
    }

    pub(crate) fn install_controlled(&self, pack: &Pack, control: &Control) -> Result<(), String> {
        self.install_inner(pack, control, true)
    }

    fn install_inner(&self, pack: &Pack, control: &Control, wait: bool) -> Result<(), String> {
        control.update(Stage::Waiting, 0, 0)?;
        no_link(&self.dir)?;
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let _lock = self.lock_controlled(control, wait)?;
        let target = self.dir.join(format!("{}.csrpack", pack.manifest.id));
        no_link(&target)?;
        control.update(Stage::CheckingInstalled, 0, 0)?;
        let (existing, errors) = self.load_controlled(control);
        control.check()?;
        for error in errors {
            eprintln!("warning: invalid installed pack skipped: {error}");
        }
        let mut registry = Registry::default();
        for other in existing
            .iter()
            .filter(|p| p.manifest.id != pack.manifest.id)
        {
            registry.add_pack(other)?;
        }
        registry.add_pack(pack)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = self.dir.join(format!(
            ".install-{}-{}.part",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let pending = Pending(temp);
        let mut written = 0;
        let writing = (|| {
            for chunk in pack.archive.chunks(job::CHUNK) {
                control.update(Stage::Writing, written, pack.archive_size())?;
                file.write_all(chunk).map_err(|e| e.to_string())?;
                written += chunk.len() as u64;
            }
            file.sync_all().map_err(|e| e.to_string())
        })();
        // Close the file before the pending guard removes it on cancellation,
        // including platforms where an open handle prevents unlinking.
        drop(file);
        writing?;
        // The final cancellation boundary precedes the atomic commit. There is
        // deliberately no cancellation check after publication succeeds.
        control.update(Stage::Publishing, written, pack.archive_size())?;
        publish(&pending.0, &target)
            .map_err(|e| format!("installing {}: {e}", pack.manifest.name))?;
        Ok(())
    }

    /// Returns independent valid snapshots. Corrupt files are reported and
    /// never mounted; an unreadable folder is reported rather than silently empty.
    pub fn load(&self) -> (Vec<Pack>, Vec<String>) {
        self.load_controlled(&Control::default())
    }

    fn load_controlled(&self, control: &Control) -> (Vec<Pack>, Vec<String>) {
        if let Err(e) = no_link(&self.dir) {
            return (Vec::new(), vec![e]);
        }
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), Vec::new()),
            Err(e) => return (Vec::new(), vec![e.to_string()]),
        };
        let mut paths = Vec::new();
        let mut errors = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry)
                    if entry
                        .path()
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION)) =>
                {
                    paths.push(entry.path())
                }
                Ok(_) => {}
                Err(e) => errors.push(e.to_string()),
            }
        }
        paths.sort();
        let mut packs = Vec::new();
        let mut registry = Registry::default();
        let mut ids = BTreeSet::new();
        for path in paths {
            if let Err(e) = control.check() {
                errors.push(e);
                break;
            }
            let result: Result<Pack, String> = (|| {
                no_link(&path)?;
                let pack = Pack::open_controlled(&path, Limits::default(), control)?;
                if path.file_stem().and_then(|s| s.to_str()) != Some(&pack.manifest.id) {
                    return Err("installed filename must match pack id".into());
                }
                if !ids.insert(pack.manifest.id.clone()) {
                    return Err("duplicate installed pack id".into());
                }
                registry.add_pack(&pack)?;
                Ok(pack)
            })();
            match result {
                Ok(pack) => packs.push(pack),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        (packs, errors)
    }

    /// Rejects unsafe ids even when removing a corrupt pack.
    pub fn remove(&self, id: &str) -> Result<bool, String> {
        if !valid_pack_id(id) {
            return Err("invalid pack id".into());
        }
        no_link(&self.dir)?;
        if !self.dir.exists() {
            return Ok(false);
        }
        let _lock = self.lock()?;
        let path = self.dir.join(format!("{id}.csrpack"));
        no_link(&path)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }

    fn lock(&self) -> Result<std::fs::File, String> {
        self.lock_controlled(&Control::default(), false)
    }

    fn lock_controlled(&self, control: &Control, wait: bool) -> Result<std::fs::File, String> {
        let path = self.dir.join(".install.lock");
        no_link(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| e.to_string())?;
        loop {
            control.check()?;
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if wait => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(e) => return Err(format!("another pack installation is in progress: {e}")),
            }
        }
        Ok(file)
    }
}
