mod api;
mod auth;
mod bootstrap;
mod db;
mod rate_limiter;

use axum::{routing::get, Json, Router};
use hms_core::ApiResponse;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub session_secret: String,
    pub rate_limiter: Arc<tokio::sync::Mutex<rate_limiter::RateLimiter>>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .compact()
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/hms".to_string());

    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .expect("failed to connect to PostgreSQL");

    // Migrations run on server start (README §17)
    sqlx::migrate!("../../migrations").run(&db).await.expect("migrations failed");
    tracing::info!("migrations applied");
    bootstrap::ensure_platform_admin(&db).await.expect("bootstrap failed");
    tracing::info!("bootstrap ensured");

    let state = AppState {
        db,
        session_secret: std::env::var("SESSION_SECRET").unwrap_or_else(|_| {
            let generated = uuid::Uuid::new_v4().to_string();
            tracing::warn!("SESSION_SECRET not set; using a random value (sessions invalidated on restart): set SESSION_SECRET in .env for persistence");
            generated
        }),
        rate_limiter: Arc::new(tokio::sync::Mutex::new(rate_limiter::RateLimiter::new(
            std::env::var("RATE_LIMIT_LOGIN").ok().and_then(|v| v.parse().ok()).unwrap_or(10),
            60_000,
        ))),
    };

    // Development-grade CORS. Locked down before production (README §8).
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health_check))
        .nest("/api/v1/auth", auth::router())
        .nest(
            "/api/v1",
            api::router()
                .route_layer(axum::middleware::from_fn_with_state(state.clone(), auth_waterfall)),
        )
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("HMS Server listening on 0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

/// Simple barrier middleware for /api/v1: requires a valid session.
/// Handlers additionally extract `AuthUser` for scoping and permissions.
async fn auth_waterfall(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let (mut parts, body) = req.into_parts();
    let _auth = <crate::auth::AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(&mut parts, &state)
        .await?;
    req = axum::extract::Request::from_parts(parts, body);
    Ok(next.run(req).await)
}

async fn health_check(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<ApiResponse<String>> {
    match sqlx::query_scalar::<_, i64>("SELECT 1::int8").fetch_one(&state.db).await {
        Ok(_) => Json(ApiResponse { success: true, data: "HMS Server is running (db ok)".to_string(), error: None }),
        Err(e) => Json(ApiResponse { success: false, data: String::new(), error: Some(format!("db error: {e}")) }),
    }
}
