# Benchmarks Report (todo F8)

> **Status**: the scale seed (`seed/scale.sql`) and the load-test infrastructure
> are in place. This file records results once a scale run is executed on a
> properly provisioned host. The targets below are from README §13.

## Targets (README §13)

| Metric | Target |
|---|---|
| Board load p95 | < 100 ms (50 rooms × 30 days) |
| Reservation save p95 | < 50 ms |
| Availability check p95 | < 20 ms |
| Full-year report p95 | < 500 ms (1 hotel, 365 days) |
| Sync push (100 ops) p95 | < 200 ms |
| Sync pull (500 changes) p95 | < 300 ms |
| Concurrent saves (10 clients) | No serialization, no deadlocks |

## Results

### Run 1: single hotel, 50 rooms (smoke test)

- **Date**: ______
- **Host**: ______
- **Seed**: `psql -v hotels=1 -f seed/scale.sql`
- **PostgreSQL**: ______
- **Pool size**: 20

| Metric | p50 | p95 | p99 | Target | Pass? |
|---|---|---|---|---|---|
| Board load | — | — | — | < 100 ms | ☐ |
| Reservation save | — | — | — | < 50 ms | ☐ |
| Availability | — | — | — | < 20 ms | ☐ |
| Report (365 days) | — | — | — | < 500 ms | ☐ |
| Sync push (100 ops) | — | — | — | < 200 ms | ☐ |
| Sync pull (500) | — | — | — | < 300 ms | ☐ |

Notes:

### Run 2: 10 hotels × 50 rooms (medium)

- **Date**: ______
- **Seed**: `psql -v hotels=10 -f seed/scale.sql`
- Repeat the table above.

Notes:

### Run 3: 1,000 hotels × 50 rooms (full scale)

- **Date**: ______
- **Seed**: `psql -v hotels=1000 -f seed/scale.sql` (~50K rooms, ~50 GB)
- Repeat the table above.

Notes:

## Index verification

The following indexes are critical for the targets above. Verify they exist
after migration:

```sql
SELECT indexname, tablename FROM pg_indexes
WHERE schemaname = 'public'
AND indexname IN ('idx_res_board', 'idx_res_status', 'idx_res_stay_dates',
                   'idx_payments_res', 'idx_payments_hotel',
                   'idx_res_created', 'change_log_pkey')
ORDER BY tablename, indexname;
```

## Pool tuning

If the concurrent-save target is missed, increase `DB_POOL_MAX` (default 20)
and verify PostgreSQL's `max_connections` is at least `DB_POOL_MAX × N_instances
+ 10`.

## Load-test tooling

- k6 script: `scripts/loadtest.js` (if present)
- Rust loadtest: `cargo run -p hms-tests --bin loadtest`
- Manual: `curl` / PowerShell `Invoke-WebRequest` in a loop with timing
