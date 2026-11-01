# Migration runbook (current implementation)

This document records how schema changes are applied today. It is a development/operator runbook, not a zero-downtime production migration plan.

## PostgreSQL server schema

- Location: `migrations/*.sql` (SQLx embedded migration source).
- Invocation: `crates/hms-server/src/main.rs` calls `sqlx::migrate!("../../migrations").run(&db)` before serving requests, then ensures the platform admin bootstrap record.
- Local development: start PostgreSQL (`docker compose up -d db`), set `DATABASE_URL`, then run `cargo run -p hms-server`. The server applies pending migrations automatically at startup.
- Inspect applied state with the SQLx `_sqlx_migrations` table. Do not edit a migration already applied to a shared database; add a new numbered migration instead.
- Backup before deploying schema changes. The repo does not yet provide automated backups, migration rollback scripts, or an operator-tested production rollout procedure.

## Desktop SQLite mirror schema

- Current schema file: `apps/desktop/src-tauri/desktop-migrations/0001_init.sql`.
- Invocation: `LocalStore::open` enables WAL/NORMAL and executes that SQL on each open. The DDL uses `CREATE TABLE IF NOT EXISTS`; there is currently no SQLite migration-version ledger or ordered migration runner.
- Adding a new column/index/table is not automatically upgraded for existing installations unless the DDL is explicitly migration-safe. Do not ship a breaking SQLite schema edit until a versioned migration ledger and upgrade tests are added.
- The mirror is a single-hotel operational cache, not a backup. On hotel switch, mirror data and sync bookkeeping are cleared to prevent tenant mixing. Auth token/offline login cache are DPAPI-protected in `kv`; other mirror data is not SQLCipher-encrypted.
- Before a desktop upgrade, close HMS Desktop and preserve a copy of `%APPDATA%/com.hms.desktop/hms.sqlite` (including `-wal` and `-shm` files if present). This is a recovery copy only, not a substitute for server backups.

## Remaining operational work

- Implement SQLite migration versioning before the next incompatible mirror schema change.
- Add and rehearse PostgreSQL backup/restore and replication procedures.
- Define a production release order and tested rollback strategy for server migrations and desktop updates.
- SQLCipher and Tauri auto-updater remain separately tracked blockers in `final_product_remains.md`.
