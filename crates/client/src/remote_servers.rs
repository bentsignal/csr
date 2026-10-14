//! Owned address/favorites discovery. No rendering, admission, Steam or content loading.
//! The caller retains this backend through cancellation until `busy()` is false.
use netcode::{
    protocol::ServerInfo,
    remote_session::{RemoteSession, Status},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_FAVORITES: usize = 32;
pub const MAX_MANUAL: usize = 32;
const MAX_FILE: u64 = 16 * 1024;
const MAX_ADDRESS: usize = 256;
const MAX_DNS: usize = 8;
const MAX_PROBES: usize = 8;
const PROBE_TIME: Duration = Duration::from_secs(5);
const BATCH_TIME: Duration = Duration::from_secs(30);
const FRESH_TIME: Duration = Duration::from_secs(30);
const HEADER: &str = "CSGO-Rewrite favorites 1\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ServerKey(pub u64);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Internet,
    Favorites,
    Lan,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Refresh(Tab),
    ProbeAddress(String),
    AddFavorite(String),
    RemoveFavorite { key: ServerKey, generation: u64 },
    Connect { key: ServerKey, generation: u64 },
    CancelAddress,
    Close,
}
#[derive(Clone, Debug)]
pub struct ServerRow {
    pub key: ServerKey,
    pub endpoint: String,
    pub name: Option<String>,
    pub map: Option<String>,
    pub players: Option<(u16, u16)>,
    pub ping_ms: Option<u32>,
    pub favorite: bool,
    pub connectable: bool,
    /// Resolution plus Inspect exchange, including handshake/retries; not RTT ping.
    pub discovery_elapsed: Option<Duration>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub generation: u64,
    pub rows: Vec<ServerRow>,
    pub busy: bool,
    pub error: Option<String>,
    pub status: String,
}
#[derive(Clone, Debug)]
pub struct ConnectRequest {
    pub key: ServerKey,
    pub generation: u64,
    pub address: SocketAddr,
    pub info: ServerInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Endpoint(String);
/// Shared literal/hostname normalization for address input and copy round-trips.
pub(crate) fn normalize_address(input: &str) -> Result<String, String> {
    Endpoint::parse(input).map(|endpoint| endpoint.0)
}
impl Endpoint {
    fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.is_empty() || input.len() > MAX_ADDRESS || input.chars().any(char::is_control) {
            return Err("Enter an IP address or hostname followed by :port.".into());
        }
        if let Ok(address) = input.parse::<SocketAddr>() {
            valid_address(address)?;
            return Ok(Self(address.to_string()));
        }
        let (host, port) = input
            .rsplit_once(':')
            .ok_or("Include a port, for example localhost:27015.")?;
        let port: u16 = port
            .parse()
            .map_err(|_| "The server port must be between 1 and 65535.")?;
        let labels = host.trim_end_matches('.');
        if port == 0
            || labels.is_empty()
            || labels.len() > 253
            || !host.is_ascii()
            || host.ends_with("..")
            || labels.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err("Use an IP address or ASCII hostname and a nonzero port; URLs and paths are unsupported.".into());
        }
        Ok(Self(format!("{}:{port}", host.to_ascii_lowercase())))
    }
    fn resolve(&self) -> Result<Vec<SocketAddr>, String> {
        if let Ok(address) = self.0.parse() {
            return Ok(vec![address]);
        }
        // OS DNS can block, but runs only in this one owned worker. Cancellation
        // is observed on return; no second worker may start in the meantime.
        let addresses = self
            .0
            .to_socket_addrs()
            .map_err(|e| format!("Could not resolve {}: {e}", self.0))?;
        let mut addresses: Vec<_> = addresses.take(MAX_DNS + 1).collect();
        if addresses.is_empty() || addresses.len() > MAX_DNS {
            return Err("DNS returned no addresses or exceeded the address limit.".into());
        }
        for address in &addresses {
            valid_address(*address)?;
        }
        addresses.sort_by_key(|address| (address.is_ipv6(), *address));
        addresses.dedup();
        Ok(addresses)
    }
}
fn valid_address(address: SocketAddr) -> Result<(), String> {
    let mapped = match address.ip() {
        std::net::IpAddr::V6(ip) => ip.to_ipv4_mapped(),
        _ => None,
    };
    if address.port() == 0
        || address.ip().is_unspecified()
        || address.ip().is_multicast()
        || address.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::BROADCAST)
        || mapped.is_some_and(|ip| {
            ip.is_unspecified() || ip.is_multicast() || ip == std::net::Ipv4Addr::BROADCAST
        })
    {
        Err("Choose a single server address with a nonzero port.".into())
    } else {
        Ok(())
    }
}

#[derive(Clone)]
struct Observed {
    address: SocketAddr,
    info: ServerInfo,
    elapsed: Duration,
    at: Instant,
}
struct Row {
    key: ServerKey,
    manual: bool,
    favorite: bool,
    observed: Option<Observed>,
    error: Option<String>,
}
#[derive(Clone)]
enum Action {
    Refresh(Tab),
    Probe(Endpoint),
    Add(Endpoint),
    Remove(Endpoint),
}
struct Task {
    root: PathBuf,
    action: Action,
    manual: Vec<Endpoint>,
}
struct ResultSet {
    favorites: BTreeSet<Endpoint>,
    observations: BTreeMap<Endpoint, Result<Observed, String>>,
    status: String,
}
struct Job {
    lan: bool,
    generation: u64,
    address: Option<Endpoint>,
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<ResultSet, String>>,
    thread: JoinHandle<()>,
}
impl Job {
    fn start(generation: u64, task: Task) -> Result<Self, String> {
        let lan = matches!(&task.action, Action::Refresh(Tab::Lan));
        let address = match &task.action {
            Action::Probe(endpoint) | Action::Add(endpoint) => Some(endpoint.clone()),
            _ => None,
        };
        let mut job = Self::start_with(generation, address, move |cancel| {
            run(task, cancel, Endpoint::resolve)
        })?;
        job.lan = lan;
        Ok(job)
    }
    fn start_with(
        generation: u64,
        address: Option<Endpoint>,
        work: impl FnOnce(&AtomicBool) -> Result<ResultSet, String> + Send + 'static,
    ) -> Result<Self, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let owned = cancel.clone();
        let (tx, result) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("server-catalog".into())
            .spawn(move || {
                let _ = tx.send(work(&owned));
            })
            .map_err(|e| format!("Could not start server discovery: {e}"))?;
        Ok(Self {
            lan: false,
            generation,
            address,
            cancel,
            result,
            thread,
        })
    }
}

pub struct Backend {
    lan: BTreeSet<Endpoint>,
    root: PathBuf,
    tab: Tab,
    rows: BTreeMap<Endpoint, Row>,
    next_key: u64,
    generation: u64,
    job: Option<Job>,
    error: Option<String>,
    status: String,
}
impl Backend {
    /// Absolute launcher-selected userdata root; construction performs no I/O.
    pub fn new(root: PathBuf) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("Server favorites require an absolute player data root.".into());
        }
        Ok(Self {
            lan: BTreeSet::new(),
            root,
            tab: Tab::Internet,
            rows: BTreeMap::new(),
            next_key: 1,
            generation: 0,
            job: None,
            error: None,
            status: "Internet directory unavailable. Probe an address or open Favorites.".into(),
        })
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn snapshot(&self) -> Snapshot {
        let now = Instant::now();
        Snapshot {
            generation: self.generation,
            busy: self.busy(),
            error: self.error.clone(),
            status: self.status.clone(),
            rows: self
                .rows
                .iter()
                .filter(|(endpoint, row)| match self.tab {
                    Tab::Favorites => row.favorite,
                    Tab::Internet => row.manual,
                    Tab::Lan => self.lan.contains(*endpoint),
                })
                .map(|(endpoint, row)| ServerRow {
                    key: row.key,
                    endpoint: endpoint.0.clone(),
                    name: None,
                    players: row
                        .observed
                        .as_ref()
                        .map(|o| (o.info.players, o.info.max_players)),
                    ping_ms: None,
                    map: row
                        .observed
                        .as_ref()
                        .map(|observed| observed.info.content.map_name.clone()),
                    favorite: row.favorite,
                    connectable: !self.busy()
                        && row
                            .observed
                            .as_ref()
                            .is_some_and(|o| now.duration_since(o.at) <= FRESH_TIME),
                    discovery_elapsed: row.observed.as_ref().map(|o| o.elapsed),
                    error: row.error.clone(),
                })
                .collect(),
        }
    }
    fn bump(&mut self) -> Result<(), String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Server catalog generation exhausted.")?;
        Ok(())
    }
    fn key(&mut self) -> Result<ServerKey, String> {
        let key = ServerKey(self.next_key);
        self.next_key = self
            .next_key
            .checked_add(1)
            .ok_or("Server identity space exhausted.")?;
        Ok(key)
    }
    fn selected(&self, key: ServerKey, generation: u64) -> Result<(&Endpoint, &Row), String> {
        if self.busy() {
            return Err("Wait for server discovery or cancellation to finish.".into());
        }
        if generation != self.generation {
            return Err("The server list changed. Select the server again.".into());
        }
        self.rows
            .iter()
            .find(|(_, row)| row.key == key)
            .ok_or_else(|| "The selected server is no longer listed.".into())
    }
    pub fn apply(&mut self, intent: Intent) -> Result<Option<ConnectRequest>, String> {
        let result = self.apply_inner(intent);
        if let Err(error) = &result {
            self.error = Some(error.clone());
        } else if matches!(&result, Ok(Some(_))) {
            self.error = None;
        }
        result
    }
    fn apply_inner(&mut self, intent: Intent) -> Result<Option<ConnectRequest>, String> {
        if intent == Intent::Close {
            self.cancel();
            return Ok(None);
        }
        if intent == Intent::CancelAddress {
            if let Some(endpoint) = self.job.as_ref().and_then(|job| job.address.clone()) {
                self.job
                    .as_ref()
                    .unwrap()
                    .cancel
                    .store(true, Ordering::Release);
                self.bump()?;
                if let Some(row) = self.rows.get_mut(&endpoint) {
                    row.observed = None;
                }
                self.status = "Finishing address cancellation…".into();
            }
            return Ok(None);
        }
        if let Intent::Connect { key, generation } = intent {
            let (_, row) = self.selected(key, generation)?;
            let observed = row
                .observed
                .as_ref()
                .ok_or("Discover this server before connecting.")?;
            if observed.at.elapsed() > FRESH_TIME {
                return Err("Server discovery expired. Refresh before connecting.".into());
            }
            return Ok(Some(ConnectRequest {
                key,
                generation,
                address: observed.address,
                info: observed.info.clone(),
            }));
        }
        if self.busy() {
            return Err("Wait for server discovery or cancellation to finish.".into());
        }
        let adding = matches!(&intent, Intent::AddFavorite(_));
        let action = match intent {
            Intent::Refresh(tab) => {
                self.tab = tab;
                if tab == Tab::Lan { self.lan.clear(); }
                Action::Refresh(tab)
            }
            Intent::ProbeAddress(input) | Intent::AddFavorite(input) => {
                let endpoint = Endpoint::parse(&input)?;
                if !self.rows.get(&endpoint).is_some_and(|row| row.manual) {
                    if self.rows.values().filter(|row| row.manual).count() >= MAX_MANUAL {
                        return Err("The manually probed address list is full.".into());
                    }
                    if let Some(row) = self.rows.get_mut(&endpoint) {
                        row.manual = true;
                    } else {
                        let key = self.key()?;
                        self.rows.insert(
                            endpoint.clone(),
                            Row {
                                key,
                                manual: true,
                                favorite: false,
                                observed: None,
                                error: None,
                            },
                        );
                    }
                }
                if adding {
                    Action::Add(endpoint)
                } else {
                    Action::Probe(endpoint)
                }
            }
            Intent::RemoveFavorite { key, generation } => {
                let (endpoint, row) = self.selected(key, generation)?;
                if !row.favorite {
                    return Err("That server is not a favorite.".into());
                }
                Action::Remove(endpoint.clone())
            }
            _ => unreachable!(),
        };
        self.bump()?;
        match &action {
            Action::Refresh(_) => {
                for row in self.rows.values_mut() {
                    row.observed = None;
                    row.error = None;
                }
            }
            Action::Probe(endpoint) | Action::Add(endpoint) | Action::Remove(endpoint) => {
                if let Some(row) = self.rows.get_mut(endpoint) {
                    row.observed = None;
                    row.error = None;
                }
            }
        }
        self.error = None;
        self.status = "Discovering servers…".into();
        let manual = self
            .rows
            .iter()
            .filter(|(_, row)| row.manual)
            .map(|(endpoint, _)| endpoint.clone())
            .collect();
        self.job = Some(Job::start(
            self.generation,
            Task {
                root: self.root.clone(),
                action,
                manual,
            },
        )?);
        Ok(None)
    }
    /// Invalidate tickets immediately; retain ownership until the worker exits.
    pub fn cancel(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Release);
        }
        let _ = self.bump();
        for row in self.rows.values_mut() {
            row.observed = None;
        }
        self.status = if self.busy() {
            "Finishing cancellation…"
        } else {
            "Discovery cancelled."
        }
        .into();
    }
    pub fn poll(&mut self) {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| job.thread.is_finished())
        {
            return;
        }
        let job = self.job.take().unwrap();
        let result = job
            .result
            .try_recv()
            .map_err(|_| "Server discovery worker ended without a result.".to_string());
        if job.thread.join().is_err() {
            self.error = Some("Server discovery worker failed.".into());
        }
        if job.generation != self.generation {
            self.status =
                "Discovery cancelled. A favorite update already committed may remain saved.".into();
            return;
        }
        if let Err(error) = self.bump() {
            self.error = Some(error);
            return;
        }
        match result.and_then(|result| result) {
            Ok(result) => {
                self.error = None;
                if job.lan {
                    // Hints alone never create connectable rows. A new LAN
                    // refresh replaces membership with successful Inspect results.
                    self.lan = result.observations.iter()
                        .filter(|(_, observation)| observation.is_ok())
                        .map(|(endpoint, _)| endpoint.clone()).collect();
                }
                let mut previous = std::mem::take(&mut self.rows);
                let endpoints: BTreeSet<_> = result
                    .favorites
                    .iter()
                    .cloned()
                    .chain(
                        previous
                            .iter()
                            .filter(|(_, row)| row.manual)
                            .map(|(endpoint, _)| endpoint.clone()),
                    )
                    .chain(self.lan.iter().cloned())
                    .collect();
                for endpoint in endpoints {
                    let mut row = match previous.remove(&endpoint) {
                        Some(row) => row,
                        None => match self.key() {
                            Ok(key) => Row {
                                key,
                                manual: false,
                                favorite: false,
                                observed: None,
                                error: None,
                            },
                            Err(error) => {
                                self.error = Some(error);
                                break;
                            }
                        },
                    };
                    row.favorite = result.favorites.contains(&endpoint);
                    if let Some(observation) = result.observations.get(&endpoint) {
                        match observation {
                            Ok(observed) => row.observed = Some(observed.clone()),
                            Err(error) => row.error = Some(error.clone()),
                        }
                    }
                    self.rows.insert(endpoint, row);
                }
                self.status = result.status;
                let failed = self.rows.values().filter(|row| row.error.is_some()).count();
                if failed != 0 {
                    self.error = Some(format!(
                        "Could not discover {failed} server(s). Inspect the address or refresh."
                    ));
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Release);
        }
        // No blocking render-thread join. Root owns the lifetime and retains
        // this resource through busy()==false before creating a replacement.
    }
}

fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Discovery cancelled.".into())
    } else {
        Ok(())
    }
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
                Err("Favorite storage cannot be a link.".into())
            } else {
                Ok(())
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
fn read_favorites(path: &Path) -> Result<BTreeSet<Endpoint>, String> {
    no_link(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Favorite file exceeds its size limit; it was not changed.".into());
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| "Favorite file is not valid UTF-8; it was not changed.")?;
    let body = text
        .strip_prefix(HEADER)
        .ok_or("Unsupported favorite file; it was not changed.")?;
    let mut favorites = BTreeSet::new();
    for line in body.lines() {
        let endpoint = Endpoint::parse(line)
            .map_err(|_| "Invalid favorite address; the file was not changed.")?;
        if endpoint.0 != line || !favorites.insert(endpoint) || favorites.len() > MAX_FAVORITES {
            return Err(
                "Invalid, duplicate or too many favorites; the file was not changed.".into(),
            );
        }
    }
    Ok(favorites)
}
struct Pending(PathBuf);
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn write_favorites(
    path: &Path,
    favorites: &BTreeSet<Endpoint>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_file_name(format!(
        ".favorites-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let pending = Pending(tmp);
    file.write_all(HEADER.as_bytes())
        .map_err(|e| e.to_string())?;
    for endpoint in favorites {
        writeln!(file, "{}", endpoint.0).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    check(cancel)?;
    no_link(path)?;
    fs::rename(&pending.0, path).map_err(|e| format!("Could not publish favorites: {e}"))?;
    Ok(())
}

fn run(
    task: Task,
    cancel: &AtomicBool,
    resolve: impl Fn(&Endpoint) -> Result<Vec<SocketAddr>, String>,
) -> Result<ResultSet, String> {
    check(cancel)?;
    no_link(&task.root)?;
    fs::create_dir_all(&task.root).map_err(|e| e.to_string())?;
    let lock_path = task.root.join(".server-favorites.lock");
    no_link(&lock_path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    let lock_deadline = Instant::now() + PROBE_TIME;
    loop {
        check(cancel)?;
        match lock.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < lock_deadline => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(format!("Could not lock favorites: {error}")),
        }
    }
    let path = task.root.join("server-favorites.txt");
    let mut favorites = read_favorites(&path)?;
    match &task.action {
        Action::Add(endpoint) => {
            if !favorites.contains(endpoint) && favorites.len() >= MAX_FAVORITES {
                return Err("The favorite list is full.".into());
            }
            favorites.insert(endpoint.clone());
            write_favorites(&path, &favorites, cancel)?;
        }
        Action::Remove(endpoint) => {
            favorites.remove(endpoint);
            write_favorites(&path, &favorites, cancel)?;
        }
        _ => {}
    }
    drop(lock); // No storage lock is held during DNS or network discovery.
    let (targets, status) = match &task.action {
        Action::Refresh(Tab::Favorites) => {
            (favorites.iter().cloned().collect(), "Favorites refreshed.")
        }
        Action::Refresh(Tab::Internet) => (
            task.manual,
            "Internet directory unavailable. Showing only manually probed addresses.",
        ),
        Action::Refresh(Tab::Lan) => (
            netcode::lan::collect(cancel)
                .map_err(|error| format!("LAN discovery unavailable: {error}. Direct addresses and Favorites remain available."))?
                .into_iter().map(|address| Endpoint(address.to_string())).collect(),
            "LAN refresh completed. Only verified local announcements are listed; network filtering may hide servers.",
        ),
        Action::Probe(endpoint) => (
            vec![endpoint.clone()],
            "Address discovery completed; no Internet directory was queried.",
        ),
        Action::Add(endpoint) => (vec![endpoint.clone()], "Favorite saved."),
        Action::Remove(_) => (vec![], "Favorite removed."),
    };
    let observations = probe(targets, cancel, resolve)?;
    Ok(ResultSet {
        favorites,
        observations,
        status: status.into(),
    })
}

struct Probe {
    endpoint: Endpoint,
    addresses: VecDeque<SocketAddr>,
    address: SocketAddr,
    session: RemoteSession,
    started: Instant,
    last: Instant,
    attempt: Instant,
    attempt_limit: Duration,
}
fn probe(
    targets: Vec<Endpoint>,
    cancel: &AtomicBool,
    resolve: impl Fn(&Endpoint) -> Result<Vec<SocketAddr>, String>,
) -> Result<BTreeMap<Endpoint, Result<Observed, String>>, String> {
    let deadline = Instant::now() + BATCH_TIME;
    let mut waiting: VecDeque<_> = targets.into();
    let mut active: Vec<Probe> = Vec::new();
    let mut results = BTreeMap::new();
    while !waiting.is_empty() || !active.is_empty() {
        check(cancel)?;
        while active.len() < MAX_PROBES && !waiting.is_empty() {
            let endpoint = waiting.pop_front().unwrap();
            if Instant::now() >= deadline {
                results.insert(endpoint, Err("Discovery batch deadline exceeded.".into()));
                for endpoint in waiting.drain(..) {
                    results.insert(endpoint, Err("Discovery batch deadline exceeded.".into()));
                }
                break;
            }
            let started = Instant::now();
            let addresses = resolve(&endpoint);
            check(cancel)?;
            if Instant::now() >= deadline {
                results.insert(endpoint, Err("Discovery batch deadline exceeded.".into()));
                continue;
            }
            let mut addresses: VecDeque<_> = match addresses {
                Ok(addresses) if !addresses.is_empty() && addresses.len() <= MAX_DNS => {
                    addresses.into()
                }
                Ok(_) => {
                    results.insert(
                        endpoint,
                        Err("DNS address limit exceeded or no address returned.".into()),
                    );
                    continue;
                }
                Err(error) => {
                    results.insert(endpoint, Err(error));
                    continue;
                }
            };
            let attempt_limit = PROBE_TIME / addresses.len() as u32;
            let address = addresses.pop_front().unwrap();
            match RemoteSession::discover(
                address,
                "Browser probe".into(),
                sim::Team::CounterTerrorist,
            ) {
                Ok(session) => active.push(Probe {
                    endpoint,
                    addresses,
                    address,
                    session,
                    started,
                    last: Instant::now(),
                    attempt: Instant::now(),
                    attempt_limit,
                }),
                Err(error) => {
                    results.insert(
                        endpoint,
                        Err(format!("Could not start discovery: {error:?}")),
                    );
                }
            }
        }
        let mut index = 0;
        while index < active.len() {
            check(cancel)?;
            let item = &mut active[index];
            let now = Instant::now();
            let elapsed = now.duration_since(item.last).as_secs_f64();
            item.last = now;
            let update = if now >= deadline || item.started.elapsed() >= PROBE_TIME {
                Err(netcode::remote_session::Error::Timeout)
            } else {
                item.session.update(elapsed)
            };
            if let Status::AwaitingMap(info) = item.session.status() {
                results.insert(
                    item.endpoint.clone(),
                    Ok(Observed {
                        address: item.address,
                        info: info.clone(),
                        elapsed: item.started.elapsed(),
                        at: now,
                    }),
                );
                active.swap_remove(index);
            } else if update.is_err()
                || item.attempt.elapsed() >= item.attempt_limit
                || now >= deadline
                || item.started.elapsed() >= PROBE_TIME
            {
                item.session.cancel();
                if let Some(address) = item
                    .addresses
                    .pop_front()
                    .filter(|_| now < deadline && item.started.elapsed() < PROBE_TIME)
                {
                    match RemoteSession::discover(
                        address,
                        "Browser probe".into(),
                        sim::Team::CounterTerrorist,
                    ) {
                        Ok(session) => {
                            item.session = session;
                            item.address = address;
                            item.attempt = now;
                            item.last = now;
                            index += 1;
                            continue;
                        }
                        Err(_) => {}
                    }
                }
                results.insert(
                    item.endpoint.clone(),
                    Err("Server discovery failed or timed out.".into()),
                );
                active.swap_remove(index);
            } else {
                index += 1;
            }
        }
        if !waiting.is_empty() || !active.is_empty() {
            thread::sleep(Duration::from_millis(5));
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netcode::{
        protocol::{ContentManifest, VERSION},
        session::Session,
        transport::Server,
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "csr-server-catalog-{}-{}-用户",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn backend(&self) -> Backend {
            Backend::new(self.0.clone()).unwrap()
        }
        fn favorites(&self) -> PathBuf {
            self.0.join("server-favorites.txt")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn info() -> ServerInfo {
        ServerInfo {
            version: VERSION,
            players: 2,
            max_players: 4,
            tick_rate: 128,
            snapshot_interval_ticks: 4,
            max_unlag_ticks: 26,
            interpolation_allowance_ticks: 8,
            content: ContentManifest {
                map_id: "csgo_map_catalog_test".into(),
                map_name: "catalog_test".into(),
                map_hash: "0".repeat(40),
                catalog_hash: "1".repeat(40),
            },
        }
    }
    fn observed(address: SocketAddr) -> Observed {
        Observed {
            address,
            info: info(),
            elapsed: Duration::from_millis(17),
            at: Instant::now(),
        }
    }
    fn drain(backend: &mut Backend, mut server: Option<&mut Server>) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while backend.busy() {
            if let Some(server) = server.as_deref_mut() {
                server.update(Duration::from_millis(1)).unwrap();
                assert!(
                    server.session().world().players.is_empty(),
                    "Inspect unexpectedly admitted gameplay"
                );
            }
            backend.poll();
            assert!(
                Instant::now() < deadline,
                "Owned catalog job did not finish"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn publish(
        backend: &mut Backend,
        favorites: BTreeSet<Endpoint>,
        observations: BTreeMap<Endpoint, Result<Observed, String>>,
    ) {
        backend.bump().unwrap();
        backend.job = Some(
            Job::start_with(backend.generation, None, move |_| {
                Ok(ResultSet {
                    favorites,
                    observations,
                    status: "test".into(),
                })
            })
            .unwrap(),
        );
        drain(backend, None);
    }
    fn fake_server() -> Server {
        let world = sim::World::new(Arc::new(collision::CollisionWorld::new()));
        Server::bind(
            "127.0.0.1:0".parse().unwrap(),
            Session::new(world, info().content).unwrap(),
            4,
        )
        .unwrap()
    }

    #[test]
    fn lan_refresh_admits_only_verified_rows_and_drops_missing_members_without_losing_favorites() {
        fn publish_lan(backend: &mut Backend, favorite: Endpoint, observations: BTreeMap<Endpoint, Result<Observed, String>>) {
            backend.bump().unwrap();
            let mut job = Job::start_with(backend.generation, None, move |_| Ok(ResultSet {
                favorites: BTreeSet::from([favorite]), observations, status: "LAN fixture".into(),
            })).unwrap();
            job.lan = true;
            backend.job = Some(job);
            drain(backend, None);
        }
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        backend.tab = Tab::Lan;
        let good = Endpoint::parse("127.0.0.1:27018").unwrap();
        let bad = Endpoint::parse("127.0.0.1:27019").unwrap();
        publish_lan(&mut backend, good.clone(), BTreeMap::from([
            (good.clone(), Ok(observed("127.0.0.1:27018".parse().unwrap()))),
            (bad.clone(), Err("Inspect rejected the hint".into())),
        ]));
        let snapshot = backend.snapshot();
        assert_eq!(snapshot.rows.len(), 1);
        let row = &snapshot.rows[0];
        assert!(row.connectable && row.favorite);
        assert_eq!(row.endpoint, good.0);
        assert_eq!(row.players, Some((2, 4)));
        assert!(row.ping_ms.is_none());
        assert!(backend.apply(Intent::Connect { key: row.key, generation: snapshot.generation }).unwrap().is_some());
        assert!(!backend.rows.contains_key(&bad), "unverified advertisement created a row");
        publish_lan(&mut backend, good.clone(), BTreeMap::new());
        assert!(backend.snapshot().rows.is_empty(), "vanished server remained in LAN results");
        assert!(backend.apply(Intent::Connect { key: row.key, generation: snapshot.generation }).is_err(), "stale LAN ticket survived refresh");
        backend.tab = Tab::Favorites;
        assert_eq!(backend.snapshot().rows.len(), 1);
        assert_eq!(backend.snapshot().rows[0].key, row.key);
    }

    #[test]
    fn parses_canonical_literals_and_hostnames_without_accepting_urls_or_paths() {
        assert_eq!(
            Endpoint::parse(" LOCALHOST:027015 ").unwrap().0,
            "localhost:27015"
        );
        assert_eq!(
            Endpoint::parse("[0:0:0:0:0:0:0:1]:27015").unwrap().0,
            "[::1]:27015"
        );
        for input in [
            "localhost",
            "http://localhost:27015",
            "host/path:27015",
            "a..b:27015",
            "a..:27015",
            "bad\nhost:27015",
            "host:0",
            "0.0.0.0:27015",
            "[::]:27015",
            "224.0.0.1:27015",
            "255.255.255.255:27015",
            "[::ffff:224.0.0.1]:27015",
            "[::ffff:255.255.255.255]:27015",
        ] {
            assert!(Endpoint::parse(input).is_err(), "accepted {input:?}");
        }
        assert!(Endpoint::parse(&format!("{}:27015", "a".repeat(254))).is_err());
        assert!(Backend::new(PathBuf::from("relative-userdata")).is_err());
    }

    #[test]
    fn actual_hostname_inspect_returns_content_but_never_admits_gameplay_or_fakes_metadata() {
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        let mut server = fake_server();
        backend
            .apply(Intent::ProbeAddress(format!(
                "localhost:{}",
                server.address().port()
            )))
            .unwrap();
        assert!(backend.snapshot().busy);
        let initial = backend.snapshot();
        assert!(initial.rows[0].players.is_none());
        assert!(
            backend
                .apply(Intent::Connect {
                    key: initial.rows[0].key,
                    generation: initial.generation
                })
                .is_err()
        );
        drain(&mut backend, Some(&mut server));
        let snapshot = backend.snapshot();
        assert_eq!(snapshot.rows.len(), 1);
        let row = &snapshot.rows[0];
        assert!(row.connectable, "{:?}", row.error);
        assert_eq!(
            row.endpoint,
            format!("localhost:{}", server.address().port())
        );
        assert!(
            snapshot.error.is_none(),
            "stale busy error survived successful discovery"
        );
        assert_eq!(row.map.as_deref(), Some("catalog_test"));
        assert_eq!(row.players, Some((0, 4)));
        assert!(row.name.is_none() && row.ping_ms.is_none());
        assert!(row.discovery_elapsed.unwrap() > Duration::ZERO);
        let request = backend
            .apply(Intent::Connect {
                key: row.key,
                generation: snapshot.generation,
            })
            .unwrap()
            .unwrap();
        assert_eq!(request.address, server.address());
        assert_eq!(request.info.content, info().content);
        assert_eq!(request.key, row.key);
        assert_eq!(request.generation, snapshot.generation);
        assert!(server.session().world().players.is_empty());
        assert!(
            !fixture.favorites().exists(),
            "Probe persisted a favorite without permission"
        );
    }

    #[test]
    fn stable_keys_survive_reorder_refresh_and_tickets_reject_changed_generation_and_age() {
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        backend.tab = Tab::Favorites;
        let first = Endpoint::parse("127.0.0.1:27020").unwrap();
        let earlier = Endpoint::parse("127.0.0.1:27010").unwrap();
        publish(
            &mut backend,
            BTreeSet::from([first.clone()]),
            BTreeMap::from([(
                first.clone(),
                Ok(observed("127.0.0.1:27020".parse().unwrap())),
            )]),
        );
        let before = backend.snapshot();
        let key = before.rows[0].key;
        publish(
            &mut backend,
            BTreeSet::from([first.clone(), earlier]),
            BTreeMap::from([(
                first.clone(),
                Ok(observed("127.0.0.1:27020".parse().unwrap())),
            )]),
        );
        let after = backend.snapshot();
        assert_eq!(after.rows[1].key, key);
        assert!(
            backend
                .apply(Intent::Connect {
                    key,
                    generation: before.generation
                })
                .is_err()
        );
        assert!(
            backend
                .apply(Intent::RemoveFavorite {
                    key,
                    generation: before.generation
                })
                .is_err()
        );
        assert!(
            backend
                .apply(Intent::Connect {
                    key: after.rows[0].key,
                    generation: after.generation
                })
                .is_err(),
            "undiscovered row connected"
        );
        backend
            .rows
            .get_mut(&first)
            .unwrap()
            .observed
            .as_mut()
            .unwrap()
            .at = Instant::now() - FRESH_TIME - Duration::from_secs(1);
        assert!(!backend.snapshot().rows[1].connectable);
        assert!(
            backend
                .apply(Intent::Connect {
                    key,
                    generation: after.generation
                })
                .is_err()
        );
    }

    #[test]
    fn favorite_atomic_replacement_and_prepublication_cancellation_preserve_old_bytes() {
        let fixture = Fixture::new();
        let cancel = AtomicBool::new(false);
        let first = BTreeSet::from([Endpoint::parse("localhost:27015").unwrap()]);
        write_favorites(&fixture.favorites(), &first, &cancel).unwrap();
        let before = fs::read(fixture.favorites()).unwrap();
        let second = BTreeSet::from([Endpoint::parse("[::1]:27016").unwrap()]);
        cancel.store(true, Ordering::Release);
        assert!(write_favorites(&fixture.favorites(), &second, &cancel).is_err());
        assert_eq!(fs::read(fixture.favorites()).unwrap(), before);
        assert_eq!(
            fs::read_dir(&fixture.0).unwrap().count(),
            1,
            "temporary write leaked"
        );
        cancel.store(false, Ordering::Release);
        write_favorites(&fixture.favorites(), &second, &cancel).unwrap();
        assert_eq!(read_favorites(&fixture.favorites()).unwrap(), second);
    }

    #[test]
    fn corrupt_oversized_duplicate_and_noncanonical_favorites_are_never_overwritten() {
        let fixture = Fixture::new();
        let malformed = [
            b"wrong version\n".to_vec(),
            vec![b'x'; MAX_FILE as usize + 1],
            vec![0xff],
            format!("{HEADER}localhost:27015\nlocalhost:27015\n").into_bytes(),
            format!("{HEADER}LOCALHOST:27015\n").into_bytes(),
        ];
        for bytes in malformed {
            fs::write(fixture.favorites(), &bytes).unwrap();
            let task = Task {
                root: fixture.0.clone(),
                action: Action::Add(Endpoint::parse("localhost:27016").unwrap()),
                manual: vec![],
            };
            assert!(
                run(task, &AtomicBool::new(false), |_| panic!(
                    "corruption reached DNS"
                ))
                .is_err()
            );
            assert_eq!(fs::read(fixture.favorites()).unwrap(), bytes);
        }
    }

    #[test]
    fn favorite_actions_are_async_truthful_and_stable_across_tabs() {
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        let mut server = fake_server();
        let endpoint = server.address().to_string();
        backend
            .apply(Intent::AddFavorite(endpoint.clone()))
            .unwrap();
        assert!(
            !backend.snapshot().rows[0].favorite,
            "optimistic favorite before publication"
        );
        drain(&mut backend, Some(&mut server));
        let key = backend.snapshot().rows[0].key;
        assert!(backend.snapshot().rows[0].favorite);
        assert!(backend.snapshot().status.contains("saved"));
        backend.apply(Intent::Refresh(Tab::Favorites)).unwrap();
        drain(&mut backend, Some(&mut server));
        let snapshot = backend.snapshot();
        assert_eq!(snapshot.rows[0].key, key);
        backend
            .apply(Intent::RemoveFavorite {
                key,
                generation: snapshot.generation,
            })
            .unwrap();
        drain(&mut backend, None);
        assert!(
            backend.snapshot().rows.is_empty(),
            "non-favorite leaked into Favorites tab"
        );
        assert!(read_favorites(&fixture.favorites()).unwrap().is_empty());
        backend.apply(Intent::Refresh(Tab::Internet)).unwrap();
        drain(&mut backend, Some(&mut server));
        assert_eq!(backend.snapshot().rows[0].key, key);
        assert!(backend.snapshot().status.contains("directory unavailable"));
    }

    #[test]
    fn cancelled_dns_retains_one_worker_and_discards_late_results_until_owned_exit() {
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        let endpoint = Endpoint::parse("localhost:27015").unwrap();
        let (entered, arrived) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let late = endpoint.clone();
        backend.bump().unwrap();
        backend.job = Some(
            Job::start_with(backend.generation, Some(endpoint), move |_| {
                entered.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(2)).unwrap();
                // Model a resolver that cannot notice cancellation until it returns.
                Ok(ResultSet {
                    favorites: BTreeSet::from([late.clone()]),
                    observations: BTreeMap::from([(
                        late,
                        Ok(observed("127.0.0.1:27015".parse().unwrap())),
                    )]),
                    status: "late".into(),
                })
            })
            .unwrap(),
        );
        arrived.recv_timeout(Duration::from_secs(2)).unwrap();
        backend.apply(Intent::CancelAddress).unwrap();
        backend.poll();
        assert!(backend.busy());
        assert!(
            backend
                .apply(Intent::ProbeAddress("localhost:27016".into()))
                .is_err()
        );
        release.send(()).unwrap();
        drain(&mut backend, None);
        assert!(
            backend.snapshot().rows.is_empty(),
            "late DNS restored cancelled metadata"
        );
    }

    #[test]
    fn closing_address_dialog_does_not_cancel_unrelated_favorite_refresh() {
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        let (release, wait) = mpsc::channel();
        let favorite = Endpoint::parse("127.0.0.1:27015").unwrap();
        backend.bump().unwrap();
        let generation = backend.generation;
        backend.job = Some(
            Job::start_with(generation, None, move |cancel| {
                wait.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(!cancel.load(Ordering::Acquire));
                Ok(ResultSet {
                    favorites: BTreeSet::from([favorite]),
                    observations: BTreeMap::new(),
                    status: "refreshed".into(),
                })
            })
            .unwrap(),
        );
        backend.apply(Intent::CancelAddress).unwrap();
        assert_eq!(backend.generation, generation);
        release.send(()).unwrap();
        drain(&mut backend, None);
        assert_eq!(backend.rows.len(), 1);
        assert!(backend.rows.values().next().unwrap().favorite);
    }

    #[test]
    fn cancellation_while_another_process_owns_favorite_lock_needs_no_stale_recovery() {
        let fixture = Fixture::new();
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(fixture.0.join(".server-favorites.lock"))
            .unwrap();
        lock.lock().unwrap();
        let mut backend = fixture.backend();
        backend
            .apply(Intent::AddFavorite("localhost:27015".into()))
            .unwrap();
        backend.cancel();
        drain(&mut backend, None);
        assert!(!fixture.favorites().exists());
        drop(lock);
        let task = Task {
            root: fixture.0.clone(),
            action: Action::Refresh(Tab::Favorites),
            manual: vec![],
        };
        assert!(
            run(task, &AtomicBool::new(false), |_| panic!(
                "empty list reached DNS"
            ))
            .is_ok()
        );
    }

    #[test]
    fn favorite_and_manual_limits_are_enforced_without_growing_unbounded_work() {
        let fixture = Fixture::new();
        let mut favorites = BTreeSet::new();
        for port in 1..=MAX_FAVORITES {
            favorites.insert(Endpoint::parse(&format!("localhost:{port}")).unwrap());
        }
        write_favorites(&fixture.favorites(), &favorites, &AtomicBool::new(false)).unwrap();
        let before = fs::read(fixture.favorites()).unwrap();
        let task = Task {
            root: fixture.0.clone(),
            action: Action::Add(Endpoint::parse("localhost:60000").unwrap()),
            manual: vec![],
        };
        assert!(
            run(task, &AtomicBool::new(false), |_| panic!(
                "full list reached DNS"
            ))
            .is_err()
        );
        assert_eq!(fs::read(fixture.favorites()).unwrap(), before);
        let mut backend = fixture.backend();
        for port in 1..=MAX_MANUAL {
            let key = backend.key().unwrap();
            backend.rows.insert(
                Endpoint::parse(&format!("localhost:{port}")).unwrap(),
                Row {
                    key,
                    manual: true,
                    favorite: false,
                    observed: None,
                    error: None,
                },
            );
        }
        assert!(
            backend
                .apply(Intent::ProbeAddress("localhost:60000".into()))
                .is_err()
        );
        assert!(!backend.busy());
    }
}
