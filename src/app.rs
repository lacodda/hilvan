//! The router: what the browser asks the server for.

use std::path::{Path, PathBuf};

use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

/// The router: the API under `/api`, the tutor app everywhere else.
///
/// The SPA is served from `web_dir` as real files when they exist and as
/// `index.html` otherwise, so a deep link into a lesson loads the app rather
/// than a 404. The API never falls through to it: a misspelled endpoint has to
/// look like a mistake, not like a page.
pub fn router(pool: SqlitePool, web_dir: &Path) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .fallback(api_not_found)
        .with_state(AppState { pool });

    // `ServeDir`'s own `not_found_service` is deliberately not used: it serves
    // the fallback body but keeps the 404 of the request that missed, which a
    // browser renders fine and every crawler and uptime monitor reads as
    // "broken". Routing the miss through the router's fallback gives the
    // handler's own status.
    let index = web_dir.join("index.html");
    let files = ServeDir::new(web_dir);
    let spa = get(move || serve_index(index.clone()));

    Router::new()
        .nest("/api", api)
        .fallback_service(files.fallback(spa))
        .layer(TraceLayer::new_for_http())
}

/// The SPA's entry point, answered with 200 for any client-side route.
///
/// A missing entry point is not an error in the process but in what was
/// deployed next to it: the tutor app was not built, or `HILVAN_WEB_DIR`
/// points elsewhere. Saying so beats a blank page.
async fn serve_index(path: PathBuf) -> Response {
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            "the tutor app is not built; run `pnpm build` in web/ or set HILVAN_WEB_DIR",
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "the SPA entry point could not be read");
            (StatusCode::INTERNAL_SERVER_ERROR, "the application could not be loaded").into_response()
        }
    }
}

/// What `GET /api/health` answers.
#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

/// Liveness and readiness in one place: the process answers, and the database
/// round-trip tells whether the server can actually do its job.
async fn health(State(state): State<AppState>) -> Response {
    let version = env!("CARGO_PKG_VERSION");
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (StatusCode::OK, Json(Health { status: "ok", version })).into_response(),
        Err(error) => {
            tracing::error!(%error, "health check: database unreachable");
            (StatusCode::SERVICE_UNAVAILABLE, Json(Health { status: "degraded", version })).into_response()
        }
    }
}

/// Anything under `/api` that does not exist is a client's mistake, answered
/// in JSON like every other API failure.
async fn api_not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "no such endpoint" }))).into_response()
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    /// An in-memory database with the schema applied: the real thing, minus
    /// the file.
    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        pool
    }

    /// A directory laid out the way a built SPA is.
    fn web_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(dir.path().join("index.html"), "<!doctype html><title>hilvan</title>").unwrap();
        std::fs::create_dir_all(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets/app.js"), "console.log('hilvan')").unwrap();
        dir
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let body = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn health_reports_ok_with_the_version() {
        let dir = web_root();
        let response = router(pool().await, dir.path())
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = body_json(response).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn an_existing_file_is_served_as_it_is() {
        let dir = web_root();
        let response = router(pool().await, dir.path())
            .oneshot(Request::get("/assets/app.js").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"console.log('hilvan')", "the file came back altered");
    }

    #[tokio::test]
    async fn an_unknown_path_falls_back_to_the_spa() {
        // A deep link into a lesson is a client route, not a file: it has to
        // load the app, and with a 200 rather than the 404 the miss produced.
        let dir = web_root();
        let response = router(pool().await, dir.path())
            .oneshot(Request::get("/lesson/some-formula").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.starts_with(b"<!doctype"), "a client route did not get the SPA");
    }

    #[tokio::test]
    async fn an_unknown_api_path_never_gets_the_spa() {
        // A misspelled endpoint has to look like a mistake. Serving the app
        // here would tell a client that a wrong URL worked.
        let dir = web_root();
        let response = router(pool().await, dir.path())
            .oneshot(Request::get("/api/no-such-endpoint").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_json(response).await["error"], "no such endpoint");
    }

    #[tokio::test]
    async fn an_unbuilt_spa_says_so() {
        // The directory exists but holds no build: the answer names the fix
        // instead of a blank page or a stack trace.
        let dir = tempfile::tempdir().unwrap();
        let response = router(pool().await, dir.path())
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            String::from_utf8_lossy(&body).contains("pnpm build"),
            "the message should say how to build the app"
        );
    }
}
