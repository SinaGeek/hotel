# Guest field encryption key rotation

`guests.id_number_encrypted`, `guests.phone_encrypted` and
`guests.email_encrypted` are encrypted with XChaCha20Poly1305 using
`GUEST_FIELD_ENCRYPTION_KEY` (base64, 32 bytes).

To rotate the key without losing data, use the bundled operator tool. It
re-encrypts every stored ciphertext from the old key to the new key in a single
transaction.

## 1. Generate the new key

```bash
openssl rand -base64 32        # or: head -c 32 /dev/urandom | base64
```

Keep the old key. You need both during the rotation.

## 2. Dry run

```bash
OLD_GUEST_FIELD_ENCRYPTION_KEY="<old>" \
NEW_GUEST_FIELD_ENCRYPTION_KEY="<new>" \
DATABASE_URL="postgres://postgres:...@localhost:5432/hms" \
cargo run --release -p hms-server --bin rotate-guest-keys -- --dry-run
```

The report shows `scanned`, `rotated`, `already_new` and `failed`. The dry run
rolls back, so nothing changes.

## 3. Rotate

```bash
OLD_GUEST_FIELD_ENCRYPTION_KEY="<old>" \
NEW_GUEST_FIELD_ENCRYPTION_KEY="<new>" \
DATABASE_URL="postgres://postgres:...@localhost:5432/hms" \
cargo run --release -p hms-server --bin rotate-guest-keys
```

Notes:

- Idempotent: rows already encrypted with the new key are detected and skipped,
  so a crashed or partial run can simply be re-run.
- Rows that open with neither key are left untouched and reported; the tool
  exits with code 3 after committing the rows it could rotate.
- `--batch N` (default 500) controls the page size.
- Each rotated row appends an `audit_log` entry (`entity_type='guest'`,
  `action='key_rotation'`).

## 4. Switch the server

Set `GUEST_FIELD_ENCRYPTION_KEY` to the new key and restart the server. Delete
the old key from the environment afterwards.

## 5. Verify

```bash
# pick a guest that has identity data and check the export endpoint decrypts
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/guests/<id>/export
```

If the export returns plaintext identity fields, the rotation succeeded.

## Important

- Do not run the server with the new key **before** rotating: writes would mix
  keys and reads would fail for rotated rows.
- Rotation needs a role that can read and update all tenants (the tool sets the
  platform admin scope; connect as a superuser or `hms_app` with platform
  privileges).
- Take a database backup first (`scripts/backup-postgres.ps1`).
