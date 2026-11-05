# HMS LAN Hub (G4)

The optional LAN Hub lets multiple reception PCs on one hotel LAN see one
another's queued offline operations before the WAN returns. It is a small Axum
service with a local SQLite outbox/change log; it is **not** a replacement for
the PostgreSQL-backed cloud server and it does not hold a full hotel mirror.

## Windows native start (no Docker)

Build with the repository toolchain:

```powershell
cargo build --release -p hms-lan-hub
$env:HOTEL_ID = '4442'
$env:LAN_PORT = '8089'
$env:LAN_DB_PATH = 'C:\ProgramData\HMS\lan-hub.sqlite'
$env:LAN_HUB_TOKEN = '<generate a long random secret; configure the same value on every peer>'
$env:HMS_API_BASE = 'https://hms.example.com/api/v1'
.\target\release\hms-lan-hub.exe
```

Open TCP `8089` only to the hotel's trusted reception VLAN. Because hotel
networks are not inherently trusted, the daemon **refuses to start without
`LAN_HUB_TOKEN`**. Every route, including status, requires
`Authorization: Bearer <LAN_HUB_TOKEN>`. Do not use the development CORS
permissive layer as a reason to expose this service to the public Internet; use
a host firewall allow-list and, for untrusted Wi-Fi, terminate TLS in Caddy.

Peers configure `http://<hub-private-ip>:8089` and the same token. The hub's
API accepts the canonical `hms_core::sync` wire DTOs:

- `POST /api/v1/lan/push` — durable, idempotent outbox enqueue + local change
  log append.
- `GET /api/v1/lan/pull?since=<counter>&limit=<n>` — pull local changes for
  another reception PC.
- `GET /api/v1/lan/status` — pending upstream count and local sequence.

## Upstream forwarding status

The library exposes `forward_outbox_upstream(state, bearer_token, client)`,
which posts queued operations to the canonical cloud `/sync/push` endpoint and
marks them forwarded only after an HTTP success. Call it from a managed service
loop that has a valid cloud user token; the daemon intentionally does not store
a cloud credential by default. The current desktop/mobile clients can probe the
hub, but do not yet route their outbox through it automatically. That integration
requires peer identity/token provisioning and is therefore an operator/product
rollout step, not an implicit LAN broadcast.

## Boundaries and test evidence

- Storage is per configured `HOTEL_ID`; a hub must never be reused between
  hotels. The push body is checked against the configured hotel id before
  accepting operations.
- The hub does not make server-side reservation decisions. It is a relay; cloud
  validation/conflict handling remains authoritative when WAN returns.
- Unit tests: `cargo test -p hms-lan-hub` covers peer push/pull, tenant
  isolation, outbox drain/forwarded marking, and change-log retention.
- This is a basic relay, not a consensus system: do not run multiple hubs for
  one hotel or treat a hub-local reservation as a confirmed cloud booking.
