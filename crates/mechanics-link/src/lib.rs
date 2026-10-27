//! Process boundary for explicitly installed, application-shipped simulation
//! providers. Packs supply data; they cannot choose a program or execute code.
//! Geometry and poses use Source axes/inches. Controller reports use XInput.
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
pub const VERSION: u32 = 1;
pub const MAX_MESSAGE: usize = 64 << 20;
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
pub struct Triangle { pub points: [[f32; 3]; 3], pub surface: u32 }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Door { pub origin: [f32; 3], pub triangles: Vec<Triangle> }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Breakable { pub killed_by: Vec<usize>, pub triangles: Vec<Triangle> }
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub triangles: Vec<Triangle>, pub doors: Vec<Door>, pub breakables: Vec<Breakable>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneState { pub doors: Vec<f32>, pub broken: Vec<bool> }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Start { pub position: [f32; 3], pub velocity: [f32; 3], pub yaw: f32 }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame { pub input: Controller, pub dt: f32, pub scene: SceneState }
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub generation: u64,
    pub position: [f32; 3], pub velocity: [f32; 3], pub grounded: bool,
    /// Completed provider-space world matrices. Rig::to_source converts units
    /// and axes; bone_basis converts an imported mesh's authored skin basis.
    pub bones: Vec<Matrix>,
    pub state: u32, pub camera: Option<Matrix>, pub fov: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rig { pub names: Vec<String>, pub parents: Vec<i32>, pub rest: Vec<Matrix>,
    pub to_source: Matrix, pub bone_basis: Matrix }
#[derive(Serialize, Deserialize)]
pub enum Request {
    Initialize { version: u32, assets: String, scene: Scene },
    /// History is sent only to reconstruct a missing derived cache. Every
    /// authoritative input remains in the host's cloneable snapshot journal.
    Restore { session: u64, start: Start, frames: Vec<Frame> },
    Advance { session: u64, at: u64, frame: Frame },
}
#[derive(Serialize, Deserialize)]
pub enum Response { Ready(Rig), Pose(Pose), NeedRestore, Error(String) }
pub fn send<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), String> {
    let bytes = postcard::to_allocvec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MESSAGE { return Err("mechanics message exceeds limit".into()); }
    writer.write_all(&(bytes.len() as u32).to_le_bytes()).and_then(|()| writer.write_all(&bytes))
        .and_then(|()| writer.flush()).map_err(|e| e.to_string())
}
pub fn receive<T: for<'de> Deserialize<'de>>(reader: &mut impl Read) -> Result<T,String> {
    let mut size=[0;4]; reader.read_exact(&mut size).map_err(|e|e.to_string())?;
    let size=u32::from_le_bytes(size) as usize;
    if size > MAX_MESSAGE { return Err("mechanics message exceeds limit".into()); }
    let mut bytes=vec![0;size]; reader.read_exact(&mut bytes).map_err(|e|e.to_string())?;
    postcard::from_bytes(&bytes).map_err(|e|e.to_string())
}
