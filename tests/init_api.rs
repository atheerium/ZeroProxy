use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tempfile::tempdir;
use tower::util::ServiceExt;
use zeroproxy::db::Db;
use zeroproxy::server::state::AppState;

async fn app_state() -> AppState {
    let temp = tempdir().expect("tempdir");
    let db = Arc::new(Db::load_from(temp.path()).await.expect("db"));
    AppState::new(db)
}

#[tokio::test]
async fn init_route_matches_cipherroute_plain_text_response() {
    let app = zeroproxy::build_app(app_state().await);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/init")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), "Initialized");
}
