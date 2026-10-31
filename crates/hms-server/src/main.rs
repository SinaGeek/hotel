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
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
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
        .max_connections(
            std::env::var("DB_POOL_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
        )
        // Pre-warm so the first requests after start don't pay per-connection
        // spawn cost (~100-300 ms per fresh backend on Windows) — otherwise the
        // load-test p95 shows a cold-start tail (README §13).
        .min_connections(
            std::env::var("DB_POOL_MIN")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        )
        .connect(&database_url)
        .await
        .expect("failed to connect to PostgreSQL");

    // Migrations run on server start (README §17)
    sqlx::migrate!("../../migrations")
        .run(&db)
        .await
        .expect("migrations failed");
    tracing::info!("migrations applied");
    bootstrap::ensure_platform_admin(&db)
        .await
        .expect("bootstrap failed");
    tracing::info!("bootstrap ensured");

    let session_secret = session_secret_for(
        std::env::var("SESSION_SECRET").ok().as_deref(),
        std::env::var("APP_ENV")
            .map(|v| v.eq_ignore_ascii_case("production"))
            .unwrap_or(false),
    )
    .expect("invalid SESSION_SECRET configuration");
    let state = AppState {
        db,
        session_secret,
        rate_limiter: Arc::new(tokio::sync::Mutex::new(rate_limiter::RateLimiter::new(
            std::env::var("RATE_LIMIT_LOGIN").ok().and_then(|v| v.parse().ok()).unwrap_or(10),
            60_000,
        ))),
    };

    let cors_origins = cors_allowed_origins();
    if cors_origins.is_empty() {
        tracing::warn!("CORS_ALLOWED_ORIGINS is empty; cross-origin browser requests are disabled");
    }
    let cors = CorsLayer::new()
        .allow_origin(cors_origins)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ]);

    let app = Router::new()
        .route("/health", get(health_check))
        .nest("/api/v1/auth", auth::router())
        .nest(
            "/api/v1",
            api::router().route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_waterfall,
            )),
        )
        .layer(cors)
        .layer(security_headers())
        // README §13: compress JSON responses (gzip/br) — big win for board/report payloads
        .layer(CompressionLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("HMS Server listening on 0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

fn session_secret_for(configured: Option<&str>, production: bool) -> Result<String, String> {
    match configured.filter(|v| !v.trim().is_empty()) {
        Some(secret) if secret.as_bytes().len() >= 32 => Ok(secret.to_owned()),
        Some(_) => Err("SESSION_SECRET must be at least 32 bytes".to_owned()),
        None if production => Err("SESSION_SECRET must be set in production".to_owned()),
        None => {
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut bytes);
            let generated = bytes.iter().map(|b| format!("{b:02x}")).collect();
            tracing::warn!("SESSION_SECRET not set; using an ephemeral development value; sessions invalidate on restart");
            Ok(generated)
        }
    }
}

fn security_headers() -> tower::ServiceBuilder<
    tower::layer::util::Stack<
        SetResponseHeaderLayer<axum::http::HeaderValue>,
        tower::layer::util::Stack<
            SetResponseHeaderLayer<axum::http::HeaderValue>,
            tower::layer::util::Stack<
                SetResponseHeaderLayer<axum::http::HeaderValue>,
                tower::layer::util::Stack<
                    SetResponseHeaderLayer<axum::http::HeaderValue>,
                    tower::layer::util::Stack<
                        SetResponseHeaderLayer<axum::http::HeaderValue>,
                        tower::layer::util::Identity,
                    >,
                >,
            >,
        >,
    >,
> {
    tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            axum::http::HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_FRAME_OPTIONS,
            axum::http::HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::REFERRER_POLICY,
            axum::http::HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::HeaderName::from_static("permissions-policy"),
            axum::http::HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CONTENT_SECURITY_POLICY,
            axum::http::HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))
}

/// Read exact allowed origins from `CORS_ALLOWED_ORIGINS` (comma-separated).
/// Development defaults are localhost Vite origins; production has no implicit
/// cross-origin allowlist. Same-origin requests do not depend on CORS.
fn cors_allowed_origins() -> Vec<axum::http::HeaderValue> {
    let configured = std::env::var("CORS_ALLOWED_ORIGINS").ok();
    let production = std::env::var("APP_ENV")
        .map(|v| v.eq_ignore_ascii_case("production"))
        .unwrap_or(false);
    cors_origins_for(configured.as_deref(), production)
}

fn cors_origins_for(configured: Option<&str>, production: bool) -> Vec<axum::http::HeaderValue> {
    let values = configured
        .filter(|v| !v.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if production {
                String::new()
            } else {
                "http://localhost:5173,http://localhost:4173".to_owned()
            }
        });
    parse_cors_origins(&values)
}

fn parse_cors_origins(values: &str) -> Vec<axum::http::HeaderValue> {
    values
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .filter_map(|origin| {
            let parsed = url::Url::parse(origin).ok();
            let Some(url) = parsed else {
                tracing::error!(origin = %origin, "ignoring invalid CORS origin");
                return None;
            };
            let valid = matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && origin != "*";
            if !valid {
                tracing::error!(origin = %origin, "ignoring invalid CORS origin; configure origins only, without paths or wildcards");
                return None;
            }
            let serialized = url.origin().ascii_serialization();
            match axum::http::HeaderValue::from_str(&serialized) {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::error!(origin = %origin, %error, "ignoring invalid CORS header value");
                    None
                }
            }
        })
        .collect()
}

/// Simple barrier middleware for /api/v1: requires a valid session.
/// Handlers additionally extract `AuthUser` for scoping and permissions.
async fn auth_waterfall(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let (mut parts, body) = req.into_parts();
    let _auth =
        <crate::auth::AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
            &mut parts, &state,
        )
        .await?;
    req = axum::extract::Request::from_parts(parts, body);
    Ok(next.run(req).await)
}

async fn health_check(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<ApiResponse<String>> {
    match sqlx::query_scalar::<_, i64>("SELECT 1::int8")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => Json(ApiResponse {
            success: true,
            data: "HMS Server is running (db ok)".to_string(),
            error: None,
        }),
        Err(error) => {
            tracing::error!(%error, "health check database probe failed");
            Json(ApiResponse {
                success: false,
                data: String::new(),
                error: Some("database unavailable".to_owned()),
            })
        }
    }
}

#[cfg(test)]
mod cors_tests {
    use super::{parse_cors_origins, security_headers, session_secret_for};

    async fn security_header_probe() -> axum::http::StatusCode {
        axum::http::StatusCode::OK
    }

    #[tokio::test]
    async fn security_headers_are_attached_to_responses() {
        use axum::routing::get;
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route("/probe", get(security_header_probe))
            .layer(security_headers());
        let response = app
            .oneshot(axum::http::Request::builder().uri("/probe").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["x-frame-options"], "DENY");
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        assert_eq!(response.headers()["permissions-policy"], "camera=(), microphone=(), geolocation=()");
        assert_eq!(response.headers()["content-security-policy"], "default-src 'none'; frame-ancestors 'none'");
    }

    #[test]
    fn session_secret_requires_stable_strong_value_in_production() {
        assert!(session_secret_for(None, true).is_err());
        assert!(session_secret_for(Some("short"), false).is_err());
        assert_eq!(session_secret_for(Some(&"x".repeat(32)), true).unwrap().len(), 32);
        assert_eq!(session_secret_for(Some(""), false).unwrap().len(), 64);
    }

    #[test]
    fn accepts_only_exact_http_origins() {
        let origins = parse_cors_origins(" https://hms.example.com ,http://localhost:4173 ");
        assert_eq!(origins.len(), 2);
        assert_eq!(origins[0], "https://hms.example.com");
        assert_eq!(origins[1], "http://localhost:4173");
        assert_eq!(
            parse_cors_origins("https://hms.example.com/")[0],
            "https://hms.example.com"
        );
    }

    #[test]
    fn rejects_wildcard_paths_credentials_and_non_http_schemes() {
        let origins = parse_cors_origins(
            "*,https://hms.example.com/app,https://user:pass@hms.example.com,file://localhost,https://hms.example.com?x=1",
        );
        assert!(origins.is_empty());
    }

    #[test]
    fn blank_config_uses_localhost_only_outside_production() {
        assert_eq!(super::cors_origins_for(Some(""), false).len(), 2);
        assert!(super::cors_origins_for(Some(""), true).is_empty());
        let explicit = super::cors_origins_for(Some("https://hms.example.com"), true);
        assert_eq!(explicit.len(), 1);
        assert_eq!(explicit[0], "https://hms.example.com");
    }
}
