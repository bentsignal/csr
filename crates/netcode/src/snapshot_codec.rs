//! Lossless transport frames. Baselines are exact canonical WorldState bytes,
//! never predicted worlds. This module does not admit decoded gameplay state.
use crate::protocol::{MAX_MESSAGE, MAX_PROTOCOL_COUNTER};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

pub(crate) const CACHE_FRAMES: usize = 32;
pub(crate) const CACHE_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_BODY: usize = MAX_MESSAGE - 64;
const MAX_SPANS: usize = 8192;
const HEADER: usize = 37;
const DELTA_HEADER: usize = HEADER + 16;
const MAX_PATCHES_PER_FRAME: usize = 8;
const FULL: u8 = 0;
const DELTA: u8 = 1;

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub id: u64,
    pub tick: u64,
    pub hash: u64,
    pub bytes: Arc<[u8]>,
}
impl Frame {
    fn new(id: u64, tick: u64, bytes: Vec<u8>) -> Result<Self, String> {
        if id == 0
            || id > MAX_PROTOCOL_COUNTER
            || tick > MAX_PROTOCOL_COUNTER
            || bytes.is_empty()
            || bytes.len() > MAX_BODY
        {
            return Err("snapshot frame bounds exceeded".into());
        }
        Ok(Self {
            id,
            tick,
            hash: sim::state::fnv1a(&bytes),
            bytes: bytes.into(),
        })
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    frames: VecDeque<Frame>,
    bytes: usize,
}
impl Cache {
    pub fn get(&self, id: u64) -> Option<&Frame> {
        self.frames.iter().find(|f| f.id == id)
    }
    pub fn latest(&self) -> Option<&Frame> {
        self.frames.back()
    }
    /// Caller has validated both the envelope and decoded ownership/state.
    pub fn insert(&mut self, frame: Frame) {
        debug_assert!(self.latest().is_none_or(|last| frame.id > last.id));
        self.bytes += frame.bytes.len();
        self.frames.push_back(frame);
        while self.frames.len() > CACHE_FRAMES || self.bytes > CACHE_BYTES {
            self.bytes -= self.frames.pop_front().unwrap().bytes.len();
        }
    }
}

#[derive(Default)]
struct Peer {
    sent: VecDeque<(u64, u64)>,
    confirmed: Option<u64>,
    full: bool,
}
/// One shared byte history across all peers. No world/collision clones live here.
#[derive(Default)]
pub(crate) struct Sender {
    pub cache: Cache,
    peers: HashMap<u64, Peer>,
    next_id: u64,
    // At most eight different baseline scans per emitted frame; other peers
    // receive a full body. Results are shared by peers acknowledging that base.
    patches: HashMap<u64, Option<Vec<u8>>>,
}
impl Sender {
    pub fn capture(&mut self, tick: u64, bytes: Vec<u8>) -> Result<(), String> {
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("snapshot frame counter exhausted")?;
        let frame = Frame::new(id, tick, bytes)?;
        self.cache.insert(frame);
        self.next_id = id;
        self.patches.clear();
        Ok(())
    }
    pub fn remove(&mut self, peer: u64) {
        self.peers.remove(&peer);
    }
    pub fn receipt(&mut self, peer: u64, id: u64, hash: u64) -> Result<(), String> {
        if id == 0 || id > MAX_PROTOCOL_COUNTER {
            return Err("invalid snapshot receipt counter".into());
        }
        let p = self
            .peers
            .get_mut(&peer)
            .ok_or("snapshot receipt before admission")?;
        // Late receipts can outlive cache/sent history. They do not resurrect it.
        if p.confirmed.is_some_and(|latest| id <= latest) {
            return Ok(());
        }
        if !p.sent.iter().any(|sent| *sent == (id, hash)) {
            if p.sent.front().is_some_and(|first| id < first.0) {
                return Ok(());
            }
            return Err("snapshot receipt was not sent to this peer".into());
        }
        p.confirmed = self.cache.get(id).filter(|f| f.hash == hash).map(|_| id);
        Ok(())
    }
    pub fn recover(&mut self, peer: u64) -> Result<(), String> {
        let p = self
            .peers
            .get_mut(&peer)
            .ok_or("snapshot recovery before admission")?;
        p.confirmed = None;
        p.full = true;
        Ok(())
    }
    pub fn message(&mut self, peer: u64, acknowledged_input: u64) -> Result<Vec<u8>, String> {
        let target = self.cache.latest().ok_or("no captured snapshot")?;
        let p = self.peers.entry(peer).or_default();
        let base_id = if p.full || target.id % CACHE_FRAMES as u64 == 0 {
            None
        } else {
            p.confirmed
        };
        let base = base_id
            .and_then(|id| self.cache.get(id))
            .filter(|base| base.id < target.id);
        if let Some(base) = base {
            if !self.patches.contains_key(&base.id) && self.patches.len() < MAX_PATCHES_PER_FRAME {
                self.patches
                    .insert(base.id, make_patch(&base.bytes, &target.bytes));
            }
        }
        let patch = base.and_then(|base| self.patches.get(&base.id).and_then(Option::as_deref));
        // Renet accounts queued payload lengths. Do not retain a full-body
        // allocation in a much smaller queued delta's Vec/Bytes backing store.
        let size = patch.map_or(HEADER + target.bytes.len(), |patch| {
            DELTA_HEADER + patch.len()
        });
        let mut result = Vec::with_capacity(size);
        result.push(if patch.is_some() { DELTA } else { FULL });
        for field in [target.id, target.tick, target.hash, acknowledged_input] {
            result.extend(field.to_le_bytes());
        }
        result.extend((target.bytes.len() as u32).to_le_bytes());
        if let Some(patch) = patch {
            let base = base.unwrap();
            result.extend(base.id.to_le_bytes());
            result.extend(base.hash.to_le_bytes());
            result.extend(patch);
        } else {
            result.extend(&*target.bytes);
        }
        Ok(result)
    }
    /// Only record frames actually admitted to the transport send queue.
    pub fn sent(&mut self, peer: u64) {
        let f = self.cache.latest().unwrap();
        let p = self.peers.get_mut(&peer).unwrap();
        p.full = false;
        p.sent.push_back((f.id, f.hash));
        while p.sent.len() > CACHE_FRAMES {
            p.sent.pop_front();
        }
    }
    #[cfg(test)]
    pub fn confirmed(&self, peer: u64) -> Option<u64> {
        self.peers.get(&peer).and_then(|p| p.confirmed)
    }
}

/// Build ordered literal spans; unchanged gaps copy from the baseline. Joining
/// short equal runs avoids excessive span overhead. Scan and allocation are
/// bounded by MAX_BODY; an expensive/larger patch falls back to full bytes.
fn make_patch(base: &[u8], target: &[u8]) -> Option<Vec<u8>> {
    let mut patch = Vec::new();
    let mut cursor = 0;
    let mut previous_end = 0;
    let mut spans = 0;
    while cursor < target.len() {
        if target[cursor] == base.get(cursor).copied().unwrap_or(0) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        let mut last_difference = cursor;
        cursor += 1;
        while cursor < target.len() {
            if target[cursor] != base.get(cursor).copied().unwrap_or(0) {
                last_difference = cursor;
            } else if cursor - last_difference >= 6 {
                break;
            }
            cursor += 1;
        }
        let end = last_difference + 1;
        spans += 1;
        if spans > MAX_SPANS {
            return None;
        }
        write_varint(&mut patch, (start - previous_end) as u32);
        write_varint(&mut patch, (end - start) as u32);
        patch.extend(&target[start..end]);
        previous_end = end;
        cursor = end;
        if DELTA_HEADER + patch.len() >= HEADER + target.len() {
            return None;
        }
    }
    (DELTA_HEADER + patch.len() < HEADER + target.len()).then_some(patch)
}
fn write_varint(out: &mut Vec<u8>, mut value: u32) {
    while value >= 128 {
        out.push(value as u8 | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn read_varint(bytes: &mut &[u8]) -> Result<u32, String> {
    let mut value = 0_u32;
    for i in 0..5 {
        let byte = *bytes.first().ok_or("truncated patch integer")?;
        *bytes = &bytes[1..];
        if i == 4 && byte > 15 {
            return Err("patch integer overflow".into());
        }
        value |= u32::from(byte & 127) << (i * 7);
        if byte & 128 == 0 {
            if i > 0 && byte == 0 {
                return Err("noncanonical patch integer".into());
            }
            return Ok(value);
        }
    }
    Err("patch integer overflow".into())
}
fn spans(
    mut patch: &[u8],
    length: usize,
    mut apply: impl FnMut(usize, &[u8]),
) -> Result<(), String> {
    let mut end: usize = 0;
    let mut count = 0;
    while !patch.is_empty() {
        count += 1;
        if count > MAX_SPANS {
            return Err("too many snapshot patch spans".into());
        }
        let skip = read_varint(&mut patch)? as usize;
        let literal = read_varint(&mut patch)? as usize;
        let start = end.checked_add(skip).ok_or("patch offset overflow")?;
        end = start.checked_add(literal).ok_or("patch length overflow")?;
        if literal == 0 || end > length || literal > patch.len() {
            return Err("snapshot patch span exceeds bounds".into());
        }
        apply(start, &patch[..literal]);
        patch = &patch[literal..];
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) enum Decoded {
    Frame(Frame, u64),
    Missing,
    Stale,
}
/// Parsing borrows packet bytes; no untrusted span vector/count allocation.
/// Nothing mutates cache here, including successful reconstruction.
pub(crate) fn decode(bytes: &[u8], cache: &Cache) -> Result<Decoded, String> {
    if bytes.len() < HEADER || bytes.len() > MAX_MESSAGE {
        return Err("snapshot envelope bounds exceeded".into());
    }
    let u64_at = |at| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let id = u64_at(1);
    let tick = u64_at(9);
    let hash = u64_at(17);
    let ack = u64_at(25);
    let length = u32::from_le_bytes(bytes[33..37].try_into().unwrap()) as usize;
    if id == 0
        || id > MAX_PROTOCOL_COUNTER
        || tick > MAX_PROTOCOL_COUNTER
        || ack > MAX_PROTOCOL_COUNTER
        || length == 0
        || length > MAX_BODY
    {
        return Err("invalid snapshot envelope counters/length".into());
    }
    if !matches!(bytes[0], FULL | DELTA) {
        return Err("unknown snapshot frame kind".into());
    }
    if cache.latest().is_some_and(|last| id <= last.id) {
        return Ok(Decoded::Stale);
    }
    if cache.latest().is_some_and(|last| tick < last.tick) {
        return Err("snapshot timeline regressed".into());
    }
    let body = match bytes[0] {
        FULL => {
            if bytes.len() != HEADER + length {
                return Err("full snapshot length mismatch".into());
            }
            bytes[HEADER..].to_vec()
        }
        DELTA => {
            if bytes.len() < DELTA_HEADER {
                return Err("truncated snapshot baseline".into());
            }
            let base_id = u64_at(37);
            let base_hash = u64_at(45);
            let patch = &bytes[DELTA_HEADER..];
            if base_id == 0 || base_id >= id || base_id > MAX_PROTOCOL_COUNTER {
                return Err("invalid snapshot baseline counter".into());
            }
            // Validate all spans before looking up a baseline or allocating output.
            spans(patch, length, |_, _| {})?;
            let Some(base) = cache.get(base_id).filter(|base| base.hash == base_hash) else {
                return Ok(Decoded::Missing);
            };
            let mut body = base.bytes.to_vec();
            body.resize(length, 0);
            spans(patch, length, |start, literal| {
                body[start..start + literal.len()].copy_from_slice(literal)
            })?;
            body
        }
        _ => unreachable!(),
    };
    if sim::state::fnv1a(&body) != hash {
        return Err("snapshot fingerprint mismatch".into());
    }
    Ok(Decoded::Frame(
        Frame {
            id,
            tick,
            hash,
            bytes: body.into(),
        },
        ack,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_float_bits_growth_shrink_and_eviction() {
        let mut sender = Sender::default();
        let mut receiver = Cache::default();
        let mut bytes = vec![42; 4096];
        bytes[..4].copy_from_slice(&(-0.0_f32).to_le_bytes());
        sender.capture(0, bytes.clone()).unwrap();
        let packet = sender.message(7, 0).unwrap();
        sender.sent(7);
        let Decoded::Frame(frame, ack) = decode(&packet, &receiver).unwrap() else {
            panic!()
        };
        assert_eq!(&*frame.bytes, bytes);
        assert_eq!(ack, 0);
        sender.receipt(7, frame.id, frame.hash).unwrap();
        receiver.insert(frame);
        for id in 2..=40 {
            bytes[17] = id as u8;
            if id == 3 {
                bytes.extend([0, 1, 0]);
            }
            if id == 4 {
                bytes.truncate(4000);
            }
            sender.capture(0, bytes.clone()).unwrap(); // Frozen world tick, unique frame IDs.
            let packet = sender.message(7, id).unwrap();
            sender.sent(7);
            if id != 32 {
                assert_eq!(packet[0], DELTA);
            }
            let Decoded::Frame(frame, _) = decode(&packet, &receiver).unwrap() else {
                panic!()
            };
            assert_eq!(&*frame.bytes, bytes);
            sender.receipt(7, frame.id, frame.hash).unwrap();
            receiver.insert(frame);
        }
        assert_eq!(receiver.frames.len(), CACHE_FRAMES);
        assert!(receiver.get(1).is_none());
        assert!(receiver.bytes <= CACHE_BYTES);
        assert_eq!(sender.cache.frames.len(), CACHE_FRAMES);
    }
    #[test]
    fn receipt_loss_missing_baseline_and_explicit_full_recovery() {
        let mut sender = Sender::default();
        let mut cache = Cache::default();
        sender.capture(1, vec![3; 4096]).unwrap();
        let packet = sender.message(7, 0).unwrap();
        sender.sent(7);
        let Decoded::Frame(frame, _) = decode(&packet, &cache).unwrap() else {
            panic!()
        };
        cache.insert(frame.clone()); // Receipt lost: next packet remains independently full.
        sender.capture(2, vec![3; 4096]).unwrap();
        assert_eq!(sender.message(7, 0).unwrap()[0], FULL);
        sender.sent(7);
        sender.receipt(7, frame.id, frame.hash).unwrap();
        sender.capture(3, vec![3; 4096]).unwrap();
        let delta = sender.message(7, 0).unwrap();
        sender.sent(7);
        assert!(matches!(
            decode(&delta, &Cache::default()).unwrap(),
            Decoded::Missing
        ));
        sender.recover(7).unwrap();
        let recovery = sender.message(7, 0).unwrap();
        assert_eq!(recovery[0], FULL);
        assert!(matches!(
            decode(&recovery, &Cache::default()).unwrap(),
            Decoded::Frame(..)
        ));
        assert!(sender.receipt(9, frame.id, frame.hash).is_err());
        assert!(sender.receipt(7, 100, frame.hash).is_err());
    }
    #[test]
    fn shared_history_and_bounded_distinct_patch_work() {
        let mut sender = Sender::default();
        for id in 1..=10 {
            sender.capture(id, vec![3; 4096]).unwrap();
            let _ = sender.message(id, 0).unwrap();
            sender.sent(id);
            let f = sender.cache.latest().unwrap().clone();
            sender.receipt(id, f.id, f.hash).unwrap();
        }
        sender.capture(11, vec![3; 4096]).unwrap();
        for id in 1..=10 {
            let packet = sender.message(id, 0).unwrap();
            if id <= 8 {
                assert_eq!(packet[0], DELTA);
            } else {
                assert_eq!(packet[0], FULL);
            }
        }
        assert_eq!(sender.patches.len(), MAX_PATCHES_PER_FRAME);
        assert_eq!(sender.cache.frames.len(), 11); // Not eleven copies per peer.
    }
    #[test]
    fn malformed_patch_is_rejected_before_baseline_lookup_or_cache_mutation() {
        let mut sender = Sender::default();
        sender.capture(1, vec![3; 256]).unwrap();
        let _ = sender.message(7, 0).unwrap();
        sender.sent(7);
        let f = sender.cache.latest().unwrap().clone();
        sender.receipt(7, f.id, f.hash).unwrap();
        sender.capture(2, vec![3; 256]).unwrap();
        let packet = sender.message(7, 0).unwrap();
        for payload in [
            vec![0, 0],
            vec![0xff; 5],
            vec![0, 3, 1],
            vec![0x80, 0, 1, 1],
            vec![0xff, 3, 1, 1],
        ] {
            let mut bad = packet[..DELTA_HEADER].to_vec();
            bad.extend(payload);
            assert!(decode(&bad, &Cache::default()).is_err());
        }
        let mut bad = packet.clone();
        bad[33..37].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&bad, &Cache::default()).is_err());
        let mut bad = packet.clone();
        bad[1..9].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode(&bad, &Cache::default()).is_err());
        let mut bad = packet;
        bad[37..45].copy_from_slice(&2_u64.to_le_bytes());
        assert!(decode(&bad, &Cache::default()).is_err());
    }
    #[test]
    fn expired_confirmed_baseline_and_current_frame_are_independently_full() {
        let mut sender = Sender::default();
        sender.capture(0, vec![3; 4096]).unwrap();
        let _ = sender.message(7, 0).unwrap();
        sender.sent(7);
        let first = sender.cache.latest().unwrap().clone();
        sender.receipt(7, first.id, first.hash).unwrap();
        // Reusing a captured frame must never emit a self-reference.
        assert_eq!(sender.message(7, 0).unwrap()[0], FULL);
        for tick in 1..=40 {
            sender.capture(tick, vec![3; 4096]).unwrap();
            let _ = sender.message(7, 0).unwrap();
            sender.sent(7);
        }
        assert!(sender.cache.get(first.id).is_none());
        let full = sender.message(7, 0).unwrap();
        assert_eq!(full[0], FULL);
        assert!(matches!(
            decode(&full, &Cache::default()).unwrap(),
            Decoded::Frame(..)
        ));
        sender.receipt(7, first.id, first.hash).unwrap(); // Late old ACK cannot resurrect it.
        assert_eq!(sender.message(7, 0).unwrap()[0], FULL);
    }
    #[test]
    fn span_count_message_length_and_fingerprint_bounds_are_enforced() {
        let mut sender = Sender::default();
        sender.capture(1, vec![3; 256]).unwrap();
        let full = sender.message(7, 0).unwrap();
        sender.sent(7);
        let first = sender.cache.latest().unwrap().clone();
        sender.receipt(7, first.id, first.hash).unwrap();
        sender.capture(2, vec![3; 256]).unwrap();
        let delta = sender.message(7, 0).unwrap();
        let mut bad = delta[..DELTA_HEADER].to_vec();
        bad[33..37].copy_from_slice(&(MAX_BODY as u32).to_le_bytes());
        for _ in 0..=MAX_SPANS {
            bad.extend([0, 1, 9]);
        }
        assert!(
            decode(&bad, &Cache::default())
                .unwrap_err()
                .contains("too many")
        );
        let mut bad = full.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(decode(&bad, &Cache::default()).is_err());
        let mut bad = full;
        bad.push(0);
        assert!(decode(&bad, &Cache::default()).is_err());
        assert!(Frame::new(1, 0, vec![0; MAX_BODY + 1]).is_err());
    }
}
