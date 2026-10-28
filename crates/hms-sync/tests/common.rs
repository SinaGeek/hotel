//! Shared fixtures: in-memory fake store + scripted transport for the
//! integration tests in this directory.

use hms_core::sync::{
    ChangeOperation, EntityType, OperationType, ServerChangeRecord, SyncOpResult, SyncOpStatus,
    SyncPullResponse, SyncPushRequest, SyncPushResponse,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

pub const HOTEL: i64 = 4443;

// ---------------------------------------------------------------- fake store

#[derive(Default)]
pub struct FakeStore {
    pub outbox: Vec<QueuedOperation>,
    pub settled: HashMap<Uuid, Settle>,
    pub failures: Vec<QueuedOperation>,
    pub cursor: i64,
    pub last_synced: Option<i64>,
    pub applied_remote: Vec<ServerChangeRecord>,
    pub conflicts: usize,
}

impl FakeStore {
    pub fn new() -> Self {
        Self::default()
    }
}

pub use hms_sync::store::{QueuedOperation, RemoteOutcome, Settle, StoreError, SyncStore};

fn queued(op: ChangeOperation, now_ms: i64) -> QueuedOperation {
    QueuedOperation {
        op,
        enqueued_at_ms: now_ms,
        attempts: 0,
        last_error: None,
        next_retry_at_ms: None,
    }
}

impl SyncStore for FakeStore {
    fn enqueue(&mut self, op: ChangeOperation, now_ms: i64) -> Result<(), StoreError> {
        self.outbox.push(queued(op, now_ms));
        Ok(())
    }

    fn pending(&mut self, limit: usize) -> Result<Vec<QueuedOperation>, StoreError> {
        Ok(self.outbox.iter().take(limit).cloned().collect())
    }

    fn record_attempt(
        &mut self,
        operation_id: Uuid,
        attempts: u32,
        last_error: Option<String>,
        next_retry_at_ms: Option<i64>,
    ) -> Result<(), StoreError> {
        let q = self
            .outbox
            .iter_mut()
            .find(|q| q.op.operation_id == operation_id)
            .expect("attempt on a queued op");
        q.attempts = attempts;
        q.last_error = last_error;
        q.next_retry_at_ms = next_retry_at_ms;
        Ok(())
    }

    fn settle(&mut self, operation_id: Uuid, outcome: Settle) -> Result<(), StoreError> {
        self.settled.insert(operation_id, outcome);
        if let Some(pos) = self
            .outbox
            .iter()
            .position(|q| q.op.operation_id == operation_id)
        {
            let q = self.outbox.remove(pos);
            if outcome == Settle::FailedPermanent {
                self.failures.push(q);
            }
        }
        Ok(())
    }

    fn failures(&mut self) -> Result<Vec<QueuedOperation>, StoreError> {
        Ok(self.failures.clone())
    }

    fn clear_failure(&mut self, operation_id: Uuid) -> Result<(), StoreError> {
        self.failures.retain(|q| q.op.operation_id != operation_id);
        Ok(())
    }

    fn pending_count(&mut self) -> Result<usize, StoreError> {
        Ok(self.outbox.len())
    }

    fn pull_cursor(&mut self) -> Result<i64, StoreError> {
        Ok(self.cursor)
    }

    fn set_pull_cursor(&mut self, seq: i64) -> Result<(), StoreError> {
        self.cursor = seq;
        Ok(())
    }

    fn apply_remote(&mut self, record: &ServerChangeRecord) -> Result<RemoteOutcome, StoreError> {
        if record.payload.get("already_known").is_some() {
            return Ok(RemoteOutcome::AlreadyKnown);
        }
        self.applied_remote.push(record.clone());
        if record.entity_type == EntityType::Reservation
            && record.payload.get("conflict").is_some()
        {
            self.conflicts += 1;
        }
        Ok(RemoteOutcome::Applied)
    }

    fn last_synced_at_ms(&mut self) -> Result<Option<i64>, StoreError> {
        Ok(self.last_synced)
    }

    fn set_last_synced_at_ms(&mut self, ms: i64) -> Result<(), StoreError> {
        self.last_synced = Some(ms);
        Ok(())
    }

    fn conflict_count(&mut self) -> Result<usize, StoreError> {
        Ok(self.conflicts)
    }
}

// -------------------------------------------------------- scripted transport

#[derive(Clone, Debug)]
pub enum Scripted {
    Push(SyncPushResponse),
    Pull(SyncPullResponse),
    PushErr(TransportError),
    #[allow(dead_code)]
    PullErr(TransportError),
}

#[derive(Clone, Debug)]
pub enum Seen {
    Push(SyncPushRequest),
    Pull { hotel_id: i64, since: i64, limit: usize },
}

pub use hms_sync::transport::{SyncTransport, TransportError};

pub struct FakeTransport {
    script: Mutex<Vec<Scripted>>,
    pub seen: Mutex<Vec<Seen>>,
}

impl Default for FakeTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeTransport {
    pub fn new() -> Self {
        Self { script: Mutex::new(vec![]), seen: Mutex::new(vec![]) }
    }

    pub fn script(&self, s: Scripted) {
        self.script.lock().unwrap().push(s);
    }

    pub fn then_push_status(
        &self,
        req_ops: &[ChangeOperation],
        statuses: Vec<SyncOpStatus>,
    ) {
        let results = req_ops
            .iter()
            .zip(statuses)
            .map(|(op, status)| SyncOpResult {
                operation_id: op.operation_id,
                entity_type: op.entity_type,
                entity_id: op.entity_id,
                status,
            })
            .collect();
        self.script(Scripted::Push(SyncPushResponse {
            hotel_id: HOTEL,
            results,
            server_latest_counter: 0,
        }));
    }

    pub fn then_push_err(&self, e: TransportError) {
        self.script(Scripted::PushErr(e));
    }

    pub fn then_pull_page(
        &self,
        records: Vec<ServerChangeRecord>,
        latest_counter: i64,
        has_more: bool,
    ) {
        self.script(Scripted::Pull(SyncPullResponse {
            hotel_id: HOTEL,
            records,
            latest_counter,
            has_more,
        }));
    }

    #[allow(dead_code)]
    pub fn then_pull_err(&self, e: TransportError) {
        self.script(Scripted::PullErr(e));
    }

    /// Build one change record at a given seq, optionally flagged as a local
    /// `conflict` reservation for conflict-count assertions.
    pub fn record_at(&self, seq: i64, op_id: Uuid, conflict: bool) -> ServerChangeRecord {
        ServerChangeRecord {
            change_counter: seq,
            hotel_id: HOTEL,
            operation_id: op_id,
            entity_type: EntityType::Reservation,
            entity_id: Uuid::now_v7(),
            operation_type: OperationType::Insert,
            payload: if conflict {
                json!({"conflict": true})
            } else {
                json!({"ok": true})
            },
            server_timestamp: chrono::Utc::now(),
        }
    }
}

impl SyncTransport for FakeTransport {
    async fn push(&self, req: SyncPushRequest) -> Result<SyncPushResponse, TransportError> {
        self.seen.lock().unwrap().push(Seen::Push(req));
        let mut script = self.script.lock().unwrap();
        let idx = script
            .iter()
            .position(|s| matches!(s, Scripted::Push(_) | Scripted::PushErr(_)));
        let idx = match idx {
            Some(i) => i,
            None => return Err(TransportError::Network("no scripted push response".into())),
        };
        match script.remove(idx) {
            Scripted::Push(r) => Ok(r),
            Scripted::PushErr(e) => Err(e),
            _ => unreachable!(),
        }
    }

    async fn pull(
        &self,
        hotel_id: i64,
        since: i64,
        limit: usize,
    ) -> Result<SyncPullResponse, TransportError> {
        self.seen.lock().unwrap().push(Seen::Pull { hotel_id, since, limit });
        let mut script = self.script.lock().unwrap();
        let idx = script
            .iter()
            .position(|s| matches!(s, Scripted::Pull(_) | Scripted::PullErr(_)));
        let idx = match idx {
            Some(i) => i,
            None => return Err(TransportError::Network("no scripted pull response".into())),
        };
        match script.remove(idx) {
            Scripted::Pull(r) => Ok(r),
            Scripted::PullErr(e) => Err(e),
            _ => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------- op helpers

pub fn guest_op(full_name: &str) -> ChangeOperation {
    ChangeOperation {
        operation_id: Uuid::now_v7(),
        hotel_id: HOTEL,
        entity_type: EntityType::Guest,
        entity_id: Uuid::now_v7(),
        operation_type: OperationType::Insert,
        payload: json!({"full_name": full_name}),
        client_timestamp: chrono::Utc::now(),
    }
}
