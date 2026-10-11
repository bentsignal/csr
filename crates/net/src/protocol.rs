//! What travels between client and server.
//!
//! Two channels each way over one UDP socket (renet over netcode):
//!
//! | channel | client → server | server → client |
//! |---|---|---|
//! | [`UNRELIABLE`] | [`InputPacket`] every client tick | [`SnapshotPacket`] every server tick |
//! | [`RELIABLE`] (ordered) | [`ClientMessage`]: hello, loadout, chat | [`ServerMessage`]: welcome, the tick's events, chat |
//!
//! Inputs are resent (the last few in every packet) rather than made
//! reliable, so one lost packet costs nothing. Snapshots carry the whole
//! replicated [`sim::WorldState`], delta-coded against the newest snapshot
//! the client has acknowledged ([`crate::delta`]); a lost snapshot is simply
//! superseded by the next one.
//!
//! Everything is serialised with postcard ([`sim::state::encode`]).

use serde::{Deserialize, Serialize};
use sim::{GameEvent, GameMode, Loadout, PlayerId, Team, TickInput};
use std::time::Duration;

/// Netcode protocol id: "CSR" plus the protocol version. Bump the low byte
/// on any incompatible change; mismatched clients cannot connect.
pub const PROTOCOL_ID: u64 = 0x0043_5352_0000_0001;
/// The game version, sent in [`ClientMessage::Hello`] so a server can say
/// why it refuses an outdated client.
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");
/// CS:GO's default port.
pub const DEFAULT_PORT: u16 = 27015;
/// Channel ids.
pub const UNRELIABLE: u8 = 0;
pub const RELIABLE: u8 = 1;
/// Inputs repeated in each [`InputPacket`] (16 ms of input at 128 tick)
/// so a lost packet is covered by the next one.
pub const INPUT_REDUNDANCY: usize = 6;

/// Channel layout shared by both ends.
pub fn connection_config() -> renet::ConnectionConfig {
    let channels = || {
        vec![
            renet::ChannelConfig {
                channel_id: UNRELIABLE,
                max_memory_usage_bytes: 8 * 1024 * 1024,
                send_type: renet::SendType::Unreliable,
            },
            renet::ChannelConfig {
                channel_id: RELIABLE,
                max_memory_usage_bytes: 8 * 1024 * 1024,
                send_type: renet::SendType::ReliableOrdered { resend_time: Duration::from_millis(100) },
            },
        ]
    };
    renet::ConnectionConfig {
        available_bytes_per_tick: 256 * 1024,
        server_channels_config: channels(),
        client_channels_config: channels(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// First message after connecting.
    Hello { name: String, version: String, team: Option<Team>, loadout: Loadout },
    /// The player changed their knife, finishes or practice weapons.
    SetLoadout(Loadout),
    Chat(String),
}

/// The client's newest inputs, one per client tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputPacket {
    /// Newest snapshot tick the client has decoded: the server's delta
    /// baseline. 0 when it has none.
    pub ack_snapshot: u64,
    /// Sequence number of the last entry of `inputs` (the first input of a
    /// session is 1).
    pub last_seq: u32,
    /// The most recent inputs, oldest first.
    pub inputs: Vec<TickInput>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    Welcome { player: PlayerId, map: String, tick_rate: u32, mode: GameMode, server: String },
    /// Everything that happened in one tick, in order.
    Events { tick: u64, events: Vec<GameEvent> },
    Chat { from: String, text: String },
    /// The server will not take this client (wrong version, full).
    Refused(String),
}

/// One server tick for one client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotPacket {
    /// `WorldState::tick` of the state inside.
    pub tick: u64,
    /// Snapshot tick `data` is a delta against; 0 for a full state.
    pub baseline: u64,
    /// Sequence number of this client's newest input the server has
    /// applied (0 for none).
    pub ack_input: u32,
    /// Inputs from this client waiting in the server's queue, for the
    /// client's clock to keep it short but never empty.
    pub queued: u8,
    pub data: Vec<u8>,
}

/// Serialises a message (never fails for these types).
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    sim::state::encode(value)
}

/// Deserialises a message that holds no weapon state.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

/// A display name made safe to show and log.
pub fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(32).collect();
    let name = name.trim();
    if name.is_empty() { "Player".into() } else { name.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let hello = ClientMessage::Hello { name: "a".into(), version: GAME_VERSION.into(), team: Some(Team::Terrorist), loadout: Loadout::default() };
        assert_eq!(decode::<ClientMessage>(&encode(&hello)), Some(hello));
        let input = InputPacket { ack_snapshot: 9, last_seq: 3, inputs: vec![TickInput { forward_move: 450.0, view_tick: Some(7), ..Default::default() }; 3] };
        assert_eq!(decode::<InputPacket>(&encode(&input)), Some(input));
        assert!(decode::<InputPacket>(&[0xFF, 0xFF, 0xFF]).is_none());
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  \u{7}bob\n "), "bob");
        assert_eq!(clean_name(""), "Player");
        assert_eq!(clean_name(&"x".repeat(100)).len(), 32);
    }
}
