# Architectural restructure: server/ and client/ isolation

## Decision

The repository is now split into two independently buildable Cargo workspaces
with no shared local crate dependencies:

- `server/` — the HMS server (axum API + Postgres + LAN hub + integration tests,
  migrations, deploy config, container files).
- `client/` — the standalone desktop client (Tauri shell + React SPA + mobile),
  with its own vendored copies of only what it needs:
  - `client/crates/hms-core` — slim core: `roles` and `sync` protocol types only
    (the only `hms-core` modules the client uses).
  - `client/crates/hms-sync` — the sync engine (unchanged code, now client-side).

The desktop app is a standalone native Windows application in the spirit of
Notepad++:
- The `.exe` contains no web server; a Tauri webview loads the UI embedded in
  `client/web/dist`.
- It runs on its own SQLCipher-encrypted local SQLite database (per-user
  `%APPDATA%\com.hms.desktop`).
- First run offers Local-only mode (never contacts a server) or Cloud mode
  (server address + login). Remote work continues from the local mirror, and
  writes queue in the outbox until the server is reachable again or the user
  returns online.
- Settings ▸ Remote server lets the user change the server address after
  install, and Settings ▸ Sync scheduling chooses between automatic background
  sync and manual ("Sync now" only). Permanent offline mode remains a separate
  toggle. Credentials are never stored in Settings — authentication is the
  standard login (hotel code, username, password), with an opt-in cached
  offline-login verifier after a first successful online login.

## Why duplicate the core instead of sharing it

The protocol data structures (`server/crates/hms-core/src/sync.rs`) are
wire contracts. Isolation is deliberate: the client tree can be checked out,
built, tested, published and versioned standalone. The contract to keep is
explicit: `client/crates/hms-core/src/sync.rs` must stay textually equivalent
to `server/crates/hms-core/src/sync.rs` (and `roles.rs` only feeds local
permission gating). A protocol mismatch is caught immediately as a sync
handshake failure, never silently.

## Build commands (changed paths)

| Task     | Command                         | Working dir   |
|----------|---------------------------------|---------------|
| Server   | `cargo test --workspace`        | `server/`     |
| Server   | `cargo run -p hms-server`       | `server/`     |
| Client   | `cargo test -p hms-desktop`     | `client/`     |
| Client   | `cargo build --release -p hms-desktop` | `client/` || Web UI   | `pnpm build` / `pnpm dev`       | `client/web/` |
| Dev stack| `scripts/dev-stack.ps1`         | repo root     |

Old root `Cargo.toml`/`Cargo.lock` are gone; each workspace has its own.

## Directory equivalence (old → new)

| Old                              | New                                |
|----------------------------------|------------------------------------|
| `crates/hms-core`                | `server/crates/hms-core` (+ slim copy in `client/crates/hms-core`) |
| `crates/hms-server`              | `server/crates/hms-server`         |
| `crates/hms-lan-hub`             | `server/crates/hms-lan-hub`        |
| `crates/hms-tests`               | `server/crates/hms-tests`          |
| `crates/hms-sync`                | `client/crates/hms-sync`           |
| `apps/mobile`                    | `client/mobile`                    |
| `apps/desktop/src-tauri`         | `client/desktop` (crate root; `src-tauri` level removed on request) |
| `apps/web`                       | `client/web`                       |
| `apps/mobile`                    | `client/mobile`                    |
| `migrations/`, `seed/`, `deploy/`, `benchmarks/`, `Dockerfile`, `docker-compose.yml`, `.env(.example)` | moved under `server/` |

## Known follow-ups

- Desktop crate layout (2026-11-06): on user request the `src-tauri` level was
  removed — the Tauri crate root is now `client/desktop` itself (src, icons,
  gen, desktop-migrations, build.rs, tauri.conf.json, Cargo.toml). Deps point
  to `../crates/*` and `frontendDist` is `../web/dist`.
- **Standalone exe root cause (fixed)**: Tauri treats a build as "dev"
  (webview loads `build.devUrl`, i.e. `http://localhost:4173` → "localhost
  refused to connect") unless the `tauri/custom-protocol` feature is enabled.
  The Tauri CLI enables it for `tauri build`, but plain `cargo build --release`
  did not. Fix: the desktop crate now declares
  `[features] default = ["custom-protocol"]`, so every `cargo build --release`
  in `client/` embeds `client/web/dist` into the exe — no dev server, no
  localhost, no Edge-facing web page.

- `pnpm gen:api` (in `client/web`) still pulls the OpenAPI snapshot from the
  server — this is a build-time dev convenience, not a runtime dependency.
- The first-run "local only" mode initializes the encrypted client database;
  the full serverless hotel-creation wizard (hotel, users, room types, rates)
  remains future work — Setup text currently says so honestly.
