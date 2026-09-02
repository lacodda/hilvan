//! The server as a client sees it.
//!
//! The unit tests in `src/app.rs` prove each handler answers correctly; this
//! proves the pieces are wired together the way a deployment relies on: a
//! database opened the real way and migrated from empty, and the router built
//! over it answering the one endpoint every monitor calls first.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn health_answers_ok_with_the_crate_version() {
    // A real file in a fresh directory, not an in-memory database: this is
    // the path a stand takes on its first start.
    let data = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}?mode=rwc", data.path().join("hilvan.db").display());
    let pool = hilvan::db::connect(&url).await.expect("the database should open and migrate from empty");

    let web = tempfile::tempdir().expect("a temporary directory");
    let app = hilvan::app::router(pool, web.path());

    let response = app.oneshot(Request::get("/api/health").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}
