//! Multiplayer for CSGO Rewrite.
//!
//! The server is authoritative: it alone steps the real [`sim::World`].
//! Clients send [`sim::TickInput`]s and draw what the server sends back,
//! predicting their own player with the same `sim` code so their input
//! feels immediate. See `docs/ARCHITECTURE.md` (Networking) for the design.
//!
//! * [`protocol`]: messages, channels, versioning.
//! * [`delta`]: snapshot delta coding.
//! * [`server`]: the dedicated (or listen) server loop.
//! * [`client`]: the connection thread, prediction, reconciliation and
//!   interpolation.
//!
//! Transport: [renet](https://github.com/lucaspoffo/renet) channels over
//! netcode UDP (unreliable for inputs and snapshots, reliable ordered for
//! joins, events and chat). Nothing here depends on Bevy, so the server
//! runs without a window or GPU.

pub mod client;
pub mod delta;
pub mod protocol;
pub mod server;

pub use client::{Connection, Incoming, Predictor};
pub use protocol::{ClientMessage, ServerMessage, DEFAULT_PORT};
pub use server::{Server, ServerConfig};

#[cfg(test)]
mod tests;
