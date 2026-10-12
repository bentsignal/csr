//! One owned worker for installing private content and refreshing menu metadata.
//! UI callers keep immutable catalog entries; this flow never mounts game data.
use std::{collections::VecDeque, path::PathBuf};

enum Worker {
    Install(packs::InstallJob),
    Catalog(packs::CatalogJob),
}
impl Worker {
    fn cancel(&self) {
        match self {
            Self::Install(job) => job.cancel(),
            Self::Catalog(job) => job.cancel(),
        }
    }
}

pub(crate) struct Flow {
    store: packs::Installed,
    worker: Option<Worker>,
    queued: VecDeque<PathBuf>,
    refresh: bool,
    cancelling: bool,
    pub catalog: packs::Catalog,
    pub revision: u64,
    pub notice: String,
}
impl Flow {
    pub fn new(store: packs::Installed) -> Self {
        Self {
            store,
            worker: None,
            queued: VecDeque::new(),
            refresh: true,
            cancelling: false,
            catalog: Default::default(),
            revision: 0,
            notice: String::new(),
        }
    }
    pub fn busy(&self) -> bool {
        self.worker.is_some() || !self.queued.is_empty() || self.refresh
    }
    pub fn store(&self) -> packs::Installed {
        self.store.clone()
    }
    pub fn request_refresh(&mut self) {
        self.refresh = true;
    }
    /// File drops and native picker results remain literal paths. Never turn
    /// a name in a manifest, URL or ContentId into an installation source.
    pub fn install(&mut self, path: PathBuf) -> Result<(), String> {
        if self.cancelling {
            return Err("Wait for cancellation to finish before adding another pack.".into());
        }
        if !path.is_absolute()
            || !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csrpack"))
        {
            return Err("Choose a .csrpack content pack.".into());
        }
        if self.queued.contains(&path) {
            return Ok(());
        }
        if self.queued.len() >= 8 {
            return Err("The installation queue is full. Wait for these packs to finish.".into());
        }
        self.queued.push_back(path);
        Ok(())
    }
    /// Keep polling until the worker reports its terminal outcome. In
    /// particular, a publication that already committed reports Installed.
    pub fn cancel(&mut self) {
        self.queued.clear();
        self.refresh = false;
        self.cancelling = self.worker.is_some();
        if let Some(worker) = &self.worker {
            worker.cancel();
            self.notice = "Cancelling…".into();
        }
    }
    pub fn poll(&mut self) {
        let finished = match self.worker.as_mut() {
            Some(Worker::Install(job)) => match job.poll() {
                None => false,
                Some(packs::Outcome::Installed(receipt)) => {
                    self.notice = format!("{} was installed.", caption(&receipt.manifest.name));
                    self.refresh = true;
                    true
                }
                Some(packs::Outcome::Cancelled) => {
                    self.notice = "Installation cancelled.".into();
                    true
                }
                Some(packs::Outcome::Failed(error)) => {
                    self.notice = format!("Could not install the pack: {}", caption(&error));
                    true
                }
            },
            Some(Worker::Catalog(job)) => match job.poll() {
                None => false,
                Some(packs::CatalogOutcome::Loaded(catalog)) => {
                    if self.cancelling {
                        self.notice = "Content packs refreshed.".into();
                    }
                    if self.catalog != catalog {
                        self.catalog = catalog;
                        self.revision = self.revision.wrapping_add(1);
                    }
                    true
                }
                Some(packs::CatalogOutcome::Cancelled) => {
                    self.notice = "Refresh cancelled.".into();
                    true
                }
                Some(packs::CatalogOutcome::Failed(error)) => {
                    self.notice = format!("Could not refresh content packs: {}", caption(&error));
                    true
                }
            },
            None => false,
        };
        if finished {
            self.worker = None;
            self.cancelling = false;
        }
        if self.worker.is_none() {
            if let Some(path) = self.queued.pop_front() {
                self.worker = Some(Worker::Install(packs::InstallJob::start(
                    self.store.clone(),
                    path,
                )));
            } else if self.refresh {
                self.refresh = false;
                self.worker = Some(Worker::Catalog(packs::CatalogJob::start(
                    self.store.clone(),
                )));
            }
        }
    }
    pub fn status(&self) -> String {
        if self.cancelling {
            return "Cancelling…".into();
        }
        match &self.worker {
            None => self.notice.clone(),
            Some(Worker::Catalog(_)) => "Checking installed content packs…".into(),
            Some(Worker::Install(job)) => {
                let progress = job.progress();
                let action = match progress.stage {
                    packs::Stage::Reading => "Reading content pack",
                    packs::Stage::Validating => "Checking content pack",
                    packs::Stage::Waiting => "Waiting for another installation",
                    packs::Stage::CheckingInstalled => "Checking installed packs",
                    packs::Stage::Writing => "Installing content pack",
                    packs::Stage::Publishing => "Finishing installation",
                };
                if progress.total > 0 {
                    format!(
                        "{action}… {}%",
                        progress
                            .bytes
                            .saturating_mul(100)
                            .checked_div(progress.total)
                            .unwrap_or(0)
                            .min(100)
                    )
                } else {
                    format!("{action}…")
                }
            }
        }
    }
}
pub(crate) fn caption(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_control())
        .take(512)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "csr-pack-flow-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn flow(&self) -> Flow {
            Flow::new(packs::Installed::new(&self.0.join("userdata")))
        }
        fn recipe(&self) -> PathBuf {
            let manifest = packs::Manifest {
                format_version: 1,
                id: "friend_recipe".into(),
                name: "Friend's arena".into(),
                version: "1.0.0".into(),
                origin: "mod".into(),
                kind: packs::PackKind::Recipe,
                provides: vec!["mod_map_arena".into()],
                content_hash: String::new(),
                min_game_version: "0.0.0".into(),
                description: String::new(),
                resources: vec![],
                maps: vec![packs::MapDef {
                    id: "mod_map_arena".into(),
                    bsp: "de_dust2".into(),
                    name: "Arena".into(),
                    source: packs::MapSource::Legacy,
                }],
            };
            let source = self.0.join("友達's recipe.CSRPACK");
            std::fs::write(&source, packs::build(&manifest, &[]).unwrap()).unwrap();
            source
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn finish(flow: &mut Flow) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while flow.busy() {
            flow.poll();
            assert!(
                Instant::now() < deadline,
                "worker did not finish: {}",
                flow.status()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn actual_unicode_install_refreshes_metadata_and_failure_retains_selection() {
        let fixture = Fixture::new();
        let mut flow = fixture.flow();
        flow.install(fixture.recipe()).unwrap();
        finish(&mut flow);
        assert_eq!(flow.catalog.entries.len(), 1);
        assert_eq!(flow.catalog.entries[0].manifest().name, "Friend's arena");
        assert_eq!(flow.notice, "Friend's arena was installed.");
        let old = flow.catalog.entries[0].clone();
        flow.install(fixture.0.join("missing.csrpack")).unwrap();
        finish(&mut flow);
        assert!(flow.notice.starts_with("Could not install"));
        assert_eq!(flow.catalog.entries[0], old);
    }
    #[test]
    fn cancel_waits_for_the_active_worker_and_does_not_start_queued_installs() {
        let fixture = Fixture::new();
        let mut flow = fixture.flow();
        flow.poll(); // Start the only catalog worker.
        flow.install(fixture.recipe()).unwrap();
        flow.cancel();
        assert!(flow.busy());
        assert!(flow.install(fixture.0.join("next.csrpack")).is_err());
        finish(&mut flow);
        assert!(flow.catalog.entries.is_empty());
        assert_ne!(flow.status(), "Cancelling…");
        assert!(!flow.store.dir().join("friend_recipe.csrpack").exists());
        flow.request_refresh();
        finish(&mut flow);
        assert!(flow.catalog.entries.is_empty());
    }
    #[test]
    fn burst_drops_are_bounded_and_never_interpret_relative_sources() {
        let fixture = Fixture::new();
        let mut flow = fixture.flow();
        assert!(flow.install("mod_map_arena".into()).is_err());
        assert!(flow.install("relative.csrpack".into()).is_err());
        for index in 0..8 {
            flow.install(fixture.0.join(format!("{index}.csrpack")))
                .unwrap();
        }
        flow.install(fixture.0.join("0.csrpack")).unwrap();
        assert_eq!(flow.queued.len(), 8);
        assert!(flow.install(fixture.0.join("overflow.csrpack")).is_err());
        flow.cancel();
        assert!(!flow.busy());
    }
}
