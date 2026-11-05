//! LAN Hub daemon binary.
//! Usage:
//!   hms-lan-hub --hotel 4442 --port 8089 --db lan_hub.sqlite

use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let hotel_id: i64 = std::env::var("HOTEL_ID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4442);
    let port: u16 = std::env::var("LAN_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8089);
    let db_path = std::env::var("LAN_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("lan_hub.sqlite"));
    let upstream_url = std::env::var("HMS_API_BASE").ok();
    let hub_token = std::env::var("LAN_HUB_TOKEN").ok();
    if hub_token.is_none() {
        panic!("LAN_HUB_TOKEN must be configured; refusing to expose an unauthenticated LAN hub");
    }

    tracing::info!("starting HMS LAN Hub for hotel {hotel_id} on port {port}");

    let state =
        hms_lan_hub::LanHubState::new_with_token(hotel_id, db_path, upstream_url, hub_token);
    let app = hms_lan_hub::router(state).layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind LAN Hub listener");
    tracing::info!("LAN Hub listening on http://{addr}");
    axum::serve(listener, app)
        .await
        .expect("LAN Hub server crashed");
}
