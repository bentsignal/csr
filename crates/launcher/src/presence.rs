//! Kernel locks, rather than PID/stale-file guesses, track games using this
//! userdata root. A short exclusive gate serializes check-and-launch races.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;

pub struct Game(File);

pub(crate) fn file(root: &Path, name: &str) -> Result<File, String> {
    let dir = root.join("config");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    for path in [&dir, &path] {
        if let Ok(meta) = fs::symlink_metadata(path) {
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                meta.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let linked = meta.file_type().is_symlink();
            if linked {
                return Err(format!("game lock cannot use a link: {}", path.display()));
            }
        }
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| e.to_string())
}

impl Game {
    pub fn running(root: &Path) -> Result<bool, String> {
        let presence = file(root, "game.instance.lock")?;
        match presence.try_lock() {
            Ok(()) => Ok(false),
            Err(TryLockError::WouldBlock) => Ok(true),
            Err(TryLockError::Error(e)) => Err(e.to_string()),
        }
    }

    /// Ordinary explicit launches may coexist, but each holds a shared lock
    /// until `client::run` returns. The gate also coordinates file-open launches.
    pub fn launch(root: &Path) -> Result<Self, String> {
        let gate = file(root, "game.launch.lock")?;
        gate.lock().map_err(|e| e.to_string())?;
        let presence = file(root, "game.instance.lock")?;
        presence.lock_shared().map_err(|e| e.to_string())?;
        Ok(Self(presence))
    }

    /// Reserve a new game only when no game already uses this userdata root.
    /// Drop the exclusive probe before taking a shared lock, still under the
    /// gate: lock conversion semantics differ between operating systems.
    pub fn launch_if_absent(root: &Path) -> Result<Option<Self>, String> {
        let gate = file(root, "game.launch.lock")?;
        gate.lock().map_err(|e| e.to_string())?;
        let presence = file(root, "game.instance.lock")?;
        match presence.try_lock() {
            Ok(()) => {
                presence.unlock().map_err(|e| e.to_string())?;
                presence.lock_shared().map_err(|e| e.to_string())?;
                Ok(Some(Self(presence)))
            }
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e.to_string()),
        }
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    fn root(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("csr-presence-{}-{name}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn simultaneous_file_opens_reserve_one_game_and_all_games_must_exit() {
        let root = root("race");
        let barrier = Arc::new(Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let root = root.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    Game::launch_if_absent(&root).unwrap()
                })
            })
            .collect();
        let guards: Vec<_> = workers
            .into_iter()
            .filter_map(|t| t.join().unwrap())
            .collect();
        assert_eq!(guards.len(), 1);
        assert!(Game::running(&root).unwrap());
        let explicit = Game::launch(&root).unwrap();
        drop(guards);
        assert!(Game::launch_if_absent(&root).unwrap().is_none());
        drop(explicit);
        assert!(!Game::running(&root).unwrap());
        assert!(Game::launch_if_absent(&root).unwrap().is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "subprocess helper, invoked by process_exit_releases_presence"]
    fn child() {
        let Some(root) = std::env::var_os("CSR_PRESENCE_TEST_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let _game = Game::launch(&root).unwrap();
        fs::write(root.join("ready"), b"ready").unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn process_exit_releases_presence_without_deleting_lock_files() {
        let root = root("process");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "presence::tests::child", "--ignored"])
            .env("CSR_PRESENCE_TEST_ROOT", &root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !root.join("ready").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ready = root.join("ready").exists();
        let occupied = Game::launch_if_absent(&root).unwrap().is_none();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(ready && occupied);
        assert!(root.join("config/game.instance.lock").exists());
        assert!(Game::launch_if_absent(&root).unwrap().is_some());
        fs::remove_dir_all(root).unwrap();
    }
}
