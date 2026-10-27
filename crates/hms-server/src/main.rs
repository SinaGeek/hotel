use axum::{Json, Router, http::{HeaderValue, StatusCode}, routing::{get, post}};
use hms_core::ApiResponse;
mod rate_limiter;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    database_url: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let state = AppState {
        database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://postgres:postgres@localhost/hms".to_string()),
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/auth/login", post(login))
        .with_state(Arc::new(state))
        .layer(axum::middleware::from_fn(cors_layer))
        .layer(axum::middleware::from_fn(rate_limit_middleware));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("HMS Server listening on 0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> Json<ApiResponse<String>> {
    Json(ApiResponse {
        success: true,
        data: "HMS Server is running".to_string(),
        error: None,
    })
}

async fn login() -> Json<ApiResponse<()>> {
    Json(ApiResponse {
        success: false,
        data: (),
        error: Some("Not implemented yet".to_string()),
    })
}

async fn cors_layer(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "access-control-allow-origin",
        HeaderValue::from_static("*"),
    );
    response.headers_mut().insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response.headers_mut().insert(
        "access-control-allow-headers",
        HeaderValue::from_static("Content-Type, Authorization"),
    );
    Ok(response)
}

async fn rate_limit_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    Ok(next.run(req).await)
}
