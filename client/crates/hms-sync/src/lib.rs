//! hms-sync: client-side sync engine for the offline desktop app (README §11).
//!
//! The engine is storage-agnostic: [`SyncStore`] is the seam the desktop's
//! SQLite database implements (outbox + pull cursor + remote applier inside
//! its own write transactions), and [`SyncTransport`] is the seam for the
//! HTTP client. [`SyncEngine`] drives retry/backoff push batches, paged pulls
//! and the status snapshot the UI renders (online/offline, pending, conflicts).
//!
//! Protocol: every push carries `SYNC_PROTOCOL_VERSION` (hms-core); the server
//! rejects stale clients with 426 when it enforces SYNC_PROTOCOL_MIN_VERSION.
//! Engines surface an auth rejection as [`SyncFailure::AccountLocked`] —
//! a user deactivated on the server is locked out at the NEXT sync: the caller
//! wipes the persisted [`hms_core::sync::OfflineLoginCache`] and forces an
//! online login.

pub mod engine;
pub mod offline_login;
pub mod store;
pub mod transport;

pub use engine::{SyncEngine, SyncFailure, SyncOutcome};
pub use store::{QueuedOperation, Settle, StoreError, SyncStore};
pub use transport::{HttpSyncTransport, TransportError, SyncTransport};
