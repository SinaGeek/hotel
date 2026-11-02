# Migration runbook (current implementation)

This document records how schema changes are applied today. It is a development/operator runbook, not a zero-downtime production migration plan.

## PostgreSQL server schema

- Location: `migrations/*.sql` (SQLx embedded migration source).
- Invocation: `crates/hms-server/src/main.rs` calls `sqlx::migrate!("../../migrations").run(&db)` before serving requests, then ensures the platform admin bootstrap record.
- Local development: start PostgreSQL (`docker compose up -d db`), set `DATABASE_URL`, then run `cargo run -p hms-server`. The server applies pending migrations automatically at startup.
- Inspect applied state with the SQLx `_sqlx_migrations` table. Do not edit a migration already applied to a shared database; add a new numbered migration instead.
- Backup before deploying schema changes. The repo does not yet provide automated backups, migration rollback scripts, or an operator-tested production rollout procedure.

## Desktop SQLite mirror schema

- Current schema files: `apps/desktop/src-tauri/desktop-migrations/0001_init.sql` (base) and `0002_rate_columns.sql` (A3 per-night rates).
- Invocation: `LocalStore::open` opens the mirror through `sqlcipher::open_encrypted`, which (a) reads/creates the DPAPI-protected 32-byte key beside the DB (`hms.key`) and (b) migrates a plaintext file to SQLCipher in place via `sqlcipher_export()` on first open of an existing plain install. Then WAL/NORMAL are set, `PRAGMA cipher_memory_security = ON` is applied, and the embedded DDL runs.
- Versioning: the DDL uses `CREATE TABLE IF NOT EXISTS`; ordered upgrades are guarded by `PRAGMA user_version` (0002 requires version >= 2). Add a new numbered file plus a user_version bump for each migration-safe change; do not ship a breaking edit without a ledger entry and an upgrade test.
- The mirror is a single-hotel operational cache, not a backup. On hotel switch, mirror data and sync bookkeeping are cleared to prevent tenant mixing. Auth token/offline login cache are DPAPI-protected in `kv`; the entire database file is SQLCipher-encrypted, so a plain `sqlite3` open fails with "file is not a database".
- Before a desktop upgrade, close HMS Desktop and preserve a copy of `%APPDATA%/com.hms.desktop/hms.sqlite` **and** `hms.key` (the DPAPI key blob). Both are needed together; without `hms.key` the DB is unreadable by design.
- SQLCipher build prerequisites (MSVC C++ tools, Strawberry Perl, NASM) and the `scripts/build-desktop.ps1` wrapper are documented in `docs/desktop-build.md`.

## Remaining operational work

- Add and rehearse PostgreSQL backup/restore and replication procedures.
- Define a production release order and tested rollback strategy for server migrations and desktop updates.
- Tauri auto-updater remains a separately tracked blocker in `final_product_remains.md`.
