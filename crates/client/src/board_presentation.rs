//! Held equipment pose, separate from the tick-owned board. The viewmodel
//! publishes it each frame from the posed knife handle (see `board_grip`).
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct HeldBoard {
    /// Source-space board-local inches to world, on the viewmodel layer.
    pub transform: Option<Mat4>,
}
