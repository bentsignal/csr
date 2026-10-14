//! LAN endpoint hints only. Admission and all metadata still require Inspect.
//! A separate multicast socket never consumes packets from the game socket.
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::BTreeSet,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 73, 82);
const PORT: u16 = 42690;
const MAGIC: &[u8; 8] = b"CSGRLAN1";
const PACKET_LEN: usize = 14;
const INTERVAL: Duration = Duration::from_secs(1);
const COLLECT_TIME: Duration = Duration::from_millis(2500);
const MAX_PACKETS: usize = 256;
pub const MAX_SERVERS: usize = 32;

fn packet(port: u16) -> [u8; PACKET_LEN] {
    let mut bytes = [0; PACKET_LEN];
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..12].copy_from_slice(&crate::protocol::VERSION.to_le_bytes());
    bytes[12..].copy_from_slice(&port.to_le_bytes());
    bytes
}

fn hint(bytes: &[u8], source: SocketAddr) -> Option<SocketAddr> {
    let IpAddr::V4(ip) = source.ip() else {
        return None;
    };
    if bytes.len() != PACKET_LEN
        || &bytes[..8] != MAGIC
        || u32::from_le_bytes(bytes[8..12].try_into().ok()?) != crate::protocol::VERSION
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip == Ipv4Addr::BROADCAST
    {
        return None;
    }
    let port = u16::from_le_bytes(bytes[12..14].try_into().ok()?);
    (port != 0).then_some(SocketAddr::from((ip, port)))
}

/// Owned by a running authority, with no thread or incoming discovery port.
pub(crate) struct Announcer {
    socket: UdpSocket,
    bytes: [u8; PACKET_LEN],
    next: Duration,
}
impl Announcer {
    pub(crate) fn new(game: SocketAddr) -> io::Result<Option<Self>> {
        let SocketAddr::V4(game) = game else {
            return Ok(None);
        };
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_nonblocking(true)?;
        socket.set_multicast_ttl_v4(1)?;
        socket.set_multicast_loop_v4(true)?;
        if !game.ip().is_unspecified() {
            socket.set_multicast_if_v4(game.ip())?;
        }
        socket.bind(&SocketAddr::from((*game.ip(), 0)).into())?;
        Ok(Some(Self {
            socket: socket.into(),
            bytes: packet(game.port()),
            next: Duration::ZERO,
        }))
    }
    pub(crate) fn update(&mut self, now: Duration) -> io::Result<()> {
        if !self.due(now) {
            return Ok(());
        }
        self.socket
            .send_to(&self.bytes, SocketAddrV4::new(GROUP, PORT))?;
        Ok(())
    }
    fn due(&mut self, now: Duration) -> bool {
        if now < self.next {
            return false;
        }
        // One send even after a stall; elapsed time never creates a burst.
        self.next = now.saturating_add(INTERVAL);
        true
    }
}

fn listener(port: u16, interface: Ipv4Addr) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())?;
    socket.join_multicast_v4(&GROUP, &interface)?;
    socket.set_multicast_loop_v4(true)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}
/// Run inside the browser's one retained worker. Multiple browsers can receive
/// the same multicast stream; SO_REUSEADDR is deliberately set before bind.
pub fn collect(cancel: &AtomicBool) -> io::Result<Vec<SocketAddr>> {
    if cancel.load(Ordering::Acquire) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let socket = listener(PORT, Ipv4Addr::UNSPECIFIED)?;
    let deadline = Instant::now() + COLLECT_TIME;
    let mut endpoints = BTreeSet::new();
    let mut packets = 0;
    // An extra byte rejects oversized packets even when recv truncates them.
    let mut bytes = [0; PACKET_LEN + 1];
    while Instant::now() < deadline && packets < MAX_PACKETS && endpoints.len() < MAX_SERVERS {
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        match socket.recv_from(&mut bytes) {
            Ok((len, source)) => {
                packets += 1;
                if let Some(endpoint) = hint(&bytes[..len], source) {
                    endpoints.insert(endpoint);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            // Winsock reports truncation as WSAEMSGSIZE instead of a short read.
            Err(error) if error.raw_os_error() == Some(10040) => packets += 1,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(endpoints.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hints_cannot_supply_an_address_or_metadata_and_require_exact_version_and_length() {
        let source: SocketAddr = "192.168.2.3:40000".parse().unwrap();
        let bytes = packet(27018);
        assert_eq!(
            hint(&bytes, source),
            Some("192.168.2.3:27018".parse().unwrap())
        );
        for length in 0..PACKET_LEN {
            assert_eq!(hint(&bytes[..length], source), None);
        }
        let mut extra = bytes.to_vec();
        extra.push(0);
        assert_eq!(hint(&extra, source), None);
        let mut incompatible = bytes;
        incompatible[8] ^= 1;
        assert_eq!(hint(&incompatible, source), None);
        assert_eq!(hint(&packet(0), source), None);
        assert_eq!(hint(&bytes, "0.0.0.0:1234".parse().unwrap()), None);
        assert_eq!(hint(&bytes, "239.1.2.3:1234".parse().unwrap()), None);
        assert_eq!(hint(&bytes, "[::1]:1234".parse().unwrap()), None);
    }

    #[test]
    fn two_local_browsers_receive_both_servers_and_release_the_shared_port() {
        let first = listener(0, Ipv4Addr::LOCALHOST).unwrap();
        let port = first.local_addr().unwrap().port();
        let second = listener(port, Ipv4Addr::LOCALHOST).unwrap();
        let mut a = Announcer::new("127.0.0.1:27018".parse().unwrap())
            .unwrap()
            .unwrap();
        let mut b = Announcer::new("127.0.0.1:27019".parse().unwrap())
            .unwrap()
            .unwrap();
        // Use the same multicast sockets and encoding, with an isolated
        // ephemeral receiver port so tests cannot consume production discovery.
        a.socket
            .send_to(&a.bytes, SocketAddrV4::new(GROUP, port))
            .unwrap();
        b.socket
            .send_to(&b.bytes, SocketAddrV4::new(GROUP, port))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        for socket in [&first, &second] {
            let mut seen = BTreeSet::new();
            while seen.len() < 2 && Instant::now() < deadline {
                let mut bytes = [0; PACKET_LEN + 1];
                match socket.recv_from(&mut bytes) {
                    Ok((len, source)) => {
                        if let Some(address) = hint(&bytes[..len], source) {
                            seen.insert(address.port());
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("multicast receive: {error}"),
                }
            }
            assert_eq!(seen, BTreeSet::from([27018, 27019]));
        }
        // Cancel/drop has no retained receiver thread or port reservation.
        drop(first);
        drop(second);
        UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).unwrap();
        assert_eq!(
            collect(&AtomicBool::new(true)).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        // Rate control is independent of IO update frequency and does not
        // create catch-up packets after a wall-clock stall.
        assert!(a.due(Duration::from_secs(10)));
        assert_eq!(a.next, Duration::from_secs(11));
        assert!(!a.due(Duration::from_millis(10001)));
        assert_eq!(a.next, Duration::from_secs(11));
        assert!(b.due(Duration::from_secs(50)));
        assert_eq!(b.next, Duration::from_secs(51));
    }
}
