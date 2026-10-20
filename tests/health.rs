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
    let app = hilvan::app::router(pool, hash, web.path(), hilvan::voice::Voices::none());
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
    let app = hilvan::app::router(pool, None, web.path(), hilvan::voice::Voices::none());
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

/// An answer as a device sends it: under an identity of its own, given now.
fn sent(mut answer: Value) -> Value {
    use std::sync::atomic::{AtomicU64, Ordering};
    static MINTED: AtomicU64 = AtomicU64::new(0);
    answer["answer_id"] = Value::from(format!("health-{}", MINTED.fetch_add(1, Ordering::Relaxed)));
    answer["answered_at"] = Value::from(chrono::Utc::now().to_rfc3339());
    answer
}

async fn post(app: &Router, uri: &str, body: &Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    json(app, request).await
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
        loaded.formulas * 2 + loaded.words,
        "every formula should arrive with a card in each direction, and every word with one"
    );

    // The container runs the load command on every start; the second run must
    // not touch a row.
    let again = hilvan::pack::load(&pool, &pack).await.unwrap();
    assert!(!again.changed, "loading the shipped pack twice rewrote it");

    let web = tempfile::tempdir().unwrap();
    let app = hilvan::app::router(pool, None, web.path(), hilvan::voice::Voices::none());

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

    // Said, the formula is typed: its first sentence, contracted the way the
    // pack accepts, is a right answer.
    let sample = &today["queue"][0]["formula"]["samples"][0];
    let sentence = sample["sentence"].as_i64().expect("a sample is a sentence");
    let contracted = sample["accept"][0].as_str().expect("the first sample accepts its contraction");
    let turn = serde_json::json!({ "asked": { "sample": sentence }, "typed": contracted });
    // The app checks the turn itself, with the core built to WebAssembly:
    // the formula exactly as the queue hands it over is what it reads.
    let held: hilvan_core::check::Formula =
        serde_json::from_value(today["queue"][0]["formula"].clone()).expect("the formula the queue sends is one the checker reads");
    let typed: hilvan_core::check::Turn = serde_json::from_value(turn.clone()).unwrap();
    let (_, checked) = hilvan_core::check::judge(&held.asking(), &typed).expect("the turn asks what the formula asks");
    assert!(checked.correct, "{contracted:?} was marked wrong: {checked:?}");

    let answer = sent(serde_json::json!({ "direction": "produce", "turns": [turn], "duration_ms": 3100 }));
    let (status, reviewed) = post(&app, &format!("/api/formulas/{first}/review"), &answer).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(reviewed["stitch"], "new");
    assert_eq!(reviewed["rating"], "good");

    // Producing it once opens the other direction - the same shape, asked
    // backwards - and the words of its sentences, which have now been met.
    // The day's one new formula has been used up.
    let (_, today) = json(&app, Request::get("/api/today").body(Body::empty()).unwrap()).await;
    let queue = today["queue"].as_array().expect("a queue");
    assert_eq!(queue[0]["kind"], "formula", "the formulas come first: {today}");
    assert_eq!(queue[0]["direction"], "recognise");
    assert_eq!(queue[0]["formula"]["id"], first.as_str());
    let words: Vec<&str> = queue[1..].iter().map(|due| due["word"]["id"].as_str().expect("then only words")).collect();
    let mut sorted = words.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        vec!["en:doctor", "en:friend", "en:home", "en:ready", "en:work"],
        "the words of the first formula's sentences should open, and no others"
    );
    assert_eq!(words[0], "en:work", "the commonest word should open first");
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
            .body(Body::from(
                sent(serde_json::json!({ "rating": "good", "direction": "recognise", "duration_ms": 1800 })).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(backwards["stitch"], "new");

    for word in &words {
        let (status, reviewed) = json(
            &app,
            Request::builder()
                .method("POST")
                .uri(format!("/api/words/{word}/review"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(sent(serde_json::json!({ "rating": "good", "duration_ms": 2400 })).to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{word}");
        assert_ne!(reviewed["stitch"], "new");
    }

    let (_, today) = json(&app, Request::get("/api/today").body(Body::empty()).unwrap()).await;
    assert!(today["queue"].as_array().unwrap().is_empty(), "the sitting did not end: {today}");
    assert_eq!(today["reviewed_today"], 2 + words.len());
    assert_eq!(
        today["words"]["levels"][0]["basted"].as_i64().unwrap() + today["words"]["levels"][0]["sewn"].as_i64().unwrap(),
        i64::try_from(words.len()).unwrap(),
        "the five words are all among the commonest thousand"
    );

    // The word is heard in the sentence it was met in.
    let (_, doctor) = json(&app, Request::get("/api/words/en:doctor").body(Body::empty()).unwrap()).await;
    assert_eq!(doctor["contexts"][0]["text"], "He is a doctor.");
    assert_eq!(doctor["contexts"][0]["anchor"], true);
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

#[tokio::test]
async fn the_inventory_asks_the_commonest_words_and_takes_answers_back() {
    let (app, _data, _web) = taught().await;
    let (status, run) = json(&app, Request::get("/api/inventory").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(run["language"], "en");
    let first = run["next"][0]["word"].as_str().unwrap().to_string();
    let second = run["next"][1]["word"].as_str().unwrap().to_string();
    assert_eq!(run["next"][0]["rank"], 1);

    let answers = serde_json::json!({ "answers": [{ "word": first, "known": true }, { "word": second, "known": false }] });
    let (status, run) = post(&app, "/api/inventory", &answers).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!((run["answered"].as_i64(), run["known"].as_i64()), (Some(2), Some(1)));
    assert_ne!(run["next"][0]["word"], first.as_str(), "an answered word should not be asked again");

    let (_, today) = json(&app, Request::get("/api/today").body(Body::empty()).unwrap()).await;
    assert_eq!(today["words"]["declared"], 1);
    assert_eq!(today["reviewed_today"], 0, "a declaration is not an answer");

    let request = Request::builder()
        .method("DELETE")
        .uri(format!("/api/inventory/{first}"))
        .body(Body::empty())
        .unwrap();
    let (status, run) = json(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(run["answered"], 1);
    assert_eq!(run["next"][0]["word"], first.as_str(), "a word taken back should be asked first again");

    let (status, refused) = post(
        &app,
        "/api/inventory",
        &serde_json::json!({ "answers": [{ "word": "en:qwzx", "known": true }] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused["error"].as_str().unwrap().contains("en:qwzx"));
}

#[tokio::test]
async fn the_learner_model_downloads_as_a_file_and_looks_back() {
    let (app, _data, _web) = taught().await;
    let answers = serde_json::json!({ "answers": [{ "word": "en:house", "known": true }] });
    let (status, _) = post(&app, "/api/inventory", &answers).await;
    assert_eq!(status, StatusCode::OK);

    let response = app.clone().oneshot(Request::get("/api/model").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().to_string();
    assert!(disposition.starts_with("attachment; filename=\"hilvan-model-"), "{disposition}");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let model = hilvan::model::Model::parse(std::str::from_utf8(&body).unwrap()).expect("the download should be a model hilvan reads back");
    assert_eq!(model.inventory.len(), 1);

    // Before the first answer, nothing was held; now one word is.
    let (status, standing) = json(&app, Request::get("/api/model/standing?at=2020-01-01T00:00:00Z").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{standing}");
    assert_eq!(standing["then"]["words"]["levels"][0]["sewn"], 0);
    assert_eq!(standing["now"]["words"]["levels"][0]["sewn"], 1);
    assert_eq!(standing["began"], Value::Null, "a declaration is not the first answer");

    let (status, _) = json(&app, Request::get("/api/model?at=2020-01-01T00:00:00Z").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
}
