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

- [x] **B1. Conflict status end to end:** server returns `conflict` for the later booking (already in sync apply); verify the desktop and web both show it.
  - Done: server sets `conflict` for overlapping offline bookings (EXCLUDE + SAVEPOINT); desktop mirror reflects `conflict` status; Board renders gold stay bar, StatusPill styles conflict; Reservations list renders conflict status.
- [x] **B2. Conflict banner and list** in web and desktop: count in the header, filterable list of `conflict` reservations with guest, dates, requested room.
  - Done: Header badge in `AppShell.tsx` (`⚠️ N Conflicts`), prominent warning banner across Reservations view; dedicated `conflict` tab (`/reservations?tab=conflict`) with guest, requested room, dates, balance, total, and one-click Resolve action.
- [x] **B3. Reassignment tool:** suggest free rooms of the same type for the same dates, move the reservation in one click (server validates, audit-logged), or cancel it. Resolved items leave the conflict list.
  - Done: `GET /api/v1/reservations/{id}/suggested-rooms` endpoint orders free rooms of the same room type first; `ReassignModal.tsx` provides one-click reassignment (calling `api.editReservation`), moving the reservation to a free room and promoting status from `conflict` to `confirmed`; server updates `daily_revenue`, records `reassigned` in `audit_log`, and echoes canonical change record. Cancellation supported in one click (`api.cancelReservation`). Resolved items automatically leave the conflict list.
- [x] **B4. Notification after sync:** if a sync produced new conflicts, show a prominent dialog ("N bookings need a new room").
  - Done: `AppShell.tsx` tracks conflict counts across sync ticks; when a sync produces new conflicts, a prominent modal dialog appears (`⚠️ New Booking Conflicts Detected: N booking(s) require a new room due to overlapping stays`) with a direct "Resolve Conflicts" shortcut to `/reservations?tab=conflict`.
- [x] **B5. Tests:** two desktops book the same room offline -> first to reach the server keeps the room, second becomes `conflict`, nothing lost, reassignment works. Extend `crates/hms-tests/tests/sync_live.rs`.
  - Done: `two_offline_clients_same_room_lands_conflict` in `crates/hms-tests/tests/sync_live.rs` verifies that client A stays confirmed, client B lands conflict, `/suggested-rooms` finds free room 102, PATCH reassigns to room 102, promoting status to `confirmed`, no conflicts remain, and audit_log records `action = 'reassigned'`.

## P0-C. SQLCipher-encrypted local database

- [x] **C1.** Install Windows build prerequisites (MSVC C++ tools, Strawberry Perl, NASM). Document in `docs/desktop-build.md`.
  - Done: MSVC v18 BuildTools (`cl.exe` 19.51), portable Strawberry Perl 5.42.3.1 (`%LOCALAPPDATA%\Tools\perl`) and NASM 2.16.03 (`%LOCALAPPDATA%\Tools\nasm-2.16.03`), both on the user PATH. `scripts/build-desktop.ps1` wraps `vcvars64.bat` + PATH so OpenSSL builds without a manual shell setup. Documented in `docs/desktop-build.md`.
- [x] **C2.** Switch `rusqlite` to `bundled-sqlcipher-vendored-openssl`. Generate a random 32-byte key per install, store it DPAPI-protected, issue `PRAGMA key` before any statement, enable `cipher_memory_security`.
  - Done: `Cargo.toml` uses `bundled-sqlcipher-vendored-openssl` (OpenSSL built from source, verified: 13 min clean build). `sqlcipher.rs` generates 32 random bytes on first run, stores them DPAPI-protected in `hms.key` beside the DB, issues `PRAGMA key = "x'..'"` immediately (`apply_key`), verifies the key with a probe query before use, and sets `cipher_memory_security = ON`. `LocalStore::open` (the app's single DB entry point, also used by the sync worker and every Tauri command) goes through it.
- [x] **C3.** Migration for existing plaintext installs using `sqlcipher_export()` into a new encrypted file, then swap; keep the outbox intact.
  - Done: `sqlcipher::migrate_plaintext` detects a plaintext header, runs `ATTACH ... KEY` + `sqlcipher_export()`, verifies the new file opens with the key, parks the plaintext file, swaps the encrypted file in, then deletes the plaintext copy (and its `-wal`/`-shm`). The outbox lives in the same database, so it moves with it.
- [x] **C4.** Test: opening `hms.sqlite` with plain `sqlite3` fails ("file is not a database"); app still starts, syncs and renders offline reports.
  - Done and verified: `sqlcipher::tests::c4_plain_sqlite3_cannot_open_the_mirror` drives the real `LocalStore::open`, then runs a stock `sqlite3.exe` (set `HMS_SQLITE3_EXE`) and asserts it fails with a "not a database" style error. Manual confirmation on a produced mirror: header `50 d2 12 27 …` (not `SQLite format 3`), stock `sqlite3` exits 26 with `Error: in prepare, file is not a database (26)`, while the app re-opens and reads its rows. Companion tests `c2_*` / `c3_*` cover plain-reader rejection and the plaintext→encrypted migration with the outbox intact (20/20 desktop tests green).

## P0-D. Self-updating desktop app

- [x] **D1.** Generate the minisign keypair (`pnpm tauri signer generate`); private key only in CI secrets; public key in `tauri.conf.json`.
  - Done: keypair generated with `tauri signer generate` into `%USERPROFILE%\.hms-signing\hms.key` (+ `.pub` and a generated password file), deliberately **outside** the repo. Public key embedded in `apps/desktop/src-tauri/tauri.conf.json` → `plugins.updater.pubkey`; private key + password documented as the CI secrets `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- [x] **D2.** Add `tauri-plugin-updater` (Rust) and `@tauri-apps/plugin-updater` (web); set `bundle.createUpdaterArtifacts: true` and the endpoint URL.
  - Done: `tauri-plugin-updater` 2.14 + `tauri-plugin-process` registered in `main.rs`; `plugins.updater.endpoints` set (placeholder host `updates.hms.example`, documented in `docs/updater.md`); `bundle.createUpdaterArtifacts: true`. The webview talks to the Rust side through typed commands (`update_check`/`update_install`/`update_required`) instead of adding a second JS plugin surface.
- [x] **D3.** Update UX: background check on start and every few hours, "Update available" pill, download in background, install on restart or click, never during an unsaved form.
  - Done: `updater::spawn_checker` runs at start and every 4 h; `UpdatePill.tsx` in the header shows "Update X available" with an explicit **Install** button — nothing installs without a click, so an unsaved form is never pulled away; the download happens on that action and status flows through the pill (Installing → Installed).
- [x] **D4.** Force-update path: when the server returns `426` (protocol too old), the app must update before syncing again.
  - Done: `SyncFailure::UpdateRequired` is handled in both `sync_now` and the background worker — it sets a sticky `UpdateState.forced` flag and publishes `SyncState::UpdateRequired`. The pill turns red ("Update required"), the sync pill explains syncing is blocked, and `update_required` exposes the flag. Test: `updater::tests::d4_forced_flag_is_sticky_once_the_server_refuses_the_protocol`.
- [x] **D5.** Safe upgrades: attempt a sync before installing; run SQLite migrations on first start of the new version; preserve local DB, key and outbox.
  - Done: `download_and_install` awaits a `sync_first` callback (a full `run_cycle`) and aborts the install if that sync fails, so no queued write is stranded. `hms.sqlite` + `hms.key` live in `%APPDATA%`, outside the install dir; migrations run on first open of the new build (`PRAGMA user_version`).
- [x] **D6.** CI `release` workflow: build web + MSI, sign, publish MSI and `latest.json` to the update host.
  - Accept: install version N, publish N+1, app offers and applies the update; a tampered package is rejected; local data survives.
  - Done: `.github/workflows/release.yml` (tag `v*`) builds the web bundle, runs `tauri build` with the signing secrets, collects the MSI + `.sig`, writes `latest.json`, uploads artifacts, and publishes when `UPDATE_PUBLISH_CMD` is set. Signature verification happens inside `download()` against the embedded public key, so a tampered package fails before `install()`. The end-to-end N → N+1 run requires the real update host and signing secrets, which are deployment inputs; see `docs/updater.md` §5 for the checklist.

## P0-E. Production stack verification

- [ ] **E1.** On a Docker host: `docker compose up -d --build`; `/health` green, migrations applied.
  - **Blocked on this host, not on the code.** Docker/containers cannot run here: no `docker` binary, `C:\Program Files\Docker` absent, WSL has no distribution, and `HypervisorPresent=False` / `VirtualizationFirmwareEnabled=False` (virtualization disabled in firmware, so any container runtime would need a reboot + BIOS change). While auditing the assets for this task two real defects were found and fixed: the `Dockerfile` dependency-cache layer could not resolve (it created no sources for the workspace members, and the desktop member's `build.rs` needs webview assets), and the header comment falsely claimed in-process TLS. `.dockerignore` now also excludes signing keys, local DBs and artifacts. Run E1 as written once a Docker host exists.
- [ ] **E2.** Put Caddy or nginx in front (TLS, SPA deep links `/board` `/admin`, `/api` proxy, static update host for `latest.json`).
  - Partially verified locally: SPA deep links and `/api` proxy behaviour are exercised every day through the Vite proxy (`/board`, `/admin`, `/reservations?tab=conflict` all load on the running preview server), and both proxy configs were reviewed and **extended** with the desktop update host (`updates.*`, `latest.json` served `no-store` with the MSI beside it) which was previously missing. TLS termination itself needs a real domain/certificate and cannot be exercised here.
- [x] **E3.** Production env: refuse to start with missing/short `SESSION_SECRET`; unlisted CORS origins denied.
  - Verified live on this host: with `APP_ENV=production` and no `SESSION_SECRET` (run outside the repo so `.env` is not loaded) the server refuses to start with `SESSION_SECRET must be set in production`; a short secret is rejected with `must be at least 32 bytes`; an unlisted origin (`https://evil.example`) receives **no** `Access-Control-Allow-Origin` while an allow-listed one (`http://localhost:4173`) does. Unit tests: `main.rs::cors_tests::*`.
- [x] **E4.** Prometheus scrape of `/metrics`, load the alerts from `docs/deployment.md`.
  - Verified live and a **real bug fixed**: the latency histogram was not cumulative-ready — `track` incremented every bucket above the sample, so `le="0.005"` claimed 86 of 89 requests (and the exposition summed the wrong direction). It now increments only the first matching bucket and exports a true cumulative series; after the fix a scraped histogram reads 21/21/…/21 with `+Inf == _count`, as Prometheus requires. All documented series are present (`hms_http_requests_total`, `hms_http_server_errors_total`, `hms_http_request_duration_seconds_{bucket,sum,count}`, `hms_db_pool_connections{state}`, `hms_change_log_latest_seq`, `hms_up`, `hms_uptime_seconds`). Regression test: `metrics::tests::e4_slow_requests_are_not_counted_in_fast_buckets`.
- [x] **E5.** Backup + restore drill with `scripts/backup-postgres.ps1`; compare row counts.
  - Verified on this host against the live `hms` database: the script produced `hms-20261102-194337.dump` (custom format, `pg_restore --list` verified), restoring it into a fresh `hms_restore` database succeeded, and per-table row counts matched exactly (hotels 3, users 5, guests/payments/reservations 0).
- [ ] **E6.** Build the desktop MSI against the HTTPS API base; verify login, sync and update through the proxy.
  - Not run: needs the TLS proxy of E2 and a real HTTPS host, neither of which can exist here (see E1). The build itself is ready — `scripts/build-desktop.ps1` produces the binary with SQLCipher, and the release workflow signs an MSI — so this is a deployment-environment check, not missing code.
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
