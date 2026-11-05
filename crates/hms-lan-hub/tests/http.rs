use axum::body::Body;
use axum::http::{Request, StatusCode};
use hms_lan_hub::{router, LanHubState};
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn state() -> LanHubState {
    let path = std::env::temp_dir().join(format!(
        "hms-lan-hub-http-{}-{}.sqlite",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    LanHubState::new_with_token(4442, path, None, Some("test-token".into()))
}

#[tokio::test]
async fn lan_routes_require_bearer_token() {
    let app = router(state());

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/lan/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let wrong = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/lan/status")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let valid = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/lan/status")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(valid.status(), StatusCode::OK);
}
