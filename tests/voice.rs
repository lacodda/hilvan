//! The voice end to end: the router, the cache and both engines, with the
//! engines stood in for by local servers that count what they are asked.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tower::ServiceExt;

use hilvan::voice::{ElevenLabs, Piper, Voices};

/// What a stand-in engine was asked.
#[derive(Default)]
struct Asked {
    sentences: AtomicUsize,
    last: Mutex<Option<Value>>,
}

/// A WAV of a quarter second of silence at 16 kHz.
fn wav() -> Vec<u8> {
    let samples: u32 = 4000;
    let data = samples * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    bytes.resize(bytes.len() + data as usize, 0);
    bytes
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// A Piper service with a Russian and an English voice.
async fn piper() -> (String, Arc<Asked>) {
    let asked = Arc::new(Asked::default());
    let counter = asked.clone();
    let app = Router::new()
        .route(
            "/voices",
            get(|| async {
                Json(json!({
                    "ru_RU-irina-medium": { "language": { "code": "ru_RU", "family": "ru" } },
                    "ru_RU-dmitri-medium": { "language": { "code": "ru_RU", "family": "ru" } },
                    "en_US-lessac-medium": { "language": { "code": "en_US", "family": "en" } },
                }))
            }),
        )
        .route(
            "/synthesize",
            post(move |Json(body): Json<Value>| {
                let asked = counter.clone();
                async move {
                    asked.sentences.fetch_add(1, Ordering::SeqCst);
                    *asked.last.lock().await = Some(body);
                    ([(header::CONTENT_TYPE, "audio/wav")], wav()).into_response()
                }
            }),
        );
    (serve(app).await, asked)
}

/// An `ElevenLabs` account with one voice and a third of its month spent.
async fn elevenlabs() -> (String, Arc<Asked>) {
    let asked = Arc::new(Asked::default());
    let counter = asked.clone();
    let app = Router::new()
        .route(
            "/v1/voices",
            get(|| async { Json(json!({ "voices": [{ "voice_id": "rachel-id", "name": "Rachel" }] })) }),
        )
        .route(
            "/v1/user/subscription",
            get(|| async {
                Json(json!({
                    "character_count": 10_000,
                    "character_limit": 30_000,
                    "next_character_count_reset_unix": 4_102_444_800_i64,
                }))
            }),
        )
        .route(
            "/v1/text-to-speech/{voice}",
            post(move |Json(body): Json<Value>| {
                let asked = counter.clone();
                async move {
                    asked.sentences.fetch_add(1, Ordering::SeqCst);
                    *asked.last.lock().await = Some(body);
                    (
                        [(header::CONTENT_TYPE, "audio/mpeg"), (header::HeaderName::from_static("character-cost"), "7")],
                        vec![0u8; 800],
                    )
                        .into_response()
                }
            }),
        );
    (serve(app).await, asked)
}

async fn taught() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let toml = r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be"
name = "n"
pattern = "<pronoun> + am/is/are + <rest>"
say = "<pronoun> <pronoun:be> <rest>."
explanation = "Связка обязательна."
order = 10

  [[formula.sample]]
  native = "Я дома."
  target = "I am at home."

  [[formula.slot]]
  name = "pronoun"
  values = [{ native = "я", target = "I", be = "am" }, { native = "он", target = "he", be = "is" }]

  [[formula.slot]]
  name = "rest"
  values = [{ native = "устал", target = "tired" }]
"#;
    hilvan::pack::load(&pool, &toml::from_str(toml).unwrap()).await.unwrap();
    pool
}

fn app(pool: SqlitePool, voices: Voices) -> Router {
    let web = tempfile::tempdir().unwrap();
    hilvan::app::router(pool, None, web.path(), voices)
}

fn speech(language: &str, text: &str, tempo: &str) -> Request<Body> {
    let text: String = text
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    Request::get(format!("/api/speech?language={language}&text={text}&tempo={tempo}"))
        .body(Body::empty())
        .unwrap()
}

async fn json_of(response: axum::response::Response) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn both() -> (Voices, Arc<Asked>, Arc<Asked>) {
    let (piper_url, piper_asked) = piper().await;
    let (eleven_url, eleven_asked) = elevenlabs().await;
    let voices = Voices::new(Some(Piper::new(&piper_url).unwrap()), Some(ElevenLabs::new("sk_test", &eleven_url).unwrap()));
    (voices, piper_asked, eleven_asked)
}

#[tokio::test]
async fn each_language_is_spoken_by_its_own_engine() {
    let (voices, piper, eleven) = both().await;
    let app = app(taught().await, voices);

    let english = app.clone().oneshot(speech("en", "He is tired.", "normal")).await.unwrap();
    assert_eq!(english.status(), StatusCode::OK);
    assert_eq!(english.headers()[header::CONTENT_TYPE], "audio/mpeg");
    assert_eq!(
        eleven.sentences.load(Ordering::SeqCst),
        1,
        "the language being learnt goes to the native speaker"
    );

    let russian = app.clone().oneshot(speech("ru", "Я дома.", "normal")).await.unwrap();
    assert_eq!(russian.status(), StatusCode::OK);
    assert_eq!(russian.headers()[header::CONTENT_TYPE], "audio/wav");
    assert_eq!(piper.sentences.load(Ordering::SeqCst), 1, "the learner's own language goes to Piper");
    assert_eq!(
        eleven.sentences.load(Ordering::SeqCst),
        1,
        "the paid budget was spent on the learner's own language"
    );

    let sent = eleven.last.lock().await.clone().unwrap();
    assert_eq!(sent["language_code"], "en");
    assert_eq!(sent["model_id"], "eleven_flash_v2_5");
}

#[tokio::test]
async fn a_sentence_is_made_once() {
    let (voices, _, eleven) = both().await;
    let app = app(taught().await, voices);

    let first = app.clone().oneshot(speech("en", "I am at home.", "normal")).await.unwrap();
    let etag = first.headers()[header::ETAG].clone();
    let again = app.clone().oneshot(speech("en", "I am at home.", "normal")).await.unwrap();
    assert_eq!(again.status(), StatusCode::OK);
    assert_eq!(eleven.sentences.load(Ordering::SeqCst), 1, "a cached sentence was paid for twice");

    // The browser's copy is confirmed with an empty answer.
    let mut revalidate = speech("en", "I am at home.", "normal");
    revalidate.headers_mut().insert(header::IF_NONE_MATCH, etag);
    let not_modified = app.clone().oneshot(revalidate).await.unwrap();
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn a_slow_sentence_is_a_different_sound() {
    let (voices, piper, _) = both().await;
    let app = app(taught().await, voices);

    app.clone().oneshot(speech("ru", "Я дома.", "normal")).await.unwrap();
    app.clone().oneshot(speech("ru", "Я дома.", "slow")).await.unwrap();
    assert_eq!(piper.sentences.load(Ordering::SeqCst), 2, "the slow sentence was served from the normal one");
    let sent = piper.last.lock().await.clone().unwrap();
    assert!(sent["length_scale"].as_f64().unwrap() > 1.0, "slow should lengthen the phonemes: {sent}");
}

#[tokio::test]
async fn text_outside_the_material_is_refused_before_any_engine_hears_it() {
    let (voices, piper, eleven) = both().await;
    let app = app(taught().await, voices);

    let response = app.oneshot(speech("en", "Read my whole novel aloud.", "normal")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(eleven.sentences.load(Ordering::SeqCst) + piper.sentences.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn without_a_key_piper_speaks_the_language_being_learnt() {
    let (piper_url, piper) = piper().await;
    let app = app(taught().await, Voices::new(Some(Piper::new(&piper_url).unwrap()), None));

    let response = app.clone().oneshot(speech("en", "He is tired.", "normal")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let sent = piper.last.lock().await.clone().unwrap();
    assert_eq!(sent["voice"], "en_US-lessac-medium", "English went to a voice that does not speak it");

    // And the voices screen says so rather than hiding it.
    let screen = json_of(app.oneshot(Request::get("/api/voices").body(Body::empty()).unwrap()).await.unwrap()).await;
    let english = screen["languages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|language| language["code"] == "en")
        .unwrap();
    assert_eq!(english["spoken"]["voice"]["engine"], "piper");
    assert_eq!(screen["elevenlabs"], "off");
}

#[tokio::test]
async fn the_paid_voice_is_not_offered_for_the_learners_own_language() {
    let (voices, _, _) = both().await;
    let app = app(taught().await, voices);

    let response = app
        .oneshot(
            Request::put("/api/voices/ru")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"engine":"elevenlabs","voice":"rachel-id"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "the budget would go on Russian");
}

#[tokio::test]
async fn a_chosen_voice_is_the_one_that_speaks() {
    let (voices, piper, _) = both().await;
    let app = app(taught().await, voices);

    let chose = app
        .clone()
        .oneshot(
            Request::put("/api/voices/ru")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"engine":"piper","voice":"ru_RU-dmitri-medium"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(chose.status(), StatusCode::OK);

    app.clone().oneshot(speech("ru", "Связка обязательна.", "normal")).await.unwrap();
    assert_eq!(piper.last.lock().await.clone().unwrap()["voice"], "ru_RU-dmitri-medium");

    let screen = json_of(app.oneshot(Request::get("/api/voices").body(Body::empty()).unwrap()).await.unwrap()).await;
    let russian = screen["languages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|language| language["code"] == "ru")
        .unwrap();
    assert_eq!(russian["spoken"]["chosen"], true);
    assert_eq!(russian["role"], "native");
}

#[tokio::test]
async fn a_voice_piper_does_not_have_is_refused_rather_than_swapped() {
    // Piper itself answers an unknown voice with its default one; hilvan
    // must not pass that on as the voice the learner chose.
    let (piper_url, piper_asked) = piper().await;
    let piper = Piper::new(&piper_url).unwrap();
    let error = hilvan::voice::Speaker::speak(&piper, "ru_RU-nobody-medium", "Я дома.", "ru", hilvan::voice::Tempo::Normal)
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("no voice"), "{error:#}");
    assert_eq!(piper_asked.sentences.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn health_reports_the_voice_and_the_budget() {
    let (voices, _, _) = both().await;
    let app = app(taught().await, voices);
    app.clone().oneshot(speech("en", "I am at home.", "normal")).await.unwrap();

    let health = json_of(app.oneshot(Request::get("/api/health").body(Body::empty()).unwrap()).await.unwrap()).await;
    assert_eq!(health["status"], "ok");
    assert_eq!(health["voice"]["piper"], "ok");
    assert_eq!(health["voice"]["elevenlabs"], "ok");
    assert_eq!(health["voice"]["budget"]["remaining"], 20_000);
    assert_eq!(health["voice"]["cache"]["sounds"], 1);
    assert_eq!(health["voice"]["cache"]["credits"], 7, "the cost the answer named should be what is counted");
}

#[tokio::test]
async fn a_voice_that_is_down_does_not_take_the_tutor_down() {
    // Nothing listens on this port: the address of a service that stopped.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let app = app(taught().await, Voices::new(Some(Piper::new(&dead).unwrap()), None));

    let health = app.clone().oneshot(Request::get("/api/health").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(health.status(), StatusCode::OK, "a sleeping voice failed the health check");
    let health = json_of(health).await;
    assert_eq!(health["voice"]["piper"], "unreachable");

    let response = app.oneshot(speech("ru", "Я дома.", "normal")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "no voice answers, so none speaks the language");
}
