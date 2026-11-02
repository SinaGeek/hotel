# TODOS: HMS

Work top to bottom. Each task has acceptance criteria; a task is done only when
they are proven by a test or a documented manual run. Read `README.md` first
(section 2 rules are mandatory).

Legend: `[ ]` open, `[x]` done. **P0** = release blocker, **P1** = needed for
the product to be complete, **P2** = polish / later.

## Already done (do not redo, only keep green)

- [x] PostgreSQL schema with RLS, `EXCLUDE` no-double-booking, change_log, sync_ops, audit_log, daily_revenue
- [x] Axum server: auth, RBAC, hotels, users, rooms, guests (encrypted), reservations, payments, board, reports, audit, sync push/pull, metrics, key rotation tool
- [x] React web app incl. board, bookings, guests, rooms, housekeeping, reports, admin panel
- [x] Tauri 2 shell, SQLite mirror, outbox, background sync, offline login, DPAPI, offline reports, MSI
- [x] Load test on one hotel meets targets; CI runs test/clippy/lint/build

---

## P0-A. Local-first desktop writes (online DB stays the authority)

Today reservation and payment writes in the desktop app fail when offline.
This is the main missing piece of the product vision.

- [x] **A1. Local-first command layer.** Route every desktop write (reservation create/edit/check-in/out/cancel, payment, folio extra, guest create/edit, cleanliness) through Rust commands that: (a) when online, call the server first and mirror the server's response into SQLite; (b) when offline or the call fails with a network error, write SQLite + outbox in one transaction and return an "queued" result to the UI.
  - Accept: with the network disabled, all listed actions work in the UI and show `Offline - N queued`; after reconnect the queue drains and server data matches.
  - Done: `local_first::stage_local_write` commits the optimistic mirror row + outbox op in ONE SQLite transaction; Tauri command `local_write` runs one push+pull cycle right after staging when a session exists (`confirmed` when the op left the outbox, `queued` otherwise). Webview routes createReservation/edit/checkIn/checkOut/cancel/addPayment/reversePayment/addFolio/createGuest/setCleanliness through it in Tauri (`apps/web/src/api/client.ts` -> `desktop.ts::localWrite`). Tests: `local_first::tests::*` (stage-atomicity + payload → mirror).
- [x] **A2. Server decides.** After push, replace local rows with server results (ids, numbers, status, price, `conflict` flag). Add a test that a server-changed field overwrites the local value.
  - Done: all change-stream echoes are now server-canonical (reservation/payment/guest/folio/room/channel: REST + sync appliers, `reservation_echo`/`payment_echo`); desktop `apply_remote` upserts the canonical row (sticky-conflict removed — server status wins). Test: `local_first::tests::a2_server_pull_overwrites_local_values`.
- [x] **A3. Per-night rates in the mirror** so offline reports match server reports (currently spread evenly). Mirror `price_rules` and per-night amounts; use real payment dates, not sync-apply time.
  - Accept: offline report totals equal server report totals for the same data (test with seed data).
  - Done: mirror column `reservations.daily_rates` (migration `0002_rate_columns.sql`, `PRAGMA user_version` guard), server echo carries `daily_rates`; desktop reports use per-night rates -> price_rules -> room-type base with the server's precedence, even split only unpriced; payments in the mirror use the record's server timestamp as the collection day. Server folio spread fixed to even per-night (was adding the full total per night in REST + sync). Test: `reports::tests::a3_priced_rates_drive_room_revenue_then_price_rules_then_base`.
- [x] **A4. Local availability check** while offline: warn (not block) if the room looks taken locally; always allow the booking (overbooking is accepted, see README 2.3).
  - Done: `local_overlap_warning` in the staged write; `WriteOutcome.warning` (skip-None) + `desktop.ts` type; booking is always staged. Test: `local_first::tests::a4_offline_overlap_warns_but_does_not_block`.
- [x] **A5. Automatic sync triggers:** on app start, after every local write, on network-up event, on timer, before applying an update. Exponential backoff on failures; never lose or reorder outbox operations.
  - Accept: kill the server mid-sync, restart it; no duplicates, no lost operations.
  - Done: worker fires immediately at app start (0-delay first attempt), timer interval, fast 2 s re-poll while offline (network-up approximated: connectivity restore is picked up within 2 s) and back to interval after the first success; `local_write` triggers a cycle after every write; per-op exponential backoff, FIFO order, idempotent pushes stay in the engine. ("Before applying an update" is moot until the updater exists — see D-block; a cycle runs before any update flow will be added there.)

## P0-B. Overbooking / conflict handling

- [ ] **B1. Conflict status end to end:** server returns `conflict` for the later booking (already in sync apply); verify the desktop and web both show it.
- [ ] **B2. Conflict banner and list** in web and desktop: count in the header, filterable list of `conflict` reservations with guest, dates, requested room.
- [ ] **B3. Reassignment tool:** suggest free rooms of the same type for the same dates, move the reservation in one click (server validates, audit-logged), or cancel it. Resolved items leave the conflict list.
- [ ] **B4. Notification after sync:** if a sync produced new conflicts, show a prominent dialog ("N bookings need a new room").
- [ ] **B5. Tests:** two desktops book the same room offline -> first to reach the server keeps the room, second becomes `conflict`, nothing lost, reassignment works. Extend `crates/hms-tests/tests/sync_live.rs`.

## P0-C. SQLCipher-encrypted local database

- [ ] **C1.** Install Windows build prerequisites (MSVC C++ tools, Strawberry Perl, NASM). Document in `docs/desktop-build.md`.
- [ ] **C2.** Switch `rusqlite` to `bundled-sqlcipher-vendored-openssl`. Generate a random 32-byte key per install, store it DPAPI-protected, issue `PRAGMA key` before any statement, enable `cipher_memory_security`.
- [ ] **C3.** Migration for existing plaintext installs using `sqlcipher_export()` into a new encrypted file, then swap; keep the outbox intact.
- [ ] **C4.** Test: opening `hms.sqlite` with plain `sqlite3` fails ("file is not a database"); app still starts, syncs and renders offline reports.

## P0-D. Self-updating desktop app

- [ ] **D1.** Generate the minisign keypair (`pnpm tauri signer generate`); private key only in CI secrets; public key in `tauri.conf.json`.
- [ ] **D2.** Add `tauri-plugin-updater` (Rust) and `@tauri-apps/plugin-updater` (web); set `bundle.createUpdaterArtifacts: true` and the endpoint URL.
- [ ] **D3.** Update UX: background check on start and every few hours, "Update available" pill, download in background, install on restart or click, never during an unsaved form.
- [ ] **D4.** Force-update path: when the server returns `426` (protocol too old), the app must update before syncing again.
- [ ] **D5.** Safe upgrades: attempt a sync before installing; run SQLite migrations on first start of the new version; preserve local DB, key and outbox.
- [ ] **D6.** CI `release` workflow: build web + MSI, sign, publish MSI and `latest.json` to the update host.
  - Accept: install version N, publish N+1, app offers and applies the update; a tampered package is rejected; local data survives.

## P0-E. Production stack verification

- [ ] **E1.** On a Docker host: `docker compose up -d --build`; `/health` green, migrations applied.
- [ ] **E2.** Put Caddy or nginx in front (TLS, SPA deep links `/board` `/admin`, `/api` proxy, static update host for `latest.json`).
- [ ] **E3.** Production env: refuse to start with missing/short `SESSION_SECRET`; unlisted CORS origins denied.
- [ ] **E4.** Prometheus scrape of `/metrics`, load the alerts from `docs/deployment.md`.
- [ ] **E5.** Backup + restore drill with `scripts/backup-postgres.ps1`; compare row counts.
- [ ] **E6.** Build the desktop MSI against the HTTPS API base; verify login, sync and update through the proxy.
  - Accept: green checklist in `docs/deployment.md`, including the restore drill.

## P1. Product completeness

- [ ] **F1. Desktop end-to-end verification** on Windows/WebView2: script and results in `docs/desktop-verification.md` (online sync, offline work for hours/days, reconnect, two-instance conflict, update).
- [ ] **F2. OpenAPI + generated TS types:** `utoipa` annotations, `GET /api/v1/openapi.json`, `pnpm gen:api`, CI fails on drift.
- [ ] **F3. Hotel time zone:** store a time zone per hotel and bucket reports/board by it instead of UTC.
- [ ] **F4. Turkish UI** (i18n scaffolding exists) with language switch; TRY and other currencies per hotel.
- [ ] **F5. PDF export** server side (today PDF = browser print), including reservation vouchers and reports.
- [ ] **F6. Sync status UX:** online/offline state, last synced time, queued operations, unresolved conflicts, last error, "Sync now" button.
- [ ] **F7. Password and security hardening:** make default/short provisioning passwords expire on first login (already forced), add 2FA option, session revocation on deactivation.
- [ ] **F8. Scale seed + load test:** `seed/scale.sql` for 1,000 hotels x 50 rooms x 2 years; run board, availability, save, full-year report; record in `benchmarks/REPORT.md`; tune indexes/pool if a target is missed.
- [ ] **F9. Reference visual polish** for `/guests` and `/channels`; keep 375 px and 700 px layouts.

## P2. Later

- [ ] G1. Branding: real multi-size `.ico` (16-256) and PNGs, regenerate with `pnpm tauri icon`.
- [ ] G2. Release builds with `cargo auditable`; verify with `cargo auditable extract`.
- [ ] G3. Read caching only if a target breaks (room types/rooms/board per window, invalidate on write).
- [ ] G4. Optional LAN hub so several reception PCs in one hotel share offline state.
- [ ] G5. Mobile app reusing the API and sync protocol.
- [ ] G6. Deferred site-2 modules (e-invoice, stock/expenses, agencies, currency exchange) only after a product decision.

---

## Definition of done for the whole product

1. A hotel can run for days offline on the desktop app (all daily actions work), and everything syncs when the connection returns.
2. The online PostgreSQL data is always the final truth; overbooked bookings become visible `conflict` items and can be reassigned, never lost.
3. The desktop app updates itself with signed packages and keeps local data.
4. Local database is encrypted; secrets are DPAPI-protected; no secrets in the repo.
5. All checks green: `cargo test --workspace`, `cargo clippy --workspace`, `pnpm lint`, `pnpm build`, load-test targets met, deployment checklist green.
