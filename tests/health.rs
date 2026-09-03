//! The server as a client sees it.
//!
//! The unit tests in `src/app.rs` prove each handler answers correctly; this
//! proves the pieces are wired together the way a deployment relies on: a
//! database opened the real way and migrated from empty, the pack that ships
//! with the product loaded into it, and a learner working through a sitting
//! over HTTP.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

/// A stand on its first start: a real file in a fresh directory, migrated
/// from empty.
async fn stand(password: Option<&str>) -> (Router, tempfile::TempDir, tempfile::TempDir) {
    let data = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}?mode=rwc", data.path().join("hilvan.db").display());
    let pool = hilvan::db::connect(&url).await.expect("the database should open and migrate from empty");

    let web = tempfile::tempdir().expect("a temporary directory");
    let hash = password.map(|password| hilvan::auth::hash(password).expect("a password should hash"));
    let app = hilvan::app::router(pool, hash, web.path());
    (app, data, web)
}

/// A stand with the pack the product ships loaded into it.
async fn taught() -> (Router, tempfile::TempDir, tempfile::TempDir) {
    let data = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}?mode=rwc", data.path().join("hilvan.db").display());
    let pool = hilvan::db::connect(&url).await.expect("the database should open");

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/en-from-ru/pack.toml");
    let pack = hilvan::pack::Pack::read(&path).expect("the shipped pack should read");
    hilvan::pack::load(&pool, &pack).await.expect("the shipped pack should load");

    let web = tempfile::tempdir().expect("a temporary directory");
    let app = hilvan::app::router(pool, None, web.path());
    (app, data, web)
}

async fn json(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, body)
}

#[tokio::test]
async fn health_answers_ok_with_the_crate_version() {
    let (app, _data, _web) = stand(None).await;

    let (status, body) = json(&app, Request::get("/api/health").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn the_shipped_pack_loads_into_a_real_database_and_can_be_drilled() {
    // The whole of v0.1.0 end to end: the pack the product ships with, loaded
    // the way `hilvan load-pack` loads it, drilled the way the app drills it.
    let data = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}?mode=rwc", data.path().join("hilvan.db").display());
    let pool = hilvan::db::connect(&url).await.expect("the database should open");

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/en-from-ru/pack.toml");
    let pack = hilvan::pack::Pack::read(&path).expect("the shipped pack should read");
    let loaded = hilvan::pack::load(&pool, &pack).await.expect("the shipped pack should load");
    assert_eq!(
        loaded.new_cards,
        loaded.formulas * 2,
        "every formula should arrive with a card in each direction"
    );

    // The container runs the load command on every start; the second run must
    // not touch a row.
    let again = hilvan::pack::load(&pool, &pack).await.unwrap();
    assert!(!again.changed, "loading the shipped pack twice rewrote it");

    let web = tempfile::tempdir().unwrap();
    let app = hilvan::app::router(pool, None, web.path());

    let (status, today) = json(&app, Request::get("/api/today").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(today["queue"].as_array().map(Vec::len), Some(1), "a first sitting should open one formula");
    assert_eq!(today["counts"]["new"], i64::try_from(loaded.formulas).unwrap());

    let first = today["queue"][0]["formula"]["id"]
        .as_str()
        .expect("the queue should carry a formula")
        .to_string();
    assert!(
        !today["queue"][0]["formula"]["samples"].as_array().unwrap().is_empty(),
        "the drill was handed a formula with nothing to show"
    );

    let (status, reviewed) = json(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!("/api/formulas/{first}/review"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"rating":"good","duration_ms":3100}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(reviewed["stitch"], "new");

    // Producing it once opens the other direction: the same shape, asked
    // backwards. It is the only thing left in the queue - the day's one new
    // formula has been used up.
    let (_, today) = json(&app, Request::get("/api/today").body(Body::empty()).unwrap()).await;
    let queue = today["queue"].as_array().expect("a queue");
    assert_eq!(queue.len(), 1, "the reverse side of the formula just shown should be waiting: {today}");
    assert_eq!(queue[0]["direction"], "recognise");
    assert_eq!(queue[0]["formula"]["id"], first.as_str());
    assert_eq!(today["reviewed_today"], 1);

    // The two directions are counted apart, which is the whole point of
    // having two: recognition is untouched by having produced the shape.
    assert_eq!(today["progress"]["produce"]["new"], i64::try_from(loaded.formulas - 1).unwrap());
    assert_eq!(today["progress"]["recognise"]["new"], i64::try_from(loaded.formulas).unwrap());

    let (status, backwards) = json(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!("/api/formulas/{first}/review"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"rating":"good","direction":"recognise","duration_ms":1800}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(backwards["stitch"], "new");

    let (_, today) = json(&app, Request::get("/api/today").body(Body::empty()).unwrap()).await;
    assert!(today["queue"].as_array().unwrap().is_empty(), "the sitting did not end: {today}");
    assert_eq!(today["reviewed_today"], 2);
}

#[tokio::test]
async fn the_shipped_pack_carries_the_three_forms_of_a_shape() {
    // What the switch on the card is drawn from, checked against the material
    // the product actually ships rather than a fixture.
    let (app, _data, _web) = taught().await;

    let (status, formula) = json(&app, Request::get("/api/formulas/be-present-statement").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(formula["family"], "be-present");
    assert_eq!(formula["form"], "statement");

    let sisters = formula["sisters"].as_array().expect("the other forms of the shape");
    let forms: Vec<&str> = sisters.iter().filter_map(|sister| sister["form"].as_str()).collect();
    assert_eq!(forms, vec!["negation", "question"], "the switch should offer the other two forms");
    assert!(
        sisters.iter().all(|sister| sister["stitch"] == "new"),
        "a learner who has answered nothing should see every form as new"
    );
}

#[tokio::test]
async fn a_locked_stand_keeps_the_learner_model_to_itself() {
    let (app, _data, _web) = stand(Some("hunter2")).await;

    let (status, _) = json(&app, Request::get("/api/today").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Health stays open: the stand is watched by something that cannot hold
    // a session.
    let (status, _) = json(&app, Request::get("/api/health").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
}
