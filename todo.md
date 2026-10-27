# TODO — Unified HMS: remaining work

Generated from an analysis of the repository against `README.md` (the target design).
Legend: **[BLOCKER]** stops everything else · **[P0]** critical path · **[P1]** important · **[P2]** later / polish

---

## 0. Current state (what exists today)

| Area | Status |
|---|---|
| README (target design) | complete |
| Rust workspace skeleton | exists: `crates/hms-core`, `crates/hms-server`, `crates/hms-desktop/src-tauri` |
| `hms-core` models / roles / sync types / validation | first draft written, compiles with warnings |
| `hms-server` | stub only (`/health` + fake login), **does not compile** (15 errors) |
| `hms-desktop` | placeholder `main.rs` creating `/tmp` dirs — not a Tauri app, no SQLite, no sync |
| Web app (`apps/web` / `frontend/`) | **does not exist** — only an empty `frontend/src/types/generated/` |
| PostgreSQL migrations (`migrations/`) | **empty directory** — no schema at all |
| Seed data (`seed/`) | **empty directory** |
| Load tests (`benchmarks/`, `loadtest/`) | **empty directory** |
| Tests (`tests/`) | none anywhere in the repo |
| `docker-compose.yml`, `.env.example` | **missing** |
| Feature inventory (README §4) | all 13 rows still `pending` |
| Reference-site artifacts | `flow_index.js`, `flow_app.js`, `inspect_*.py` captured (good raw material for §4) |
| Git | repository is **not under version control** |

**Bottom line:** the project is at roughly 5% of the README roadmap. Almost everything in sections 4–17 remains to be built.

---

## 1. Foundation fixes (do first) — [BLOCKER] ✅ DONE (commit `52728de`)

- [x] **Fix `cargo check --workspace` failure** — `crates/hms-server/src/main.rs` had 15 compile errors:
  - [x] Missing imports: `Router`, `Json`, `Arc`, `StatusCode`, `HeaderValue`, `axum::routing::{get, post}`
  - [x] `hms_core::ApiResponse` did not exist — added generic `ApiResponse<T>` to `hms-core/src/api_types.rs`
  - [x] Duplicate `use` lines removed
- [x] **Resolve duplicate modules in `hms-core`** — deleted `request_types.rs` (byte-identical to `api_types.rs`), removed its `mod`/`pub use` from `lib.rs`
- [~] **Clean unused-dependency warnings** — unused imports fixed; the remaining flagged deps (`sqlx`, `argon2`, `jsonwebtoken`, `tower-http`, `rusqlite`, `reqwest`, `thiserror`) are **kept intentionally**: each is used in the imminent phases (3/4: server DB+auth, 6: desktop). Remove them only if their phase is abandoned.
- [x] **Decide final repo layout** — aligned to README §5: desktop crate moved to `apps/desktop/src-tauri` (workspace member + `hms-core` path updated); web app will be `apps/web` (Phase 5); crates stay under `crates/`
- [x] **`git init`** — installed portable MinGit 2.56 to `%LOCALAPPDATA%\Tools\mingit` (no admin needed), added to user PATH, `.gitignore` created, first commit made
- [x] **Create `docker-compose.yml`** (PostgreSQL 16 + healthcheck; server service commented until server image exists) and **`.env.example`** (all README §15 vars incl. SITE2_*)
- [x] Implement the stub `rate_limiter.rs` — real per-key fixed-window limiter + 3 unit tests passing; dummy `init.rs` deleted

## 2. Feature inventory (README §4) — [P0] ✅ DONE

- [x] Complete the §4 table by verifying every area against **app.hms.gen.tr** (site 2) — verified live 2026-10-27, logged in as the user's account (no captcha appeared; the OpenCode browser holds the session for future visits)
- [x] Route tree enumerated from the live menu (56+ routes) + all report sub-pages inspected
- [x] Findings recorded in `docs/feature-inventory.md` (observations per README §4 area + evidence screenshots in `docs/screenshots/` — room board, reservation detail, housekeeping, channel list)
- [x] Scope reconciled: 13 §4 rows marked **verified** in README §4; site-2-only modules (invoices/e-archive, agency currents, currency exchange, cash/banks, online payments, open sale/expenses/stock, SMS/mail tooling, Wi-Fi/IP, notifications, end-of-day, licensing) marked **deferred/out** in the inventory doc — README now links to it
- Notes for later phases (recorded in inventory doc): site-2 cleanliness = Dirty/Clean + Out of Order; reservation detail embeds Transaction Logs; board price/availability is per room_type × date; no per-channel earnings report on site 2

## 3. Database schema (README §7–§9) — [P0]

- [ ] Write PostgreSQL migrations in `migrations/` (currently empty) for all tables:
  - [ ] `hotels` (with per-hotel `change_seq` counter), `users`, `roles`, `role_permissions`
  - [ ] `room_types`, `rooms`, `guests`, `channels`, `price_rules`
  - [ ] `reservations`, `payments` (append-only), `folio_transactions` (model exists in Rust, table missing)
  - [ ] `change_log`, `sync_ops`, `daily_revenue`, `audit_log`
- [ ] `CREATE EXTENSION btree_gist` + the `no_double_booking` EXCLUDE constraint on `reservations` (README §9) — double booking must be prevented **by the database**
- [ ] Indexes per §13: `(hotel_id, room_id, stay)`, `(hotel_id, status)`, `(hotel_id, created_at)` + keyset-pagination support
- [ ] Enable **row-level security** on every table; every query must run with a tenant scope (README §8)
- [ ] Run migrations on server start (`sqlx::migrate!` — the `migrate` feature is already enabled but unused)
- [ ] Seed the platform tenant (hotel ID 0) + `admin` account with `ADMIN_BOOTSTRAP_PASSWORD` and forced password change on first login

## 4. Server: auth, RBAC, core services (README §6, §8, §12) — [P0]

- [ ] **Auth:** real `POST /api/v1/auth/login` (hotel ID + username + password), Argon2id hashing, short-lived sessions + `POST /auth/refresh`, `POST /auth/logout`, `POST /auth/change-password`, login rate limiting (`RATE_LIMIT_LOGIN`)
- [ ] **Session middleware** — currently zero endpoints require a session; CORS also must not be `*` in production
- [ ] **RBAC enforcement:** permissions are currently hardcoded in `crates/hms-core/src/roles.rs`; README §6 requires them to be **data** (`roles`/`role_permissions` tables) enforced in the server (and desktop core)
- [ ] Fix permission-matrix mismatch: code grants `ReportViewTenant` to **reception unconditionally**; README says reception report access **defaults off and is configurable per hotel**
- [ ] Add missing `Hotel` model in `hms-core` (no `hotels` struct exists at all) + hotel CRUD API (admin only): `GET/POST /hotels`, `GET/PATCH /hotels/{id}`
- [ ] Users API: `GET/POST /users`, `PATCH /users/{id}` (manager can create accounts below manager level)
- [ ] Roles API: `GET /roles`, `PUT /roles/{id}/permissions`
- [ ] Room types & rooms APIs: `GET/POST/PATCH` + `PATCH /rooms/{id}/cleanliness`
- [ ] Guests APIs + guest history (per §4)
- [ ] Reservations APIs: list/create/detail/edit, `check-in`, `check-out`, `cancel`, availability search — with keyset pagination (never `OFFSET`)
- [ ] Payments APIs: `GET/POST /reservations/{id}/payments`, `POST /payments/{id}/reverse` (append-only ledger, corrections = reversal + new payment, never edits)
- [ ] Folio: folio transactions per reservation (model exists, no API/table)
- [ ] Board endpoint: `GET /board?from=&to=`
- [ ] Price rules API + price-rule engine (seasonal pricing; validation helpers exist in `validation.rs` but nothing calls them)
- [ ] Channels: per-hotel configurable channel list (currently a hardcoded `Channel` enum — README §4/§7 says configurable per hotel)
- [ ] **Audit log writer** for every change (who/what/when/old/new) + `GET` endpoint for change history
- [ ] Wire up `daily_revenue` aggregation **in the same transaction** as every reservation/payment change (§10) — the struct exists, no logic does
- [ ] OpenAPI spec + generate TypeScript types from Rust (`ts-rs` or openapi) into `frontend/src/types/generated/`
- [ ] Align Rust models with README §7: `ReservationStatus` uses `ConflictNeedsReassignment` vs documented `conflict`; `RoomCleanliness` has extra `OutOfOrder` state vs §6's four states — confirm and document

## 5. Web app (README §3, §4, §5) — [P0] *(does not exist yet)*

- [ ] Scaffold React + Vite + TypeScript SPA (static, no Next.js server features — required for desktop embedding) — decide location: `apps/web` (README) vs `frontend/` (current empty dir)
- [ ] `pnpm` project files, ESLint, VS Code config (README §3)
- [ ] API client layer from generated types (`src/api/`)
- [ ] **Room calendar / resource timeline board** (rooms as rows, dates as columns) — the centerpiece UI
- [ ] Pages: reservations list/detail/create/edit, guests, rooms & room types, housekeeping board (cleanliness only), users/roles, payments/folio, settings
- [ ] Reports pages with charts (ECharts or Recharts): earnings by channel/day/month/room, occupancy, ADR, RevPAR — with date-range filter and CSV/PDF export
- [ ] **i18n: Turkish + English** (`src/i18n/`), TRY + other currencies
- [ ] Role-aware UI: hide what a user may not do (housekeeping sees room board only — no names, prices, payments)
- [ ] Mobile-friendly responsive layout (§4)
- [ ] Login page (hotel ID + username + password) and forced password-change flow
- [ ] Style match to app.hmsflow.com (layout, navigation, colors, component style)

## 6. Offline desktop app + sync (README §11) — [P1]

- [ ] Create `crates/hms-sync` (client sync engine: outbox, pull cursor, retry) — **crate does not exist**
- [ ] Replace the placeholder `crates/hms-desktop/src-tauri/src/main.rs` (currently creates `/tmp/hermes-*` dirs — wrong for Windows and not Tauri):
  - [ ] Add Tauri 2 dependencies and shell (`tauri.conf.json`, icons, WebView2)
  - [ ] Embed the same React UI (`apps/web` build)
- [ ] SQLite + **SQLCipher** encrypted local DB; key protected via Windows DPAPI / Credential Manager
- [ ] Local desktop migrations directory (`desktop-migrations/`) — empty/missing
- [ ] Outbox: every local write appends an operation in the same transaction; push to `POST /api/v1/sync/push` (idempotent by operation ID)
- [ ] Pull: `GET /api/v1/sync/pull?since=<change_seq>` using per-hotel monotonic counter (never `updated_at`)
- [ ] Server side: `POST /sync/push` / `GET /sync/pull` handlers, `sync_ops` idempotency table, `change_seq` row-lock assignment inside write transactions
- [ ] Conflict rules by data type (§11): payments append-only; housekeeping last-write-wins; rooms/users/price-rules server wins; reservations → DB constraint → later booking accepted as `conflict`, never silently dropped
- [ ] Clock-skew protection: reject far-future client timestamps (validation variant exists in `validation.rs`, unwired)
- [ ] Offline login: cache password verifier + permissions, 14-day expiry (`OFFLINE_LOGIN_DAYS`), lockout of deactivated users at next sync (`OfflineLoginCache` type exists, logic does not)
- [ ] Sync protocol versioning + schema migrations on both sides (`SYNC_PROTOCOL_MIN_VERSION`)
- [ ] Status UI: online/offline state, last-synced time, pending op count, unresolved conflicts; warnings about offline channel-booking overbooking risk and card payments
- [ ] Tauri auto-updater + `.msi` installer packaging

## 7. Reports (README §10) — [P1]

- [ ] Report queries must read `daily_revenue`, never raw reservations
- [ ] Earnings per channel (donut + bar), per day (line), per month (bar + YoY line), per room/room type (ranked bar + heatmap)
- [ ] Occupancy %, ADR, RevPAR (line / KPI cards)
- [ ] Collected-vs-accrued toggle (default collected) — **and confirm with hotels which they expect** (open question in §10)
- [ ] CSV + PDF export for every report
- [ ] Platform admin sees all hotels; hotel users see only their own
- [ ] Desktop computes the same reports from local data when offline

## 8. Tests (README §16) — [P1] *(currently zero tests)*

- [ ] Permission matrix test: every role × every action matches §6
- [ ] Tenant isolation test: hotel A can never read/write hotel B (RLS)
- [ ] Double-booking race test: concurrent bookings for one room never both succeed
- [ ] Report integrity test: report totals == sum of raw payments
- [ ] Sync tests: offline N days then sync; two offline clients same room → `conflict`; server outage mid-sync; interrupted/repeated sync → no duplicates; clock-skewed client cannot win
- [ ] `tests/` directory for integration tests (missing) + CI running `cargo test` and `cargo clippy`

## 9. Seed data + load testing (README §13) — [P1]

- [ ] Seed generator (`seed/` is empty; README references `cargo run -p hms-seed` — **no such crate exists**): 1,000 hotels × 50 rooms × 2 years of reservations/payments
- [ ] k6 load-test scripts (`loadtest/` or `benchmarks/`, currently empty)
- [ ] Prove targets: room board / availability search / reservation save p95 < 200 ms; full-year report < 500 ms
- [ ] Publish results in `loadtest/REPORT.md`
- [ ] Caching of read-heavy data (room types, rooms, today's board) with invalidation on write; connection pooling, response compression, hashed static assets (§13)

## 10. Security & compliance (README §8) — [P1]

- [ ] Guest data (KVKK/GDPR): per-hotel export and deletion endpoints, encryption at rest where practical
- [ ] Encrypt sensitive guest fields at rest (models have `*_encrypted` fields — no key management exists)
- [ ] Rate limiting actually enforced (middleware is a pass-through today)
- [ ] Production CORS lockdown, TLS termination, security headers
- [ ] Ensure reference-site credentials (`SITE1_*`, `SITE2_*`) stay only in env — never in code/logs/commits (verify the captured `flow_*.js`/`inspect_*.py` contain no secrets)

## 11. Deployment & operations (README §17) — [P2]

- [ ] Container image for the server behind a TLS-terminating reverse proxy
- [ ] PostgreSQL automated backups + replication (desktop copy is explicitly not a backup)
- [ ] Monitoring: p95/p99 latency, error rate, sync lag per hotel, DB connections, slow queries
- [ ] Migration runbook (server start = Postgres migrations, app start = SQLite migrations)

## 12. Documentation (README §5) — [P2]

- [ ] `docs/`: API reference, sync protocol spec, design decisions
- [ ] Keep README §4 inventory and §14 commands truthful as implementation lands (README currently states commands/endpoints are "intended interfaces" not yet verified)

## 13. Roadmap / future (README §18) — [P2]

- [ ] Feature-parity pass against site 2 (after §4 inventory is completed)
- [ ] Mobile app for the hotel system (reusing API + sync protocol) — planned after web + Windows apps
- [ ] Optional local-network hub so several reception PCs in one hotel see each other while offline
- [ ] Out-of-scope decisions from the site-2 inventory (see §2 above) — invoices, stock/expenses, agencies, currency exchange, channel-booking imports, packages/licensing

---

### Suggested order of work

1. Phase 1 (foundation fixes) → 2. Phase 2 (feature inventory, per README: *before feature work starts*)
2. Phase 3 (schema) → Phase 4 (auth/RBAC/APIs) → Phase 5 (web app)
3. Phase 7 (reports, needs `daily_revenue` from Phase 4) → Phase 8 (tests) → Phase 9 (seed + load test)
4. Phase 6 (desktop + sync, needs a stable API) → Phase 10–12 (security, ops, docs)
5. Phase 13 (parity pass, mobile, hub)

### Open questions to resolve

- Earnings: **collected** vs **accrued**? (README §10 — confirm with hotels)
- Reception report access: default off + configurable — current code disagrees (§6)
- Final directory layout: `apps/web` + `apps/desktop` (README §5) vs `frontend/` + `crates/hms-desktop` (current repo)
- Which site-2 modules beyond README §4 are in scope (invoices, stock, expenses, agencies, currency exchange, channel bookings, notifications)?
- Server language fallback: Rust is failing to compile at the scaffold stage — README §3 allows TypeScript (Fastify) with all rules unchanged if Rust proves unworkable.
