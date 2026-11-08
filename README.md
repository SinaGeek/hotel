# HMS: Multi-Hotel Management System

A multi-tenant hotel management system for 1,000+ hotels (about 50 rooms each).
It ships as:

1. a **web app** (React) for any browser, and
2. a **Windows desktop app** (Tauri 2) that works with no internet, keeps a
   **local database**, syncs with the **online database**, and **updates itself**.

> **For AI agents:** this file is the specification. `TODOS.md` is the ordered
> work list. Read both fully before changing anything. When they conflict with
> code that already exists, the rules in section 2 win.

It merges two existing products:

- **app.hmsflow.com**: visual reference only (layout, navigation, colors). All UI code is original.
- **app.hms.gen.tr**: every feature it offers must exist here (see `docs/feature-inventory.md`).

---

## 1. Language and stack rules

**Rust for every program. TypeScript/React only for the UI.**

| Layer | Choice |
|---|---|
| Server | Rust, Axum, SQLx |
| Shared logic | Rust crate `hms-core` (models, validation, permissions, sync types) used by server and desktop |
| Sync engine | Rust crate `hms-sync` (outbox, pull cursor, retry, conflict handling) |
| Online database | PostgreSQL 16+ (RLS, `btree_gist`, exclusion constraints) |
| Local database | SQLite + SQLCipher inside the desktop app |
| Web UI | React + Vite + TypeScript, static SPA, also embedded in the desktop app |
| Desktop | Tauri 2 (WebView2), Rust core, `tauri-plugin-updater`, MSI installer |
| Types | OpenAPI generated from Rust (`utoipa`) -> TypeScript (`openapi-typescript`) |
| Load tests | k6 and `crates/hms-tests` runner |
| Containers | Docker + Compose |

Never add a second backend language. Never add a heavy UI framework beyond React.

---

## 2. Non-negotiable rules (read first)

### 2.1 The online database is the authority

- The **PostgreSQL server is the single source of truth.** The local database is
  a working copy that exists so the hotel can keep operating when the internet
  or server is down.
- Whenever the two disagree, **the online database wins.** The desktop app
  never overwrites server data by guessing.
- The desktop app must **always try to sync**: automatically on start, on a
  timer (default 30 s), when the network comes back, and after every local
  write.

### 2.2 Local and online work at the same time

- When online, the desktop app talks to the server and mirrors every change
  into the local database at the same moment (the local copy is always warm).
- When offline, every write goes into the local database **and** an outbox in
  one transaction. Nothing is lost.
- When the connection returns, the outbox is pushed, then changes are pulled.
  The user never has to press a "sync" button (a manual button may exist too).

### 2.3 Overbooking during an internet outage is accepted, and resolved by the server

An offline PC cannot know about bookings made elsewhere (other PCs, Booking.com,
Expedia, the web app). So overbooking can happen. This is allowed by design.
The rule for resolving it:

1. The server enforces no double booking with a PostgreSQL `EXCLUDE` constraint.
2. The booking that **reached the server first wins** and keeps its room.
3. The later booking is **never dropped or silently changed**. It is stored with
   status `conflict` and shown to reception with a clear banner and a
   reassignment tool (move to another room/date, or cancel with guest contact).
4. The desktop app shows `conflict` items prominently after every sync.
5. Client clocks are never trusted. Order is by server arrival.

### 2.4 Other hard rules

- **Multi-tenant:** every row has `hotel_id`; PostgreSQL row-level security plus
  handler checks. Never trust a `hotel_id` from a request body.
- **Payments are append-only.** Corrections are reversal rows plus a new payment.
- **No hard deletes.** Use tombstones so deletes can sync.
- **Every write** runs in one transaction that also writes `change_log` +
  `audit_log` and updates `daily_revenue` where relevant.
- **Secrets never go in the repo or in docs.** Passwords, keys, tokens and test
  logins come from `.env`, a secret store, or CI secrets. `.env` and any
  `userpass*.md` are gitignored. Do not copy credentials from chat or notes
  into code, tests, docs or commit messages. Use the dev bootstrap variables
  (section 8) for local logins.
- Keep the permission matrix in `crates/hms-core/src/roles.rs` identical to the
  table in section 5; `crates/hms-tests/tests/roles_matrix.rs` enforces it.
- Dates are stored in UTC and shown in the hotel's time zone.

---

## 3. Architecture

```
                 +-----------------------------+
                 |   Web app (React + Vite)    |
                 +--------------+--------------+
                                | HTTPS / JSON
                                v
+-----------------------+   +---+---------------------------+   +--------------+
| Windows desktop app   |   | hms-server (Rust, Axum)       |   | PostgreSQL   |
| Tauri 2 + same UI     |<->| auth, RBAC, REST, sync        |<->| RLS          |
| Rust core + SQLCipher |   | reports, audit, conflicts     |   | (AUTHORITY)  |
| (local, offline)      |   +-------------------------------+   +--------------+
+-----------+-----------+
            | push outbox / pull changes (versioned sync protocol)
            | auto-update check -> signed MSI from the update host
            +----------------------------> hms-server / update host
```

---

## 4. Sync protocol (summary)

1. **Local write:** update local row + append operation to outbox, one transaction.
2. **Push:** `POST /api/v1/sync/push` sends outbox operations. Idempotent by
   operation id (`sync_ops`), so retries never duplicate.
3. **Pull:** each hotel has a monotonic `change_seq`.
   `GET /api/v1/sync/pull?since=<seq>` returns changes after that number. Never
   use `updated_at > last_sync`.
4. **Protocol version:** server answers `426` to too-old clients; the desktop app
   then triggers the updater.
5. **Conflict rules**

| Data | Rule |
|---|---|
| Reservations | Server enforces no double booking. Later arrival becomes `conflict` (see 2.3). |
| Payments | Append-only; cannot conflict. |
| Housekeeping status | Last write wins in server arrival order. |
| Rooms, room types, users, price rules | Server wins. |

6. **After sync** the desktop replaces local state with server state for anything
   the server decided (including conflict flags and server-assigned numbers).

### Offline login

After the last online login the app caches the password verifier and role
permissions (DPAPI-protected). The cache expires after `OFFLINE_LOGIN_DAYS`
(default 14). A user deactivated on the server is locked out at the next sync.

---

## 5. Roles and permissions

Users log in with **hotel ID + username + password**. Hotel `0` is the platform tenant.
Permissions are data (DB matrix + per-hotel overrides), enforced on the server and
in the desktop core. The UI only hides what a user may not do.

| Capability | admin (hotel 0) | manager | reception | housekeeping |
|---|:-:|:-:|:-:|:-:|
| Create / edit / disable hotels | yes | no | no | no |
| Define role abilities | yes | no | no | no |
| Create accounts | any | below manager | no | no |
| Define room types, create/edit rooms | yes | yes | no | no |
| Change reservation details | yes | yes | yes | no |
| Record / change payments | yes | yes | yes | no |
| See guest names, prices, payments | yes | yes | yes | no |
| See room status | yes | yes | yes | yes |
| Change room cleanliness | yes | yes | yes | yes |
| View reports | all hotels | own hotel | configurable (default off) | no |

Cleanliness: `dirty`, `cleaning`, `clean`, `inspected`. Housekeeping never sees
guest names, prices or payments.

---

## 6. Features

Reservations (board, list, detail, check-in/out, cancel), guests (encrypted
identity fields, KVKK/GDPR export and delete), rooms and room types, pricing and
price rules, payments and folio, channels, housekeeping, users and roles, reports
(earnings by channel/day/month/room, occupancy, ADR, RevPAR, forecast, in-house,
main courante), audit log, CSV/PDF export, English UI (Turkish planned), mobile
friendly web layout, platform admin panel (`/admin`).

Deferred site-2 modules (do not start without a product decision): e-invoice,
stock/expenses, agency accounts, currency exchange, SMS campaigns, packages and
licensing. Details in `docs/feature-inventory.md`.

---

## 7. Repository layout

Independently buildable workspaces (restructure decision — the server and the
standalone desktop client share no local crate dependencies; see
`docs/restructure.md`):

```
server/                          self-contained server workspace (own Cargo.lock/target)
  crates/hms-core                models, roles, sync protocol types, validation, guest encryption
  crates/hms-server              Axum API, sync endpoints, migrations runner, operator bins
  crates/hms-lan-hub             LAN sync hub service
  crates/hms-tests               integration tests + load-test runner
  migrations/                    PostgreSQL migrations
  seed/, benchmarks/, deploy/    dev seeds, load tests, and Caddy / nginx samples
  Dockerfile, docker-compose.yml container deployment
  .env.example                   server configuration template

client/                          self-contained client workspace (own Cargo.lock/target)
  crates/hms-core                vendored slim core: only roles + sync protocol types
  crates/hms-sync                client sync engine (outbox/pull, offline login)
  desktop/                       Tauri 2 crate (shell, SQLite/SQLCipher mirror, DPAPI, updater)
  desktop/desktop-migrations     SQLite mirror migrations
  web/                           React + Vite SPA (also embedded in the desktop shell)
  mobile/                        React Native app

scripts/                         cross-cutting dev scripts (dev-stack, desktop build)
docs/                            API snapshot, feature inventory, runbooks
```

The client's `hms-core` is wire-compatible by contract: `sync.rs` types and
`SYNC_PROTOCOL_VERSION` must stay identical to `server/crates/hms-core/src/sync.rs`.

---

## 8. Configuration (`.env`, never committed)

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | PostgreSQL connection string |
| `SESSION_SECRET` | at least 32 bytes in production |
| `GUEST_FIELD_ENCRYPTION_KEY` | base64 32 bytes, encrypts guest identity fields |
| `ADMIN_BOOTSTRAP_PASSWORD` | initial hotel-0 `admin` password (forced change on first login) |
| `APP_ENV` | `development` or `production` |
| `CORS_ALLOWED_ORIGINS` | exact HTTPS origins in production |
| `OFFLINE_LOGIN_DAYS` | offline credential lifetime (default 14) |
| `RATE_LIMIT_LOGIN` | login attempts per window |
| `SYNC_PROTOCOL_MIN_VERSION` | oldest accepted desktop protocol |
| `TEST_USERS` | dev/test only: `hotel_id:username:password:role,...` |
| `VITE_API_BASE` | absolute API base for the web bundle shipped inside the desktop app |
| `HMS_API_BASE`, `SYNC_INTERVAL_SECS` | desktop runtime: server URL and sync interval |
| `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)` | updater signing key, CI secret only |

---

## 9. Getting started

Prerequisites: Rust (stable), Node.js 22 + pnpm, Docker or PostgreSQL 16+, and
for the desktop build the Tauri 2 Windows prerequisites (WebView2, MSVC build
tools, Strawberry Perl and NASM for SQLCipher).

```bash
cd server
cp .env.example .env                 # fill DATABASE_URL, SESSION_SECRET, ADMIN_BOOTSTRAP_PASSWORD
docker compose up -d db              # or: scripts/dev-stack.ps1 (native Windows stack)
cargo run -p hms-server              # migrations run on start, listens on :8080
cd ../client/web && pnpm install && pnpm dev
```

First login: hotel `0`, user `admin`, password from `ADMIN_BOOTSTRAP_PASSWORD`
(then forced change). Create hotels and their users in `/admin`.

Desktop:

```bash
cd client/web
VITE_API_BASE=http://localhost:8080/api/v1 pnpm build
cd ../desktop
cargo tauri dev
cargo tauri build                    # MSI + updater artifacts
```

Checks that must stay green: `cargo test --workspace` (in `server/` and in
`client/`), `cargo clippy --workspace` (both), `pnpm lint`, `pnpm build`.

---

## 10. Auto-update (desktop)

- `tauri-plugin-updater` checks `https://<update-host>/updates/latest.json` on
  start and every few hours, and when the server answers `426`.
- Updates are **signed (minisign)**; the public key is in `tauri.conf.json`, the
  private key only in CI secrets. A tampered or unsigned package must be rejected.
- Update flow: download in background, never interrupt an open booking form,
  install at next restart (or on user click), keep the local database and outbox
  intact, run SQLite migrations on first start of the new version.
- Never apply an update while the outbox has unsynced operations without
  first attempting a sync.

---

## 11. Performance targets

| Target | Value |
|---|---|
| Room board, availability, reservation save (p95) | < 200 ms |
| Full-year report for one hotel | < 500 ms |
| Scale | 1,000 hotels x 50 rooms x 2 years of reservations |

Reports read `daily_revenue`, never raw reservations. Use keyset pagination.

---

## 12. Testing requirements

Permission matrix, tenant isolation, double-booking race, report integrity,
offline for N days then sync, **two offline clients booking the same room (later
becomes `conflict`, nothing lost)**, server outage mid-sync, repeated/interrupted
sync, clock-skewed client, SQLCipher plaintext-open failure, updater rejects
tampered package, load test against targets.

---

## 13. Known limitations (shown to hotels)

- Offline desktops cannot see bookings made meanwhile; overbooking is possible
  until sync, then resolved as in section 2.3.
- Card payments cannot be processed offline, only recorded.
- A deactivated user can log in offline until next sync or cache expiry.
- Several PCs in one hotel do not see each other offline (optional LAN hub is future work).
- The desktop copy covers one hotel and is **not a backup**.
- Reports bucket dates in UTC unless the hotel time-zone fix (TODOS) is done.

---

## 14. Working agreement for agents

1. Take the first unchecked item in `TODOS.md` unless told otherwise.
2. Make small, tested changes; keep all checks green.
3. Update `TODOS.md` (check the box, add notes) and docs in the same change.
4. Prefer Rust. Ask a human only for product decisions or secrets.
5. Never commit secrets, installers, build output or reference-site captures.
