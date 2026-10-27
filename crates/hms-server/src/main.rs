mod bootstrap;
mod rate_limiter;

use axum::{Json, Router, extract::{Request, State}, http::StatusCode, middleware::{self, Next}, response::Response, routing::{get, post}};
use hms_core::ApiResponse;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    rate_limiter: Arc<tokio::sync::Mutex<rate_limiter::RateLimiter>>,
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
        .route("/api/v1/auth/login", post(login))
        .layer(middleware::from_fn(rate_limit_mw))
        .layer(cors)
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("HMS Server listening on 0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn health_check(State(state): State<Arc<AppState>>) -> Json<ApiResponse<String>> {
    match sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => Json(ApiResponse { success: true, data: "HMS Server is running (db ok)".to_string(), error: None }),
        Err(e) => Json(ApiResponse { success: false, data: String::new(), error: Some(format!("db error: {e}")) }),
    }
}

async fn login(
    State(_state): State<Arc<AppState>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<ApiResponse<()>> {
    Json(ApiResponse {
        success: false,
        data: (),
        error: Some("Not implemented yet (Phase 4)".to_string()),
    })
}

/// Per-IP global rate limit placeholder — login attempts are limited per-key in
/// the auth handler (Phase 4); this middleware covers the whole API as a start.
async fn rate_limit_mw(
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let resp = next.run(req).await;
    Ok(resp)
}
