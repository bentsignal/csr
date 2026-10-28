//! Process boundary for explicitly installed, application-shipped simulation
//! providers. Packs supply data; they cannot choose a program or execute code.
//! Geometry and poses use Source axes/inches. Controller reports use XInput.
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
pub const VERSION: u32 = 4;
pub const MAX_MESSAGE: usize = 64 << 20;
/// Provider audio sample rate (interleaved stereo frames per second).
pub const AUDIO_RATE: u32 = 48_000;
pub type Matrix = [[f32; 4]; 4];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Controller {
    pub connected: bool,
    pub buttons: u16,
    pub triggers: [u8; 2],
    pub left: [i16; 2],
    pub right: [i16; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Triangle {
    pub points: [[f32; 3]; 3],
    pub surface: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Door {
    pub origin: [f32; 3],
    pub triangles: Vec<Triangle>,
    pub parts: Vec<Vec<[f32; 3]>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Breakable {
    pub killed_by: Vec<usize>,
    pub triangles: Vec<Triangle>,
    pub parts: Vec<Vec<[f32; 3]>>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub triangles: Vec<Triangle>,
    pub doors: Vec<Door>,
    pub breakables: Vec<Breakable>,
    pub rails: Vec<Vec<[f32; 3]>>,
    /// Host rolling-audio family per collision surface id (`Triangle::surface`;
    /// Source surfaceprop resolved to `A`, `W`, `M`, `R` or its gamematerial).
    /// Ids past the end use the provider default.
    pub surfaces: Vec<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneState {
    /// Contacted host rolling-audio family (resolved from Source surfaceprops).
    pub ground_material: u8,
    pub doors: Vec<f32>,
    pub broken: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Start {
    pub grounded:bool,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub yaw: f32,
    pub checkpoint: Option<Checkpoint>,
}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
pub struct Checkpoint {
    pub transform:Matrix,
    pub on_board:bool,
    pub foot_forward:bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub input: Controller,
    pub dt: f32,
    pub scene: SceneState,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MarkerUi {
    pub visible:bool,
    pub can_place:bool,
    pub can_return:bool,
    pub progress:f32,
    pub event:u64,
    /// 1=open, 2=placed, 3=return accepted.
    pub action:u8,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub marker:MarkerUi,
    pub line_score:i32,
    pub multiplier:f32,
    pub checkpoint: Option<Checkpoint>,
    pub generation: u64,
    pub phase: f32,
    pub period: f32,
    pub discontinuity: bool,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub grounded: bool,
    /// Completed provider-space world matrices. Rig::to_source converts units
    /// and axes; bone_basis converts an imported mesh's authored skin basis.
    pub bones: Vec<Matrix>,
    pub state: u32,
    pub camera: Option<Matrix>,
    pub fov: f32,
    pub trick: String,
    pub score: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    pub names: Vec<String>,
    pub parents: Vec<i32>,
    pub rest: Vec<Matrix>,
    pub to_source: Matrix,
    pub bone_basis: Matrix,
}
#[derive(Serialize, Deserialize)]
pub enum Request {
    Initialize {
        version: u32,
        assets: String,
        scene: Scene,
    },
    /// History is sent only to reconstruct a missing derived cache. Every
    /// authoritative input remains in the host's cloneable snapshot journal.
    Restore {
        session: u64,
        start: Start,
        frames: Vec<Frame>,
    },
    Advance {
        session: u64,
        at: u64,
        frame: Frame,
    },
}
#[derive(Serialize, Deserialize)]
pub enum Response {
    Ready(Rig),
    /// Cosmetic board audio rendered since the last pose: interleaved stereo
    /// (left, right) at [`AUDIO_RATE`], never part of simulation state.
    Pose(Pose, Vec<f32>),
    NeedRestore,
    Error(String),
}
pub fn send<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), String> {
    let bytes = postcard::to_allocvec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MESSAGE {
        return Err("mechanics message exceeds limit".into());
    }
    writer
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .and_then(|()| writer.write_all(&bytes))
        .and_then(|()| writer.flush())
        .map_err(|e| e.to_string())
}
pub fn receive<T: for<'de> Deserialize<'de>>(reader: &mut impl Read) -> Result<T, String> {
    let mut size = [0; 4];
    reader.read_exact(&mut size).map_err(|e| e.to_string())?;
    let size = u32::from_le_bytes(size) as usize;
    if size > MAX_MESSAGE {
        return Err("mechanics message exceeds limit".into());
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    postcard::from_bytes(&bytes).map_err(|e| e.to_string())
}
