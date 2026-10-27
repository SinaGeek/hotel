# TODO â€” Unified HMS: remaining work

Generated from an analysis of the repository against `README.md` (the target design).
Legend: **[BLOCKER]** stops everything else Â· **[P0]** critical path Â· **[P1]** important Â· **[P2]** later / polish

---

## 0. Current state (what exists today)

| Area | Status |
|---|---|
| README (target design) | complete |
| Rust workspace skeleton | exists: `crates/hms-core`, `crates/hms-server`, `crates/hms-desktop/src-tauri` |
| `hms-core` models / roles / sync types / validation | first draft written, compiles with warnings |
| `hms-server` | stub only (`/health` + fake login), **does not compile** (15 errors) |
| `hms-desktop` | placeholder `main.rs` creating `/tmp` dirs â€” not a Tauri app, no SQLite, no sync |
| Web app (`apps/web` / `frontend/`) | **does not exist** â€” only an empty `frontend/src/types/generated/` |
| PostgreSQL migrations (`migrations/`) | **empty directory** â€” no schema at all |
| Seed data (`seed/`) | **empty directory** |
| Load tests (`benchmarks/`, `loadtest/`) | **empty directory** |
| Tests (`tests/`) | none anywhere in the repo |
| `docker-compose.yml`, `.env.example` | **missing** |
| Feature inventory (README Â§4) | all 13 rows still `pending` |
| Reference-site artifacts | `flow_index.js`, `flow_app.js`, `inspect_*.py` captured (good raw material for Â§4) |
| Git | repository is **not under version control** |

**Bottom line:** the project is at roughly 5% of the README roadmap. Almost everything in sections 4â€“17 remains to be built.

---

## 1. Foundation fixes (do first) â€” [BLOCKER] âœ… DONE (commit `52728de`)

- [x] **Fix `cargo check --workspace` failure** â€” `crates/hms-server/src/main.rs` had 15 compile errors:
  - [x] Missing imports: `Router`, `Json`, `Arc`, `StatusCode`, `HeaderValue`, `axum::routing::{get, post}`
  - [x] `hms_core::ApiResponse` did not exist â€” added generic `ApiResponse<T>` to `hms-core/src/api_types.rs`
  - [x] Duplicate `use` lines removed
- [x] **Resolve duplicate modules in `hms-core`** â€” deleted `request_types.rs` (byte-identical to `api_types.rs`), removed its `mod`/`pub use` from `lib.rs`
- [~] **Clean unused-dependency warnings** â€” unused imports fixed; the remaining flagged deps (`sqlx`, `argon2`, `jsonwebtoken`, `tower-http`, `rusqlite`, `reqwest`, `thiserror`) are **kept intentionally**: each is used in the imminent phases (3/4: server DB+auth, 6: desktop). Remove them only if their phase is abandoned.
- [x] **Decide final repo layout** â€” aligned to README Â§5: desktop crate moved to `apps/desktop/src-tauri` (workspace member + `hms-core` path updated); web app will be `apps/web` (Phase 5); crates stay under `crates/`
- [x] **`git init`** â€” installed portable MinGit 2.56 to `%LOCALAPPDATA%\Tools\mingit` (no admin needed), added to user PATH, `.gitignore` created, first commit made
- [x] **Create `docker-compose.yml`** (PostgreSQL 16 + healthcheck; server service commented until server image exists) and **`.env.example`** (all README Â§15 vars incl. SITE2_*)
- [x] Implement the stub `rate_limiter.rs` â€” real per-key fixed-window limiter + 3 unit tests passing; dummy `init.rs` deleted

## 2. Feature inventory (README Â§4) â€” [P0] âœ… DONE

- [x] Complete the Â§4 table by verifying every area against **app.hms.gen.tr** (site 2) â€” verified live 2026-10-27, logged in as the user's account (no captcha appeared; the OpenCode browser holds the session for future visits)
- [x] Route tree enumerated from the live menu (56+ routes) + all report sub-pages inspected
- [x] Findings recorded in `docs/feature-inventory.md` (observations per README Â§4 area + evidence screenshots in `docs/screenshots/` â€” room board, reservation detail, housekeeping, channel list)
- [x] Scope reconciled: 13 Â§4 rows marked **verified** in README Â§4; site-2-only modules (invoices/e-archive, agency currents, currency exchange, cash/banks, online payments, open sale/expenses/stock, SMS/mail tooling, Wi-Fi/IP, notifications, end-of-day, licensing) marked **deferred/out** in the inventory doc â€” README now links to it
- Notes for later phases (recorded in inventory doc): site-2 cleanliness = Dirty/Clean + Out of Order; reservation detail embeds Transaction Logs; board price/availability is per room_type Ã— date; no per-channel earnings report on site 2

## 3. Database schema (README Â§7â€“Â§9) â€” [P0] âœ… DONE (commit `9eb9f9b` + runtime verification on 2026-10-27)

- [x] PostgreSQL migrations in `migrations/` for all tables:
  - [x] `hotels` (with per-hotel `next_change_seq` counter), `users`, `roles`, `role_permissions` + `hotel_permission_overrides`
  - [x] `room_types`, `rooms`, `guests`, `channels` (configurable per hotel), `price_rules`
  - [x] `reservations` (daterange `stay`), `payments` (append-only grants), `folio_transactions`
  - [x] `change_log`, `sync_ops`, `daily_revenue`, `audit_log`
- [x] `CREATE EXTENSION btree_gist` + `no_double_booking` EXCLUDE constraint â€” **verified live**: overlapping confirmed booking rejected by DB; `conflict` status allowed (sync-collision path works)
- [x] Indexes per Â§13: `(hotel_id, room_id, stay)`, `(hotel_id, status)`, `(hotel_id, created_at)` + keyset support
- [x] Row-level security on every table â€” **verified live as non-superuser `hms_app`**: hotel 4442 scope sees only its rows; other-hotel scope and no-scope return 0; cross-hotel INSERT blocked by WITH CHECK
- [x] Migrations run on server start (`sqlx::migrate!("../../migrations")` in `hms-server/src/main.rs`) â€” verified live
- [x] Platform tenant (hotel 0) seeded by migration; `admin` account created by server bootstrap with `ADMIN_BOOTSTRAP_PASSWORD` (or a generated one printed once), `force_password_change = true` â€” verified live

**Dev DB note:** Docker Desktop is unusable on this machine (virtualization disabled in firmware until the user enables VT-x in BIOS). Using portable PostgreSQL 16.9 installed to `%LOCALAPPDATA%\Tools\pgsql` with data at `%LOCALAPPDATA%\Tools\pgdata` (started via `pg_ctl`, database `hms`, superuser `postgres`, trust auth, port 5432). When VT-x is enabled the Docker path (`docker compose up -d db`) becomes available again.

## 4. Server: auth, RBAC, core services (README §6, §8, §12) ✅ DONE (verified live on portable Postgres, 2026-10-27)

- [x] **Auth:** real `POST /api/v1/auth/login` (hotel ID + username + password), Argon2id hashing (Argon2id via `argon2` crate `Argon2::default()`), short-lived sessions (24 h × `SESSION_DAYS` claim `fpc`) + `POST /auth/refresh`, `POST /auth/logout`, `POST /auth/change-password`, login rate limiting (`RATE_LIMIT_LOGIN`, default 10/window 60 s). **Verified live:** wrong password ×10 ⇒ 10th returns 429 "too many login attempts"; change-password flow: login with fpc → change → re-login, fpc=false; all non-auth endpoints blocked with 403 while fpc pending.
- [x] **Session middleware** — `auth_waterfall` route-layers the whole `/api/v1` nest (auth nest excluded); JWT `Bearer` extracted by `AuthUser` in handlers for scoping. CORS still `*` in dev (README §8: lock down in production — production lockdown lives in Phase 10).
- [x] **RBAC enforcement** — `auth::has_permission(pool, role, hotel, perm)`: DB matrix (`role_permissions` + `hotel_permission_overrides`) lookup, `hms-core::roles` fallback; platform admin short-circuits to true. **Verified live:** manager gets 19 perms on login, housekeeping exactly 2; housekeeping 403 on `/reservations` & `/users`; endpoint guards added on every mutating/reading handler.
- [x] Permission-matrix mismatch fixed: reception's `ReportViewTenant` **off** in code fallback too; per-hotel overridable via `hotel_permission_overrides` (roles API writes overrides).
- [x] Hotel CRUD API (admin only): `GET/POST /hotels`, `GET/PATCH /hotels/{id}` (platform scope for admin, tenant scope for hotel users).
- [x] Users API: `GET/POST /users`, `PATCH /users/{id}` (rank check: managers create only below manager level; **verified** — manager created housekeeping user via API; platform admin may pass `hotel_id` to provision another hotel).
- [x] Roles API: `GET /roles` (matrix), `GET/PUT /roles/{role}/permissions` (only per-hotel configurable perms, currently `report_view_tenant`).
- [x] Room types & rooms APIs: `GET/POST/PATCH` + `PATCH /rooms/{id}/cleanliness` (last-write-wins per README §11) — **verified**: cleanliness update persisted + audit row written.
- [x] Guests APIs: `GET/POST /guests` (search by name; encrypted identity columns written on create).
- [x] Reservations APIs: list (keyset pagination on created_at+id, never OFFSET), create, detail, edit, `check-in`, `check-out`, `cancel`. **Verified live end-to-end**: create (3-night stay priced from base 3000 TRY/night, daily_rates JSONB), double-booking overlap → INSERT fails 409 (EXCLUDE constraint), check-in → checked_in, edit-after-checkout → 409, check-out → checked_out.
- [x] Payments APIs: `GET/POST /reservations/{id}/payments`, `POST /payments/{id}/reverse` — append-only (INSERT only), reversal row negative with `is_reversal=true`. **Verified**: payment 1000 TRY + reversal → payments list shows `credit_card:+100000 / refund:-100000:rev=true`; reversing a reversal → 400; daily_revenue payment day nets to 0 after reversal (README §7 "corrections = reversal, never edits").
- [x] Folio: `GET/POST /reservations/{id}/folio` — extra revenue lands on the stay dates' `daily_revenue.extra_revenue_cents` in the SAME transaction (rows auto-seeded if none exist).
- [x] Board endpoint: `GET /board?from=&to=` — rooms rows with stays, cleanliness, seasonal price; per-day availability/occupied (per-day subquery — deduped) + cheapest price across room types (rules → base fallback). **Verified**: correct occupancy counts; `conflict` reservations shown but NOT counted occupied; housekeeping sees guest_name=None and no guest financials.
- [x] Price rules API: `GET/POST /price-rules` with `hms-core::validation::validate_price_rule` now actually called (added `status_code()` + `impl From<ValidationError> for ApiError`).
- [x] Channels: per-hotel configurable list, `change_log` entity extended with `channel` (migration `0007`); create restricted to `room_manage`. *TODO (Phase 5+): first-touch seeding should add **missing** standard kinds even when the list is not empty — hotel 4442 currently only has `Direct` from Phase-3 seeding.*
- [x] **Audit log writer** — `db::write_audit` in every write transaction (who/what/when/old/new). **Verified**: 9 distinct audit row types present after the live run. `GET /audit` read endpoint deferred to Phase 7/10.
- [x] **`daily_revenue` wired in the same transaction** — `upsert_daily_revenue` on reservation create/edit/cancel; payment ± on record/reverse; folio extras via UPDATE. **Verified via psql**: room 202 rows for Nov 1–3 = 300000 room + 10000 extra revenue, occupied=t; payment day 2026-10-27 net 0 after reversal.
- [ ] OpenAPI + TS type generation — **still open**: Phase 5 ships hand-mirrored types (`apps/web/src/types/generated/api.ts`); codegen pending (Phase 12)
- [x] Status naming aligned: `ReservationStatus::ConflictNeedsReassignment` serializes to `conflict` (README §9 wording); cleanliness model = README's 4 states + `out_of_order` (kept; site-2 discrepancy documented in the inventory doc).

**Verification setup (dev):** server runs with `DATABASE_URL=postgres://postgres:postgres@localhost:5432/hms`, `ADMIN_BOOTSTRAP_PASSWORD`, `SESSION_SECRET` set; logins: hotel 0 admin, hotel 4442 test users `mtest` (manager) / `h1` (housekeeping) with Argon2 hashes and first-login forced password change. Live test data: room 202 reservation `01a17e15-6971-763c-95d7-d345ef4e2f34` (checked_out), payment+reversal, minibar folio, room 101 cleanliness=cleaning.

**Known follow-ups:** (1) the server currently connects as superuser `postgres`, so RLS is BYPASSED at app level; production must connect as the non-superuser `hms_app` (RLS itself verified in Phase 3) — wire this in Phase 10 hardening and document in README §14. (2) Board seasonal price & per-room rate fall back to the room-type base price when no rule matches (rules take precedence via `date_from DESC LIMIT 1`). (3) `PATCH /guests/{id}` is a stub until guest edit is needed. (4) `channels` first-touch seeding only seeds when the list is empty (see above).

## 5. Web app (README Â§3, Â§4, Â§5) â€” [P0] *(core done & live-verified; commit `6eb4b6e`; UI is ENGLISH-ONLY per user decision)*

- [x] Scaffold React + Vite + TypeScript SPA (static, no Next.js server features â€” required for desktop embedding) â€” located at `apps/web` (README layout)
- [x] `pnpm` project files, ESLint (README Â§3); VS Code config inherited from workspace root
- [x] API client layer (`src/api/client.ts`) â€” **hand-mirrored wire types** in `src/types/generated/api.ts` (ts-rs/OpenAPI codegen still open, see below); token session in localStorage, 401 â†’ drop session; `sessionHasPermission` helper
- [x] **Room calendar / resource timeline board** (`src/pages/Board.tsx`) â€” rooms as rows, dates as columns, CSS grid **explicit placement, stays span their nights** (`gridColumn: n / span m` â€” verified 3-night stay = 138px vs 42px single night); header shows `available/total` + cheapest price; click empty cell â†’ create-reservation modal; click stay â†’ detail modal with check-in/out/cancel, payments (+refund reversal shown as `-TRY x`), folio. **Live-verified end-to-end as manager**: create â†’ check-in â†’ record TRY 3,000 cash â†’ folio minibar TRY 150 â†’ refund â†’ check-out, all through the UI.
- [x] Pages: reservations list (`Reservations.tsx`, keyset pagination "more"), guests (search/create), rooms & cleanliness dropdown (PATCH verified 200), users list/create, payments/folio inside board detail modal
- [ ] Reports pages with charts (ECharts or Recharts): earnings by channel/day/month/room, occupancy, ADR, RevPAR â€” with date-range filter and CSV/PDF export *(Phase 7 â€” reads `daily_revenue`)*
- [x] **i18n: English-only** (`src/i18n/`) â€” per user decision overrides README's TR requirement; no Turkish literals in `apps/web` (grep-verified pre-commit). Multi-language structure kept for later.
- [x] Role-aware UI â€” AppShell hides nav by permission (housekeeping sees only Dashboard/Room board/Housekeeping); board masks guest names/prices to `â—` for housekeeping; housekeeping server responses contain no names. **Live-verified as `h1`.** Housekeeping-cleanliness page = same Rooms page with select when permitted.
- [ ] Mobile-friendly responsive layout (Â§4)
- [x] Login page (hotel ID + username + password) and forced password-change flow (`ChangePassword.tsx` re-logins to clear fpc flag); server refresh-endpoint bug fixed (`auth.rs`: refresh kept issuing `fpc=false` tokens)
- [ ] Style match to app.hmsflow.com (layout, navigation, colors, component style) â€” clean custom CSS now, cosmetic pass later
- [ ] OpenAPI/ts-rs TS type generation (moved here from Phase 4, then Phase 12 polish) â€” still TODO; types are hand-mirrored, watch for drift
- [ ] Board re-fetch storm fix note: `useCallback` deps held a fresh-object session; fixed with a `sessionRef` (committed) â€” pattern to copy in other pages
- [ ] Dev-infra note: vite **dev** server on 5173 intermittently 502s through the automation-browser proxy â†’ **browser testing uses `pnpm preview` on 4173** (`vite.config.ts` preview proxy `/api` â†’ `:8080`, commit `6eb4b6e`)

## 6. Offline desktop app + sync (README Â§11) â€” [P1]

- [ ] Create `crates/hms-sync` (client sync engine: outbox, pull cursor, retry) â€” **crate does not exist**
- [ ] Replace the placeholder `crates/hms-desktop/src-tauri/src/main.rs` (currently creates `/tmp/hermes-*` dirs â€” wrong for Windows and not Tauri):
  - [ ] Add Tauri 2 dependencies and shell (`tauri.conf.json`, icons, WebView2)
  - [ ] Embed the same React UI (`apps/web` build)
- [ ] SQLite + **SQLCipher** encrypted local DB; key protected via Windows DPAPI / Credential Manager
- [ ] Local desktop migrations directory (`desktop-migrations/`) â€” empty/missing
- [ ] Outbox: every local write appends an operation in the same transaction; push to `POST /api/v1/sync/push` (idempotent by operation ID)
- [ ] Pull: `GET /api/v1/sync/pull?since=<change_seq>` using per-hotel monotonic counter (never `updated_at`)
- [ ] Server side: `POST /sync/push` / `GET /sync/pull` handlers, `sync_ops` idempotency table, `change_seq` row-lock assignment inside write transactions
- [ ] Conflict rules by data type (Â§11): payments append-only; housekeeping last-write-wins; rooms/users/price-rules server wins; reservations â†’ DB constraint â†’ later booking accepted as `conflict`, never silently dropped
- [ ] Clock-skew protection: reject far-future client timestamps (validation variant exists in `validation.rs`, unwired)
- [ ] Offline login: cache password verifier + permissions, 14-day expiry (`OFFLINE_LOGIN_DAYS`), lockout of deactivated users at next sync (`OfflineLoginCache` type exists, logic does not)
- [ ] Sync protocol versioning + schema migrations on both sides (`SYNC_PROTOCOL_MIN_VERSION`)
- [ ] Status UI: online/offline state, last-synced time, pending op count, unresolved conflicts; warnings about offline channel-booking overbooking risk and card payments
- [ ] Tauri auto-updater + `.msi` installer packaging

## 7. Reports (README Â§10) â€” [P1]

- [ ] Report queries must read `daily_revenue`, never raw reservations
- [ ] Earnings per channel (donut + bar), per day (line), per month (bar + YoY line), per room/room type (ranked bar + heatmap)
- [ ] Occupancy %, ADR, RevPAR (line / KPI cards)
- [ ] Collected-vs-accrued toggle (default collected) â€” **and confirm with hotels which they expect** (open question in Â§10)
- [ ] CSV + PDF export for every report
- [ ] Platform admin sees all hotels; hotel users see only their own
- [ ] Desktop computes the same reports from local data when offline

## 8. Tests (README Â§16) â€” [P1] *(currently zero tests)*

- [ ] Permission matrix test: every role Ã— every action matches Â§6
- [ ] Tenant isolation test: hotel A can never read/write hotel B (RLS)
- [ ] Double-booking race test: concurrent bookings for one room never both succeed
- [ ] Report integrity test: report totals == sum of raw payments
- [ ] Sync tests: offline N days then sync; two offline clients same room â†’ `conflict`; server outage mid-sync; interrupted/repeated sync â†’ no duplicates; clock-skewed client cannot win
- [ ] `tests/` directory for integration tests (missing) + CI running `cargo test` and `cargo clippy`

## 9. Seed data + load testing (README Â§13) â€” [P1]

- [ ] Seed generator (`seed/` is empty; README references `cargo run -p hms-seed` â€” **no such crate exists**): 1,000 hotels Ã— 50 rooms Ã— 2 years of reservations/payments
- [ ] k6 load-test scripts (`loadtest/` or `benchmarks/`, currently empty)
- [ ] Prove targets: room board / availability search / reservation save p95 < 200 ms; full-year report < 500 ms
- [ ] Publish results in `loadtest/REPORT.md`
- [ ] Caching of read-heavy data (room types, rooms, today's board) with invalidation on write; connection pooling, response compression, hashed static assets (Â§13)

## 10. Security & compliance (README Â§8) â€” [P1]

- [ ] Guest data (KVKK/GDPR): per-hotel export and deletion endpoints, encryption at rest where practical
- [ ] Encrypt sensitive guest fields at rest (models have `*_encrypted` fields â€” no key management exists)
- [ ] Rate limiting actually enforced (middleware is a pass-through today)
- [ ] Production CORS lockdown, TLS termination, security headers
- [ ] Ensure reference-site credentials (`SITE1_*`, `SITE2_*`) stay only in env â€” never in code/logs/commits (verify the captured `flow_*.js`/`inspect_*.py` contain no secrets)

## 11. Deployment & operations (README Â§17) â€” [P2]

- [ ] Container image for the server behind a TLS-terminating reverse proxy
- [ ] PostgreSQL automated backups + replication (desktop copy is explicitly not a backup)
- [ ] Monitoring: p95/p99 latency, error rate, sync lag per hotel, DB connections, slow queries
- [ ] Migration runbook (server start = Postgres migrations, app start = SQLite migrations)

## 12. Documentation (README Â§5) â€” [P2]

- [ ] `docs/`: API reference, sync protocol spec, design decisions
- [ ] Keep README Â§4 inventory and Â§14 commands truthful as implementation lands (README currently states commands/endpoints are "intended interfaces" not yet verified)

## 13. Roadmap / future (README Â§18) â€” [P2]

- [ ] Feature-parity pass against site 2 (after Â§4 inventory is completed)
- [ ] Mobile app for the hotel system (reusing API + sync protocol) â€” planned after web + Windows apps
- [ ] Optional local-network hub so several reception PCs in one hotel see each other while offline
- [ ] Out-of-scope decisions from the site-2 inventory (see Â§2 above) â€” invoices, stock/expenses, agencies, currency exchange, channel-booking imports, packages/licensing

---

### Suggested order of work

1. Phase 1 (foundation fixes) â†’ 2. Phase 2 (feature inventory, per README: *before feature work starts*)
2. Phase 3 (schema) â†’ Phase 4 (auth/RBAC/APIs) â†’ Phase 5 (web app)
3. Phase 7 (reports, needs `daily_revenue` from Phase 4) â†’ Phase 8 (tests) â†’ Phase 9 (seed + load test)
4. Phase 6 (desktop + sync, needs a stable API) â†’ Phase 10â€“12 (security, ops, docs)
5. Phase 13 (parity pass, mobile, hub)

### Open questions to resolve

- Earnings: **collected** vs **accrued**? (README Â§10 â€” confirm with hotels)
- Reception report access: default off + configurable â€” current code disagrees (Â§6)
- Final directory layout: `apps/web` + `apps/desktop` (README Â§5) vs `frontend/` + `crates/hms-desktop` (current repo)
- Which site-2 modules beyond README Â§4 are in scope (invoices, stock, expenses, agencies, currency exchange, channel bookings, notifications)?
- Server language fallback: Rust is failing to compile at the scaffold stage â€” README Â§3 allows TypeScript (Fastify) with all rules unchanged if Rust proves unworkable.
