# Unified HMS: Multi-Hotel Management System

A multi-tenant hotel management system for 1,000+ hotels (about 50 rooms each), with a web app and a standalone Windows desktop app that keeps working when the internet or the server is down and syncs when the connection returns.

It merges two existing products into one:

- **app.hmsflow.com**: the visual reference for the UI (layout, navigation, colors, component style). The UI is original code.
- **app.hms.gen.tr** (HMS Hotel Software): every feature it offers must exist in this system.

> **Status (2026-10-31):** the core Rust/Axum server, Postgres migrations, React/Vite web app, reporting endpoints, versioned sync engine, Tauri desktop shell, SQLite mirror, and Windows MSI packaging are implemented. `docs/server-api.md` is the source-derived route snapshot; `docs/feature-inventory.md` is the site-2 inventory. Remaining limitations include SQLCipher (desktop uses bundled SQLite + DPAPI-protected secrets), full local-first write support/offline report parity, OpenAPI/TS type generation, production CORS/TLS/deployment hardening, and the Tauri auto-updater (signing/manifest setup pending). See `todo.md` for the current checklist.

---

## Table of contents

1. [Goals and scale targets](#1-goals-and-scale-targets)
2. [Architecture](#2-architecture)
3. [Tech stack](#3-tech-stack)
4. [Feature inventory](#4-feature-inventory)
5. [Repository structure](#5-repository-structure)
6. [Roles and permissions](#6-roles-and-permissions)
7. [Data model](#7-data-model)
8. [Multi-tenancy and security](#8-multi-tenancy-and-security)
9. [Reservations and availability](#9-reservations-and-availability)
10. [Reports](#10-reports)
11. [Offline desktop app and sync](#11-offline-desktop-app-and-sync)
12. [API overview](#12-api-overview)
13. [Performance plan](#13-performance-plan)
14. [Getting started](#14-getting-started)
15. [Configuration](#15-configuration)
16. [Testing](#16-testing)
17. [Deployment and operations](#17-deployment-and-operations)
18. [Roadmap](#18-roadmap)
19. [Known limitations](#19-known-limitations)

---

## 1. Goals and scale targets

| Goal | Target |
|---|---|
| Hotels (tenants) | 1,000+ |
| Rooms per hotel | about 50 (about 50,000 rooms total) |
| Data volume | millions of reservations and payments over time |
| Room board, availability search, reservation save | p95 under 200 ms |
| Full-year report for one hotel | under 500 ms |
| Offline use | each hotel keeps working with no internet or server |
| Tenant isolation | one hotel can never read or write another hotel's data |
| Double booking | prevented by the database, not just application code |

At this scale, speed comes mostly from database design (indexes, pre-aggregated reports, caching) rather than from the choice of language. Rust gives low latency spikes, low memory use and low server cost.

---

## 2. Architecture

```
                    +-----------------------------+
                    |   Web app (React + Vite)    |
                    |   browser, any device       |
                    +--------------+--------------+
                                   | HTTPS / JSON
                                   v
+----------------------+   +-------+-----------------------+   +--------------+
| Windows desktop app  |   |  hms-server (Rust, Axum)      |   | PostgreSQL   |
| Tauri 2 + same UI    |<->|  auth, RBAC, REST API, sync   |<->| RLS, hotel_id|
| Rust core + SQLite   |   |  report aggregation, audit    |   | on every row |
| (encrypted, offline) |   +-------------------------------+   +--------------+
+----------+-----------+
           ^ push outbox / pull changes (sync protocol, versioned)
           +--------------------------> hms-server
```

Key ideas:

- **One UI codebase** runs in the browser and inside the Tauri desktop shell.
- **One shared Rust crate** (`hms-core`) holds models, validation, the permission matrix and sync protocol types, used by both the server and the desktop core. TypeScript types are generated from it so UI, API and desktop never drift.
- **The server is the authority.** Desktop apps are full replicas of their own hotel only and reconcile with the server by rules, not by guessing.

---

## 3. Tech stack

| Layer | Choice | Notes |
|---|---|---|
| Server | Rust, Axum, SQLx | If Rust is not workable, TypeScript (Fastify) with every rule in this document unchanged |
| Database | PostgreSQL | Row-level security, exclusion constraints, `btree_gist` |
| Web UI | TypeScript, React, Vite | Static single-page app (no Next.js server features) so the desktop app can embed it |
| Charts | ECharts or Recharts | Same chart components on web and desktop |
| Room calendar | Resource timeline/scheduler component | Rooms as rows, dates as columns |
| Desktop shell | Tauri 2 (WebView2) | Small installer, native Windows integration |
| Desktop core | Rust + SQLite with SQLCipher | Encrypted local database; key protected via Windows DPAPI / Credential Manager |
| Type sharing | OpenAPI generation or `ts-rs` | Generates TypeScript from Rust types |
| Load testing | k6 | Run against seed data for 1,000 hotels |
| Containers | Docker Compose | Server + database for development and simple deployments |
| Editor | VS Code | rust-analyzer, ESLint, Tauri extensions |

---

## 4. Feature inventory

Verified against site 2 (**app.hms.gen.tr**, live session on 2026-10-27) and the site 1 UI reference. Full evidence per area in `docs/feature-inventory.md` (+ screenshots). "Deferred" = module exists on site 2 but is intentionally out of the first version (see inventory doc).

| Area | Planned features | Verified against site 2 |
|---|---|---|
| Reservations | calendar/room board, list, detail, create/edit/cancel, check-in/out | **verified** (list, detail with tabs, check-in/out, print; group/channel reservations deferred) |
| Guests | guest records, history | **verified** (records, ID/passport, balance, import/export) |
| Rooms | room types (name, capacity, base price, amenities), rooms | **verified** (rooms + 7 room types as data; CRUD menu admin-only) |
| Pricing | base price, price rules (seasonal etc.) | **verified** (periodic per-day calendar 2021–2031 + conditional pricing rules) |
| Payments | payments and folio per reservation, reversal entries | **verified** (collection/payment tab, account activities, bank transactions) |
| Channels | direct, walk-in, Booking, Expedia, agency, other (configurable per hotel) | **verified as list/status**; deep channel-manager sync **deferred** |
| Housekeeping | room board with cleanliness status | **verified** (board + print; site-2 uses Dirty/Clean + Out of Order) |
| Users and roles | hotel accounts, role permissions | **verified** (profile, password change, 2FA present; CRUD admin-only) |
| Reports | earnings per channel/day/month/room, occupancy, ADR, RevPAR | **verified** (earnings by section, occupancy w/ chart; site 2 has no per-channel earnings report) |
| Audit | change history for every record | **verified** (/raporlar/log/ with user, IP, section, add/delete/update, comments) |
| Localization | Turkish and English UI, TRY and other currencies | **verified** (Program Language switch; TRY everywhere) |
| Export | CSV and PDF | **verified** (Print + guest/agency import-export) |
| Layout | mobile-friendly web layout | **verified** (board works at ~510px viewport) |

Out-of-scope site-2 modules (deferred): invoices with e-invoice/e-archive, agency currents, currency exchange, cash accounts/banks, online guest payments, open sales, expenses/stock, SMS/e-mail campaign tooling, Wi-Fi/IP tools, notifications/announcements/messaging, end-of-day close, packages/licensing. See `docs/feature-inventory.md` for the proposal table.

---

## 5. Repository structure

```
hms/
├── Cargo.toml                    # Rust workspace
├── README.md
├── docker-compose.yml            # server + PostgreSQL for development
├── .env.example                  # all configuration variables, no secrets
│
├── crates/
│   ├── hms-core/                 # shared by server and desktop
│   │   ├── models/               # hotels, rooms, reservations, payments ...
│   │   ├── validation/           # input rules used on both sides
│   │   ├── permissions/          # role -> permission matrix
│   │   └── sync/                 # operation types, protocol version, conflict rules
│   ├── hms-server/               # Axum API
│   │   ├── auth/                 # login, sessions, rate limiting
│   │   ├── api/                  # REST handlers (hotels, rooms, reservations ...)
│   │   ├── sync/                 # /sync/push, /sync/pull, idempotency
│   │   ├── reports/              # aggregate maintenance and report queries
│   │   └── audit/                # audit log writer
│   └── hms-sync/                 # client-side sync engine (outbox, pull cursor, retry)
│
├── apps/
│   ├── web/                      # React + Vite UI (also embedded by desktop)
│   │   ├── src/pages/            # board, reservations, rooms, users, reports ...
│   │   ├── src/components/
│   │   ├── src/i18n/             # tr, en
│   │   └── src/api/              # generated client + types
│   └── desktop/
│       └── src-tauri/            # Tauri 2 Rust core: SQLite/SQLCipher, commands, updater
│
├── migrations/                   # PostgreSQL schema migrations (server)
├── desktop-migrations/           # SQLite schema migrations (desktop)
├── seed/                         # seed data generator (1,000 hotels x 50 rooms x 2 years)
├── loadtest/                     # k6 scripts and benchmark report
├── tests/                        # integration tests: permissions, isolation, sync, races
└── docs/                         # API reference, sync protocol, decisions
```

---

## 6. Roles and permissions

Permissions are data: a role-to-permission matrix stored in the database, enforced on the server and in the desktop core. The UI only hides what a user may not do.

Each hotel has an ID, and users log in with **hotel ID + username + password**. Hotel ID `0` is the platform tenant.

| Capability | admin (hotel 0) | manager | reception | housekeeping |
|---|:-:|:-:|:-:|:-:|
| Create / edit / disable hotels | yes | no | no | no |
| Define role abilities | yes | no | no | no |
| Create accounts | yes (any) | yes (below manager) | no | no |
| Define room types | yes | yes | no | no |
| Create / edit rooms | yes | yes | no | no |
| Change reservation details | yes | yes | yes | no |
| Record / change payments | yes | yes | yes | no |
| See guest names, prices, payments | yes | yes | yes | no |
| See room status (empty / occupied) | yes | yes | yes | yes |
| Change room cleanliness | yes | yes | yes | yes |
| View reports | all hotels | own hotel | configurable | no |

Cleanliness states: `dirty`, `cleaning`, `clean`, `inspected`. Housekeeping sees only the room board and cleanliness, with no guest names, prices or payments.

Report access for reception is not specified in the requirements. It defaults to off and is configurable per hotel.

---

## 7. Data model

Every table carries `hotel_id`. IDs are client-generated UUIDv7 so offline machines can create records without the server.

| Table | Purpose |
|---|---|
| `hotels` | tenant record; holds the per-hotel `change_seq` counter |
| `users` | accounts (hotel_id, username, Argon2id hash, role, active) |
| `roles`, `role_permissions` | permission matrix |
| `room_types` | name, capacity, base price, amenities |
| `rooms` | number, room type, cleanliness status, active |
| `guests` | guest records |
| `channels` | direct, walk-in, Booking, Expedia, agency, other |
| `price_rules` | seasonal or special pricing |
| `reservations` | guest, room, channel, stay date range, price, status |
| `payments` | **append-only** ledger; corrections are reversal entries |
| `change_log` | per-hotel monotonic change number, entity, operation (drives sync pull) |
| `sync_ops` | processed operation IDs (idempotency) |
| `daily_revenue` | pre-aggregated per hotel, room, channel, day |
| `audit_log` | who, what, when, old value, new value |

Rules:

- **No hard deletes.** Deletes are tombstones, so they can sync.
- **Payments are never edited.** A correction is a new reversal entry plus a new payment.
- Dates are stored in UTC and shown in the hotel's time zone.

---

## 8. Multi-tenancy and security

- One database, `hotel_id` on every row, enforced by **PostgreSQL row-level security** and by the auth layer. No query runs without a tenant scope.
- Passwords hashed with **Argon2id**. Login rate limiting. Short-lived sessions with refresh.
- **Audit log** for every change.
- The seeded `admin` account (hotel ID 0) takes its initial password from configuration and **must change it on first login**. No fixed default password in production builds.
- Credentials for the reference sites (used only during the feature inventory) are supplied through environment variables and never written to code, logs or commits.
- Guest data is personal data (KVKK / GDPR): support per-hotel data export and deletion, and encrypt at rest where practical.
- The desktop database is encrypted (SQLCipher) with the key protected by Windows DPAPI / Credential Manager.

---

## 9. Reservations and availability

Double booking is prevented in the database:

```sql
CREATE EXTENSION IF NOT EXISTS btree_gist;

ALTER TABLE reservations
  ADD CONSTRAINT no_double_booking
  EXCLUDE USING gist (room_id WITH =, stay WITH &&)
  WHERE (status NOT IN ('cancelled', 'conflict'));
```

- `stay` is a date range (check-in to check-out).
- Reservations with status `conflict` come from offline sync collisions (see section 11) and are shown to reception for reassignment.
- Availability search and the room board use indexes on `(hotel_id, room_id, stay)`.

---

## 10. Reports

All reports have a date-range filter and CSV/PDF export. Platform admin can see all hotels; hotel users see only their own.

| Report | Chart |
|---|---|
| Earnings per **channel** | donut + bar |
| Earnings per **day** | line |
| Earnings per **month** | bar with year-over-year line |
| Earnings per **room** and **room type** | ranked bar + heatmap |
| Occupancy %, ADR, RevPAR | line / KPI cards |

How they stay fast: reports never scan raw reservations. The `daily_revenue` table (hotel, room, channel, day) is updated in the same transaction as every reservation or payment change. Month and year views roll up from it.

"Earnings" can mean money collected (payments by payment date) or room revenue accrued per stay night. Reports show **collected** by default and can switch to accrued. Confirm which one your hotels expect.

The desktop app computes the same reports from its local data when offline.

---

## 11. Offline desktop app and sync

The desktop app holds a full local copy of its own hotel and works with no internet or server. It syncs through the API when connectivity returns.

### How it works

1. Every write updates the local SQLite row **and** appends an operation to a local **outbox** in one transaction.
2. **Push:** the app sends outbox operations to `POST /api/v1/sync/push`. The endpoint is idempotent (deduplicated by operation ID), so repeated or interrupted syncs create no duplicates.
3. **Pull:** each hotel has a monotonically increasing `change_seq`, assigned inside the write transaction. The client stores the last number it saw and calls `GET /api/v1/sync/pull?since=<seq>`. Do **not** use `updated_at > last_sync_time`, which can miss late-committing rows.
4. Client clocks are never trusted. Far-future timestamps are rejected and conflicts are ordered by server arrival.

### Conflict rules (by data type)

| Data | Rule |
|---|---|
| Payments | Append-only. Corrections are reversal entries, so they cannot conflict. |
| Housekeeping status | Last write wins, in server order. |
| Rooms, room types, users, price rules | Manager-level data; server wins. |
| Reservations | The server enforces the no-double-booking constraint. If two machines sold the same room, the later booking is accepted as `conflict` and flagged for reassignment. It is never silently overwritten or dropped. |

CRDTs merge data well but cannot enforce business rules like "one guest per room per night", so this system uses an operation log with server arbitration.

### Offline login

After the last online login, the app caches the password verifier and role permissions. The cache expires after 14 days (configurable). A user deactivated on the server is locked out at the next sync. The desktop webview requests the verifier with `offline: true` in the login body; the server then echoes the argon2 PHC hash in `offline_verifier` (plain browser requests never receive it), and the desktop protects the persisted copy with Windows DPAPI.

### Versioning and updates

The sync protocol is versioned and schema migrations run on both sides, so old desktop versions in the field keep syncing. Tauri's auto-updater and an `.msi` installer handle distribution.

### What the user sees

Online/offline state, last-synced time, pending operation count and unresolved conflicts. The app clearly warns that:

- Channel bookings (Booking, Expedia) made while offline are unknown until sync, so overbooking is possible until then.
- Card payments cannot be processed offline; they can only be recorded.

### Future option

Several reception PCs in one hotel will not see each other's changes while the internet is down. The design leaves room for a local-network hub later; it is not part of the first version.

---

## 12. API overview

All endpoints are under `/api/v1`, JSON over HTTPS, and require a session except login. Planned endpoints:

| Area | Endpoints |
|---|---|
| Auth | `POST /auth/login` (hotel ID, username, password), `POST /auth/refresh`, `POST /auth/logout`, `POST /auth/change-password` |
| Hotels (admin) | `GET/POST /hotels`, `GET/PATCH /hotels/{id}` |
| Roles (admin) | `GET /roles`, `PUT /roles/{id}/permissions` |
| Users | `GET/POST /users`, `PATCH /users/{id}` |
| Room types | `GET/POST /room-types`, `PATCH /room-types/{id}` |
| Rooms | `GET/POST /rooms`, `PATCH /rooms/{id}`, `PATCH /rooms/{id}/cleanliness` |
| Reservations | `GET/POST /reservations`, `GET/PATCH /reservations/{id}`, `POST /reservations/{id}/check-in`, `.../check-out`, `.../cancel` |
| Payments | `GET/POST /reservations/{id}/payments`, `POST /payments/{id}/reverse` |
| Board | `GET /board?from=&to=` |
| Reports | `GET /reports/earnings?by=channel|day|month|room&from=&to=`, `GET /reports/occupancy` |
| Sync | `POST /sync/push`, `GET /sync/pull?since=` |

List endpoints use keyset pagination (never `OFFSET` on large tables). The API contract is published as OpenAPI and used to generate TypeScript types.

---

## 13. Performance plan

- Indexes: `(hotel_id, room_id, stay)`, `(hotel_id, status)`, `(hotel_id, created_at)`.
- Keyset pagination everywhere on large tables.
- Report queries read `daily_revenue`, not raw rows.
- Cache read-heavy data (room types, rooms, today's room board) with invalidation on write.
- Connection pooling, response compression, hashed static assets.
- Per-hotel `change_seq` is incremented with a row lock inside the write transaction. At about 50 rooms per hotel this serializes writes within one hotel only, which is fine.

Proof: a k6 load test against seed data of 1,000 hotels x 50 rooms x 2 years of reservations. The seed script (`seed/`) and results (`loadtest/REPORT.md`) are part of the deliverables. Targets are in section 1.

---

## 14. Getting started

Verified development workflow (prerequisites: Rust toolchain, Node.js with pnpm, Docker, and the Tauri 2 Windows prerequisites for desktop builds):

```bash
# 1. Configure local development environment
cp .env.example .env
# Set DATABASE_URL, SESSION_SECRET, and ADMIN_BOOTSTRAP_PASSWORD in .env.

# 2. Start PostgreSQL (Compose does not start the Rust server)
docker compose up -d db

# 3. Start the API server in one terminal (applies PostgreSQL migrations on startup)
cargo run -p hms-server

# 4. Start the web app in another terminal
cd apps/web
pnpm install
pnpm dev

# 5. Optional: seed the load-test hotel (destructive to hotel 4443 only)
psql "$DATABASE_URL" -f ../../seed/loadtest.sql

# 6. Windows desktop app (install the Tauri CLI; from the Tauri crate directory)
cd apps/desktop/src-tauri
cargo tauri dev
cargo tauri build               # emits the MSI bundle; auto-update is not configured
```

First login: hotel ID `0`, username `admin`, and the bootstrap password from `.env`; a forced password change is required. The `seed/loadtest.sql` script creates the separate load-test hotel 4443; it is not a general-purpose 1,000-hotel seed generator.

The server and web workflow is verified. Desktop MSI packaging is implemented; release signing and auto-update feed/manifest are not configured. Production deployment still requires CORS/TLS/security-header hardening and operational controls listed in sections 17 and `todo.md`.

---

## 15. Configuration

Set in `.env` (never commit real values):

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | PostgreSQL connection string |
| `SESSION_SECRET` | signs sessions/tokens |
| `ADMIN_BOOTSTRAP_PASSWORD` | initial password for `admin` on hotel 0 (forced change on first login) |
| `OFFLINE_LOGIN_DAYS` | how long cached desktop credentials stay valid (default 14) |
| `RATE_LIMIT_LOGIN` | login attempts per window |
| `SYNC_PROTOCOL_MIN_VERSION` | oldest desktop sync protocol still accepted |
| `SITE1_*`, `SITE2_*` | reference-site credentials, only for the feature inventory (hotel ID, user, password) |

---

## 16. Testing

| Test | What it proves |
|---|---|
| Permission matrix | every role x every action behaves as in section 6 |
| Tenant isolation | hotel A can never read or write hotel B |
| Double-booking race | concurrent bookings for one room never both succeed |
| Report integrity | report totals equal the sum of raw payments |
| Offline for N days, then sync | local work survives and merges correctly |
| Two offline clients, same room | the later booking becomes a `conflict` for reassignment, nothing lost |
| Server outage mid-sync | sync resumes cleanly |
| Interrupted and repeated sync | no duplicate records |
| Clock-skewed client | bad timestamps cannot win conflicts |
| Load test (k6) | latency targets in section 1 |

---

## 17. Deployment and operations

- **Server:** container image behind a TLS-terminating reverse proxy. PostgreSQL with automated backups and replication. The desktop copy is not a backup.
- **Migrations:** run on server start (PostgreSQL) and on app start (SQLite). Sync protocol versions are checked on every sync call.
- **Desktop distribution:** `.msi` installer plus Tauri auto-updater.
- **Monitoring:** request latency (p95/p99), error rate, sync lag per hotel, database connections and slow queries.

---

## 18. Roadmap

1. Server, database schema, RBAC, web app (reservations, rooms, payments, housekeeping).
2. Reports and daily revenue aggregation.
3. Feature parity pass against site 2.
4. Offline Windows app and sync engine.
5. Load test and tuning to meet the targets.
6. Mobile app for the hotel system (planned after the web and Windows apps), reusing the API and sync protocol.
7. Optional: local-network hub for several reception PCs in one hotel.

---

## 19. Known limitations

- An offline desktop app cannot know about bookings from Booking.com or Expedia that arrived meanwhile, so overbooking is possible until sync.
- Card payments cannot be processed offline.
- A deactivated user can still log in offline until the next sync or until the offline credentials expire.
- Offline PCs in the same hotel do not see each other's changes until the internet returns (unless the optional hub is built).
- The desktop copy covers one hotel and is not a backup of the server.
