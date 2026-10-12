//! One bounded installation worker shared by file opening and window drops.
//! Cancellation is checked during reads, decompression, hashing and writes;
//! publication is the commit point. A completed publish always reports success.
use crate::{Catalog, Installed, Limits, Manifest, Pack};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

const CANCELLED: &str = "pack installation cancelled";
pub(crate) const CHUNK: usize = 128 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Reading,
    Validating,
    Waiting,
    CheckingInstalled,
    Writing,
    Publishing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub bytes: u64,
    pub total: u64,
}

#[derive(Clone)]
pub(crate) struct Control {
    cancelled: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    #[cfg(test)]
    pub hook: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(Progress {
                stage: Stage::Reading,
                bytes: 0,
                total: 0,
            })),
            #[cfg(test)]
            hook: None,
        }
    }
}
impl Control {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(CANCELLED.into())
        } else {
            Ok(())
        }
    }
    pub fn update(&self, stage: Stage, bytes: u64, total: u64) -> Result<(), String> {
        let progress = Progress {
            stage,
            bytes,
            total,
        };
        *self.progress.lock().unwrap_or_else(|e| e.into_inner()) = progress;
        #[cfg(test)]
        if let Some(hook) = &self.hook {
            hook(progress);
        }
        self.check()
    }
}

#[derive(Debug)]
pub struct Receipt {
    pub manifest: Manifest,
    pub archive_hash: String,
}

#[derive(Debug)]
pub enum Outcome {
    Installed(Box<Receipt>),
    Cancelled,
    Failed(String),
}

struct Job<T> {
    control: Control,
    result: mpsc::Receiver<T>,
    finished: bool,
    failure: fn(String) -> T,
}
impl<T: Send + 'static> Job<T> {
    fn start(
        name: &str,
        task: impl FnOnce(Control) -> T + Send + 'static,
        failure: fn(String) -> T,
    ) -> Self {
        let control = Control::default();
        let worker = control.clone();
        let (sender, result) = mpsc::channel();
        let failed = sender.clone();
        let spawned = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let _ = sender.send(task(worker));
            });
        if let Err(error) = spawned {
            let _ = failed.send(failure(format!("could not start {name} worker: {error}")));
        }
        Self {
            control,
            result,
            finished: false,
            failure,
        }
    }
    fn progress(&self) -> Progress {
        *self
            .control
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
    fn cancel(&self) {
        self.control.cancel();
    }
    fn poll(&mut self) -> Option<T> {
        if self.finished {
            return None;
        }
        match self.result.try_recv() {
            Ok(result) => {
                self.finished = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.finished = true;
                Some((self.failure)("pack worker stopped unexpectedly".into()))
            }
        }
    }
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

pub struct InstallJob(Job<Outcome>);
impl InstallJob {
    /// Callers queue rather than spawn unbounded concurrent jobs. The worker
    /// holds no Bevy/UI objects and stores only a small receipt after finishing.
    pub fn start(store: Installed, source: PathBuf) -> Self {
        Self(Job::start(
            "pack-install",
            move |worker| {
                let outcome = (|| {
                    let pack = Pack::open_controlled(&source, Limits::default(), &worker)?;
                    store.install_controlled(&pack, &worker)?;
                    Ok(Receipt {
                        manifest: pack.manifest().clone(),
                        archive_hash: pack.archive_hash().into(),
                    })
                })();
                match outcome {
                    Ok(receipt) => Outcome::Installed(Box::new(receipt)),
                    Err(e) if e == CANCELLED => Outcome::Cancelled,
                    Err(e) => Outcome::Failed(e),
                }
            },
            Outcome::Failed,
        ))
    }
    pub fn progress(&self) -> Progress {
        self.0.progress()
    }
    pub fn cancel(&self) {
        self.0.cancel();
    }
    /// Returns the terminal result once. A cancelled worker must finish cleanup
    /// before a caller closes its process or starts the next queued installation.
    pub fn poll(&mut self) -> Option<Outcome> {
        self.0.poll()
    }
}

#[derive(Debug)]
pub enum CatalogOutcome {
    Loaded(Catalog),
    Cancelled,
    Failed(String),
}

/// Queue a scan on the frontend's one-worker refresh flow. Completed scans
/// retain definitions/hashes only and never change a running level's mounts.
pub struct CatalogJob(Job<CatalogOutcome>);
impl CatalogJob {
    pub fn start(store: Installed) -> Self {
        Self(Job::start(
            "pack-catalog",
            move |worker| match store.catalog_controlled(&worker) {
                Ok(catalog) => CatalogOutcome::Loaded(catalog),
                Err(e) if e == CANCELLED => CatalogOutcome::Cancelled,
                Err(e) => CatalogOutcome::Failed(e),
            },
            CatalogOutcome::Failed,
        ))
    }
    pub fn progress(&self) -> Progress {
        self.0.progress()
    }
    pub fn cancel(&self) {
        self.0.cancel();
    }
    pub fn poll(&mut self) -> Option<CatalogOutcome> {
        self.0.poll()
    }
}
