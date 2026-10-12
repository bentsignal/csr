//! Owned host address receipts. Interface enumeration is local metadata, not
//! LAN discovery or evidence of Internet reachability. No packets are sent.
//!
//! Keep this owner alive through `clear()` until `busy()` is false, including
//! app exit. A synchronous OS adapter call cannot be interrupted; cancellation
//! invalidates its receipt immediately, and no second worker starts meanwhile.
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

pub(crate) const MAX_CANDIDATES: usize = 32;
const MAX_INTERFACES: usize = 256;
const MAX_LABEL: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CandidateKey(SocketAddr);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    OnThisNetwork,
    ThisPc,
}
impl Scope {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OnThisNetwork => "On this network",
            Self::ThisPc => "This PC",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub key: CandidateKey,
    /// A literal address using the actual bound port. IPv6 includes its scope.
    pub endpoint: SocketAddr,
    pub scope: Scope,
    pub interface: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Receipt {
    /// Opaque receipt revision. Stamp rendered Copy actions with this value.
    pub generation: u64,
    pub host_generation: Option<u64>,
    pub bound_address: Option<SocketAddr>,
    pub candidates: Vec<Candidate>,
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CopyIntent {
    pub generation: u64,
    pub key: CandidateKey,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CopyResult {
    Copied(String),
    Failed(String),
}

#[derive(Clone, Debug)]
struct Interface {
    address: SocketAddr,
    name: String,
    up: bool,
    loopback: bool,
}
type Enumerator = Arc<dyn Fn(bool, &AtomicBool) -> Result<Vec<Interface>, String> + Send + Sync>;

#[derive(Clone, Copy)]
struct Request {
    generation: u64,
    bound: SocketAddr,
}
struct Worker {
    request: Request,
    cancel: Arc<AtomicBool>,
    result: Mutex<mpsc::Receiver<Result<Vec<Interface>, String>>>,
    thread: JoinHandle<()>,
}
pub(crate) struct Sharing {
    receipt: Receipt,
    pending: Option<Request>,
    worker: Option<Worker>,
    enumerate: Enumerator,
}
impl Default for Sharing {
    fn default() -> Self {
        Self::new(Arc::new(native::enumerate))
    }
}
impl Sharing {
    fn new(enumerate: Enumerator) -> Self {
        Self {
            receipt: Receipt::default(),
            pending: None,
            worker: None,
            enumerate,
        }
    }

    /// One controlled enumeration for root UI lifetime tests. Subsequent
    /// requests fail explicitly; this is never compiled into player builds.
    #[cfg(test)]
    pub(crate) fn with_test_enumerator(
        enumerate: impl FnOnce() -> Result<Vec<Candidate>, String> + Send + 'static,
    ) -> Self {
        let enumerate = Mutex::new(Some(enumerate));
        Self::new(Arc::new(move |_, _| {
            let enumerate = enumerate
                .lock()
                .unwrap()
                .take()
                .ok_or("Test enumeration already consumed")?;
            enumerate().map(|values| {
                values
                    .into_iter()
                    .map(|value| Interface {
                        address: value.endpoint,
                        name: value.interface.unwrap_or_default(),
                        up: true,
                        loopback: value.scope == Scope::ThisPc,
                    })
                    .collect()
            })
        }))
    }

    /// The caller supplies its immutable host lifetime generation and the
    /// *actual bound* socket address, never the private local-admission offer.
    /// Repeating the same host/endpoint is a no-op; use `refresh()` to rescan.
    pub(crate) fn set_host(&mut self, host_generation: u64, bound: SocketAddr) {
        if self.receipt.host_generation == Some(host_generation)
            && self.receipt.bound_address == Some(bound)
        {
            return;
        }
        self.invalidate();
        self.receipt.host_generation = Some(host_generation);
        self.receipt.bound_address = Some(bound);
        self.schedule(bound);
    }

    pub(crate) fn refresh(&mut self) {
        let Some(bound) = self.receipt.bound_address else {
            return;
        };
        self.invalidate();
        self.schedule(bound);
    }

    /// Immediately removes every copyable address. Poll until the old worker
    /// has returned before dropping/replacing this resource on app exit.
    pub(crate) fn clear(&mut self) {
        self.invalidate();
        self.receipt.host_generation = None;
        self.receipt.bound_address = None;
    }
    fn invalidate(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel.store(true, Ordering::Release);
        }
        self.pending = None;
        self.receipt.candidates.clear();
        self.receipt.error = None;
        // Refuse receipts forever on impossible exhaustion; never reuse a key.
        if let Some(next) = self.receipt.generation.checked_add(1) {
            self.receipt.generation = next;
        } else {
            self.receipt.error = Some("Address receipt limit reached.".into());
        }
    }
    fn schedule(&mut self, bound: SocketAddr) {
        if self.receipt.error.is_some() {
            return;
        }
        if bound.port() == 0 || invalid_ip(bound.ip()) || unshareable_link_local(bound.ip()) {
            self.receipt.error = Some("The server has no usable bound address.".into());
            return;
        }
        if !bound.ip().is_unspecified() {
            self.receipt.candidates.push(candidate(bound, None));
        } else {
            self.pending = Some(Request {
                generation: self.receipt.generation,
                bound,
            });
        }
        self.poll();
    }
    pub(crate) fn busy(&self) -> bool {
        self.worker.is_some() || self.pending.is_some()
    }
    pub(crate) fn receipt(&self) -> Receipt {
        let mut receipt = self.receipt.clone();
        receipt.busy = self.busy();
        receipt
    }

    /// Joins only an already finished thread; never waits on an OS call.
    pub(crate) fn poll(&mut self) {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.thread.is_finished())
        {
            let mut worker = self.worker.take().unwrap();
            let result = worker
                .result
                .get_mut()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .try_recv()
                .unwrap_or_else(|_| Err("Could not read network addresses.".into()));
            let joined = worker.thread.join();
            if !worker.cancel.load(Ordering::Acquire)
                && worker.request.generation == self.receipt.generation
                && self.receipt.bound_address == Some(worker.request.bound)
                && self.receipt.host_generation.is_some()
            {
                let result = if joined.is_err() {
                    Err("The address worker stopped unexpectedly.".into())
                } else {
                    result.and_then(|interfaces| candidates(worker.request.bound, interfaces))
                };
                match result {
                    Ok(values) => self.receipt.candidates = values,
                    Err(error) => self.receipt.error = Some(bounded(error)),
                }
            }
        }
        if self.worker.is_none()
            && let Some(request) = self.pending.take()
        {
            let cancel = Arc::new(AtomicBool::new(false));
            let owned_cancel = cancel.clone();
            let enumerate = self.enumerate.clone();
            let (send, result) = mpsc::sync_channel(1);
            match thread::Builder::new()
                .name("host-addresses".into())
                .spawn(move || {
                    let value = enumerate(request.bound.is_ipv6(), &owned_cancel);
                    let _ = send.send(value);
                }) {
                Ok(thread) => {
                    self.worker = Some(Worker {
                        request,
                        cancel,
                        result: Mutex::new(result),
                        thread,
                    })
                }
                Err(_) => self.receipt.error = Some("Could not start the address worker.".into()),
            }
        }
    }

    /// The root passes the current owned game window on its UI thread. This
    /// method is called only in response to an explicit rendered Copy action.
    pub(crate) fn copy(
        &self,
        intent: CopyIntent,
        owner: &bevy::window::RawHandleWrapper,
    ) -> CopyResult {
        self.copy_with(intent, |text| native::copy(owner, text))
    }

    /// Injectable sink supports honest engine-input proofs without replacing
    /// the user's system clipboard. It applies the same receipt gates.
    pub(crate) fn copy_with(
        &self,
        intent: CopyIntent,
        write: impl FnOnce(&str) -> Result<(), String>,
    ) -> CopyResult {
        if self.busy()
            || self.receipt.host_generation.is_none()
            || intent.generation != self.receipt.generation
        {
            return CopyResult::Failed("These addresses changed. Choose an address again.".into());
        }
        let Some(value) = self
            .receipt
            .candidates
            .iter()
            .find(|value| value.key == intent.key)
        else {
            return CopyResult::Failed("This address is no longer available.".into());
        };
        let endpoint = value.endpoint.to_string();
        match write(&endpoint) {
            Ok(()) => CopyResult::Copied(endpoint),
            Err(error) => CopyResult::Failed(bounded(error)),
        }
    }
}
impl Drop for Sharing {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel.store(true, Ordering::Release);
        }
        // The caller reaps on normal Home/exit; a forced process exit still
        // leaves only thread-owned OS buffers, never callbacks into the root.
    }
}
fn bounded(text: String) -> String {
    text.chars().filter(|c| !c.is_control()).take(256).collect()
}
fn invalid_ip(ip: IpAddr) -> bool {
    ip.is_multicast()
        || ip == IpAddr::V4(Ipv4Addr::BROADCAST)
        || matches!(ip, IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some())
}
fn unshareable_link_local(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if v6.is_unicast_link_local())
}
fn candidate(mut endpoint: SocketAddr, interface: Option<String>) -> Candidate {
    // A global/loopback IPv6 address needs no machine-local zone or flow
    // metadata in its copyable literal. Link-local addresses are excluded.
    if let SocketAddr::V6(v6) = &mut endpoint {
        v6.set_scope_id(0);
        v6.set_flowinfo(0);
    }
    Candidate {
        key: CandidateKey(endpoint),
        endpoint,
        scope: if endpoint.ip().is_loopback() {
            Scope::ThisPc
        } else {
            Scope::OnThisNetwork
        },
        interface,
    }
}
fn candidates(bound: SocketAddr, interfaces: Vec<Interface>) -> Result<Vec<Candidate>, String> {
    if interfaces.len() > MAX_INTERFACES {
        return Err("Too many network addresses were returned.".into());
    }
    let mut sorted = Vec::new();
    for interface in interfaces {
        if !interface.up
            || interface.loopback
            || interface.address.ip().is_loopback()
            || interface.address.ip().is_unspecified()
            || invalid_ip(interface.address.ip())
            || unshareable_link_local(interface.address.ip())
            || interface.address.is_ipv6() != bound.is_ipv6()
        {
            continue;
        }
        let mut endpoint = interface.address;
        endpoint.set_port(bound.port());
        let name: String = interface
            .name
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_LABEL)
            .collect();
        sorted.push(candidate(endpoint, (!name.is_empty()).then_some(name)));
    }
    sorted.sort_by(|a, b| (a.key, &a.interface).cmp(&(b.key, &b.interface)));
    sorted.dedup_by_key(|value| value.key);
    let local = SocketAddr::new(
        if bound.is_ipv6() {
            Ipv6Addr::LOCALHOST.into()
        } else {
            Ipv4Addr::LOCALHOST.into()
        },
        bound.port(),
    );
    sorted.push(candidate(local, None));
    if sorted.len() > MAX_CANDIDATES {
        return Err("Too many network addresses are available.".into());
    }
    Ok(sorted)
}

#[cfg(not(windows))]
mod native {
    use super::*;
    pub(super) fn enumerate(_: bool, _: &AtomicBool) -> Result<Vec<Interface>, String> {
        Err("Network address lookup is unavailable on this platform.".into())
    }
    pub(super) fn copy(_: &bevy::window::RawHandleWrapper, _: &str) -> Result<(), String> {
        Err("Address copying is unavailable on this platform.".into())
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        collections::BTreeSet,
        mem::{offset_of, size_of},
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS, GlobalFree, HGLOBAL},
        NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
        Networking::WinSock::{
            AF_INET, AF_INET6, IpDadStatePreferred, SOCKADDR_IN, SOCKADDR_IN6, SOCKET_ADDRESS,
        },
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Threading::GetCurrentThreadId,
        },
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    };

    const MAX_BUFFER: usize = 1024 * 1024;
    const MAX_ADAPTERS: usize = 64;
    const MAX_ADDRESS_NODES: usize = 512;
    const CANCELLED: &str = "Address lookup cancelled.";
    const MALFORMED: &str = "Network address data is unavailable.";

    fn check(cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Acquire) {
            Err(CANCELLED.into())
        } else {
            Ok(())
        }
    }

    // All returned pointers must stay within the one owned initialized buffer.
    // Copying C integer/pointer structs unaligned does not dereference their
    // embedded pointers. Each of those is checked independently before use.
    struct Buffer<'a>(&'a [u64]);
    impl Buffer<'_> {
        fn read<T: Copy>(&self, pointer: *const T) -> Result<T, String> {
            let start = self.0.as_ptr() as usize;
            let offset = (pointer as usize).checked_sub(start).ok_or(MALFORMED)?;
            if offset
                .checked_add(size_of::<T>())
                .is_none_or(|end| end > std::mem::size_of_val(self.0))
            {
                return Err(MALFORMED.into());
            }
            // SAFETY: bounds above cover an initialized C-layout POD value.
            Ok(unsafe { pointer.read_unaligned() })
        }
        fn name(&self, pointer: *const u16) -> Result<String, String> {
            if pointer.is_null() {
                return Ok(String::new());
            }
            let mut units = Vec::new();
            for index in 0..=MAX_LABEL {
                // Wrapping pointer arithmetic is not dereferenced until read
                // checks the resulting address against this buffer.
                let unit = self.read(pointer.wrapping_add(index))?;
                if unit == 0 {
                    return String::from_utf16(&units).map_err(|_| MALFORMED.into());
                }
                if index == MAX_LABEL {
                    return Ok(String::from_utf16_lossy(&units));
                }
                units.push(unit);
            }
            unreachable!()
        }
        fn address(&self, value: SOCKET_ADDRESS) -> Result<Option<SocketAddr>, String> {
            if value.iSockaddrLength < size_of::<u16>() as i32 {
                return Err(MALFORMED.into());
            }
            let family: u16 = self.read(value.lpSockaddr.cast())?;
            match family {
                AF_INET => {
                    if value.iSockaddrLength < size_of::<SOCKADDR_IN>() as i32 {
                        return Err(MALFORMED.into());
                    }
                    let address: SOCKADDR_IN = self.read(value.lpSockaddr.cast())?;
                    // IPv4 network bytes are the in-memory bytes of S_addr.
                    let bytes = unsafe { address.sin_addr.S_un.S_addr }.to_ne_bytes();
                    Ok(Some(SocketAddr::new(Ipv4Addr::from(bytes).into(), 0)))
                }
                AF_INET6 => {
                    if value.iSockaddrLength < size_of::<SOCKADDR_IN6>() as i32 {
                        return Err(MALFORMED.into());
                    }
                    let address: SOCKADDR_IN6 = self.read(value.lpSockaddr.cast())?;
                    Ok(Some(
                        std::net::SocketAddrV6::new(
                            Ipv6Addr::from(unsafe { address.sin6_addr.u.Byte }),
                            0,
                            0,
                            unsafe { address.Anonymous.sin6_scope_id },
                        )
                        .into(),
                    ))
                }
                _ => Ok(None),
            }
        }
    }

    fn parse(buffer: &[u64], cancel: &AtomicBool) -> Result<Vec<Interface>, String> {
        let buffer = Buffer(buffer);
        let mut adapter = buffer.0.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let mut seen_adapters = BTreeSet::new();
        let mut seen_addresses = BTreeSet::new();
        let mut result = Vec::new();
        while !adapter.is_null() {
            check(cancel)?;
            if !seen_adapters.insert(adapter as usize) || seen_adapters.len() > MAX_ADAPTERS {
                return Err(MALFORMED.into());
            }
            let value = buffer.read(adapter)?;
            let length = unsafe { value.Anonymous1.Anonymous.Length } as usize;
            if length < offset_of!(IP_ADAPTER_ADDRESSES_LH, OperStatus) + size_of::<i32>() {
                return Err(MALFORMED.into());
            }
            if value.OperStatus == IfOperStatusUp && value.IfType != IF_TYPE_SOFTWARE_LOOPBACK {
                let name = buffer.name(value.FriendlyName)?;
                let mut unicast = value.FirstUnicastAddress;
                while !unicast.is_null() {
                    check(cancel)?;
                    if !seen_addresses.insert(unicast as usize)
                        || seen_addresses.len() > MAX_ADDRESS_NODES
                    {
                        return Err(MALFORMED.into());
                    }
                    let address = buffer.read(unicast)?;
                    let length = unsafe { address.Anonymous.Anonymous.Length } as usize;
                    if length
                        < offset_of!(IP_ADAPTER_UNICAST_ADDRESS_LH, DadState) + size_of::<i32>()
                    {
                        return Err(MALFORMED.into());
                    }
                    if address.DadState == IpDadStatePreferred
                        && let Some(address) = buffer.address(address.Address)?
                    {
                        if result.len() == MAX_INTERFACES {
                            return Err("Too many network addresses were returned.".into());
                        }
                        result.push(Interface {
                            address,
                            name: name.clone(),
                            up: true,
                            loopback: false,
                        });
                    }
                    unicast = address.Next;
                }
            }
            adapter = value.Next;
        }
        Ok(result)
    }

    pub(super) fn enumerate(ipv6: bool, cancel: &AtomicBool) -> Result<Vec<Interface>, String> {
        let mut requested = 15_000usize;
        for _ in 0..3 {
            check(cancel)?;
            if requested > MAX_BUFFER {
                return Err("Network address lookup exceeded its memory limit.".into());
            }
            // u64 storage supplies native structure/union alignment. Every
            // byte starts initialized, including any native layout padding.
            let mut buffer = vec![0u64; requested.div_ceil(size_of::<u64>())];
            let mut length = requested as u32;
            let result = unsafe {
                GetAdaptersAddresses(
                    if ipv6 {
                        AF_INET6 as u32
                    } else {
                        AF_INET as u32
                    },
                    GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                    ptr::null(),
                    buffer.as_mut_ptr().cast(),
                    &mut length,
                )
            };
            check(cancel)?;
            match result {
                ERROR_SUCCESS => return parse(&buffer, cancel),
                ERROR_NO_DATA => return Ok(Vec::new()),
                ERROR_BUFFER_OVERFLOW => requested = length as usize,
                _ => return Err("Could not read network addresses.".into()),
            }
        }
        Err("Network addresses kept changing. Refresh to try again.".into())
    }

    struct Memory(HGLOBAL);
    impl Drop for Memory {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    GlobalFree(self.0);
                }
            }
        }
    }
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn fixture(ipv6: bool) -> Vec<u64> {
            let mut buffer = vec![0u64; 256];
            let base = buffer.as_mut_ptr().cast::<u8>();
            let unicast_offset = size_of::<IP_ADAPTER_ADDRESSES_LH>().next_multiple_of(8);
            let socket_offset =
                unicast_offset + size_of::<IP_ADAPTER_UNICAST_ADDRESS_LH>().next_multiple_of(8);
            let mut adapter = IP_ADAPTER_ADDRESSES_LH::default();
            adapter.Anonymous1.Anonymous.Length = size_of::<IP_ADAPTER_ADDRESSES_LH>() as u32;
            adapter.OperStatus = IfOperStatusUp;
            adapter.IfType = 6;
            adapter.FirstUnicastAddress = base.wrapping_add(unicast_offset).cast();
            let mut unicast = IP_ADAPTER_UNICAST_ADDRESS_LH::default();
            unicast.Anonymous.Anonymous.Length = size_of::<IP_ADAPTER_UNICAST_ADDRESS_LH>() as u32;
            unicast.DadState = IpDadStatePreferred;
            unicast.Address.lpSockaddr = base.wrapping_add(socket_offset).cast();
            unsafe {
                if ipv6 {
                    let mut address = SOCKADDR_IN6::default();
                    address.sin6_family = AF_INET6;
                    address.sin6_addr.u.Byte = "fe80::1234".parse::<Ipv6Addr>().unwrap().octets();
                    address.Anonymous.sin6_scope_id = 7;
                    unicast.Address.iSockaddrLength = size_of::<SOCKADDR_IN6>() as i32;
                    base.add(socket_offset)
                        .cast::<SOCKADDR_IN6>()
                        .write(address);
                } else {
                    let mut address = SOCKADDR_IN::default();
                    address.sin_family = AF_INET;
                    address.sin_addr.S_un.S_addr = u32::from_ne_bytes([192, 0, 2, 3]);
                    unicast.Address.iSockaddrLength = size_of::<SOCKADDR_IN>() as i32;
                    base.add(socket_offset).cast::<SOCKADDR_IN>().write(address);
                }
                base.cast::<IP_ADAPTER_ADDRESSES_LH>().write(adapter);
                base.add(unicast_offset)
                    .cast::<IP_ADAPTER_UNICAST_ADDRESS_LH>()
                    .write(unicast);
            }
            buffer
        }
        #[test]
        fn generated_native_layout_preserves_network_bytes_and_ipv6_scope() {
            let cancel = AtomicBool::new(false);
            let v4 = parse(&fixture(false), &cancel).unwrap();
            assert_eq!(v4[0].address, "192.0.2.3:0".parse().unwrap());
            let v6 = parse(&fixture(true), &cancel).unwrap();
            assert_eq!(v6[0].address, "[fe80::1234%7]:0".parse().unwrap());
        }
        #[test]
        fn generated_native_cycles_lengths_and_foreign_pointers_fail_closed() {
            let cancel = AtomicBool::new(false);
            let mut buffer = fixture(false);
            let adapter = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            unsafe {
                (*adapter).Next = adapter;
            }
            assert!(parse(&buffer, &cancel).is_err());
            let mut buffer = fixture(false);
            let adapter = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            unsafe {
                (*adapter).Anonymous1.Anonymous.Length = 8;
            }
            assert!(parse(&buffer, &cancel).is_err());
            let mut buffer = fixture(false);
            let adapter = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            unsafe {
                (*adapter).FriendlyName = 1usize as *mut u16;
            }
            assert!(parse(&buffer, &cancel).is_err());
            let mut buffer = fixture(false);
            let adapter = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            unsafe {
                (*(*adapter).FirstUnicastAddress).Address.iSockaddrLength = 1;
            }
            assert!(parse(&buffer, &cancel).is_err());
        }
        #[test]
        fn cancelled_native_parse_returns_no_receipt() {
            assert_eq!(
                parse(&fixture(false), &AtomicBool::new(true)).unwrap_err(),
                CANCELLED
            );
        }
    }

    pub(super) fn copy(owner: &bevy::window::RawHandleWrapper, text: &str) -> Result<(), String> {
        let raw_window_handle::RawWindowHandle::Win32(window) = owner.get_window_handle() else {
            return Err("This window cannot copy addresses.".into());
        };
        let hwnd = window.hwnd.get() as *mut std::ffi::c_void;
        let mut process = 0;
        // The borrowed RawHandleWrapper holds Bevy's owned window Arc alive
        // throughout this synchronous operation. Reject stale/foreign HWNDs
        // and worker-thread calls rather than using an unowned clipboard owner.
        unsafe {
            let thread = GetWindowThreadProcessId(hwnd, &mut process);
            if IsWindow(hwnd) == 0
                || process != std::process::id()
                || thread != GetCurrentThreadId()
            {
                return Err("Copy the address from the game window.".into());
            }
            let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
            if text.len() > 256 {
                return Err("This address is too long to copy.".into());
            }
            let mut memory = Memory(GlobalAlloc(GMEM_MOVEABLE, text.len() * size_of::<u16>()));
            if memory.0.is_null() {
                return Err("Could not prepare the address for copying.".into());
            }
            let pointer = GlobalLock(memory.0).cast::<u16>();
            if pointer.is_null() {
                return Err("Could not prepare the address for copying.".into());
            }
            ptr::copy_nonoverlapping(text.as_ptr(), pointer, text.len());
            GlobalUnlock(memory.0);
            if OpenClipboard(hwnd) == 0 {
                return Err("The clipboard is busy. Try Copy again.".into());
            }
            let _open = Open;
            // CF_UNICODETEXT=13 (public Win32 clipboard format). Unlike the
            // read-only paste path, OpenClipboard must have an owned HWND.
            if EmptyClipboard() == 0 || SetClipboardData(13, memory.0).is_null() {
                return Err("Could not copy the address. Try Copy again.".into());
            }
            memory.0 = ptr::null_mut(); // Ownership transferred to Windows.
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Condvar, Mutex, atomic::AtomicUsize},
        time::{Duration, Instant},
    };

    fn interface(address: &str, name: &str) -> Interface {
        Interface {
            address: address.parse().unwrap(),
            name: name.into(),
            up: true,
            loopback: false,
        }
    }
    #[test]
    fn sharing_owner_is_a_send_sync_resource() {
        fn require<T: Send + Sync>() {}
        require::<Sharing>();
    }
    fn settle(sharing: &mut Sharing) -> Receipt {
        let deadline = Instant::now() + Duration::from_secs(5);
        while sharing.busy() {
            sharing.poll();
            assert!(Instant::now() < deadline, "owned worker did not finish");
            thread::yield_now();
        }
        sharing.receipt()
    }
    fn intent(receipt: &Receipt, index: usize) -> CopyIntent {
        CopyIntent {
            generation: receipt.generation,
            key: receipt.candidates[index].key,
        }
    }
    #[test]
    fn wildcard_port_and_matching_interfaces_are_deterministic() {
        let bound = "0.0.0.0:45321".parse().unwrap();
        let mut down = interface("192.0.2.8:0", "Down");
        down.up = false;
        let mut values = vec![
            interface("192.0.2.4:9", "Zulu"),
            interface("192.0.2.4:0", "Alpha"),
            interface("192.0.2.3:0", "Ethernet"),
            interface("127.0.0.1:0", "Loopback"),
            interface("0.0.0.0:0", "Unspecified"),
            interface("224.0.0.1:0", "Multicast"),
            interface("255.255.255.255:0", "Broadcast"),
            interface("[2001:db8::1]:0", "Other family"),
            down,
        ];
        let result = candidates(bound, values.clone()).unwrap();
        values.reverse();
        assert_eq!(result, candidates(bound, values).unwrap());
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].endpoint, "192.0.2.3:45321".parse().unwrap());
        assert_eq!(result[1].interface.as_deref(), Some("Alpha"));
        assert_eq!(result[1].scope.label(), "On this network");
        assert_eq!(result[2].endpoint, "127.0.0.1:45321".parse().unwrap());
        assert_eq!(result[2].scope.label(), "This PC");
    }
    #[test]
    fn ipv6_matching_excludes_machine_local_scopes_and_round_trips() {
        let values = candidates(
            "[::]:45678".parse().unwrap(),
            vec![
                interface("[fe80::1%7]:0", "Adapter"),
                interface("[fe80::2]:0", "No scope"),
                interface("[2001:db8::1]:0", "Global"),
                interface("[::ffff:192.0.2.1]:0", "Mapped"),
                interface("192.0.2.1:0", "Other family"),
            ],
        )
        .unwrap();
        assert_eq!(values.len(), 2);
        assert!(
            !values
                .iter()
                .any(|value| unshareable_link_local(value.endpoint.ip()))
        );
        for value in &values {
            let literal = value.endpoint.to_string();
            assert_eq!(literal.parse::<SocketAddr>().unwrap(), value.endpoint);
            assert_eq!(
                crate::remote_servers::normalize_address(&literal).unwrap(),
                literal
            );
        }
        assert_eq!(values.last().unwrap().endpoint.to_string(), "[::1]:45678");
    }
    #[test]
    fn interface_and_candidate_limits_fail_without_partial_receipt() {
        let bound = "0.0.0.0:40000".parse().unwrap();
        assert!(
            candidates(
                bound,
                vec![interface("192.0.2.1:0", "Adapter"); MAX_INTERFACES + 1]
            )
            .is_err()
        );
        let values = (1..=MAX_CANDIDATES)
            .map(|n| interface(&format!("192.0.2.{n}:0"), "Adapter"))
            .collect();
        assert!(candidates(bound, values).is_err());
    }
    #[test]
    fn explicit_bind_skips_enumeration_and_invalid_bound_never_copies() {
        let mut sharing = Sharing::new(Arc::new(|_, _| {
            panic!("explicit bind unexpectedly enumerated")
        }));
        sharing.set_host(7, "127.0.0.1:45678".parse().unwrap());
        let receipt = sharing.receipt();
        assert!(!receipt.busy);
        assert_eq!(receipt.candidates[0].scope, Scope::ThisPc);
        let selected = intent(&receipt, 0);
        sharing.set_host(8, "0.0.0.0:0".parse().unwrap());
        assert!(sharing.receipt().candidates.is_empty());
        assert!(sharing.receipt().error.is_some());
        assert!(matches!(
            sharing.copy_with(selected, |_| panic!("invalid bind copied")),
            CopyResult::Failed(_)
        ));
    }
    #[test]
    fn copied_result_requires_success_and_old_ui_never_writes() {
        let mut sharing = Sharing::default();
        sharing.set_host(1, "192.0.2.1:45678".parse().unwrap());
        let receipt = sharing.receipt();
        let selected = intent(&receipt, 0);
        let mut clipboard = "previous content".to_string();
        assert_eq!(
            sharing.copy_with(selected, |_| Err("Clipboard busy".into())),
            CopyResult::Failed("Clipboard busy".into())
        );
        assert_eq!(clipboard, "previous content");
        assert_eq!(
            sharing.copy_with(selected, |text| {
                clipboard = text.into();
                Ok(())
            }),
            CopyResult::Copied("192.0.2.1:45678".into())
        );
        assert_eq!(clipboard, "192.0.2.1:45678");
        assert_eq!(
            crate::remote_servers::normalize_address(&clipboard).unwrap(),
            clipboard
        );
        assert_eq!(
            clipboard.parse::<SocketAddr>().unwrap(),
            receipt.candidates[0].endpoint
        );
        sharing.set_host(2, "192.0.2.1:45678".parse().unwrap());
        assert!(matches!(
            sharing.copy_with(selected, |_| panic!("old generation wrote")),
            CopyResult::Failed(_)
        ));
        let current = intent(&sharing.receipt(), 0);
        sharing.clear();
        assert!(matches!(
            sharing.copy_with(current, |_| panic!("Home wrote")),
            CopyResult::Failed(_)
        ));
    }
    #[test]
    fn repeated_same_host_does_not_invalidate_but_refresh_does() {
        let mut sharing = Sharing::default();
        let bound = "127.0.0.1:40000".parse().unwrap();
        sharing.set_host(1, bound);
        let receipt = sharing.receipt();
        sharing.set_host(1, bound);
        assert_eq!(sharing.receipt().generation, receipt.generation);
        sharing.refresh();
        assert!(sharing.receipt().generation > receipt.generation);
        assert!(matches!(
            sharing.copy_with(intent(&receipt, 0), |_| panic!("old refresh wrote")),
            CopyResult::Failed(_)
        ));
    }

    struct Gate {
        entered: mpsc::Receiver<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }
    impl Gate {
        fn release(&self) {
            *self.release.0.lock().unwrap() = true;
            self.release.1.notify_all();
        }
    }
    fn blocked() -> (Sharing, Gate, Arc<AtomicUsize>) {
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let gate = release.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let (send, entered) = mpsc::channel();
        let sharing = Sharing::new(Arc::new(move |_, _| {
            let call = observed.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                send.send(()).unwrap();
                let mut open = gate.0.lock().unwrap();
                while !*open {
                    open = gate.1.wait(open).unwrap();
                }
            }
            // This fake deliberately returns data after cancellation, as a
            // non-interruptible OS call can; the owner must discard it.
            Ok(vec![interface(
                if call == 0 {
                    "192.0.2.1:0"
                } else {
                    "192.0.2.2:0"
                },
                "Adapter",
            )])
        }));
        (sharing, Gate { entered, release }, calls)
    }
    #[test]
    fn replacement_drops_late_result_and_starts_only_latest_request() {
        let (mut sharing, gate, calls) = blocked();
        sharing.set_host(1, "0.0.0.0:40001".parse().unwrap());
        gate.entered.recv_timeout(Duration::from_secs(5)).unwrap();
        sharing.set_host(2, "0.0.0.0:40002".parse().unwrap());
        sharing.set_host(3, "0.0.0.0:40003".parse().unwrap());
        assert!(sharing.busy());
        assert!(sharing.receipt().candidates.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        gate.release();
        let receipt = settle(&mut sharing);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(receipt.host_generation, Some(3));
        assert_eq!(
            receipt.candidates[0].endpoint,
            "192.0.2.2:40003".parse().unwrap()
        );
        assert!(receipt.error.is_none());
    }
    #[test]
    fn home_invalidates_immediately_and_retains_owned_busy_until_return() {
        let (mut sharing, gate, calls) = blocked();
        sharing.set_host(1, "0.0.0.0:40001".parse().unwrap());
        gate.entered.recv_timeout(Duration::from_secs(5)).unwrap();
        sharing.clear();
        assert!(sharing.busy());
        assert!(sharing.receipt().host_generation.is_none());
        assert!(sharing.receipt().candidates.is_empty());
        gate.release();
        let receipt = settle(&mut sharing);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(receipt.candidates.is_empty());
        assert!(receipt.error.is_none());
    }
    #[test]
    fn panicked_worker_and_oversized_error_are_terminal_and_bounded() {
        let mut sharing = Sharing::new(Arc::new(|_, _| panic!("owned fixture panic")));
        sharing.set_host(1, "0.0.0.0:40001".parse().unwrap());
        assert!(settle(&mut sharing).error.is_some());
        let mut sharing = Sharing::new(Arc::new(|_, _| Err("bad\n".repeat(300))));
        sharing.set_host(1, "0.0.0.0:40001".parse().unwrap());
        let error = settle(&mut sharing).error.unwrap();
        assert_eq!(error.chars().count(), 256);
        assert!(!error.contains('\n'));
    }
}
