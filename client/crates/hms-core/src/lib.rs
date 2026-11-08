//! HMS client core (vendored slim copy of the server's hms-core).
//!
//! Architectural isolation (restructure decision): the client is a standalone
//! app and must not depend on any server crate. Only the modules the client
//! genuinely needs are vendored here:
//! - `sync` — the sync protocol types (outbox/push/pull + offline login cache)
//! - `roles` — the role/permission vocabulary used for permission gating
//! Server-only modules (site2, models, guest_identity, key_rotation,
//! validation, api_types) are intentionally NOT part of the client.
//!
//! Protocol compatibility contract: these types must stay wire-compatible with
//! `server/crates/hms-core` (same `SYNC_PROTOCOL_VERSION`) — a difference
//! breaks every sync login.

pub mod roles;
pub mod sync;

// Re-exports for convenience (mirror the original crate's API surface)
pub use roles::*;
pub use sync::*;

pub fn version() -> &'static str {
    "0.1.0"
}
