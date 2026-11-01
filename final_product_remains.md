# HMS — final product: what is done, what remains

_Last updated: 2026-11-01. Written as a handoff: everything below was verified
in this workspace unless marked otherwise._

Target (from `README.md`): a multi-tenant hotel management system for 1,000+
hotels (~50 rooms each), with a React web app and a Windows desktop app (Tauri 2)
that keeps working offline and syncs when the connection returns.

Repository layout:

```
crates/hms-core      shared models, roles/permissions, sync types, validation, guest encryption
crates/hms-server    Axum REST API + sync endpoints + migrations runner + operator bins
crates/hms-sync      client sync engine (outbox, pull cursor, conflict handling)
crates/hms-tests     integration tests + load-test runner
apps/web             React + Vite + TypeScript SPA (also embedded in the desktop app)
apps/desktop/src-tauri  Tauri 2 Windows shell, SQLite mirror, DPAPI, offline reports
migrations/          PostgreSQL schema (RLS, exclusion constraints, audit, change log)
seed/                load-test hotel 4443 + helper SQL
benchmarks/          load-test report and k6 scripts
deploy/              Caddy and nginx reference configs
scripts/             PostgreSQL backup script
docs/                API snapshot, feature inventory, runbooks
```

---

## 1. How to run it (verified)

### 1.1 Prerequisites

- PostgreSQL 16+ (verified on PostgreSQL 18.6, Windows)
- Rust toolchain (stable), Node.js 22 + pnpm, and the Tauri 2 Windows
  prerequisites (WebView2, MSVC build tools) for the desktop build

### 1.2 Environment

```bash
cp .env.example .env
```

Required values in `.env`:

| Variable | Notes |
|---|---|
| `DATABASE_URL` | e.g. `postgres://postgres:<pw>@localhost:5432/hms` |
| `SESSION_SECRET` | ≥ 32 bytes; generate with `openssl rand -hex 32` |
| `ADMIN_BOOTSTRAP_PASSWORD` | initial password for the hotel-0 `admin` (forced change on first login) |
| `GUEST_FIELD_ENCRYPTION_KEY` | base64 32 bytes; required for any guest identity write |
| `CORS_ALLOWED_ORIGINS` | empty in dev (localhost Vite origins are the default); exact https origins in production |
| `APP_ENV` | `development` (default) or `production` |
| `TEST_USERS` | optional dev bootstrap `hotel:user:pass:role,...` |

`.env` and `userpass.md` are gitignored — never commit them.

### 1.3 Database

```bash
# Docker
docker compose up -d db

# or a local PostgreSQL install
#   initdb / createdb are provider-specific; the server creates the schema itself
createdb -h localhost -U postgres hms
```

Migrations run automatically when the server starts.

### 1.4 Server

```bash
cargo run -p hms-server           # listens on 0.0.0.0:8080
curl http://localhost:8080/health # {"success":true,...}
curl http://localhost:8080/metrics # Prometheus text
```

### 1.5 Web app

```bash
cd apps/web
pnpm install
pnpm dev       # http://localhost:5173 (proxies /api to :8080)
pnpm build     # static bundle in apps/web/dist
pnpm preview   # http://localhost:4173 with the same API proxy
```

Login: hotel `0`, username `admin`, password from `ADMIN_BOOTSTRAP_PASSWORD`
(then forced change). Platform admin sees **Admin Panel** in the sidebar.

### 1.6 Admin panel (`/admin`)

- create a hotel (hotel code = the `hotels.id` value, e.g. `4442`)
- the hotel is created together with a manager account
  (`manager` / `1234`; change it on first login)
- add users per hotel with role `manager` / `reception` / `housekeeping`
- enable/disable a hotel

### 1.7 Seed + load test (optional)

```bash
psql "$DATABASE_URL" -f seed/loadtest.sql      # hotel 4443 (destructive to 4443 only)
cargo run -p hms-tests -r --example loadtest -- --base http://localhost:8080 --users 16 --n 200
```

### 1.8 Desktop

```bash
cd apps/web
VITE_API_BASE=http://localhost:8080/api/v1 pnpm build   # needed: the desktop is served from tauri.localhost
cd ../desktop/src-tauri
cargo tauri dev      # online dev, uses devUrl http://localhost:4173
cargo tauri build    # MSI in target/release/bundle/msi/
```

Desktop runtime env: `HMS_API_BASE` (default `http://localhost:8080`) and
`SYNC_INTERVAL_SECS` (default `30`). Local data lives in
`%APPDATA%\com.hms.desktop\hms.sqlite`.

---

## 2. What is DONE

### 2.1 Database (`migrations/`)

- `hotels` (per-hotel sync counter), `users`, `roles`, `role_permissions`,
  `hotel_permission_overrides`
- `room_types`, `rooms`, `guests`, `channels`, `price_rules`
- `reservations` with a PostgreSQL `daterange` and an `EXCLUDE` constraint that
  makes double booking impossible even under concurrency
- `payments` (append-only, reversal rows), `folio_transactions`
- `change_log` (monotonic sequence, sync stream), `sync_ops` (idempotency),
  `daily_revenue` (report aggregate), `audit_log`
- Row-level security on every tenant table; platform-admin scope for
  cross-hotel admin work
- Indexes for board/availability/report paths and keyset pagination

### 2.2 Server (`crates/hms-server`)

- Auth: `POST /api/v1/auth/login`, `/refresh`, `/logout`, `/change-password`;
  Argon2id hashes; 24 h tokens; forced password change; per-hotel/username/IP
  login rate limiting (`RATE_LIMIT_LOGIN`, fixed window)
- RBAC: DB-backed role matrix + per-hotel overrides, platform admin bypass;
  every mutating/reading handler checks the specific permission
- Hotels: list/get/create/update (admin only)
- Users: list (platform admin sees every hotel, tenant users see their own),
  create/patch (rank check; managers cannot create peers/admins); initial
  passwords may be short (hotel provisioning) and every account is forced to
  change it on first login
- Roles: read matrix, per-hotel overridable permissions (`report_view_tenant`)
- Rooms: room types + rooms CRUD, cleanliness PATCH (last-write-wins)
- Guests: list/search/create/**PATCH**/export/delete; identity fields encrypted
  at rest (XChaCha20Poly1305); export/delete are the GDPR/KVKK endpoints;
  `guest_export` / `guest_delete` permissions
- Reservations: keyset list + count, get, create, edit, check-in, check-out,
  cancel; pricing via price rules with base-price fallback; `daily_revenue`
  maintained in the same transaction
- Payments: record + reverse (append-only ledger), folio extras
- Board: `GET /board?from&to` — rooms with stays, cleanliness, per-day
  availability, cheapest price (batched; no N+1)
- Price rules, channels (with standard-channel seeding)
- Reports (all read `daily_revenue`): summary, by-channel, by-day, by-room,
  forecast, in-house, main courante
- Audit: `GET /audit` (`audit_log_view`)
- Sync: `POST /sync/push`, `GET /sync/pull?since=` — idempotent by operation id,
  monotonic `change_seq`, protocol-version gate (426), conflict rules
  (reservations → `conflict`, payments append-only, cleanliness LWW,
  rooms/users/price rules server-wins), clock-skew rejection, RLS on apply
- Hardening: CORS allowlist, security headers, compression, session-secret
  validation in production, DB-pool pre-warm
- Observability: `/health`, Prometheus `/metrics` (requests, 5xx, latency
  histogram, pool gauges, uptime, latest change_seq)
- **New:** `rotate-guest-keys` operator binary (dry-run, idempotent, audited) —
  see `docs/key-rotation.md`

### 2.3 Web app (`apps/web`)

- English-only SPA (i18n scaffolding kept), responsive at ≤768 px
- Login + forced password change
- Dashboard: room cards, KPI/status chips, search, quick reservation
- Monthly Room board: calendar/list, 7/15/month ranges, pager, drag-to-create,
  booking detail modal with check-in/out/cancel, payments, folio
- Bookings list: status tabs, date filters, keyset paging with true count
- Guests: search, create, CSV export, birthday/plate views
- Rooms + Housekeeping: inline cleanliness with permission gating
- Reports hub + revenue analysis (charts, accrued vs collected, CSV, monthly
  rollup, last-90-days preset) + forecast / in-house / main courante
- Channels list, Users page
- **Admin panel** (`/admin`): hotels + users + enable/disable. Live-verified in
  the browser as platform admin: created hotel `5001` (manager account
  `manager`/`1234` created and able to log in with 19 permissions), saw the
  cross-hotel user list, toggled hotel active state.
- Desktop bridge: sync status pill, offline reports fallback
- API base is configurable at build time via `VITE_API_BASE` (required for the
  packaged desktop app)

### 2.4 Desktop (`apps/desktop/src-tauri`)

- Tauri 2 shell embedding the same web build
- SQLite mirror (12 tables) + outbox + pull cursor + conflict counters
- Background sync worker (`SYNC_INTERVAL_SECS`, `HMS_API_BASE`)
- Offline login: Argon2 verifier cache, 14-day expiry, lockout on deactivation at
  next sync
- DPAPI-protected secrets (session token + offline credentials)
- Offline reports computed from the mirror (`offline_reports` command)
- MSI packaging (WiX bundled by the Tauri CLI)

### 2.5 Quality

- Tests: `hms-core` unit tests (encryption, auth/roles/validation), `hms-sync`
  engine + offline-login tests, `hms-tests` (permission matrix, RLS tenant
  isolation, double-booking race, payments append-only, report integrity, live
  sync suite gated on `TEST_API_URL`/`TEST_DATABASE_URL`), server router tests,
  desktop unit tests
- CI (`.github/workflows/ci.yml`): `cargo test`, `clippy`, `pnpm lint`, `pnpm build`
- Load test proven (`benchmarks/REPORT.md`): board p95 109 ms, availability
  13 ms, save 19 ms, full-year report 6.5 ms (targets: 200/200/200/500 ms)
- Deployment assets (new): `Dockerfile`, `.dockerignore`, `docker-compose.yml`
  server service, `deploy/Caddyfile`, `deploy/nginx.conf`,
  `scripts/backup-postgres.ps1`, `docs/deployment.md`

---

## 3. What remains (do these in order)

Each item lists why it matters, the exact steps, and how to prove it is done.
Items 3.1–3.3 are hard blockers for a public release; 3.4–3.6 are product
polish; 3.7+ are roadmap.

### 3.1 [BLOCKER] SQLCipher-encrypted desktop database

**Why:** README §3/§11 require the desktop mirror to be encrypted at rest with
SQLCipher; today `apps/desktop/src-tauri/Cargo.toml` uses
`rusqlite = { features = ["bundled"] }`, so the SQLite file is plaintext
(secrets are still DPAPI-protected, but guest/payment rows are readable).

**Current state:** blocked on the toolchain, not on code: the vendored
`bundled-sqlcipher-vendored-openssl` build needs a C compiler + Perl/OpenSSL
build chain that this machine does not have.

**Steps**

1. Install the Windows build prerequisites:
   - Visual Studio Build Tools (MSVC, C++ workload)
   - Perl (Strawberry Perl) and NASM (needed by the vendored OpenSSL build)
2. Change `apps/desktop/src-tauri/Cargo.toml`:
   ```toml
   rusqlite = { version = "0.32", features = ["bundled-sqlcipher-vendored-openssl"] }
   ```
3. In `apps/desktop/src-tauri/src/local_store.rs`:
   - derive the DB key once per install: 32 random bytes via
     `dpapi::random_bytes()` (already exists), store it as DPAPI-protected value
     in `kv` (reuse the existing DPAPI helpers)
   - on `LocalStore::open`, issue `PRAGMA key = "x'<hex>'"` **before** any other
     statement, then `PRAGMA cipher_memory_security = ON`
   - verify the key: `SELECT count(*) FROM sqlite_master` must succeed
4. Migration path for existing installs: detect an unencrypted DB (open without
   a key, `PRAGMA cipher_version` empty) and run `sqlcipher_export()` into a new
   encrypted file, then swap.
5. Add a test that asserts a plaintext `sqlite3` open of the file fails.

**Acceptance:** `sqlite3 hms.sqlite "select * from guests"` fails with
"file is not a database"; the app still starts, syncs and renders offline
reports; `cargo test -p hms-desktop` passes.

### 3.2 [BLOCKER] Tauri auto-updater

**Why:** README §17 ships "`msi` installer plus Tauri auto-updater"; today only
the manual MSI exists.

**Steps**

1. Decide the signing identity. Generate a minisign keypair:
   `pnpm tauri signer generate -w ~/.tauri/hms.key` → keep the private key in a
   secret store/CI secret, never in the repo.
2. Add `tauri-plugin-updater` (Rust) + `@tauri-apps/plugin-updater` (web) and
   register the plugin in `apps/desktop/src-tauri/src/main.rs` and
   `tauri.conf.json`:
   ```json
   "plugins": { "updater": { "endpoints": ["https://hms.example.com/updates/latest.json"], "pubkey": "<public key>" } },
   "bundle": { "createUpdaterArtifacts": true }
   ```
3. Publish `latest.json` (version, notes, signature, URL) from the release job;
   host the artifacts on the same TLS host.
4. Add a CI `release` workflow that builds the MSI, signs it, and uploads both
   the MSI and `latest.json`.
5. Surface update availability in the webview (the shell already has a status
   pill component to extend).

**Acceptance:** installing version N, publishing N+1, and restarting the app
offers and applies the update; a tampered MSI is rejected.

### 3.3 [BLOCKER] Verify the production stack end-to-end

**Why:** the deployment assets were written on a machine without Docker/TLS, so
they are unverified. Everything about the app is tested; the packaging is not.

**Steps**

1. On a machine with Docker: `docker compose up -d --build` and confirm
   `/health` is green and migrations applied.
2. Put `deploy/Caddyfile` (or `deploy/nginx.conf`) in front, serve
   `apps/web/dist`, confirm HTTPS, SPA deep links (`/board`, `/admin`) and
   `/api` proxying.
3. Set `APP_ENV=production`, a real `CORS_ALLOWED_ORIGINS`, a ≥32-byte
   `SESSION_SECRET`; confirm the server refuses to start with a missing/short
   secret and that cross-origin requests from an unlisted origin are denied.
4. Scrape `/metrics` from Prometheus and load the four suggested alerts in
   `docs/deployment.md`.
5. Run `scripts/backup-postgres.ps1`, restore the dump into a scratch database,
   and diff a few row counts.
6. Build the desktop MSI against the HTTPS API base and confirm login + sync
   through the proxy (this also proves `VITE_API_BASE`).

**Acceptance:** a checklist run in `docs/deployment.md` marked green on the
target host, including a restore drill.

### 3.4 Desktop end-to-end verification (offline sync + offline reports)

**Why:** the desktop code and 11 unit tests pass, but no automated run has
driven a real WebView2 session. Known caveats: per-night rates are absent in the
mirror (room totals are spread evenly), payment dates use sync-apply time, and
reservation/payment writes are not local-first yet.

**Steps**

1. On Windows with WebView2, `cargo tauri dev` against a seeded server.
2. Log in online, let one sync cycle complete, verify the pill shows
   `Synced` and `%APPDATA%\com.hms.desktop\hms.sqlite` contains mirror rows.
3. Disconnect the network: create a reservation **in the web UI** (it will fail
   — expected, writes are not local-first yet), then verify offline reports
   render from the mirror and the pill flips to `Offline - N queued`.
4. Reconnect and verify the queue drains, `sync_ops` has `applied`, and the
   pull cursor advances.
5. Two-instance test: two desktops book the same room offline → the later one
   arrives as `conflict` (server-side EXCLUDE already proven by
   `crates/hms-tests/tests/sync_live.rs`).
6. Fix the known mismatches surfaced by the run (documented above) or record
   them as accepted deviations.

**Acceptance:** a repeatable manual script + screenshots (do not commit raw
captures) and a `docs/desktop-verification.md` recording results.

### 3.5 OpenAPI + generated TypeScript types

**Why:** `apps/web/src/types/generated/api.ts` is hand-mirrored; drift is
possible (README §3 asks for generation).

**Steps**

1. Add `utoipa` + `utoipa-axum` to `crates/hms-server`; derive `ToSchema` on the
   DTOs and annotate handlers (`#[utoipa::path]`) or use `utoipa-axum::OpenApi`
   route collection.
2. Serve `GET /api/v1/openapi.json`.
3. Add `openapi-typescript` to `apps/web` and a `pnpm gen:api` script that reads
   the JSON (from a running dev server or a checked-in artifact) into
   `src/types/generated/api.ts`.
4. Replace hand types module-by-module, keeping the exported names stable.
5. Wire `pnpm gen:api && git diff --exit-code` into CI to catch drift.

**Acceptance:** generated file compiles, `pnpm build` passes, CI fails when the
server schema changes without regenerating.

### 3.6 Reference visual polish

**Why:** functional parity is done; a few pages deviate from the reference
(`/guests`, `/channels` table/chip styling).

**Steps:** compare each page against the (non-committed) reference captures,
tighten table headers, pills, spacing; keep English-only and mobile behaviour.

**Acceptance:** side-by-side review notes, no regressions in the responsive
pass (375 px / 700 px).

### 3.7 Read caching (only if a target breaks)

**Why deferred:** measured numbers are well inside targets (board p95 109 ms,
report 6.5 ms). README §13 lists caching; adding it now risks stale data.

**Steps if needed:** cache room types/rooms/board-per-window in-process with
invalidation on write, or add Redis behind a trait; measure before/after with
`crates/hms-tests/examples/loadtest.rs`.

### 3.8 1,000-hotel scale seed + load test

**Why:** the proven load test uses one hotel with 60 rooms. README §1 targets
1,000 hotels × ~50 rooms.

**Steps**

1. Extend `seed/loadtest.sql` (or add `seed/scale.sql`) to generate ~1,000
   tenants, 50 rooms each, 2 years of reservations, and `daily_revenue` rows.
   Keep it idempotent and separate from dev data.
2. Point the runner at a hotel in the middle of the id range; run board,
   availability, save and full-year report scenarios.
3. Record results in `benchmarks/REPORT.md`; tune indexes/pool if a target is
   missed.

### 3.9 Branding assets

**Why:** `apps/desktop/src-tauri/icons/icon.ico` is a 192-byte placeholder
(single 32×32 image).

**Steps:** design a multi-size `.ico` (16/32/48/64/128/256) and matching PNGs;
regenerate with `pnpm tauri icon path/to/logo.png`.

### 3.10 Supply-chain inventory in release builds

**Why:** the shipped exe has no `.dep-v0` section, so dependencies cannot be
audited from the artifact.

**Steps:** build release binaries with `cargo auditable build` (or
`cargo auditable` in the release workflow), verify with
`cargo auditable extract target/release/hms-server.exe`.

### 3.11 Out of scope / roadmap (do not start without a product decision)

- Site-2 modules not modelled here: invoices/e-archive, stock/expenses,
  agencies & accounts, currency exchange, channel-booking imports,
  packages/licensing (README §4 deferral).
- Mobile app reusing the API + sync protocol.
- Optional local-network hub so several reception PCs share offline state.
- PDF rendering server-side (today PDF = browser print).

---

## 4. Known limitations to communicate to hotels

- Offline desktops cannot see OTA bookings that arrived meanwhile; overbooking
  is possible until sync (server rejects the overlap, the later booking becomes
  `conflict`).
- Card payments cannot be processed offline.
- A deactivated user can still log in offline until the next sync or until the
  offline cache expires (14 days by default).
- The desktop mirror covers one hotel and is **not** a backup.
- Reports use UTC date bucketing (`iso()` helper), which can shift near midnight
  for non-UTC hotels.

---

## 5. Working agreements for whoever picks this up

1. Keep the permission matrix in `crates/hms-core/src/roles.rs` and the README §6
   table in sync; `crates/hms-tests/tests/roles_matrix.rs` enforces it.
2. Every write goes through a transaction that also writes `change_log` +
   `audit_log` and updates `daily_revenue` where relevant.
3. Tenant isolation is enforced by RLS **and** handler checks; never trust a
   `hotel_id` from the request body.
4. `cargo test --workspace`, `cargo clippy --workspace`, `pnpm lint`,
   `pnpm build` must stay green. DB-backed tests need `TEST_DATABASE_URL`
   (`postgres://postgres:...@localhost:5432/hms_test`) and skip cleanly without it.
5. Never commit `.env`, `userpass.md`, reference-site captures, installers or
   build output.

---

## 6. Quick verification checklist (current state)

```bash
cargo check --workspace --all-targets        # clean
cargo test --workspace                       # hms-core 10, hms-server 11, hms-desktop 11,
                                             # hms-sync 14, hms-tests 12 — all green
                                             # (DB/sync-live suites skip without TEST_DATABASE_URL/TEST_API_URL)
cd apps/web && pnpm lint && pnpm build       # clean, bundle builds
cargo build -p hms-server --release          # produces hms-server + rotate-guest-keys
```

`/metrics` and the rotation binary are new; everything else in section 2 has
been exercised live at least once during development.
