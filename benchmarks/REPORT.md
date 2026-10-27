# Load-testing report (README §13)

Proving the performance targets from README §13:

| scenario | target |
|---|---|
| Room board (21-day window, 60 rooms) | p95 < 200 ms |
| Availability search (7-day sweeps) | p95 < 200 ms |
| Reservation save (unique room + dates) | p95 < 200 ms |
| Full-year revenue report (by-day) | p95 < 500 ms |

## Result — ALL TARGETS MET

Two recorded official runs on the seed data below (`seed/loadtest.sql`):
release-build server, clean start, one 8-VU/50-iteration warm-up run first.

### Official run #2 — 16 VUs, 200 requests/scenario

| scenario  | p50 ms | mean ms | p95 ms | p99 ms | max ms | req/s | target |
|-----------|-------:|--------:|-------:|-------:|-------:|------:|--------|
| board (21d, 60 rooms)  | 20.5 | 36.2 | **109.4** | 127.4 | 128.2 | 392  | p95<200 OK |
| availability (7d sweep)|  6.6 |  7.0 |  **13.1** |  15.3 |  15.4 | 1720 | p95<200 OK |
| reservation save       |  9.4 | 13.1 |  **18.6** | 210.5 | 214.3 | 931  | p95<200 OK |
| report full-year       |  3.7 |  3.9 |   **6.5** |  12.3 |  13.4 | 2783 | p95<500 OK |

### Official run #1 — 12 VUs, 200 requests/scenario

| scenario  | p50 ms | mean ms | p95 ms | p99 ms | max ms | req/s | target |
|-----------|-------:|--------:|-------:|-------:|-------:|------:|--------|
| board (21d, 60 rooms)  | 17.4 | 26.0 | **63.3** |  68.9 | 155.6 | 367  | p95<200 OK |
| availability (7d sweep)|  4.9 |  5.8 | **11.6** |  20.1 |  23.7 | 1536 | p95<200 OK |
| reservation save       |  5.2 | 17.8 | **26.9** | 387.0 | 423.5 | 471  | p95<200 OK |
| report full-year       |  2.1 |  4.3 |  **6.4** |  93.5 | 139.8 | 1257 | p95<500 OK |

The single-request (1 VU) cost is stable and small (board p50 ≈ 52 ms, p95 ≈ 63 ms,
max 78 ms measured sequentially), which is the number that matters for UX:
congestion appears when the machine is saturated close to its throughput knee
(the dev box used for the runs is a shared 4-core/4-thread Ryzen 3 2200G running
PostgreSQL + the server + the load runner at the same time).

## Reproducing

```powershell
# 0. seed the load-test hotel (id 4443, wipes and re-creates it; dev data untouched)
psql -U postgres -d hms -h localhost -f seed/loadtest.sql

# 1. release-build server; per-connection pre-warm matters for cold starts
$env:DATABASE_URL = 'postgres://postgres:postgres@localhost:5432/hms'
cargo build -p hms-server --release
Start-Process target\release\hms-server.exe
# env: ADMIN_BOOTSTRAP_PASSWORD + SESSION_SECRET stay local, never committed

# 2. run the Rust runner (single login shared by all VUs — the login endpoint is
#    rate-limited 10/60s by design, so a fresh login per worker is not possible)
$env:LOADTEST_PASSWORD = '<local dev manager password>'   # set, never commit
cargo run -p hms-tests -r --example loadtest -- --base http://localhost:8080 --users 16 --n 200

# 3. or run the k6 parity scripts
k6 run -e BASE_URL=http://localhost:8080 -e HOTEL=4443 -e USERNAME=loadmgr -e PASSWORD=... benchmarks/k6/board.js
k6 run ... benchmarks/k6/availability.js
k6 run ... benchmarks/k6/reservation_save.js
k6 run ... benchmarks/k6/reports_full_year.js
```

Credentials are passed via environment only; nothing is stored in the repo.

## Seed data (`seed/loadtest.sql`)

Hotel 4443 "Load Test Hotel": 3 room types / 60 rooms (floors 1-6), 5 channels,
one annual price rule per type, **480 reservations** (8 waves x 60 rooms — past,
in-house, upcoming), **960 `daily_revenue` rows** mirroring the reservations
(2 rows per 2-night stay), 169 payments (every 3rd stay, 50% deposit) and 111
folio minibar entries (every 5th stay) — both mirrored into `daily_revenue` so
reports stay consistent. Hotel 4442 (dev fixture) and platform tenant 0 are
never touched. The script is idempotent: re-running resets the load-test hotel.

## What the runs exposed, and what was fixed

1. **Board N+1** — the board handler called `effective_price_for_date` once per
   room (~60) and once per day x room type (21 x 3) — up to hundreds of queries
   per request; 16-user board p50 was > 4 s. Fixed by batching:
   - one query for the effective price of every room type at the window start,
   - one query for occupancy + cheapest effective price across the whole window.
   Board now issues **4 queries** per request.

2. **Change-seq serialization (migration `0008`)** — every write allocated its
   sync-stream sequence by bumping `hotels.next_change_seq` (row-locked until
   commit). 16+ concurrent saves queued on that row: p95/p99 past 2 s.
   `0008_change_seq.sql` replaces the row-locked counter with a dedicated PG
   sequence (monotonically increasing per hotel — gaps allowed; the pull cursor
   only needs increasing order). `hotels.next_change_seq` stays in the schema
   for display and is served live from `change_log.max(hotel)`.

3. **Statement order** — `record_change` now runs as the LAST statement before
   commit in reservation create/edit/status-change so the (now sequence-free)
   transaction ends promptly; the change stream stays in the same transaction.

4. **Cold connection spawns** — sqlx creates Postgres connections lazily
   (~100-300 ms per backend on Windows). `DB_POOL_MIN` (default 10) pre-warms
   the pool at startup, removing the cold-start tail for the first requests.

## Measurement details

- Targets measured at the HTTP edge (request start -> full body received),
  percentiles over successful requests; failures count against the scenario.
- The Rust runner (`crates/hms-tests/examples/loadtest.rs`) enforces its
  concurrency strictly with a semaphore (earlier drafts launched all requests
  at once, which reported inflated queueing delay — fixed).
- The runner logs in once via `/api/v1/auth/login` (rate-limited 10/60 s) and
  reuses the token for all VUs.
- Hardware for the recorded runs: AMD Ryzen 3 2200G, 4 threads, PostgreSQL 16.9
  portable, Windows 11, release build (`opt-level=3, lto=thin`).

## Cleanup

The save scenario books far-future one-night stays that remain in hotel 4443.
Reset between runs with the seed script (also fixes double-booking 409s from
consecutive runs):

```sql
-- rm state for the load-test hotel only
psql -U postgres -d hms -h localhost -f seed/loadtest.sql
```

## Not yet done (honest status)

- Read caching of room types / rooms / today's board with invalidation on write
  (§13) — the batched-query optimizations above were enough to prove the
  targets; a cache layer is deferred until a target threatens to break.
- k6 scripts are CI parity harnesses; CI does not run them yet (targets are
  proven here; wiring k6 into CI needs a hosted Postgres + k6 image).
