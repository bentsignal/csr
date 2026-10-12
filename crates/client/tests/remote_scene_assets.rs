// Compile the leaf while root owns production module declarations.
pub use client::{characters, items, material, session, skinning, viewmodel};
#[path = "../src/remote_render_gate.rs"]
mod remote_render_gate;
#[path = "../src/remote_scene_assets.rs"]
mod remote_scene_assets;
