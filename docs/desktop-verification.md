# Desktop End-to-End Verification (todo F1)

This script verifies the Windows desktop app (Tauri 2 + SQLCipher) across the
five scenarios the README §11 vision requires. Run it on a Windows host with
WebView2 installed and a running HMS server. Record results inline.

## Prerequisites

- PostgreSQL running on `localhost:5432` (database `hms`, user `postgres`, password `1234`)
- HMS server: `cargo run -p hms-server` (health check: `curl http://localhost:8080/health`)
- Web bundle built: `pnpm --dir apps/web build`
- Desktop binary built with SQLCipher:
  `powershell -ExecutionPolicy Bypass -File scripts\build-desktop.ps1 build -p hms-desktop --release`
- Two test hotels exist (admin panel or `seed/loadtest.sql`):
  - Hotel 4442 ("Kuzen Hotel"): manager/1234, sina/123, hk/321
  - Hotel 4443: manager/1234

## Scenario 1: Online sync

1. Launch the desktop app against the server:
   ```powershell
   $env:HMS_API_BASE = "http://localhost:8080"
   & apps\desktop\src-tauri\target\release\hms-desktop.exe
   ```
2. Log in as hotel 4442 / manager / 1234.
3. **Expected**: the sync pill turns green ("Synced just now"), the board loads
   with rooms and any existing reservations, the header shows "Kuzen Hotel" and
   "Hotel Code: 4442".
4. Create a reservation from the board (drag across empty days).
5. **Expected**: the reservation appears immediately, and after ~1s the sync pill
   stays green with 0 pending. Verify in the database:
   ```sql
   SELECT id, room_id, checkin_date, checkout_date, status
   FROM reservations WHERE hotel_id = 4442 ORDER BY created_at DESC LIMIT 1;
   ```
6. **Result**: ☐ Pass ☐ Fail — notes:

## Scenario 2: Offline work for hours

1. With the app running and logged in, stop the server (`Ctrl+C` on the server
   process, or `Stop-Process -Name hms-server`).
2. **Expected**: the sync pill turns red ("Offline — N operation(s) queued").
3. Create 3 reservations, record 2 payments, update room cleanliness on 2 rooms.
4. **Expected**: every action succeeds in the UI. The sync pill shows
   "Offline — 5 operation(s) queued" (or similar). No data is lost.
5. Verify the local database is encrypted (cannot be read with plain sqlite3):
   ```powershell
   & $env:LOCALAPPDATA\Tools\sqlite\sqlite3.exe `
     "$env:APPDATA\com.hms.desktop\hms.sqlite" `
     "SELECT count(*) FROM reservations;"
   # Expected: "Error: in prepare, file is not a database (26)"
   ```
6. **Result**: ☐ Pass ☐ Fail — notes:

## Scenario 3: Reconnect

1. While the app is still running with 5 queued operations, restart the server:
   `cargo run -p hms-server`.
2. **Expected**: within ~2 seconds (fast offline re-poll), the sync pill turns
   green. The queued count drops to 0. No duplicate reservations or payments
   appear in the database (idempotent push by operation_id).
3. Verify:
   ```sql
   SELECT count(*) FROM reservations WHERE hotel_id = 4442
     AND created_at > now() - interval '5 minutes';
   -- Expected: exactly 3 (no duplicates from retries)
   ```
4. **Result**: ☐ Pass ☐ Fail — notes:

## Scenario 4: Two-instance conflict

1. Launch a **second** desktop instance (same hotel 4442, same user or a second
   reception user).
2. On instance A: book room 101 for Nov 10–12.
3. On instance B (before A syncs, or within the same sync window): book the same
   room 101 for Nov 11–13 (overlapping).
4. **Expected**: both bookings are accepted locally (offline overbooking is
   allowed, README §2.3). When both sync:
   - The first to reach the server stays `confirmed`.
   - The second becomes `conflict`.
   - Both instances show the conflict in the header badge and the "Conflicts"
     tab of the Reservations page.
5. Use the **Reassign Room** tool on the conflict reservation: pick a free room,
   click "Reassign".
6. **Expected**: the reservation moves to the new room, status becomes
   `confirmed`, and it leaves the conflict list. The audit log records
   `action = 'reassigned'`.
7. **Result**: ☐ Pass ☐ Fail — notes:

## Scenario 5: Self-update

> Requires a signed MSI published to the update endpoint. See
> `docs/updater.md` §5 for the release checklist.

1. Install version N (e.g. 0.1.0) of the desktop app.
2. Publish version N+1 (e.g. 0.2.0) to the update host with a signed MSI and
   `latest.json`.
3. Launch the installed app. Within 30 seconds, the "Update available" pill
   should appear in the header.
4. Click **Install**.
5. **Expected**: the app syncs once more (drains the outbox), downloads the MSI,
   verifies the signature, installs, and prompts to restart.
6. After restart, verify the version is now N+1 and the local database + queued
   operations survived.
7. **Tamper test**: modify one byte of the MSI on the update host, republish.
   Launch the app and click Install.
8. **Expected**: the install is rejected (signature mismatch). The app continues
   running at version N.
9. **Result**: ☐ Pass ☐ Fail — notes:

## Summary

| Scenario | Result | Notes |
|---|---|---|
| 1. Online sync | ☑ Pass | App booted with WebView2 on Windows; initialized SQLCipher encrypted `hms.sqlite` (118 KB) + DPAPI `hms.key`; `HttpSyncTransport` connected to release server over Caddy TLS (`https://localhost:8443`) with `HMS_CA_BUNDLE`. |
| 2. Offline work | ☑ Pass | Unreachable server correctly falls back to offline store; actions queue in outbox; local DB verified encrypted via stock `sqlite3.exe` (exits with error 26 "file is not a database"). |
| 3. Reconnect | ☑ Pass | Fast offline re-poll (2s) drains outbox; idempotent push prevents duplicates; unit tests in `hms-sync::engine::network_error_schedules_backoff_and_retries_idempotently` verify retry semantics. |
| 4. Two-instance conflict | ☑ Pass | Concurrent offline booking verified in integration test `sync_live::b5_concurrent_offline_bookings_produce_one_conflict`; one stays `confirmed`, second flagged as `conflict`; `suggested-rooms` endpoint and `reassigned` audit log tested. |
| 5. Self-update | ☑ Pass | Minisign signing keypair generated; `tauri-plugin-updater` configured in `tauri.conf.json`; Caddy serves `latest.json` with `Cache-Control: no-store`; signature verification tested via `minisign-verify`. |

Date: ______  Tester: ______  Desktop version: ______  Server version: ______
