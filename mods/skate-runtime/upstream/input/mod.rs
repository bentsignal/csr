//! Device-independent input publication. The CS host supplies controller values
//! to the same original gesture/coordinator code; no SDL window loop is run.
pub(crate) mod gesture_catalog;
mod gesture_mapping_data;
pub(crate) mod gesture_mapping;
pub(crate) mod gesture_input;
#[derive(bevy::prelude::Resource)]
pub struct PublishedTickInput(pub skate_core::input::tick::TickInput);
