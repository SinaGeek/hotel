# HMS mobile (G5)

Reception companion for phones — **reuses the same server API and the same
offline sync protocol as the web app and the desktop shell**. No server-side
change is required; the mobile build is a second client of `/api/v1`.

| Concern | Reused from | Where |
|---|---|---|
| Wire types | `apps/web/src/types/generated/api.ts` | `tsconfig.json` path alias `@hms/shared-types` |
| REST contract | `/api/v1/*` (README §12) | `src/api/client.ts` |
| Sync protocol | `hms_core::sync` (protocol v1) | `src/api/sync.ts` |
| Auth | `POST /auth/login` bearer token | `src/LoginScreen.tsx` |

The only mobile-specific piece is **storage**: AsyncStorage instead of the web's
`localStorage` or the desktop's SQLCipher SQLite. Operation ids are still
client-generated v7-shaped UUIDs, the outbox is durable, and pushes are
idempotent, so a phone that was offline for hours replays cleanly.

## Screens (all thin over existing endpoints)

| Tab | Endpoint(s) |
|---|---|
| Board | `GET /board`, `PATCH /rooms/:id/cleanliness` |
| Bookings | `GET /reservations`, `POST /reservations/:id/check-in|check-out` (offline-staged) |
| Housekeeping | `GET /rooms`, `PATCH /rooms/:id/cleanliness` (offline-staged, last-write-wins) |
| Today | `GET /reports/summary` |
| Settings | session, queue depth, manual sync, **LAN hub probe** (`GET /api/v1/lan/status`, G4) |

## Running

```powershell
cd apps/mobile
pnpm install
# point at the hotel server; emulator loopback is 10.0.2.2 on Android
$env:EXPO_PUBLIC_API_BASE = 'https://localhost:8443/api/v1'
pnpm start
```

The LAN-hub probe in **Settings** expects the optional `hms-lan-hub` binary
(`cargo run -p hms-lan-hub`) reachable on the hotel LAN; see
`docs/lan-hub.md`.

## Verification

- `pnpm typecheck` (tsc --noEmit) — types are shared with the web app, so a
  change to the generated API types breaks the mobile build at compile time.
- `GET /health` + `POST /auth/login` smoke test against the native dev stack
  (`scripts/dev-stack.ps1 -WithCaddy`).
