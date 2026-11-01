//! Operator tool: rotate the guest field encryption key.
//!
//! Re-encrypts `guests.id_number_encrypted`, `phone_encrypted` and
//! `email_encrypted` from an old key to a new key, so the server can be
//! restarted with `GUEST_FIELD_ENCRYPTION_KEY` set to the new value.
//!
//! Usage:
//!   OLD_GUEST_FIELD_ENCRYPTION_KEY=<b64-32B> \
//!   NEW_GUEST_FIELD_ENCRYPTION_KEY=<b64-32B> \
//!   DATABASE_URL=postgres://... \
//!   cargo run -p hms-server --bin rotate-guest-keys -- [--dry-run] [--batch 500]
//!
//! Fallback: if `OLD_GUEST_FIELD_ENCRYPTION_KEY` is unset, the current
//! `GUEST_FIELD_ENCRYPTION_KEY` is used as the old key.
//!
//! Safety:
//! - One transaction; `--dry-run` rolls back at the end.
//! - Rows already encrypted with the new key are detected and skipped, so a
//!   crashed/partial run can simply be re-run.
//! - Rows that open with neither key are reported and left untouched (the run
//!   exits non-zero if any exist, after committing the successfully rotated
//!   rows, so no data is lost).
//! - Each rotated row writes an `audit_log` entry (`guest` / `key_rotation`).

use hms_core::guest_identity::{
    decode_key, decrypt_guest_field_with_key, encrypt_guest_field_with_key,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Default)]
struct Stats {
    scanned: usize,
    rotated: usize,
    already_new: usize,
    failed: usize,
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args: Vec<String> = std::env::args().collect();
    let dry_run = has_flag(&args, "--dry-run");
    let batch: i64 = arg_value(&args, "--batch")
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!("DATABASE_URL is required");
        std::process::exit(2);
    });
    let old_raw = std::env::var("OLD_GUEST_FIELD_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("GUEST_FIELD_ENCRYPTION_KEY"))
        .unwrap_or_else(|_| {
            eprintln!(
                "set OLD_GUEST_FIELD_ENCRYPTION_KEY (or GUEST_FIELD_ENCRYPTION_KEY) to the current key"
            );
            std::process::exit(2);
        });
    let new_raw = std::env::var("NEW_GUEST_FIELD_ENCRYPTION_KEY").unwrap_or_else(|_| {
        eprintln!("NEW_GUEST_FIELD_ENCRYPTION_KEY is required");
        std::process::exit(2);
    });

    let old_key = decode_key(&old_raw).unwrap_or_else(|e| {
        eprintln!("invalid old key: {e}");
        std::process::exit(2);
    });
    let new_key = decode_key(&new_raw).unwrap_or_else(|e| {
        eprintln!("invalid new key: {e}");
        std::process::exit(2);
    });
    if old_key == new_key {
        eprintln!("old and new keys are identical; nothing to do");
        std::process::exit(2);
    }

    let pool: PgPool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .unwrap_or_else(|e| {
            eprintln!("cannot connect to PostgreSQL: {e}");
            std::process::exit(1);
        });

    let mut tx = pool.begin().await.unwrap_or_else(|e| {
        eprintln!("cannot begin transaction: {e}");
        std::process::exit(1);
    });
    // Platform scope: rotation is a cross-hotel operator task.
    sqlx::query("SELECT set_config('app.is_platform_admin', 'true', true)")
        .execute(&mut *tx)
        .await
        .unwrap_or_else(|e| {
            eprintln!("cannot set platform scope: {e}");
            std::process::exit(1);
        });

    let mut stats = Stats::default();
    let mut last_id: Option<Uuid> = None;

    loop {
        let rows = sqlx::query(
            r#"SELECT id, hotel_id, id_number_encrypted, phone_encrypted, email_encrypted
               FROM guests
               WHERE ($1::uuid IS NULL OR id > $1)
               ORDER BY id
               LIMIT $2"#,
        )
        .bind(last_id)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await
        .unwrap_or_else(|e| {
            eprintln!("cannot read guests: {e}");
            std::process::exit(1);
        });

        if rows.is_empty() {
            break;
        }

        for row in &rows {
            let id: Uuid = row.get("id");
            let hotel_id: i64 = row.get("hotel_id");
            last_id = Some(id);

            let fields = [
                ("id_number_encrypted", row.get::<Option<String>, _>("id_number_encrypted")),
                ("phone_encrypted", row.get::<Option<String>, _>("phone_encrypted")),
                ("email_encrypted", row.get::<Option<String>, _>("email_encrypted")),
            ];
            if fields.iter().all(|(_, v)| v.is_none()) {
                continue;
            }
            stats.scanned += 1;

            // Already rotated? (decrypts with the new key)
            let all_new = fields
                .iter()
                .filter_map(|(_, v)| v.as_deref())
                .all(|ct| decrypt_guest_field_with_key(ct, &new_key).is_ok());
            if all_new {
                stats.already_new += 1;
                continue;
            }

            let mut rotated: [Option<String>; 3] = [None, None, None];
            let mut row_ok = true;
            for (i, (name, value)) in fields.iter().enumerate() {
                let Some(ct) = value.as_deref() else { continue };
                match decrypt_guest_field_with_key(ct, &old_key) {
                    Ok(plaintext) => match encrypt_guest_field_with_key(&plaintext, &new_key) {
                        Ok(reencrypted) => rotated[i] = Some(reencrypted),
                        Err(e) => {
                            eprintln!("guest {id}: encrypt failed for {name}: {e}");
                            row_ok = false;
                        }
                    },
                    Err(e) => {
                        eprintln!("guest {id}: cannot decrypt {name} with the old key: {e}");
                        row_ok = false;
                    }
                }
            }
            if !row_ok {
                stats.failed += 1;
                continue;
            }

            let update = sqlx::query(
                r#"UPDATE guests
                   SET id_number_encrypted = $2,
                       phone_encrypted     = $3,
                       email_encrypted     = $4,
                       updated_at          = now()
                   WHERE id = $1"#,
            )
            .bind(id)
            .bind(rotated[0].clone())
            .bind(rotated[1].clone())
            .bind(rotated[2].clone())
            .execute(&mut *tx)
            .await;
            if let Err(e) = update {
                eprintln!("guest {id}: update failed: {e}");
                stats.failed += 1;
                continue;
            }

            let audit = sqlx::query(
                r#"INSERT INTO audit_log (hotel_id, user_id, entity_type, entity_id, action, old_value, new_value)
                   VALUES ($1, NULL, 'guest', $2, 'key_rotation', $3, $4)"#,
            )
            .bind(hotel_id)
            .bind(id)
            .bind(serde_json::json!({ "fields": ["id_number_encrypted", "phone_encrypted", "email_encrypted"] }))
            .bind(serde_json::json!({ "rotated": true }))
            .execute(&mut *tx)
            .await;
            if let Err(e) = audit {
                eprintln!("guest {id}: audit write failed: {e}");
                stats.failed += 1;
                continue;
            }

            stats.rotated += 1;
        }
    }

    if dry_run {
        tx.rollback().await.ok();
        println!(
            "DRY RUN (rolled back): scanned={} rotated={} already_new={} failed={}",
            stats.scanned, stats.rotated, stats.already_new, stats.failed
        );
        return;
    }

    match tx.commit().await {
        Ok(()) => println!(
            "committed: scanned={} rotated={} already_new={} failed={}",
            stats.scanned, stats.rotated, stats.already_new, stats.failed
        ),
        Err(e) => {
            eprintln!("commit failed: {e}");
            std::process::exit(1);
        }
    }

    if stats.failed > 0 {
        eprintln!(
            "{} row(s) could not be rotated; they were left untouched. Fix and re-run.",
            stats.failed
        );
        std::process::exit(3);
    }
    println!("now restart the server with GUEST_FIELD_ENCRYPTION_KEY set to the new key");
}
