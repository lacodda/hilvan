//! An audio lesson end to end: made from the queue, said by stand-in
//! engines, spliced by the real ffmpeg, priced before a paid credit is
//! spent, and answered by ear through the router.
//!
//! ffmpeg is `HILVAN_FFMPEG` or the one on the path; CI installs it, and a
//! machine without it fails here rather than passing for nothing.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

use hilvan::lesson::{self, Lessons, State};
use hilvan::study::{self, Answer, Asked, Sent, Turn};
use hilvan::voice::{ElevenLabs, Piper, Voices};

fn ffmpeg() -> PathBuf {
    std::env::var_os("HILVAN_FFMPEG").map_or_else(|| PathBuf::from("ffmpeg"), PathBuf::from)
}

/// Half a second of a tone at 16 kHz, as Piper answers: something ffmpeg
/// can decode, and something a lesson can be measured by.
fn wav() -> Vec<u8> {
    let rate: u32 = 16_000;
    let samples: u32 = 8_000;
    let data = samples * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for n in 0..samples {
        #[allow(clippy::cast_possible_truncation)]
        let value = ((f64::from(n) * 0.17).sin() * 6_000.0) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// A Piper service with a Russian and an English voice, counting the lines
/// it is asked to say.
async fn piper() -> (String, Arc<AtomicUsize>) {
    let said = Arc::new(AtomicUsize::new(0));
    let counter = said.clone();
    let app = Router::new()
        .route(
            "/voices",
            get(|| async {
                Json(json!({
                    "ru_RU-irina-medium": { "language": { "code": "ru_RU", "family": "ru" } },
                    "en_US-lessac-medium": { "language": { "code": "en_US", "family": "en" } },
                }))
            }),
        )
        .route(
            "/info",
            get(|| async { Json(json!({ "voice": { "name": "ru_RU-irina-medium" }, "last": null })) }),
        )
        .route(
            "/synthesize",
            post(move || {
                let said = counter.clone();
                async move {
                    said.fetch_add(1, Ordering::SeqCst);
                    ([(header::CONTENT_TYPE, "audio/wav")], wav()).into_response()
                }
            }),
        );
    (serve(app).await, said)
}

/// An `ElevenLabs` account with `left` credits of its month, counting the
/// lines it is asked to say.
async fn elevenlabs(left: i64) -> (String, Arc<AtomicUsize>) {
    let said = Arc::new(AtomicUsize::new(0));
    let counter = said.clone();
    let app = Router::new()
        .route(
            "/v1/voices",
            get(|| async { Json(json!({ "voices": [{ "voice_id": "rachel-id", "name": "Rachel" }] })) }),
        )
        .route(
            "/v1/user/subscription",
            get(move || async move {
                Json(json!({
                    "character_count": 30_000 - left,
                    "character_limit": 30_000,
                    "next_character_count_reset_unix": 4_102_444_800_i64,
                }))
            }),
        )
        .route(
            "/v1/text-to-speech/{voice}",
            post(move || {
                let said = counter.clone();
                async move {
                    said.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::PAYMENT_REQUIRED, "nothing should have been said").into_response()
                }
            }),
        );
    (serve(app).await, said)
}

/// A learner a month into a pack of two formulas and a word: the first
/// formula answered long ago and due again, the second never started.
async fn learner() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let toml = r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be"
name = "To be"
pattern = "<pronoun> + am/is/are + <rest>"
explanation = "Связка обязательна."
order = 10

  [[formula.sample]]
  native = "Я дома."
  target = "I am at home."
  words = ["home"]

  [[formula.sample]]
  native = "Он устал."
  target = "He is tired."

[[formula]]
id = "lets"
name = "Let's"
pattern = "Let's + verb"
explanation = "Let's - это предложение сделать что-то вместе."
order = 20

  [[formula.sample]]
  native = "Пойдём."
  target = "Let's go."

[[word]]
lemma = "home"
gloss = "дом"
"#;
    hilvan::pack::load(&pool, &toml::from_str(toml).unwrap()).await.unwrap();
    let formula = study::formula(&pool, "be").await.unwrap();
    let sent = Sent {
        answer_id: "long-ago".to_string(),
        answered_at: at("2026-01-01T09:00:00Z"),
        answer: Answer::Produce {
            turns: vec![Turn {
                asked: Asked::Sample(formula.samples[0].sentence),
                typed: "I am at home.".to_string(),
            }],
            easy: false,
            duration_ms: None,
        },
    };
    study::review(&pool, "be", sent, at("2026-01-01T09:00:00Z")).await.unwrap();
    pool
}

fn at(moment: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(moment).unwrap().with_timezone(&Utc)
}

/// A month on: the first formula is long due.
fn now() -> DateTime<Utc> {
    at("2026-02-01T07:00:00Z")
}

#[tokio::test]
async fn a_lesson_is_made_of_the_queue_and_kept_with_its_chapters() {
    let pool = learner().await;
    let (url, said) = piper().await;
    let voices = Voices::new(Some(Piper::new(&url).unwrap()), None);
    let lessons = Lessons::new(ffmpeg());

    let id = lessons.make_now(&pool, &voices, 15, now()).await.unwrap();
    let made = lesson::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(made.state, State::Ready, "{:?}", made.error);
    assert_eq!(made.done, made.total, "every line said");

    // What is due first: the formula answered a month ago, asked; then the
    // new one, heard first; the word of a sentence met comes too.
    let targets: Vec<&str> = made.phrases.iter().map(|phrase| phrase.target.as_str()).collect();
    assert_eq!(
        made.phrases[0].card,
        study::CardRef::Formula {
            id: "be".to_string(),
            direction: hilvan::scheduling::Direction::Produce
        }
    );
    assert!(targets.contains(&"Let's go."), "the new formula is in it: {targets:?}");
    let new = made.phrases.iter().position(|phrase| phrase.target == "Let's go.").unwrap();
    assert!(made.phrases[new].fresh);
    let first = made.chapters.iter().find(|chapter| chapter.phrase == new).unwrap();
    assert_eq!(first.turn, hilvan_core::lesson::Turn::Heard, "a new phrase is heard before it is asked");

    let length = made.length_ms.unwrap();
    assert!(length > 0 && length <= 15 * 60_000);
    assert_eq!(made.chapters.last().unwrap().end_ms, length);

    let audio = lesson::audio(&pool, id).await.unwrap().unwrap();
    assert_eq!(&audio[..3], b"ID3");
    let chaps = audio.windows(4).filter(|window| *window == b"CHAP").count();
    assert_eq!(chaps, made.chapters.len(), "a chapter of the file for every stretch");

    // A second lesson says nothing new: every line is in the cache.
    let before = said.load(Ordering::SeqCst);
    assert!(before > 0);
    lessons.make_now(&pool, &voices, 15, now()).await.unwrap();
    assert_eq!(said.load(Ordering::SeqCst), before, "a line was said twice");
}

#[tokio::test]
async fn a_lesson_the_month_cannot_pay_for_is_not_started() {
    let pool = learner().await;
    let (piper_url, _) = piper().await;
    let (url, said) = elevenlabs(3).await;
    let voices = Voices::new(Some(Piper::new(&piper_url).unwrap()), Some(ElevenLabs::new("sk_test", &url).unwrap()));

    let error = Lessons::new(ffmpeg()).make_now(&pool, &voices, 15, now()).await.unwrap_err();
    let said_why = format!("{error:#}");
    assert!(said_why.contains("credits") && said_why.contains("3 are left"), "{said_why}");
    assert_eq!(said.load(Ordering::SeqCst), 0, "a paid line was said before the lesson was priced");
    let failed = lesson::list(&pool).await.unwrap();
    assert_eq!(failed[0].state, State::Failed);
    assert!(failed[0].error.as_deref().is_some_and(|error| error.contains("credits")));
}

#[tokio::test]
async fn nothing_due_and_nothing_coming_is_said_so() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let (url, _) = piper().await;
    let voices = Voices::new(Some(Piper::new(&url).unwrap()), None);
    let error = Lessons::new(ffmpeg()).make_now(&pool, &voices, 15, now()).await.unwrap_err();
    assert!(error.to_string().contains("Nothing is waiting"), "{error:#}");
}

async fn body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

fn post_json(path: &str, value: &Value) -> Request<Body> {
    Request::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}

#[tokio::test]
async fn the_app_asks_for_a_lesson_follows_it_and_answers_it_by_ear() {
    let pool = learner().await;
    let (url, _) = piper().await;
    let voices = Voices::new(Some(Piper::new(&url).unwrap()), None);
    let web = tempfile::tempdir().unwrap();
    let mut state = hilvan::app::AppState::new(pool.clone(), None, voices);
    state.lessons = Lessons::new(ffmpeg());
    let app = hilvan::app::routes(state, web.path());

    let refused = app.clone().oneshot(post_json("/api/lessons", &json!({ "minutes": 30 }))).await.unwrap();
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY, "30 minutes is not a length offered");

    let asked = app.clone().oneshot(post_json("/api/lessons", &json!({ "minutes": 40 }))).await.unwrap();
    assert_eq!(asked.status(), StatusCode::ACCEPTED);
    let id = body(asked).await["id"].as_i64().unwrap();

    // Followed until it is made.
    let mut made = Value::Null;
    for _ in 0..300 {
        let response = app
            .clone()
            .oneshot(Request::get(format!("/api/lessons/{id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        made = body(response).await;
        if made["state"] != "building" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(made["state"], "ready", "{made}");

    let screen = body(app.clone().oneshot(Request::get("/api/lessons").body(Body::empty()).unwrap()).await.unwrap()).await;
    assert_eq!(screen["minutes"], 40, "the length asked for is the one kept");
    assert_eq!(screen["lessons"][0]["id"], id);

    let audio = app
        .clone()
        .oneshot(Request::get(format!("/api/lessons/{id}/audio")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(audio.status(), StatusCode::OK);
    assert_eq!(audio.headers()[header::CONTENT_TYPE], "audio/mpeg");
    assert!(audio.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));

    // The new formula, heard and held.
    let heard = json!({
        "answer_id": "heard-1",
        "answered_at": "2026-02-01T07:30:00Z",
        "card": { "kind": "formula", "id": "lets", "direction": "produce" },
        "rating": "good",
    });
    let first = app.clone().oneshot(post_json("/api/heard", &heard)).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let reply = body(first).await;
    assert_eq!(reply["rating"], "good");
    let again = body(app.clone().oneshot(post_json("/api/heard", &heard)).await.unwrap()).await;
    assert_eq!(again, reply, "a repeated delivery is told the same");
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT r.heard, r.reviewed_at FROM review r JOIN card c ON c.id = r.card_id WHERE c.kind = 'formula' AND c.subject_id = 'lets'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "taken once");
    assert_eq!(rows[0].0, 1, "marked heard");
    assert!(rows[0].1.starts_with("2026-02-01T07:30:00"), "taken at the moment it was heard");

    let nobody = json!({
        "answer_id": "heard-2",
        "answered_at": "2026-02-01T07:30:00Z",
        "card": { "kind": "word", "id": "en:nobody-teaches-this" },
        "rating": "again",
    });
    let missing = app.oneshot(post_json("/api/heard", &nobody)).await.unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}
