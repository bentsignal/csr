//! Versioned, engine-independent mechanics packet. All words are little-endian
//! IEEE f32; positions are Source inches, +Z up; angles are degrees.
//! A module exports memory, csr_packet()->i32 and csr_step()->i32 (zero=OK).
//! Its only optional import is csr_v1.trace(i32)->i32. See docs/MECHANICS.md.
#![no_std]

pub const VERSION: f32 = 1.0;
pub const WORDS: usize = 384;
pub const BYTES: usize = WORDS * 4;
pub type Packet = [f32; WORDS];
pub const API: usize = 0;
pub const DT: usize = 1;
pub const POSITION: usize = 2;
pub const VELOCITY: usize = 5;
pub const GROUNDED: usize = 8;
pub const YAW: usize = 9;
pub const PITCH: usize = 10;
pub const FORWARD: usize = 11;
pub const SIDE: usize = 12;
pub const JUMP: usize = 13;
pub const DUCK: usize = 14;
pub const BRAKE: usize = 15;
pub const RESET: usize = 16;
pub const TIME: usize = 17;
pub const STATE: usize = 20;
pub const STATE_WORDS: usize = 32;
pub const OUT_POSITION: usize = 64;
pub const OUT_VELOCITY: usize = 67;
pub const OUT_GROUNDED: usize = 70;
pub const OUT_YAW: usize = 71;
pub const OUT_PITCH: usize = 72;
pub const OUT_ROLL: usize = 73;
pub const OUT_HEIGHT: usize = 74;
pub const OUT_STATE: usize = 80;
pub const TRACE_WORDS: usize = 21;
pub const CONFIG: usize = 128;
pub const CONFIG_WORDS: usize = 256;

pub fn encode(packet: &Packet) -> [u8; BYTES] {
    let mut bytes = [0; BYTES];
    for (word, value) in bytes.chunks_exact_mut(4).zip(packet) {
        word.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn decode(bytes: &[u8; BYTES]) -> Packet {
    let mut packet = [0.; WORDS];
    for (value, word) in packet.iter_mut().zip(bytes.chunks_exact(4)) {
        *value = f32::from_le_bytes([word[0], word[1], word[2], word[3]]);
    }
    packet
}
