# HMS — Project Architecture & Database Reference (agent.md)

Context for AI agents and new developers. Scope priority: **standalone Windows desktop first**; server/web parity is deferred until the user requests it (backlog tracked in `docs/desktop-standalone-and-web-parity.md`). Do not break the "copy `client/` alone must run" rule.

---

## 1. Project overview

Hotel Management System (HMS) — two artifacts share one web UI:

| Artifact | What it is | Status |
|---|---|---|
| **Desktop app** (primary) | Tauri shell (`hms-desktop.exe`): React webview + embedded SQLite database. Runs fully offline, no server, no PostgreSQL. | Active scope |
| **Server + web** (deferred) | Rust REST server (`hms-server.exe`, port 8080) + PostgreSQL; the React UI can also be served against it. | Frozen until user requests parity |

---

## 2. Runtime architecture

```
┌─────────────────────────── hms-desktop.exe (Tauri) ───────────────────────────┐
│                                                                               │
│  ┌───────────── Webview: client/web (React + Vite bundle, dist embedded) ───┐ │
│  │  Pages (Dashboard, Board/Calendar, Bookings, Rooms, Guests, Reports,     │ │
│  │  Settings, Admin Panel /admin, Bulk Import /import, Staff & Channels)    │ │
│  │  api/client.ts  — ALL fetches go through request();                      │ │
│  │    offline session ⇒ served from local SQLite via api/desktop.ts invoke  │ │
│  │    online session   ⇒ served by the REST server (http://localhost:8080)  │ │
│  └────────────────────────────────────────────────────────────────────────── ┘ │
│                        │ Tauri commands (invoke)                              │
│  ┌─────────────────────▼────────────────────────────────────────────────────┐ │
│  │  client/desktop/src/main.rs — commands:                                  │ │
│  │  account:   login helpers via local_users, update_local_user            │ │
│  │  catalog:   list_domain_records / save_domain_record /                  │ │
│  │             delete_domain_record (rooms, room_types, guests,            │ │
│  │             reservations, channels, restaurants, price_rules)           │ │
│  │  hotels:    list_local_hotels / save_local_hotel (code change moves     │ │
│  │             scoped rows in one transaction)                             │ │
│  │  shift:     create_shift_transfer / pending_shift_transfer /            │ │
│  │             decide_shift_transfer (login approval gate)                 │ │
│  │  profiles:  save_hotel_profile / get_hotel_profile (desktop_kv JSON)    │ │
│  │  import:    bulk_import (CSV/XLSX via calamine) + pick_file             │ │
│  └─────────────────────┬────────────────────────────────────────────────────┘ │
│                        │ rusqlite + SQLCipher                                 │
│  ┌─────────────────────▼────────────────────────────────────────────────────┐ │
│  │  client/desktop/src/local_store.rs                                       │ │
│  │  Encrypted SQLite: %APPDATA%\com.hms.desktop\hms.sqlite                  │ │
│  │  Passwords: Argon2 PHC hex-wrapped with Windows DPAPI (local_users).     │ │
│  │  Schema version = PRAGMA user_version (migrations below).                │ │
│  └────────────────────────────────────────────────────────────────────────── ┘ │
└───────────────────────────────────────────────────────────────────────────────┘
```

- **Offline-first login**: authenticate against local `local_users` first (no expiry), then the online server. Default users are provisioned on first run (see §6).
- **Session modes**: `session.offline` routes every API call to SQLite (`request()` in `client/web/src/api/client.ts`); online mode uses the REST server.
- **No `localStorage` for durable data** — hotel profiles, shift handovers, users, and domain records live in encrypted SQLite. localStorage holds only the session handle and UI prefs (sidebar collapsed, theme).

---

## 3. Repository layout

```
HMS/
├── agent.md                        ← this file
├── docs/
│   ├── desktop-standalone-and-web-parity.md   (features, access matrix, pricing model, parity backlog)
│   ├── db-schema.html              (visual ER diagram of the desktop database)
│   └── ui-reference.md             (web UI parity reference)
├── client/
│   ├── web/                        React app (source + dist bundle)
│   │   └── src/
│   │       ├── api/client.ts       request() router: online REST ⇄ offline SQLite
│   │       ├── api/desktop.ts      Tauri invoke bindings
│   │       ├── api/auth.ts         session/permission helpers
│   │       ├── pages/              Dashboard, Board, Bookings(Reservations), Rooms,
│   │       │                       Housekeeping, Guests, Reports, Settings, Users,
│   │       │                       Channels, Admin (hotels/hotel profile), AdminPanel,
│   │       │                       Import, Login
│   │       ├── components/         AppShell (sidebar/avatar/shift), ui, reservation-modals
│   │       └── types/generated/api.ts   hand-written DTOs mirroring server types
│   └── desktop/
│       ├── src/main.rs             Tauri commands + windows_subsystem
│       ├── src/local_store.rs      SQLite store + migrations + domain CRUD
│       ├── src/importer.rs         bulk import parsers (CSV/XLSX)
│       ├── src/local_login.rs      offline auth (argon2 + DPAPI)
│       ├── src/update*.rs          self-update plumbing
│       └── desktop-migrations/     0001…0007 SQL migrations
├── server/                         frozen: hms-server crate, hms-core, migrations 0001…0011
├── scripts/dev-stack.ps1           online stack: PostgreSQL + hms-server (parity testing only)
└── docs/import sources: guest-daily_*.csv, collection_*.csv, Reservation.xlsx (repo root)
```

---

## 4. Desktop database structure

Encrypted SQLCipher file: `%APPDATA%\com.hms.desktop\hms.sqlite`. Schema version is `PRAGMA user_version`; migrations are applied sequentially and guarded (idempotent ALTERs via `pragma_table_info` checks). Visual diagram: `docs/db-schema.html`.

### 4.1 Migration ledger

| Version | File | Adds |
|---|---|---|
| 1 | `0001_init.sql` | sync engine (`applied_ops`, `outbox`, `failures`, `kv`) + mirror tables: guests, room_types, rooms, channels, price_rules, reservations, payments, folio_transactions |
| 2 | `0002_rate_columns.sql` | `reservations.daily_rates` JSON |
| 3 | `0003_local_users.sql` | `local_users`; `external_ref` on reservations/guests |
| 4 | `0004_desktop_workflows.sql` | `desktop_kv` (hotel profile JSON), `shift_transfers` |
| 5 | `0005_desktop_domain_records.sql` | `hotels`, `restaurants`, `reservation_groups`, `guest_reservation_links`; `reservations.reservation_group_id` |
| 6 | `0006_hotel_scoped_domain.sql` | `hotel_id` on guests, room_types, rooms, channels, reservations |
| 7 | `0007_room_type_pricing.sql` | `room_types.weekend_price_cents`, `rooms.name`, `price_rules.hotel_id` |

### 4.2 Entity map (hotel-scoped operational data)

All operational rows carry `hotel_id → hotels.id`. Deletes are soft (`deleted_at`/`deleted_ms`) except `price_rules` (physical DELETE).

```
hotels (id INTEGER PK, code UK, name, default_currency, time_zone, is_active,
        address, contact_phone, contact_email, created_ms, updated_ms)
 │  extended profile (room inventory meta, parking, restaurants meta, facilities,
 │  weekendDays, contacts) = desktop_kv["hotel_profile:{id}"] JSON
 ├── room_types (id UUID PK, name, capacity, base_price_cents, weekend_price_cents)
 │     ├── rooms (id UUID PK, room_type_id FK, room_number, name, floor, cleanliness)
 │     └── price_rules (id UUID PK, room_type_id FK, date_from, date_to,
 │                      price_cents, currency)          ← per-night calendar overrides
 ├── restaurants (id UUID PK, name, capacity, is_active, configuration_json JSON)
 ├── reservation_groups (id UUID PK, name, notes)
 ├── local_users (user_id PK, username UK, full_name, role CSV list, hotel_id,
 │                password_phc = hex(DPAPI(argon2)), created_ms)
 ├── shift_transfers (id UUID PK, hotel_id, from_user, to_user, amount_cents,
 │                     currency, notes, status pending|approved|rejected,
 │                     created_ms, decided_ms, decided_by)
 └── reservations (id UUID PK, room_id FK→rooms, channel_id FK→channels,
        guest_id FK→guests, checkin_date, checkout_date, status
        confirmed|checked_in|checked_out|cancelled|conflict,
        total_price_cents, currency, adults, children, daily_rates JSON,
        external_ref, reservation_group_id FK)
        ├── payments (append-only; amount_cents, payment_type, is_reversal)
        ├── folio_transactions (other transactions; category, unit_price_cents,
        │                        quantity, total_cents)
        └── guest_reservation_links (reservation_id PK, guest_id PK, link_type)
```

### 4.3 Fee & availability model

**Prices are stored, availability is derived.**

1. `room_types.base_price_cents` — normal/mid fee (always applies).
2. `room_types.weekend_price_cents` — weekend fee; night-date ∈ hotel profile `weekendDays` (0=Sun…6=Sat, default Sat+Sun, manager-editable per hotel). Blank/0 ⇒ same as mid fee.
3. `price_rules` — calendar overrides per room type: `date_from == date_to` = single-night (day-by-day) price; a range = batch change. Highest precedence.

Resolution per night: matched `price_rule` → weekend fee (if day ∈ weekendDays) → mid fee.
Availability per night: room exists (not soft-deleted) ∧ `cleanliness != 'out_of_order'` ∧ no overlapping reservation in `confirmed`/`checked_in`.

### 4.4 Sync engine (idle when offline)

`outbox` (pending ChangeOperations: entity, payload JSON, retries), `applied_ops` (echo/change log), `failures` (permanent rejections), `kv` (pull cursor, auth cache). With no server configured these tables simply stay empty; the offline `request()` paths never touch them.

---

## 5. UI & role model

- Left sidebar: hamburger toggle; collapsed = icons only. Avatar dropdown = Profile (password change), Shift Change (cash-in-safe handover), Logout. Logout is never in the sidebar.
- Shift change: outgoing user enters counted cash + next-shift person → `shift_transfers(pending)`; the incoming user must approve/reject at login before using the app (blocking gate); reject signs out.
- Hotel code & name are editable in Admin Panel hotel profile; changing the code moves all scoped records transactionally (`save_local_hotel`).
- Hotel table: 10 most recently changed rows + code/name/status filter row. User table lives inside the selected hotel profile with filters and multi-role checkboxes (`role` = comma-separated list; permissions are the union of roles).
- Login form has a User/Admin dropdown; hotel code field is inactive for Admin (hotel 0).

| Route | Admin | Manager | Reception | Housekeeper | Accountant |
|---|---|---|---|---|---|
| Dashboard / Monthly Room board | ✔ | ✔ | ✔ | — | — |
| Reservations (Bookings) | ✔ | ✔ | ✔ | — | ✔ |
| Rooms / Housekeeping | ✔ | ✔ | ✔ | status only | — |
| Guests | ✔ | ✔ | ✔ | — | — |
| Reports | ✔ | ✔ | ✔ | — | ✔ |
| Staff & Channels | ✔ | ✔ | — | — | — |
| Settings | ✔ | ✔ | ✔ | ✔ | ✔ |
| Admin Panel `/admin` | ✔ | — | — | — | — |
| Bulk Import `/import` | ✔ | ✔ | — | — | — |

Default home route and menu visibility come from the union of roles (`sessionHasPermission` in `client.ts`).

---

## 6. Default local users (provisioned on first run)

| Hotel | Username | Password | Role(s) |
|---:|---|---|---|
| 0 (platform admin) | admin | 1234 | admin |
| 4442 | manager | 4321 | manager |
| 4442 | reception | 123 | reception |
| 4442 | sina | 123 | reception |
| 4442 | ac | 432 | accountant |
| 4442 | hk | 321 | housekeeping |

No forced password change on login; changes are user-initiated (Profile modal → `update_local_user`, Argon2 + DPAPI rehash).

---

## 7. Build & run

```powershell
# Desktop (primary workflow) — rebuild after ANY client/web change:
cd client/web  ; npm run build          # tsc + vite → dist/
cd client      ; cargo build --release -p hms-desktop
# binaries:
#   client\target\release\hms-desktop.exe    (desktop app)
#   server\target\release\hms-server.exe     (parity testing only)

# Online stack (parity testing only):
powershell scripts/dev-stack.ps1        # PostgreSQL 18 + hms-server on :8080
# psql: postgres://postgres:1234@localhost:5432/hms
```

Calamine pinned at 0.24 (0.26 breaks build). Desktop web build must stay clean (`tsc -b` has no warnings-as-errors forgiveness: unused vars fail).

---

## 8. Working conventions for agents

1. **Desktop-first**: implement features against `local_store.rs` + offline `request()` paths; record any server/web parity need in `docs/desktop-standalone-and-web-parity.md` instead of implementing it.
2. Keep `client/` self-contained (runs without `server/` and without PostgreSQL).
3. Durable data ⇒ encrypted SQLite via Tauri commands; never degrade to localStorage.
4. Every stub must match real server DTO shapes (`types/generated/api.ts`) — e.g. offline board returns full `BoardRoom`/`BoardStay`/`BoardDay`.
5. All schema changes go through a new numbered migration with guarded ALTERs + `user_version` bump; run the migration chain both fresh (in-memory) and existing (upgrade path) in `cargo test`.
6. No emojis in admin UI; Back buttons on pages that trap users; no forced password barriers.
7. Work is local-only (uncommitted/unpushed) until the user decides about git.
