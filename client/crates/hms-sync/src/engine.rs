//! The sync engine: drains the outbox with retry/backoff, then pulls the
//! change stream until caught up, keeping the per-hotel monotonic cursor.
//!
//! Ordering guarantees (README §11):
//! - conflict resolution happens SERVER-side by arrival, never by client
//!   clocks — the engine is only a courier;
//! - pushes are idempotent by operation id: any interruption is safe to
//!   simply retry (the server replays recorded statuses);
//! - the pull cursor only ever moves forward, and only past changes that were
//!   applied locally (apply_remote is part of the same driver loop).

use crate::store::{QueuedOperation, RemoteOutcome, Settle, StoreError, SyncStore};
use crate::transport::{SyncTransport, TransportError};
use chrono::Utc;
use hms_core::sync::{ChangeOperation, SyncOpStatus, SYNC_PROTOCOL_VERSION};
use uuid::Uuid;

/// Maximum pull page size used by the engine loop.
const PULL_PAGE: usize = 500;
/// Safety cap on pull pages per sync run (2000 changes * 500/page).
const MAX_PULL_PAGES: usize = 50;
/// Push batch per HTTP call.
const PUSH_BATCH: usize = 100;

pub struct SyncEngine<S: SyncStore, T: SyncTransport> {
    pub hotel_id: i64,
    store: S,
    transport: T,
    now_ms: Box<dyn Fn() -> i64 + Send>,
}

/// What one sync run delivered; feeds the status UI (README §11.8).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    /// outbox ops that reached a terminal state this run
    pub pushed_ok: usize,
    pub pushed_deduped: usize,
    pub pushed_conflicts: usize,
    /// permanently rejected ops this run (moved to the failure list)
    pub pushed_rejected: usize,
    /// remote changes applied locally this run
    pub pulled_applied: usize,
    pub pulled_already_known: usize,
    /// outbox entries still queued after this run
    pub pending: usize,
    /// server changes not yet pulled (server was ahead mid-pull)
    pub remaining_behind: bool,
    pub last_synced_at_ms: Option<i64>,
}

/// Why a sync run stopped early (network/auth/protocol), on top of local
/// store failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFailure {
    /// Server unreachable or erroring — retry later with backoff; the app
    /// keeps working offline.
    Offline(String),
    /// 401/403 mid-sync: the user was deactivated or the token expired at
    /// the NEXT sync (README §11.7). Caller must wipe the offline login
    /// cache and require an online login.
    AccountLocked(String),
    /// 426/protocol rejection: the desktop app must be updated.
    UpdateRequired(String),
    Store(StoreError),
}

impl std::fmt::Display for SyncFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncFailure::Offline(m) => write!(f, "offline: {m}"),
            SyncFailure::AccountLocked(m) => write!(f, "account locked at next sync: {m}"),
            SyncFailure::UpdateRequired(m) => write!(f, "update required: {m}"),
            SyncFailure::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SyncFailure {}

impl From<StoreError> for SyncFailure {
    fn from(e: StoreError) -> Self {
        SyncFailure::Store(e)
    }
}

/// Exponential backoff for a failed push delivery: 5s, 10s, 20s... capped at
/// 10 minutes. Attempts counts completed failures.
pub fn backoff_secs_after(attempts: u32) -> i64 {
    const BASE: i64 = 5;
    const CAP: i64 = 600;
    BASE.saturating_mul(1i64 << attempts.min(7)).min(CAP)
}

impl<S: SyncStore, T: SyncTransport> SyncEngine<S, T> {
    pub fn new(hotel_id: i64, store: S, transport: T) -> Self {
        Self {
            hotel_id,
            store,
            transport,
            now_ms: Box::new(|| Utc::now().timestamp_millis()),
        }
    }

    /// Test hook: a deterministic clock (client clock never reaches the
    /// server; it only orders retries locally).
    pub fn with_clock(mut self, now_ms: impl Fn() -> i64 + Send + 'static) -> Self {
        self.now_ms = Box::new(now_ms);
        self
    }

    pub fn now(&self) -> i64 {
        (self.now_ms)()
    }

    /// Queue one local write for sync. The CALLER is responsible for having
    /// written the local row in the same transaction (README §11.1).
    pub fn record_local_write(&mut self, op: ChangeOperation) -> Result<(), SyncFailure> {
        self.store.enqueue(op, self.now())?;
        Ok(())
    }

    /// Full cycle: push due outbox ops, then pull everything new.
    pub async fn sync_once(&mut self) -> Result<SyncOutcome, SyncFailure> {
        let mut out = SyncOutcome::default();
        self.push_phase(&mut out).await?;
        self.pull_phase(&mut out).await?;
        let now = self.now();
        self.store.set_last_synced_at_ms(now)?;
        out.last_synced_at_ms = Some(now);
        out.pending = self.store.pending_count()?;
        Ok(out)
    }

    async fn push_phase(&mut self, out: &mut SyncOutcome) -> Result<(), SyncFailure> {
        loop {
            let now = self.now();
            let due: Vec<QueuedOperation> = self
                .store
                .pending(PUSH_BATCH)?
                .into_iter()
                .filter(|q| match q.next_retry_at_ms {
                    None => true,
                    Some(t) => t <= now,
                })
                .collect();
            if due.is_empty() {
                break;
            }

            let req = hms_core::sync::SyncPushRequest {
                hotel_id: self.hotel_id,
                operations: due.iter().map(|q| q.op.clone()).collect(),
                protocol_version: SYNC_PROTOCOL_VERSION,
            };

            let resp = match self.transport.push(req).await {
                Ok(r) => r,
                Err(e) => {
                    let failure = map_transport(&e);
                    for q in &due {
                        let attempts = q.attempts + 1;
                        let next_retry = match &failure {
                            // protocol/auth failures are not retried on a timer
                            SyncFailure::AccountLocked(_) | SyncFailure::UpdateRequired(_) => None,
                            _ => Some(self.now() + backoff_secs_after(attempts) * 1000),
                        };
                        self.store.record_attempt(
                            q.op.operation_id,
                            attempts,
                            Some(e.to_string()),
                            next_retry,
                        )?;
                    }
                    return Err(failure);
                }
            };

            for (q, result) in due.iter().zip(resp.results.iter()) {
                debug_assert_eq!(q.op.operation_id, result.operation_id, "server results align with requests");
                match &result.status {
                    SyncOpStatus::Applied => {
                        self.store.settle(q.op.operation_id, Settle::Acknowledged)?;
                        out.pushed_ok += 1;
                    }
                    SyncOpStatus::DuplicateIgnored => {
                        self.store.settle(q.op.operation_id, Settle::Acknowledged)?;
                        out.pushed_deduped += 1;
                    }
                    SyncOpStatus::ConflictResolved { .. } => {
                        self.store.settle(q.op.operation_id, Settle::Acknowledged)?;
                        out.pushed_conflicts += 1;
                    }
                    SyncOpStatus::Rejected { reason } => {
                        self.store.record_attempt(q.op.operation_id, q.attempts + 1, Some(reason.clone()), None)?;
                        self.store.settle(q.op.operation_id, Settle::FailedPermanent)?;
                        out.pushed_rejected += 1;
                    }
                }
            }
            // response carried fewer results than requests: leave missing ops
            // queued; the next run replays them idempotently.
        }
        Ok(())
    }

    async fn pull_phase(&mut self, out: &mut SyncOutcome) -> Result<(), SyncFailure> {
        let mut cursor = self.store.pull_cursor()?;
        for _ in 0..MAX_PULL_PAGES {
            let resp = match self.transport.pull(self.hotel_id, cursor, PULL_PAGE).await {
                Ok(r) => r,
                Err(e) => {
                    let mapped = map_transport(&e);
                    // a partial page already applied advanced the cursor; the
                    // next run continues from there.
                    return Err(mapped);
                }
            };
            if resp.hotel_id != self.hotel_id {
                return Err(SyncFailure::Offline(format!(
                    "pull response for hotel {} does not match ours {}",
                    resp.hotel_id, self.hotel_id
                )));
            }
            for rec in &resp.records {
                if rec.hotel_id != self.hotel_id {
                    return Err(SyncFailure::Offline("cross-hotel record in change stream".into()));
                }
                if rec.change_counter <= cursor {
                    out.pulled_already_known += 1;
                    continue;
                }
                match self.store.apply_remote(rec)? {
                    RemoteOutcome::Applied => out.pulled_applied += 1,
                    RemoteOutcome::AlreadyKnown => out.pulled_already_known += 1,
                }
                cursor = rec.change_counter;
                self.store.set_pull_cursor(cursor)?;
            }
            if !resp.has_more {
                break;
            }
        }
        Ok(())
    }

    /// Status snapshot for the UI (online/offline dot, pending count, last
    /// sync time, unresolved conflicts).
    pub fn status_snapshot(&mut self) -> Result<StatusSnapshot, StoreError> {
        Ok(StatusSnapshot {
            pending: self.store.pending_count()?,
            failures: self.store.failures()?.len(),
            pull_cursor: self.store.pull_cursor()?,
            last_synced_at_ms: self.store.last_synced_at_ms()?,
            conflicts: self.store.conflict_count()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusSnapshot {
    pub pending: usize,
    /// permanently rejected ops awaiting user attention
    pub failures: usize,
    pub pull_cursor: i64,
    pub last_synced_at_ms: Option<i64>,
    pub conflicts: usize,
}

fn map_transport(e: &TransportError) -> SyncFailure {
    match e {
        TransportError::Network(m) => SyncFailure::Offline(m.clone()),
        TransportError::AuthExpired(m) => SyncFailure::AccountLocked(m.clone()),
        TransportError::ProtocolTooOld(m) => SyncFailure::UpdateRequired(m.clone()),
    }
}

/// Convenience: new operation id (UUIDv7) for a fresh local write.
pub fn new_operation_id() -> Uuid {
    Uuid::now_v7()
}
