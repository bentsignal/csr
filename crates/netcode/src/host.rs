//! Owned in-process authority for a future menu host. Preparing polls UDP and
//! discovery without sim/bot steps. A passive local reservation may survive
//! arbitrary map/render loading; only `provide_map` creates its client.
//! Running never waits for menus, rendering or player focus again.
//!
//! This is the existing unauthenticated direct-IP transport. The private
//! reservation establishes process ownership, not Steam/friends privacy.
//! Existing preconfigured bots and transport slot limits are preserved;
//! dynamic bot filling and original competitive warmup remain separate work.
use crate::{
    protocol::{ContentManifest, ServerInfo},
    remote_session::{RemoteSession, Status},
    session::Session,
    transport::Server,
};
use sim::{PlayerId, World};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const COMMAND_CAPACITY: usize = 8;
const OWNER_ADMISSION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
pub struct HostConfig {
    pub bind: SocketAddr,
    /// Transport connections, including bounded discovery probes. These are
    /// not a gameplay roster target or a request for automatic bot filling.
    pub max_clients: usize,
}
impl Default for HostConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 27015)),
            max_clients: 10,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    InvalidAuthority(String),
    Bind(String),
    Worker(String),
    Transport(String),
    OwnerAdmissionTimeout,
    OwnerDisconnected,
    NotReady,
    WrongOwner,
    CommandFull,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostStatus {
    Binding,
    Preparing {
        local_address: SocketAddr,
        info: ServerInfo,
    },
    Running {
        local_address: SocketAddr,
        owner: PlayerId,
    },
    Stopping,
    /// Published only after Server and its socket have been dropped.
    Closed(HostError),
}

struct Shared {
    stopped: AtomicBool,
    status: Mutex<HostStatus>,
}
impl Shared {
    fn publish(&self, status: HostStatus) {
        let mut current = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if self.stopped.load(Ordering::Acquire)
            && !matches!(status, HostStatus::Stopping | HostStatus::Closed(_))
        {
            return;
        }
        *current = status;
    }
}

pub(crate) struct LocalOwner {
    pub(crate) client_id: u64,
    shared: Weak<Shared>,
}
impl LocalOwner {
    pub(crate) fn cancelled(&self) -> bool {
        self.shared
            .upgrade()
            .is_none_or(|shared| shared.stopped.load(Ordering::Acquire))
    }
}

/// One-use in-process capability. It is intentionally not Clone/serializable
/// and contains no paths. Keeping this ticket creates no connection, actor,
/// Hello or synchronization deadline. Consume it with RemoteSession::local,
/// then provide the verified map only after current render preparation.
pub struct LocalAdmission {
    pub(crate) address: SocketAddr,
    pub(crate) info: ServerInfo,
    pub(crate) owner: LocalOwner,
}
impl LocalAdmission {
    pub fn server_info(&self) -> &ServerInfo {
        &self.info
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

enum Command {
    Begin(PlayerId),
}

/// Keep ownership outside match entities until shutdown completes. `cancel`
/// is out-of-band and cannot get stuck behind a full command queue. Drop
/// signals stop and unparks the worker; it never blocks a render frame.
pub struct HostHandle {
    shared: Arc<Shared>,
    owner_id: u64,
    admission_taken: bool,
    begin_requested: bool,
    commands: SyncSender<Command>,
    worker: Option<JoinHandle<()>>,
}
impl HostHandle {
    /// The caller prepares map data once and may preconfigure server-owned
    /// bots/rules/nav. No existing human/offline player or advanced timeline
    /// may be imported. Map loading and GPU preparation are not done here.
    pub fn spawn(
        world: World,
        manifest: ContentManifest,
        config: HostConfig,
    ) -> Result<Self, HostError> {
        manifest
            .validate()
            .map_err(|e| HostError::InvalidAuthority(bounded(e)))?;
        if world.tick != 0
            || world.players.iter().any(|p| !p.is_bot)
            || world.collision.dynamic.is_some()
        {
            return Err(HostError::InvalidAuthority(
                "host requires a fresh replay-safe world with server-owned bots only".into(),
            ));
        }
        if config.max_clients == 0
            || config.max_clients > 64
            || world.players.len() + config.max_clients > 64
        {
            return Err(HostError::InvalidAuthority(
                "at most 64 bots plus transport connection slots".into(),
            ));
        }
        let session =
            Session::new(world, manifest).map_err(|e| HostError::InvalidAuthority(bounded(e)))?;
        let shared = Arc::new(Shared {
            stopped: AtomicBool::new(false),
            status: Mutex::new(HostStatus::Binding),
        });
        let (commands, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let owner_id = u64::from_le_bytes(renet_netcode::generate_random_bytes());
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("listen-authority".into())
            .spawn(move || {
                let result = run(session, config, owner_id, &worker_shared, receiver);
                // run's Server is already gone: socket release precedes Closed.
                worker_shared.stopped.store(true, Ordering::Release);
                worker_shared.publish(HostStatus::Closed(
                    result.err().unwrap_or(HostError::Cancelled),
                ));
            })
            .map_err(|e| HostError::Worker(bounded(e.to_string())))?;
        Ok(Self {
            shared,
            owner_id,
            admission_taken: false,
            begin_requested: false,
            commands,
            worker: Some(worker),
        })
    }

    pub fn status(&self) -> HostStatus {
        self.shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn take_local_admission(&mut self) -> Option<LocalAdmission> {
        if self.admission_taken || self.shared.stopped.load(Ordering::Acquire) {
            return None;
        }
        let HostStatus::Preparing {
            local_address,
            info,
        } = self.status()
        else {
            return None;
        };
        self.admission_taken = true;
        Some(LocalAdmission {
            address: local_address,
            info,
            owner: LocalOwner {
                client_id: self.owner_id,
                shared: Arc::downgrade(&self.shared),
            },
        })
    }

    /// A private process command, never a client-control message. Both exact
    /// host ownership and actual validated predictor readiness are required.
    /// The worker independently verifies this player belongs to the reservation.
    pub fn begin(&mut self, local: &RemoteSession) -> Result<(), HostError> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(HostError::Cancelled);
        }
        let owner = local.local_owner.as_ref().ok_or(HostError::WrongOwner)?;
        if owner.client_id != self.owner_id
            || !Weak::ptr_eq(&owner.shared, &Arc::downgrade(&self.shared))
        {
            return Err(HostError::WrongOwner);
        }
        if local.status() != &Status::Ready {
            return Err(HostError::NotReady);
        }
        let player = local.player().ok_or(HostError::NotReady)?;
        if self.begin_requested {
            return Ok(());
        }
        self.commands
            .try_send(Command::Begin(player))
            .map_err(|e| match e {
                TrySendError::Full(_) => HostError::CommandFull,
                TrySendError::Disconnected(_) => HostError::Cancelled,
            })?;
        self.begin_requested = true;
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        Ok(())
    }

    pub fn cancel(&self) {
        if !self.shared.stopped.swap(true, Ordering::AcqRel) {
            self.shared.publish(HostStatus::Stopping);
        }
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }

    /// Join only an already finished worker. Keep cancelled handles in a
    /// frontend-owned reaper until true before replacing/rebinding a host.
    pub fn reap_if_finished(&mut self) -> bool {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return false;
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                self.shared.stopped.store(true, Ordering::Release);
                self.shared.publish(HostStatus::Closed(HostError::Worker(
                    "authority worker panicked".into(),
                )));
            }
        }
        true
    }
}
impl Drop for HostHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn bounded(message: String) -> String {
    message.chars().take(512).collect()
}

fn local_address(bound: SocketAddr) -> SocketAddr {
    if !bound.ip().is_unspecified() {
        return bound;
    }
    SocketAddr::new(
        match bound.ip() {
            IpAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            IpAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        },
        bound.port(),
    )
}

struct Authority {
    server: Server,
    owner_id: u64,
    running: bool,
    admission_elapsed: Option<Duration>,
}
impl Authority {
    fn begin(&mut self, player: PlayerId) -> Result<(), HostError> {
        if self.server.session().player_for(self.owner_id) != Some(player) {
            return Err(HostError::WrongOwner);
        }
        if !self.running {
            self.server
                .begin(self.owner_id)
                .map_err(|e| HostError::Transport(bounded(e)))?;
            self.running = true;
        }
        Ok(())
    }
    fn update(&mut self, elapsed: Duration) -> Result<(), HostError> {
        if self.running {
            self.server
                .update(elapsed)
                .map_err(|e| HostError::Transport(bounded(e)))?;
        } else {
            self.server
                .prepare(elapsed, self.owner_id)
                .map_err(|e| HostError::Transport(bounded(e)))?;
        }
        let connected = self.server.client_connected(self.owner_id);
        if self.running {
            if !connected || self.server.session().player_for(self.owner_id).is_none() {
                return Err(HostError::OwnerDisconnected);
            }
        } else if connected {
            let waited = self.admission_elapsed.get_or_insert(Duration::ZERO);
            *waited = waited.saturating_add(elapsed);
            if *waited > OWNER_ADMISSION_TIMEOUT {
                return Err(HostError::OwnerAdmissionTimeout);
            }
        } else if self.admission_elapsed.is_some() {
            return Err(HostError::OwnerDisconnected);
        }
        Ok(())
    }
}

fn run(
    session: Session,
    config: HostConfig,
    owner_id: u64,
    shared: &Shared,
    commands: Receiver<Command>,
) -> Result<(), HostError> {
    if shared.stopped.load(Ordering::Acquire) {
        return Ok(());
    }
    let server = Server::bind(config.bind, session, config.max_clients)
        .map_err(|e| HostError::Bind(bounded(e)))?;
    let address = local_address(server.address());
    let info = server.info();
    info.validate()
        .map_err(|e| HostError::InvalidAuthority(bounded(e)))?;
    let mut authority = Authority {
        server,
        owner_id,
        running: false,
        admission_elapsed: None,
    };
    shared.publish(HostStatus::Preparing {
        local_address: address,
        info,
    });
    let mut previous = Instant::now();
    while !shared.stopped.load(Ordering::Acquire) {
        // Cancel has priority over every bounded control command.
        for _ in 0..COMMAND_CAPACITY {
            if shared.stopped.load(Ordering::Acquire) {
                return Ok(());
            }
            match commands.try_recv() {
                Ok(Command::Begin(player)) => {
                    authority.begin(player)?;
                    previous = Instant::now();
                    shared.publish(HostStatus::Running {
                        local_address: address,
                        owner: player,
                    });
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
        let now = Instant::now();
        authority.update(now.duration_since(previous))?;
        previous = now;
        thread::park_timeout(Duration::from_millis(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
