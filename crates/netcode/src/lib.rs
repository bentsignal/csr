//! Authoritative fixed-tick multiplayer without a renderer.
//! The same API can be driven by a dedicated server or a menu-owned host.
pub mod prediction;
pub mod protocol;
pub mod session;
pub mod transport;
