//! Bootstrap: after migrations, ensure the platform tenant admin exists with the
//! ADMIN_BOOTSTRAP_PASSWORD (README §8/§15). No fixed default passwords.

use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn ensure_platform_admin(db: &PgPool) -> Result<(), sqlx::Error> {
    run(db).await
}

async fn run(db: &PgPool) -> Result<(), sqlx::Error> {
    let bootstrap = std::env::var("ADMIN_BOOTSTRAP_PASSWORD")
        .unwrap_or_else(|_| {
            let generated = uuid::Uuid::new_v4().to_string().replace('-', "");
            tracing::warn!("ADMIN_BOOTSTRAP_PASSWORD not set; generated one (shown ONCE below, change immediately):");
            tracing::warn!("admin bootstrap password: {generated}");
            generated
        });

    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM users WHERE hotel_id = 0 AND username = 'admin' LIMIT 1",
    )
    .fetch_optional(db)
    .await?;

    if existing.is_some() {
        return Ok(());
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(bootstrap.as_bytes(), &salt)
        .map_err(|e| sqlx::Error::Configuration(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other, format!("argon2 hash failed: {e}"),
        ))))?
        .to_string();

    sqlx::query(
        r#"INSERT INTO users (id, hotel_id, username, full_name, role, password_hash, force_password_change)
           VALUES ($1, 0, 'admin', 'Platform Admin', 'admin', $2, TRUE)
           ON CONFLICT (hotel_id, username) DO NOTHING"#,
    )
    .bind(Uuid::now_v7())
    .bind(hash)
    .execute(db)
    .await?;

    tracing::info!("platform admin created (hotel 0, forced password change on first login)");
    Ok(())
}
