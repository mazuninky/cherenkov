//! CRDT-backed [`cherenkov_core::ChannelKind`] implementations.
//!
//! Each channel name maps to a single CRDT document held in memory. A
//! `Publish` carries an opaque CRDT update — a Y.js update in [`yrs`],
//! a binary Automerge change in [`automerge`] — and the channel kind:
//!
//! 1. Validates the update by trying to apply it to its in-memory copy
//!    of the document.
//! 2. If valid, broadcasts the update bytes back to subscribers as an
//!    opaque [`cherenkov_protocol::Publication`] payload.
//!
//! This shape lets clients exchange deltas through Cherenkov the same
//! way they would through a y-websocket or sync-message bus, while the
//! server still holds the canonical state and rejects malformed
//! updates.
//!
//! Persistence is reserved for a follow-up change. The in-memory
//! storage is suitable for development, demos, and short-lived
//! collaboration sessions.
//!
//! [`yrs`]: https://docs.rs/yrs
//! [`automerge`]: https://docs.rs/automerge

use thiserror::Error;

#[cfg(feature = "automerge")]
pub mod automerge;
#[cfg(feature = "yjs")]
pub mod yjs;

#[cfg(feature = "automerge")]
pub use automerge::AutomergeChannel;
#[cfg(feature = "yjs")]
pub use yjs::YjsChannel;

/// Errors specific to CRDT channel kinds.
#[derive(Debug, Error)]
pub enum CrdtError {
    /// The published bytes are not a valid update for this CRDT engine.
    #[error("invalid {engine} update for channel `{channel}`: {reason}")]
    InvalidUpdate {
        /// CRDT engine name (`"yjs"`, `"automerge"`).
        engine: &'static str,
        /// Channel that received the bad update.
        channel: String,
        /// Human-readable reason — payload-free.
        reason: String,
    },
}
