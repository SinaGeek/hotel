//! Local persistence seam for the sync engine.
//!
//! The desktop's SQLite layer implements this trait. Every local write MUST
//! call [`SyncStore::enqueue`] for the corresponding [`ChangeOperation`] in the
//! SAME transaction as the row write (README §11: outbox and row commit or
//! fail together). The engine then drains the outbox idempotently — the server
//! deduplicates by operation id, so retries never duplicate data.

use hms_core::sync::{ChangeOperation, ServerChangeRecord};
use std::fmt;

#[derive(Debug, Clone)]
pub struct QueuedOperation {
    pub op: ChangeOperation,
    /// RFC3339 server-independent ordering key (client clock is only used for
    /// outbox ordering, never for conflict resolution — the server decides).
    pub enqueued_at_ms: i64,
    pub attempts: u32,
    pub last_error: Option<String>,
    /// Unix ms after which the engine may retry again (backoff); None = due.
    pub next_retry_at_ms: Option<i64>,
}

/// What happened to an outbox entry in [`SyncStore::settle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settle {
    /// Terminal good/conflict outcome: the op left the outbox.
    Acknowledged,
    /// The server rejected the op permanently (Append-only rule, permission,
    /// clock skew, unknown reference). Removed from the outbox and preserved
    /// in the failure list so the UI can show it; never silently retried.
    FailedPermanent,
}

/// Result of feeding one pulled server change into the local database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOutcome {
    /// A new change of our own operation id (ack echo) or a fresh server
    /// change: applied locally.
    Applied,
    /// Already present locally (e.g. replay after a partial sync).
    AlreadyKnown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Io(String),
    Conflict(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(m) => write!(f, "local store: {m}"),
            StoreError::Conflict(m) => write!(f, "local store conflict: {m}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// The desktop database's sync-facing surface. Methods are synchronous: local
/// SQLite calls are fast and run on the caller's thread inside transactions.
pub trait SyncStore {
    /// Append an operation to the outbox. MUST be called in the same
    /// transaction as the corresponding local row write (README §11.1).
    fn enqueue(&mut self, op: ChangeOperation, now_ms: i64) -> Result<(), StoreError>;

    /// Due outbox ops, oldest enqueue first (attempts within backoff window
    /// are skipped by the engine, not here).
    fn pending(&mut self, limit: usize) -> Result<Vec<QueuedOperation>, StoreError>;

    /// Record an attempt result; `next_retry_at_ms = Some(t)` schedules the
    /// retry backoff, None clears it (due again immediately).
    fn record_attempt(
        &mut self,
        operation_id: uuid::Uuid,
        attempts: u32,
        last_error: Option<String>,
        next_retry_at_ms: Option<i64>,
    ) -> Result<(), StoreError>;

    /// Terminal result for one op (see [`Settle`]).
    fn settle(&mut self, operation_id: uuid::Uuid, outcome: Settle) -> Result<(), StoreError>;

    /// Permanently rejected ops, for the status UI (outbox drained, but flags
    /// remain until the user resolves or dismisses them).
    fn failures(&mut self) -> Result<Vec<QueuedOperation>, StoreError>;

    fn clear_failure(&mut self, operation_id: uuid::Uuid) -> Result<(), StoreError>;

    /// Pending outbox size for the status UI.
    fn pending_count(&mut self) -> Result<usize, StoreError>;

    // ---- pull side

    /// Last pulled change_seq for this hotel (per-hotel monotonic counter —
    /// NEVER a timestamp, which would miss late-committing rows, README §11.3).
    fn pull_cursor(&mut self) -> Result<i64, StoreError>;
    fn set_pull_cursor(&mut self, seq: i64) -> Result<(), StoreError>;

    /// Apply one server change to the local DB. Idempotent: re-seeing our own
    /// op echo, or a replayed change, returns AlreadyKnown rather than
    /// double-writing.
    fn apply_remote(&mut self, record: &ServerChangeRecord) -> Result<RemoteOutcome, StoreError>;

    // ---- status

    fn last_synced_at_ms(&mut self) -> Result<Option<i64>, StoreError>;
    fn set_last_synced_at_ms(&mut self, ms: i64) -> Result<(), StoreError>;

    /// Count of locally unresolved `conflict` reservations / server-wins
    /// flags displayed next to the sync dot.
    fn conflict_count(&mut self) -> Result<usize, StoreError>;
}
