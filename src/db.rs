use crate::config::AppConfig;
use anyhow::Result;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::path::Path;
use uptime_domain::Role;

pub async fn connect(config: &AppConfig) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .min_connections(config.db_min_connections)
        .acquire_timeout(config.db_acquire_timeout)
        .connect(&config.database_url)
        .await
        .map_err(Into::into)
}

pub async fn setup(config: AppConfig) -> Result<()> {
    let pool = connect(&config).await?;
    sqlx::migrate::Migrator::new(Path::new("./migrations"))
        .await?
        .run(&pool)
        .await?;
    seed(&pool, &config).await?;
    Ok(())
}

pub async fn seed(pool: &PgPool, config: &AppConfig) -> Result<()> {
    let password_hash = hash_password(&config.demo_password)?;

    sqlx::query(
        r#"
        INSERT INTO users (email, name, role, password_hash)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (email) DO UPDATE SET
          name = EXCLUDED.name,
          role = EXCLUDED.role,
          password_hash = EXCLUDED.password_hash
        "#,
    )
    .bind("admin@example.com")
    .bind("Demo Admin")
    .bind(Role::Admin.as_str())
    .bind(password_hash)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO monitors (name, url, expected_status, interval_seconds, enabled, last_status)
        VALUES
          ('Primary Website', 'https://example.com', 200, 120, true, 'pending'),
          ('Rust Website', 'https://www.rust-lang.org', 200, 300, true, 'pending'),
          ('Documentation Site', 'https://docs.rs', 200, 300, true, 'pending')
        ON CONFLICT (name, url) DO UPDATE SET
          expected_status = EXCLUDED.expected_status,
          interval_seconds = EXCLUDED.interval_seconds,
          enabled = EXCLUDED.enabled,
          updated_at = now()
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .to_string())
}
