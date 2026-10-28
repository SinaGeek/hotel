//! Engine behavior tests: drain, dedup/conflict statuses, permanent
//! rejection, retry backoff + idempotent replay, offline/lockout/update
//! failures, paged pull with the monotonic cursor.

mod common;

use common::*;
use hms_core::sync::{SyncOpStatus, SyncPushRequest, SYNC_PROTOCOL_VERSION};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use hms_sync::engine::{backoff_secs_after, SyncEngine, SyncFailure};

/// Per-test clock: Arc<AtomicI64> the test ticks forward deterministically.
fn make_clock() -> (Arc<AtomicI64>, impl Fn() -> i64 + Send) {
    let t = Arc::new(AtomicI64::new(1_700_000_000_000));
    let t2 = t.clone();
    (t, move || t2.load(Ordering::SeqCst))
}

fn seen_pushes(transport: &Arc<FakeTransport>) -> Vec<SyncPushRequest> {
    transport
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter_map(|s| match s {
            Seen::Push(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}

fn seen_pulls(transport: &Arc<FakeTransport>) -> Vec<(i64, i64, usize)> {
    transport
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter_map(|s| match s {
            Seen::Pull { hotel_id, since, limit } => Some((*hotel_id, *since, *limit)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn push_drains_outbox_pull_advances_cursor_and_records_sync_time() {
    let (_clock_ref, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());

    let op1 = guest_op("Alpha");
    let op2 = guest_op("Beta");
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);
    engine.record_local_write(op1.clone()).unwrap();
    engine.record_local_write(op2.clone()).unwrap();

    transport.then_push_status(&[op1.clone(), op2.clone()], vec![
        SyncOpStatus::Applied,
        SyncOpStatus::DuplicateIgnored,
    ]);
    // two pull pages to prove the loop retries with the advanced cursor
    let rec = transport.record_at(11, op1.operation_id, false);
    transport.then_pull_page(vec![rec], 11, true);
    transport.then_pull_page(vec![], 11, false);

    let outcome = engine.sync_once().await.expect("sync succeeds");
    assert_eq!(outcome.pushed_ok, 1, "applied");
    assert_eq!(outcome.pushed_deduped, 1, "duplicate replay");
    assert_eq!(outcome.pushed_rejected, 0);
    assert_eq!(outcome.pulled_applied, 1);
    assert_eq!(outcome.pending, 0, "both settled");
    assert_eq!(outcome.last_synced_at_ms, Some(1_700_000_000_000));

    // push batch carried both ops and the current protocol version
    let pushes = seen_pushes(&transport);
    assert_eq!(pushes.len(), 1);
    let p = &pushes[0];
    assert_eq!(p.hotel_id, HOTEL);
    assert_eq!(p.protocol_version, SYNC_PROTOCOL_VERSION);
    assert_eq!(p.operations.len(), 2);

    // pull used cursor 0 (store default), then re-pulled from 11
    let pulls = seen_pulls(&transport);
    assert_eq!(pulls.len(), 2);
    assert_eq!(pulls[0], (HOTEL, 0, 500));
    assert_eq!(pulls[1], (HOTEL, 11, 500));

    // store cursor advanced to 11
    assert_eq!(engine.status_snapshot().unwrap().pull_cursor, 11);
}

#[tokio::test]
async fn conflict_resolved_settles_and_counts() {    let (_, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());
    let op = guest_op("Overbook");
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);
    engine.record_local_write(op.clone()).unwrap();

    transport.then_push_status(&[op.clone()], vec![SyncOpStatus::ConflictResolved {
        message: "overlapping stay -> status conflict".into(),
    }]);
    transport.then_pull_page(vec![], 0, false);

    let outcome = engine.sync_once().await.expect("sync succeeds");
    assert_eq!(outcome.pushed_conflicts, 1);
    assert_eq!(outcome.pending, 0);
    let snap = engine.status_snapshot().unwrap();
    assert_eq!(snap.failures, 0, "conflict_resolved is not a failure");
}

#[tokio::test]
async fn rejected_ops_become_permanent_failures_not_retries() {
    let (_, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());
    let op = guest_op("Bad Op");
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);
    engine.record_local_write(op.clone()).unwrap();

    transport.then_push_status(&[op.clone()], vec![SyncOpStatus::Rejected {
        reason: "payments are append-only; record a reversal payment instead".into(),
    }]);
    transport.then_pull_page(vec![], 0, false);

    let outcome = engine.sync_once().await.expect("sync succeeds");
    assert_eq!(outcome.pushed_rejected, 1);
    assert_eq!(outcome.pending, 0, "outbox drained");
    let snap = engine.status_snapshot().unwrap();
    assert_eq!(snap.failures, 1, "kept for the UI, not retried");
}

#[tokio::test]
async fn network_error_schedules_backoff_and_retries_idempotently() {
    let (clock_ref, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());
    let op = guest_op("Offline Guest");
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);
    engine.record_local_write(op.clone()).unwrap();

    // first sync: push fails -> Offline
    transport.then_push_err(TransportError::Network("airplane mode".into()));
    let err = engine.sync_once().await.expect_err("fails with Offline");
    assert!(matches!(err, SyncFailure::Offline(_)));
    // attempt recorded with a backoff deadline in the future
    let snap = engine.status_snapshot().unwrap();
    assert_eq!(snap.pending, 1);
    assert!(clock_ref.load(Ordering::SeqCst) < 1_700_000_000_000 + backoff_secs_after(1) * 1000);
    assert_eq!(backoff_secs_after(1), 10);

    // immediate retry: op not due yet -> no push goes out, pull still runs
    transport.then_pull_page(vec![], 0, false);
    let outcome = engine.sync_once().await.expect("second sync: pull only");
    assert_eq!(outcome.pending, 1);
    assert_eq!(seen_pushes(&transport).len(), 1, "skipped the backed-off op");

    // advance the clock past the backoff: retry goes out with the SAME
    // operation id (server replays dedupe safely)
    clock_ref.fetch_add(backoff_secs_after(1) * 1000 + 1, Ordering::SeqCst);
    transport.then_push_status(std::slice::from_ref(&op), vec![SyncOpStatus::Applied]);
    transport.then_pull_page(vec![], 0, false);
    let outcome = engine.sync_once().await.expect("third sync succeeds");
    assert_eq!(outcome.pushed_ok, 1);
    assert_eq!(outcome.pending, 0);
    let pushes = seen_pushes(&transport);
    assert_eq!(pushes.len(), 2);
    assert_eq!(pushes[0].operations[0].operation_id, pushes[1].operations[0].operation_id,
        "idempotent replay keeps the operation id");
}

#[tokio::test]
async fn auth_error_flags_account_lockout_outbox_intact() {
    let (_, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());
    let op = guest_op("Deactivated Soon");
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);
    engine.record_local_write(op.clone()).unwrap();

    transport.then_push_err(TransportError::AuthExpired("HTTP 401".into()));
    let err = engine.sync_once().await.expect_err("locked");
    assert!(matches!(err, SyncFailure::AccountLocked(_)));
    let snap = engine.status_snapshot().unwrap();
    assert_eq!(snap.pending, 1, "outbox preserved for re-login");
    // no backoff deadline was recorded on the op: after a successful online
    // re-login the very next sync retries it immediately
}

#[tokio::test]
async fn protocol_rejection_flags_update_required() {
    let (_, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());
    let op = guest_op("Old Build");
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);
    engine.record_local_write(op.clone()).unwrap();

    transport.then_push_err(TransportError::ProtocolTooOld("HTTP 426".into()));
    let err = engine.sync_once().await.expect_err("update gate");
    assert!(matches!(err, SyncFailure::UpdateRequired(_)));
}

#[tokio::test]
async fn pull_paging_keeps_only_growing_cursor_and_counts_conflicts() {
    let (_, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);
    // empty outbox: push phase does nothing (no scripted entry needed)

    let r1 = transport.record_at(5, Uuid::now_v7(), false);
    transport.then_pull_page(vec![r1], 5, true);
    let r2 = transport.record_at(9, Uuid::now_v7(), true);
    transport.then_pull_page(vec![r2], 9, false);

    let outcome = engine.sync_once().await.expect("sync succeeds");
    assert_eq!(outcome.pulled_applied, 2);
    let snap = engine.status_snapshot().unwrap();
    assert_eq!(snap.pull_cursor, 9, "cursor advanced across pages");
    assert_eq!(snap.conflicts, 1, "local conflict count for the UI");
    let pulls = seen_pulls(&transport);
    assert_eq!(pulls[0].1, 0);
    assert_eq!(pulls[1].1, 5, "second page continues from the last record");
}

#[tokio::test]
async fn cross_hotel_stream_record_is_a_hard_failure() {
    let (_, clock) = make_clock();
    let store = FakeStore::new();
    let transport = Arc::new(FakeTransport::new());
    let mut engine = SyncEngine::new(HOTEL, store, transport.clone()).with_clock(clock);

    let mut rec = transport.record_at(2, Uuid::now_v7(), false);
    rec.hotel_id = 999;
    transport.then_pull_page(vec![rec], 2, false);

    let err = engine.sync_once().await.expect_err("cross-tenant guard");
    assert!(matches!(err, SyncFailure::Offline(_)));
}

#[test]
fn backoff_curve_is_exponential_with_cap() {
    assert_eq!(backoff_secs_after(0), 5);
    assert_eq!(backoff_secs_after(1), 10);
    assert_eq!(backoff_secs_after(2), 20);
    assert_eq!(backoff_secs_after(7), 600, "capped");
    assert_eq!(backoff_secs_after(12), 600);
}
