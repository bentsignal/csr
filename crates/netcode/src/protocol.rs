//! Versioned messages: clients select inputs, never player ids or world state.
use serde::{Deserialize, Serialize};
use sim::{GameEvent, PlayerId, Snapshot, Team, TickInput};

pub const VERSION: u32 = 1;
pub const PROTOCOL_ID: u64 = 0x4353_5257_0000_0001;
pub const CONTROL: u8 = 0;
pub const INPUT: u8 = 1;
pub const SNAPSHOT: u8 = 2;
pub const EVENTS: u8 = 3;
pub const MAX_INPUT_BATCH: usize = 8;
pub const MAX_MESSAGE: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentManifest {
    /// A stable ContentId string; the map filename is separate.
    pub map_id: String,
    pub map_name: String,
    /// SHA-1 of the installed BSP bytes (including its embedded content).
    pub map_hash: String,
    /// SHA-1 of the installed weapon definitions used by the sim.
    pub catalog_hash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientControl {
    Hello {
        version: u32,
        name: String,
        team: Team,
        content: ContentManifest,
    },
    Leave,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerControl {
    Welcome {
        version: u32,
        player: PlayerId,
        tick_rate: u32,
        server_tick: u64,
        content: ContentManifest,
    },
    Rejected {
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputCommand {
    /// Strictly increasing, starting at one. Not a requested server tick.
    pub sequence: u64,
    /// Reserved for prediction and lag-compensated view history.
    pub view_tick: u64,
    pub input: TickInput,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputBatch(pub Vec<InputCommand>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateUpdate {
    pub acknowledged_input: u64,
    pub snapshot: Snapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventBatch {
    pub tick: u64,
    pub events: Vec<GameEvent>,
}

pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, String> {
    let bytes = postcard::to_allocvec(message).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MESSAGE {
        return Err("network message exceeds limit".into());
    }
    Ok(bytes)
}

pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    if bytes.len() > MAX_MESSAGE {
        return Err("network message exceeds limit".into());
    }
    postcard::from_bytes(bytes).map_err(|e| e.to_string())
}

pub fn channels() -> renet::ConnectionConfig {
    use renet::{ChannelConfig, SendType};
    let channels = vec![
        ChannelConfig {
            channel_id: CONTROL,
            max_memory_usage_bytes: 16 * 1024,
            send_type: SendType::ReliableOrdered {
                resend_time: std::time::Duration::from_millis(100),
            },
        },
        ChannelConfig {
            channel_id: INPUT,
            max_memory_usage_bytes: 64 * 1024,
            send_type: SendType::Unreliable,
        },
        ChannelConfig {
            channel_id: SNAPSHOT,
            max_memory_usage_bytes: MAX_MESSAGE * 2,
            send_type: SendType::Unreliable,
        },
        ChannelConfig {
            channel_id: EVENTS,
            max_memory_usage_bytes: 128 * 1024,
            send_type: SendType::ReliableOrdered {
                resend_time: std::time::Duration::from_millis(100),
            },
        },
    ];
    renet::ConnectionConfig {
        available_bytes_per_tick: (MAX_MESSAGE + 128 * 1024) as u64,
        client_channels_config: channels.clone(),
        server_channels_config: channels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vr_input_round_trips_without_camera_assumptions() {
        let input = TickInput {
            tracked: Some(sim::TrackedPose::default()),
            aim: Some(sim::AimRay {
                origin: glam::Vec3::new(4., 2., 64.),
                direction: glam::Vec3::X,
            }),
            ..Default::default()
        };
        let batch = InputBatch(vec![InputCommand {
            sequence: 12,
            view_tick: 321,
            input,
        }]);
        assert_eq!(
            decode::<InputBatch>(&encode(&batch).unwrap()).unwrap(),
            batch
        );
    }
    #[test]
    fn oversized_or_invalid_messages_are_rejected() {
        assert!(decode::<InputBatch>(&vec![0; MAX_MESSAGE + 1]).is_err());
        assert!(decode::<InputBatch>(&[255, 255]).is_err());
    }
}
