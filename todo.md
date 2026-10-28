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

## 5b. Web UI parity with the reference app (app.hmsflow.com) — [P1] *(new 2026-10-28; source: REA live captures, see `docs/ui-reference.md` + `docs/apis.md`)*

Goal: restyle/extend `apps/web` to match the reference hotel app's frame and patterns, module by module, using the same server APIs we already have. English-only UI (established decision); the reference is TR in places (e.g. "%76", "Tek Kisilik") — we render English.

- [x] **5b.1 Shell + design system**: dark icon/label sidebar (Dashboard, Monthly Room, Guests, Bookings >, Facility >, Transactions >, Periodic Prices >, Reports, Invoices >, Agencies &, Tools >; Help at bottom, collapse control) with active pill + left accent bar; header with hotel name + Hotel Code, bell, language menu, dark-theme toggle, user menu; violet primary buttons; round + FAB; white cards with uppercase table headers; soft status pills; list footer (`1-20 / 86`, per-page select, first/prev/next/last). Shared components: `<CardTable>`, `<ListFooter>`, `<FilterBar>`, `<StatusPill>`, `<PageHeader>`; token file for colors.
  - Done 2026-10-28 (commit): token-driven `styles.css` (violet primary `#6366f1`, navy sidebar `#161629`, light canvas, dark-theme vars); new `AppShell` (dark icon/label sidebar with expander groups + active pill + left accent bar, header with hotel name + `Hotel Code: N` + bell/language/dark-toggle/user chip, violet FAB); `components/icons.tsx` inline SVG set; `components/ui.tsx` shared `PageHeader`/`StatusPill`/`FilterBar`/`ListFooter` (ListFooter + FilterBar get wired to real paging in 5b.4; CardTable covered by `.card` + `table.hms` styles); `nav.board` renamed "Monthly Room"; Reservations statuses now pills.
  - Deviation: deferred reference sections (Transactions, Periodic Prices, Invoices, Agencies & Accounts, Help/Expander extras) are hidden from the sidebar by deliberate scope decision; expanders only list supported children (Bookings > List, Facility > Rooms/Housekeeping, Tools > Staff).
- [x] **5b.2 Monthly Room** (upgrade `Board.tsx`): Calendar/List views; 7 days / 15 days / Month range toggle + pager (Today disabled on current period); per-room-type price header rows; grouped room rows; green (direct) vs gold (channel) booking bars with "n nights - Channel" subtitle; legend row (Reservation, Pending, Blocked, Out of Service + Clean/Dirty); drag across empty days to create; click bar -> booking detail (existing modal).
  - Done 2026-10-28 (commit): `Board.tsx` rebuilt as Monthly Room: Calendar/List views; Calendar | List + 7 days / 15 days / Month segmented controls; period pager (prev/next, Today) with Today disabled while the period contains today; sticky rail with the period label + "{{n}} rooms"; hotel row (accent bar, uppercase name, room count); per-room-type section rows with per-day prices fed by `/price-rules` (latest date_from wins, seasonal fallback) matching the reference price strips; grouped room rows with cleanliness dots; stay bars with guest name + "n nights" subtitle; today column tint + circled date; legend footer chips + "Drag across empty days to add a record" + Clean/Dirty dots.
  - Drag-to-create uses a synchronous geometry ref committed on mouseup (React state batching lag would otherwise commit a stale span) and hit-tests day cells so floating stay bars cannot interrupt; verified live: 5-night drag prefill, single-click 1-night prefill, create POST, double-booking rejection by the server, bar click detail modal.
  - Deviations (data mapping, UI parity kept): bars are colored by status (confirmed/checked-in green, conflict gold, closed-out gray) because BoardStay carries no channel; List view renders our stays with status pills; per-type daily prices come from our price rules (reference shows its own per-type rates).
- [ ] **5b.3 Dashboard**: suggestion/search row, status chips with counts (Departing Today/Tomorrow, Long Term, Will Check-in, Out of Service, Dirty Room), room-card grid (number header colored by state, guest/type name, checkout time, "Make Reservation" on vacant), stat cards (Occupied, Guests Today, Vacant, Total).
- [ ] **5b.4 List-page framework + Bookings + Guests**: tabs (Active/Upcoming/Archive/Cancelled/All) mapped to `status=1&checkoutIsNullOrZero`-style server filters; Guests with Download (CSV export), Birthday/Garage quick views (hide if data doesn't support them), category/country filters.
- [ ] **5b.5 Facility module**: Rooms, Room Type, Accommodation Type, Facilities, Update Status (inline cleanliness dropdowns), Housekeeping (building/status filters) — we already have rooms + room-types + housekeeping APIs.
- [ ] **5b.6 Reports hub + core reports**: card-grid hub grouped by category (Daily/Bookings/Rooms & Rates/Guests/Finance); implement the Daily set first as table+chart tabs with date picker + Download (CSV): Occupancy Forecast (per-date arrivals/occupied/vacant/occupancy %), In-House (KPI cards + rows), Main Courante; others follow Phase 7 report APIs.
- [ ] **5b.7 Channel-manager surfaces** (static-first): Channels list (logo/code/status, "In periodic pricing" filter) from the `channels` table; Periodic Calendar availability grid + rates grid (editable cells, Enter-to-save) using channel/price-rule endpoints. Bulk/batch update screens only if sync scope needs them.
- [ ] **5b.8 Deferred-module placeholders** (out of scope per README §4 deferral): Invoices, Transactions (Expense/Stock/Open Sale), Agencies & Accounts, Tools (Settings, Exchange, Staff, Applications) — decision: hide from sidebar for our roles, or thin disabled placeholders.
- [ ] **5b.9 Verify**: `pnpm preview` against a live server, side-by-side screenshot pass (our page vs the reference capture) per module; English-only grep for Turkish diacritics returns 0; commit + stage review.
- Note: capture lesson — `/channel-bookings` on the REFERENCE threw its own error boundary; keep our shell on route errors unchanged.

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

## 7. Reports (README §10) — [P1] *(core done; polished charts + monthly view still open)*

- [x] Report queries must read `daily_revenue`, never raw reservations — `crates/hms-server/src/api/reports.rs`: `GET /reports/{summary,by-channel,by-day,by-room}?from&to` (inclusive `from`, exclusive `to`, max 400 days; tenant-scoped; `report_view_tenant` permission — live-verified 403 for housekeeping)
- [x] Earnings per channel (bar â€” horizontal chart + table), per day (grouped bar chart + table), per room/room type (table) — **still open**: per-month rollup view, donut/heatmap variants (cosmetic)
- [x] Occupancy %, ADR, RevPAR KPI cards (`occupied_nights / rooms Ã— days`, `room_revenue / occupied_nights`, `room_revenue / available_room_nights`) — **live-verified**: 22.2% (4/18), ADR â‚º3,000, RevPAR â‚º666.67 for Oct 27 â†’ Nov 5 test data
- [ ] Collected-vs-accrued toggle (API returns both room/extra revenue AND collected payments; the UI toggle itself still TODO) â€” **and confirm with hotels which they expect** (open question in Â§10)
- [x] CSV export for every report (BOM + Blob download, per table); PDF export = Print button (browser print-to-PDF) â€” native PDF rendering still open
- [x] Hotel users see only their own hotel (queries all filter `auth.hotel_id`); platform-admin cross-hotel view deferred to Phase 10 (admin currently uses the same tenant endpoints with hotel 0 â€” to revisit with Issue "admin hotels list" in Phase 10)
- [ ] Desktop computes the same reports from local data when offline *(Phase 6)*

**Phase 7 verification notes:** endpoints exercised live on the 4173 preview as `mtest` â€” Oct 27â†’Nov 5 range: room â‚º12,000 + extras â‚º300 (minibar), collected â‚º0 (reversed payment nets out), occupancy 4/18 nights = 22.2%, ADR â‚º3,000 = room-type base price, RevPAR â‚º666.67. `by-channel`/`by-room`/`by-day` totals match `daily_revenue` sums (SUM room_revenue_cents = 1,200,000 cents, extras = 30,000 cents).

## 8. Tests (README §16) — [P1] *(hms-tests crate added; 8 tests green — commit `c2f5c21`)*

- [x] Permission matrix test: every role Ã— every permission matches §6 (`crates/hms-tests/tests/roles_matrix.rs` — matrices written BY HAND from §6 so no drift; wire-name round-trip; housekeeping masking invariant). 3 tests green.
- [x] Tenant isolation test: hotel A can never read/write hotel B (`db_guards.rs::tenant_isolation_rls_hides_and_blocks_other_hotels`) — connects as the REAL RLS role `hms_app` with `app.hotel_id`: own room visible, hotel B ops row invisible, cross-tenant UPDATE affects 0 rows (USING), cross-tenant INSERT violates WITH CHECK, `app.is_platform_admin` sees both tenants.
- [x] Double-booking tests (`db_guards.rs`): (a) exact/leading/trailing overlap REJECTED by the `no_double_booking` EXCLUDE constraint; adjacency [) allowed; cancelled/conflict don't block; tombstoned rows don't block; (b) **race test: 8 concurrent inserts -> exactly 1 wins**.
- [x] Payments append-only test: as `hms_app`, INSERT ok, SELECT ok, UPDATE/DELETE -> `permission denied` (GRANT-based).
- [x] Report integrity test: `sum(daily_revenue.room_revenue) == reservation.total_price_cents`, extras == folio ledger total, collected == payments ledger total.
- [ ] Sync tests: offline N days then sync; two offline clients same room â†’ `conflict`; server outage mid-sync; interrupted/repeated sync â†’ no duplicates; clock-skewed client cannot win *(Phase 6 — the sync crate doesn't exist yet)*
- [x] `crates/hms-tests` integration-test crate in the workspace + CI workflow (`.github/workflows/ci.yml`: cargo test + clippy + web lint/build, with a postgres:16 service container setting `TEST_DATABASE_URL`). **DB tests are gated on `TEST_DATABASE_URL`** (e.g. `postgres://postgres:postgres@localhost:5432/hms_test`) and print `[skip]` when unset, so `cargo test` stays green without infra. Local run: create the `hms_test` DB first (`DROP DATABASE IF EXISTS hms_test; CREATE DATABASE hms_test;`), set the env var, `cargo test -p hms-tests`.

**Test-run notes:** fixtures use unique-per-run hotel ids (no truncation needed); tests run in parallel (share `hms_test`); the platform-admin room-count assertion filters `hotel_id = ANY(fixture_ids)` because older fixture rows persist across runs. `ALTER ROLE hms_app LOGIN PASSWORD 'hmstest-local'` runs once per process (catalog-update race otherwise). hms-core clippy has 2 pre-existing benign warnings (custom `from_str`, derivable impl) — cleanup candidate for Phase 10.

## 9. Seed data + load testing (README §13) — [P1] *(targets proven — commit `6fbb5ef`)*

- [x] Seed generator (`seed/loadtest.sql`; the README's `hms-seed` crate is superseded by this SQL): hotel 4443 "Load Test Hotel" with 3 room types, 60 rooms, 5 channels, a year-long price rule per type, **480 reservations** (8 waves × 60 rooms: past / in-house / upcoming), **960 `daily_revenue` rows**, 169 payments, 111 folio rows. Idempotent (wipes 4443 first; dev hotel 4442 and platform tenant 0 untouched). Load-test manager `loadmgr` gets the local dev manager's password hash (hash reference only — no secret committed).
- [x] k6 load-test scripts (`benchmarks/k6/`: board.js, availability.js, reservation_save.js, reports_full_year.js — same thresholds, `PASSWORD`/`BASE_URL`/`HOTEL` env creds).
- [x] Prove targets with `crates/hms-tests/examples/loadtest.rs` (Rust HTTP runner instead of installing k6): **ALL TARGETS MET** — 16 VUs × 200 req: board 20.5/109.4 p50/p95, availability 6.6/13.1, saves 9.4/18.6, full-year report 3.7/6.5; 12-VU rerun confirms with wider margins. Results + hardware + reproduce steps in `benchmarks/REPORT.md`.
- [x] Perf fixes the proving exposed: **board N+1 removed** (batched room-type pricing + one occupancy/cheapest query per window — board was ~340 queries/request, p50 > 4 s → 4 queries, p50 ~20 ms); **migration 0008** — change_seq now from a dedicated PG sequence (the row-locked `hotels.next_change_seq` counter serialized every concurrent write); `record_change` moved to last-before-commit; `DB_POOL_MIN` pool pre-warm (cold backend spawn ~100–300 ms on Windows). Compression layer + `DB_POOL_MAX` added earlier.
- [ ] Caching of read-heavy data (room types, rooms, today's board) with invalidation on write — **open**: batched queries proved sufficient; cache layer deferred until a target threatens to break (noted in REPORT.md). Remaining §13 items from this bullet: connection pooling (done via sqlx pool + pre-warm ✓), response compression (CompressionLayer ✓), hashed static assets (web build pipeline — check at Phase 12).

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