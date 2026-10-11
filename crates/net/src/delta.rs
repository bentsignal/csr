//! Byte-level delta coding of encoded snapshots.
//!
//! Consecutive game states encode to nearly the same bytes (most players
//! and weapons do not change in 8 ms), so the server sends each snapshot as
//! the XOR with a baseline the client has acknowledged, with the runs of
//! zero bytes that XOR produces collapsed:
//!
//! ```text
//! varint new_len, then repeated: varint zeros, varint literal_len, literal bytes
//! ```
//!
//! Bytes past the end of the baseline are XORed with zero, i.e. sent as is.
//! The coding knows nothing about the state's structure, so it never needs
//! to change when the state does.

fn put_varint(out: &mut Vec<u8>, mut v: usize) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn get_varint(bytes: &[u8], at: &mut usize) -> Option<usize> {
    let mut v = 0usize;
    for shift in (0..64).step_by(7) {
        let b = *bytes.get(*at)?;
        *at += 1;
        v |= ((b & 0x7F) as usize) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// `new` as a delta against `base`.
pub fn encode(base: &[u8], new: &[u8]) -> Vec<u8> {
    let x = |i: usize| new[i] ^ base.get(i).copied().unwrap_or(0);
    let mut out = Vec::with_capacity(64);
    put_varint(&mut out, new.len());
    let mut i = 0;
    while i < new.len() {
        let zeros_start = i;
        while i < new.len() && x(i) == 0 {
            i += 1;
        }
        let literal_start = i;
        // A literal ends at a run of three or more zeros, or zeros up to the
        // end (shorter runs cost more to break out of than to copy).
        while i < new.len() {
            if x(i) != 0 {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < new.len() && x(j) == 0 {
                j += 1;
            }
            if j == new.len() || j - i >= 3 {
                break;
            }
            i = j;
        }
        put_varint(&mut out, literal_start - zeros_start);
        put_varint(&mut out, i - literal_start);
        out.extend((literal_start..i).map(x));
    }
    out
}

/// Rebuilds the new bytes from `base` and a delta from [`encode`].
pub fn decode(base: &[u8], delta: &[u8]) -> Option<Vec<u8>> {
    let mut at = 0;
    let len = get_varint(delta, &mut at)?;
    if len > 64 * 1024 * 1024 {
        return None;
    }
    let mut out = Vec::with_capacity(len);
    let base_at = |i: usize| base.get(i).copied().unwrap_or(0);
    while out.len() < len {
        let zeros = get_varint(delta, &mut at)?;
        let literal = get_varint(delta, &mut at)?;
        if out.len() + zeros + literal > len {
            return None;
        }
        for _ in 0..zeros {
            out.push(base_at(out.len()));
        }
        let bytes = delta.get(at..at + literal)?;
        at += literal;
        for b in bytes {
            out.push(b ^ base_at(out.len()));
        }
        if zeros == 0 && literal == 0 {
            return None;
        }
    }
    (at == delta.len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(base: &[u8], new: &[u8]) -> usize {
        let d = encode(base, new);
        assert_eq!(decode(base, &d).as_deref(), Some(new), "base {base:?} new {new:?}");
        d.len()
    }

    #[test]
    fn identical_states_cost_almost_nothing() {
        let state: Vec<u8> = (0..4000).map(|i| (i * 7 % 251) as u8).collect();
        assert!(round_trip(&state, &state) <= 6);
        let mut moved = state.clone();
        moved[100] ^= 3;
        moved[2000] = 9;
        assert!(round_trip(&state, &moved) < 16);
    }

    #[test]
    fn length_changes_and_edges() {
        round_trip(&[], &[]);
        round_trip(&[], &[1, 2, 3]);
        round_trip(&[1, 2, 3], &[]);
        round_trip(&[1, 2, 3, 4, 5], &[1, 2]);
        round_trip(&[1, 2], &[1, 2, 0, 0, 0, 7]);
        round_trip(&[5; 10], &[5, 5, 0, 5, 5, 5, 5, 0, 0, 5]);
        for n in 0..40u8 {
            let base: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            let new: Vec<u8> = (0..n / 2 + 3).map(|i| if i % 3 == 0 { i } else { base.get(i as usize).copied().unwrap_or(1) }).collect();
            round_trip(&base, &new);
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(&[1, 2, 3], &[]).is_none());
        assert!(decode(&[1, 2, 3], &[3, 0, 9]).is_none());
        assert!(decode(&[1, 2, 3], &[0xFF; 12]).is_none());
        assert!(decode(&[], &[2, 0, 0]).is_none());
    }
}
