//! SQLCipher key management (todo C2/C3): a random 32-byte key per install,
//! DPAPI-protected at rest, and the plaintext -> encrypted migration.
//!
//! Security model (README §11): the SQLite mirror holds guest identity and
//! financial data, so the file must be unreadable without the Windows user's
//! DPAPI context. The key never leaves the machine; the encrypted blob lives
//! next to the database (`hms.key`) and is bound to this Windows user+host.

use crate::dpapi;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// File name of the DPAPI-protected SQLCipher key beside the database.
pub const KEY_FILE: &str = "hms.key";

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("key material is not valid hex".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Load the DPAPI-protected key, generating one on first run.
pub fn load_or_create_key(db_path: &Path) -> Result<Vec<u8>, String> {
    let key_path = key_path(db_path);
    if key_path.exists() {
        let blob = std::fs::read(&key_path).map_err(|e| format!("read key: {e}"))?;
        let plain = dpapi::unprotect(&blob)?;
        let key = unhex(&String::from_utf8_lossy(&plain))?;
        if key.len() != 32 {
            return Err("stored SQLCipher key has an unexpected length".into());
        }
        return Ok(key);
    }
    let key = dpapi::random_bytes(32)?;
    let protected = dpapi::protect(hex(&key).as_bytes())?;
    std::fs::write(&key_path, &protected).map_err(|e| format!("write key: {e}"))?;
    Ok(key)
}

pub fn key_path(db_path: &Path) -> PathBuf {
    db_path.with_file_name(KEY_FILE)
}

/// `PRAGMA key` with the raw-hex form (`x'...'`) — SQLCipher derives the
/// actual encryption key through its KDF, so the file never stores the key.
pub fn apply_key(conn: &Connection, key: &[u8]) -> Result<(), String> {
    conn.pragma_update(None, "key", format!("x'{}'", hex(key)))
        .map_err(|e| format!("pragma key: {e}"))?;
    // keep decrypted pages out of the page cache once released
    let _ = conn.pragma_update(None, "cipher_memory_security", "ON");
    Ok(())
}

/// Encrypted databases start with the SQLCipher salt header, never with the
/// plaintext SQLite magic. Used to decide whether migration is needed.
fn is_encrypted(path: &Path) -> Result<bool, String> {
    use std::io::Read;
    if !path.exists() {
        return Ok(true); // nothing on disk yet: nothing to migrate
    }
    let mut head = [0u8; 16];
    let mut f = std::fs::File::open(path).map_err(|e| format!("open db: {e}"))?;
    let n = f.read(&mut head).map_err(|e| format!("read db: {e}"))?;
    if n == 0 {
        return Ok(true);
    }
    Ok(&head[..n.min(16)] != b"SQLite format 3\0")
}

/// Open the mirror encrypted, migrating a plaintext file in place first
/// (todo C3): `sqlcipher_export()` into a temp file, then swap. The outbox
/// lives in the same database, so it survives because the whole file moves.
pub fn open_encrypted(db_path: &Path, key: &[u8]) -> Result<Connection, String> {
    if !is_encrypted(db_path)? {
        migrate_plaintext(db_path, key)?;
    }
    let conn = Connection::open(db_path).map_err(|e| format!("open db: {e}"))?;
    apply_key(&conn, key)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(|e| format!("key verification failed (wrong key or corrupt db): {e}"))?;
    Ok(conn)
}

fn migrate_plaintext(db_path: &Path, key: &[u8]) -> Result<(), String> {
    let tmp = db_path.with_extension("enc.tmp");
    let _ = std::fs::remove_file(&tmp);
    let tmp_s = tmp.to_string_lossy().replace('\'', "''");

    let conn = Connection::open(db_path).map_err(|e| format!("open plaintext db: {e}"))?;
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS encrypted KEY \"x'{}'\";\
         SELECT sqlcipher_export('encrypted');\
         DETACH DATABASE encrypted;",
        tmp_s,
        hex(key)
    ))
    .map_err(|e| format!("sqlcipher_export: {e}"))?;
    drop(conn);

    // verify the new file opens with the key before replacing the original
    let check = Connection::open(&tmp).map_err(|e| format!("open temp db: {e}"))?;
    apply_key(&check, key)?;
    check
        .query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|e| format!("migrated db does not open with the key: {e}"))?;
    drop(check);

    let plain_backup = db_path.with_extension("plaintext.bak");
    let _ = std::fs::remove_file(&plain_backup);
    std::fs::rename(db_path, &plain_backup).map_err(|e| format!("park plaintext: {e}"))?;
    std::fs::rename(&tmp, db_path).map_err(|e| format!("swap encrypted: {e}"))?;
    // the plaintext copy must never stay on disk
    let _ = std::fs::remove_file(&plain_backup);
    let _ = std::fs::remove_file(db_path.with_extension("plaintext.bak-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("plaintext.bak-shm"));
    let _ = std::fs::remove_file(db_path.with_extension("enc.tmp-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("enc.tmp-shm"));
    let _ = std::fs::remove_file(format!(
        "{}.plaintext.bak",
        db_path.with_extension("").to_string_lossy()
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("hms-sqlcipher-{}-{name}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("hms.sqlite")
    }

    #[test]
    fn c2_encrypted_database_rejects_plain_sqlite_and_plaintext_reader() {
        let path = tmp_path("c2");
        let key = dpapi::random_bytes(32).unwrap();
        {
            let conn = open_encrypted(&path, &key).unwrap();
            conn.execute_batch(
                "CREATE TABLE probe (v TEXT); INSERT INTO probe VALUES ('secret-row');",
            )
            .unwrap();
        }
        // header is not the plaintext SQLite magic
        let head = std::fs::read(&path).unwrap();
        assert_ne!(&head[..16], b"SQLite format 3\0", "file must be encrypted");

        // re-opening with the same key works and the row is there
        let conn = open_encrypted(&path, &key).unwrap();
        let v: String = conn
            .query_row("SELECT v FROM probe", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, "secret-row");

        // a plain (unkeyed) reader cannot read it
        let plain = Connection::open(&path).unwrap();
        assert!(
            plain
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                    .get::<_, i64>(0))
                .is_err(),
            "plain reader must fail on an encrypted db"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn c3_plaintext_install_migrates_and_keeps_outbox_intact() {
        let path = tmp_path("c3");
        // simulate an existing plaintext install with an outbox entry
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE outbox (operation_id TEXT PRIMARY KEY, payload TEXT NOT NULL);
                 INSERT INTO outbox VALUES ('op-1', '{\"a\":1}');",
            )
            .unwrap();
        }
        let key = dpapi::random_bytes(32).unwrap();
        {
            let conn = open_encrypted(&path, &key).unwrap();
            let n: i64 = conn
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1, "outbox must survive the encryption migration");
        }
        let head = std::fs::read(&path).unwrap();
        assert_ne!(
            &head[..16],
            b"SQLite format 3\0",
            "migrated file is encrypted"
        );
        assert!(
            !path.with_extension("plaintext.bak").exists(),
            "no plaintext copy may be left behind"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// C4: the app's own `LocalStore::open` produces a file that a stock,
    /// unmodified `sqlite3.exe` (no SQLCipher) refuses to read, while the app
    /// itself re-reads its rows. Set `HMS_SQLITE3_EXE` to the binary path to
    /// enable; without it the test asserts the header check only.
    #[test]
    fn c4_plain_sqlite3_cannot_open_the_mirror() {
        let path = tmp_path("c4");
        // the REAL app path: LocalStore::open generates the DPAPI key and
        // returns an encrypted connection with the schema applied
        {
            let mut store = crate::local_store::LocalStore::open(&path).expect("open mirror");
            store.kill_all_tables_for_test();
            store
                .conn_for_local_writes()
                .execute(
                    "INSERT INTO guests (id, full_name) VALUES ('g1', 'Sina Yilmaz')",
                    [],
                )
                .unwrap();
        }

        // the app re-opens and reads it (sync engine + offline reports path)
        {
            let mut store = crate::local_store::LocalStore::open(&path).expect("reopen mirror");
            let name: String = store
                .conn_for_local_writes()
                .query_row("SELECT full_name FROM guests WHERE id = 'g1'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(name, "Sina Yilmaz");
        }
        assert!(
            crate::sqlcipher::key_path(&path).exists(),
            "DPAPI key blob is written beside the db"
        );

        let Ok(exe) = std::env::var("HMS_SQLITE3_EXE") else {
            // no stock binary available: header assertion is the substitute
            let head = std::fs::read(&path).unwrap();
            assert_ne!(&head[..16], b"SQLite format 3\0");
            return;
        };
        let out = std::process::Command::new(exe)
            .arg(&path)
            .arg("SELECT count(*) FROM guests;")
            .output()
            .expect("run sqlite3");
        let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
        assert!(
            !out.status.success(),
            "stock sqlite3 must fail on the encrypted mirror (stdout: {})",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            stderr.contains("not a database") || stderr.contains("file is encrypted"),
            "expected a 'not a database' style error, got: {stderr}"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
