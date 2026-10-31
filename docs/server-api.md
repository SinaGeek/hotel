# HMS server API (implemented contract snapshot)

This is a source-derived inventory of the Axum routes currently mounted by `crates/hms-server/src/main.rs` and `src/api/mod.rs`. It is not generated OpenAPI. Authenticated API routes require a valid bearer token and are tenant-scoped from the token; successful and ordinary API error payloads use the `ApiResponse<T>` envelope (`success`, `data`, optional `error`). The server listens on port 8080. `/health` is unauthenticated. Login/refresh/logout/password-change routes live under `/api/v1/auth`.

## Auth

| Method | Path | Notes |
|---|---|---|
| POST | `/api/v1/auth/login` | Hotel ID, username, password. Desktop may set `offline: true` to receive an offline verifier. |
| POST | `/api/v1/auth/refresh` | Refresh an authenticated session. |
| POST | `/api/v1/auth/logout` | End the current session. |
| POST | `/api/v1/auth/change-password` | Change password. |

## Tenant API

All paths below are prefixed `/api/v1` and require authentication unless a handler documents otherwise.

| Method | Path | Notes |
|---|---|---|
| GET, POST | `/hotels` | List/create hotels (admin scope enforced by handlers). |
| GET, PATCH | `/hotels/{id}` | Read/update a hotel. |
| GET, POST | `/users` | List/create hotel users. |
| PATCH | `/users/{id}` | Update a user. |
| GET | `/roles` | List roles. |
| GET, PUT | `/roles/{role}/permissions` | Read/update a role permission matrix. |
| GET, POST | `/room-types` | List/create room types. |
| PATCH | `/room-types/{id}` | Update a room type. |
| GET, POST | `/rooms` | List/create rooms. |
| PATCH | `/rooms/{id}/cleanliness` | Update cleanliness. |
| GET, POST | `/guests` | List/create guests. |
| PATCH | `/guests/{id}` | Update a guest. |
| GET, POST | `/reservations` | Keyset-paginated list/create. |
| GET | `/reservations/count` | Count matching reservations. |
| GET, PATCH | `/reservations/{id}` | Read/update a reservation. |
| POST | `/reservations/{id}/check-in` | Check in. |
| POST | `/reservations/{id}/check-out` | Check out. |
| POST | `/reservations/{id}/cancel` | Cancel. |
| GET | `/board` | Room/date board; `from` inclusive, `to` exclusive. |
| GET, POST | `/reservations/{id}/payments` | List/record payments. |
| POST | `/payments/{id}/reverse` | Reverse a payment (append-only ledger). |
| GET, POST | `/reservations/{id}/folio` | List/add folio transactions. |
| GET, POST | `/price-rules` | List/create price rules. |
| GET, POST | `/channels` | List/create channels. |
| GET | `/reports/summary` | Summary KPIs. `from` inclusive, `to` exclusive; maximum 400 days. |
| GET | `/reports/by-channel` | Channel revenue. |
| GET | `/reports/by-day` | Daily revenue. |
| GET | `/reports/by-room` | Room revenue. |
| GET | `/reports/forecast` | Daily arrivals/occupancy forecast. |
| GET | `/reports/inhouse` | In-house report. |
| GET | `/reports/maincourante` | Main-courante report. |
| POST | `/sync/push` | Push a versioned batch of offline operations. |
| GET | `/sync/pull` | Pull tenant changes after a cursor. |

## Sync protocol notes

- Tenant scope is derived from the authenticated token. A forged `hotel_id` in an operation body does not change the tenant; the push request envelope must match the token hotel.
- The protocol is versioned. The current client/server protocol constant and request/response serde types are in `crates/hms-core/src/sync.rs` (and related sync modules); keep those as the authoritative wire source.
- Pushes are idempotent by operation ID. Replays of applied operations return duplicate-ignored semantics; conflicts are preserved and represented as reservation status `conflict`, rather than dropped.
- Pull uses a monotonically increasing per-hotel change counter, not a timestamp. The client advances its cursor only as pulled changes are applied/deduplicated.
- The desktop stores its one-hotel mirror in SQLite, protects the auth token/offline verifier with Windows DPAPI, queues operations in `outbox`, and runs periodic sync. Current desktop UI writes are not yet fully local-first; see `todo.md` item 201.

## Current limitations

- This inventory is hand-maintained/source-derived; OpenAPI/TypeScript generation is still tracked in `todo.md`.
- CORS uses exact `CORS_ALLOWED_ORIGINS`; set the HTTPS web origin(s) in production. When `APP_ENV=production` and no allowlist is provided, cross-origin browser access is disabled. Same-origin deployments do not require CORS.
- Authenticated routes are mounted behind `auth_waterfall`; individual handlers apply permission checks and tenant filtering.
- Do not treat this document as a promise that deferred modules from the reference product are implemented.
