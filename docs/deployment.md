# Deployment runbook (README §17)

Everything here is deployment-side configuration. None of it runs during
`cargo test`; the automated test suite covers the application itself.

## 1. Build and start

```bash
cp .env.example .env       # set SESSION_SECRET, ADMIN_BOOTSTRAP_PASSWORD, GUEST_FIELD_ENCRYPTION_KEY, CORS_ALLOWED_ORIGINS
docker compose up -d --build
```

The server container applies PostgreSQL migrations on start and refuses to boot
in `APP_ENV=production` without a `SESSION_SECRET` of at least 32 bytes.

Build the web bundle for the reverse proxy:

```bash
cd apps/web
pnpm install --frozen-lockfile
pnpm build                # apps/web/dist
```

## 2. TLS and static hosting

Put Caddy or nginx in front of the API. Both serve `apps/web/dist` and proxy
`/api` to the server; samples live in `deploy/`:

- `deploy/Caddyfile` — automatic Let's Encrypt certificates.
- `deploy/nginx.conf` — certificate files managed by you/certbot.

TLS terminates at the proxy. The server speaks plain HTTP on 8080 and must not
be exposed publicly.

## 3. CORS

Set `CORS_ALLOWED_ORIGINS` to the exact HTTPS origins of the web app
(comma-separated, no paths, no wildcards). Same-origin requests through the
reverse proxy do not depend on CORS. The desktop app's webview origin is
`http://tauri.localhost` (or `https://tauri.localhost`); add it only if the
desktop connects directly rather than through the same host.

## 4. Monitoring

`GET /metrics` exposes Prometheus text metrics: request count, 5xx count, a
latency histogram, DB pool gauges, process uptime and the latest `change_log`
sequence (use it to alert on stalled sync). Restrict the endpoint to the
monitoring network at the proxy — it has no authentication.

Suggested alerts:

- `rate(hms_http_server_errors_total[5m]) > 0`
- `histogram_quantile(0.95, rate(hms_http_request_duration_seconds_bucket[5m])) > 0.2`
- `hms_db_pool_connections{state="idle"} == 0` for 5 minutes
- `time() - hms_change_log_latest_seq` is not meaningful on its own; alert when
  a hotel's desktop sync lag grows (see the sync status pill / `sync_ops`).

## 5. Backups

`scripts/backup-postgres.ps1` produces a compressed custom-format dump,
verifies it with `pg_restore --list`, prunes old files and prints the path:

```powershell
.\scripts\backup-postgres.ps1 -RetainDays 30
```

Schedule it daily and copy the dumps off the machine. The desktop SQLite copy
is explicitly **not** a backup (README §11/§17).

Restore drill:

```bash
createdb -h localhost -U postgres hms_restore
pg_restore -h localhost -U postgres -d hms_restore --clean --if-exists hms-YYYYmmdd-HHMMSS.dump
```

## 6. Desktop release

```bash
cd apps/web
VITE_API_BASE=https://hms.example.com/api/v1 pnpm build
cd ../desktop/src-tauri
cargo tauri build          # MSI in target/release/bundle/msi
```

The desktop app is served from `tauri.localhost`, so it needs that absolute
`VITE_API_BASE`. Automatic updates are not configured yet; distribute the MSI
manually (see `final product_remains.md`).
