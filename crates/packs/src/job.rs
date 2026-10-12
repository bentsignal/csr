//! One bounded installation worker shared by file opening and window drops.
//! Cancellation is checked during reads, decompression, hashing and writes;
//! publication is the commit point. A completed publish always reports success.
use crate::{Installed, Limits, Manifest, Pack};
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

pub struct InstallJob {
    control: Control,
    result: mpsc::Receiver<Outcome>,
    finished: bool,
}
impl InstallJob {
    /// Callers queue rather than spawn unbounded concurrent jobs. The worker
    /// holds no Bevy/UI objects and stores only a small receipt after finishing.
    pub fn start(store: Installed, source: PathBuf) -> Self {
        let control = Control::default();
        let worker = control.clone();
        let (sender, result) = mpsc::channel();
        std::thread::spawn(move || {
            let outcome = (|| {
                let pack = Pack::open_controlled(&source, Limits::default(), &worker)?;
                store.install_controlled(&pack, &worker)?;
                Ok(Receipt {
                    manifest: pack.manifest().clone(),
                    archive_hash: pack.archive_hash().into(),
                })
            })();
            let outcome = match outcome {
                Ok(receipt) => Outcome::Installed(Box::new(receipt)),
                Err(e) if e == CANCELLED => Outcome::Cancelled,
                Err(e) => Outcome::Failed(e),
            };
            let _ = sender.send(outcome);
        });
        Self {
            control,
            result,
            finished: false,
        }
    }
    pub fn progress(&self) -> Progress {
        *self
            .control
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
    pub fn cancel(&self) {
        self.control.cancel();
    }
    /// Returns the terminal result once. A cancelled worker must finish cleanup
    /// before a caller closes its process or starts the next queued installation.
    pub fn poll(&mut self) -> Option<Outcome> {
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
                Some(Outcome::Failed(
                    "pack installation worker stopped unexpectedly".into(),
                ))
            }
        }
    }
}
impl Drop for InstallJob {
    fn drop(&mut self) {
        self.cancel();
    }
}
