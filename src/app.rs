//! The router: what the browser asks the server for.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path as UrlPath, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::study::{self, Answer};
use crate::voice::{self, Engine, SpeakError, Tempo, Voices};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// Argon2 hash of the learner's password; `None` leaves the stand open.
    pub password_hash: Option<Arc<str>>,
    /// The engines that speak, and the cache behind them.
    pub voices: Voices,
}

/// The router: the API under `/api`, the tutor app everywhere else.
///
/// The SPA is served from `web_dir` as real files when they exist and as
/// `index.html` otherwise, so a deep link into a lesson loads the app rather
/// than a 404. The API never falls through to it: a misspelled endpoint has to
/// look like a mistake, not like a page.
pub fn router(pool: SqlitePool, password_hash: Option<String>, web_dir: &Path, voices: Voices) -> Router {
    let state = AppState {
        pool,
        password_hash: password_hash.filter(|hash| !hash.trim().is_empty()).map(Into::into),
        voices,
    };

    // Everything the learner's own data flows through sits behind the door;
    // health and the session endpoints are what a locked-out client is still
    // allowed to ask.
    let study = Router::new()
        .route("/today", get(today))
        .route("/formulas/{id}", get(formula))
        .route("/formulas/{id}/review", post(review))
        .route("/speech", get(speech))
        .route("/listen", get(listen))
        .route("/voices", get(voices_screen))
        .route("/voices/{language}", put(choose_voice))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_session));

    let api = Router::new()
        .route("/health", get(health))
        .route("/session", get(session).post(log_in).delete(log_out))
        .merge(study)
        .fallback(api_not_found)
        .with_state(state);

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
    voice: VoiceHealth,
}

/// The voice, as health reports it: whether each engine answers, what is
/// left of the paid budget, and what the cache holds.
///
/// Reported beside the status rather than folded into it: a tutor whose
/// voice is down still drills, and a container restarted for a sleeping
/// speech service would take the drill down with it.
#[derive(Serialize)]
struct VoiceHealth {
    piper: voice::State,
    elevenlabs: voice::State,
    budget: Option<voice::budget::Budget>,
    cache: Option<voice::cache::Stats>,
}

/// Liveness and readiness in one place: the process answers, and the database
/// round-trip tells whether the server can actually do its job.
async fn health(State(state): State<AppState>) -> Response {
    let version = env!("CARGO_PKG_VERSION");
    let (piper, elevenlabs) = state.voices.states().await;
    let voice = VoiceHealth {
        piper,
        elevenlabs,
        budget: state.voices.budget(&state.pool).await,
        cache: voice::cache::stats(&state.pool).await.ok(),
    };
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (StatusCode::OK, Json(Health { status: "ok", version, voice })).into_response(),
        Err(error) => {
            tracing::error!(%error, "health check: database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    version,
                    voice,
                }),
            )
                .into_response()
        }
    }
}

/// Anything under `/api` that does not exist is a client's mistake, answered
/// in JSON like every other API failure.
async fn api_not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "no such endpoint" }))).into_response()
}

/// Turns away a request without a live session.
///
/// A 401 rather than a redirect: the client is a single-page app, and it
/// decides for itself whether to show the login screen or keep what it has
/// cached on screen.
async fn require_session(State(state): State<AppState>, jar: CookieJar, request: Request, next: Next) -> Response {
    if signed_in(&state, &jar).await {
        return next.run(request).await;
    }
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": "sign in first" }))).into_response()
}

/// Whether this request is allowed in.
///
/// A stand with no password has no gate: everyone who can reach it is the
/// learner.
async fn signed_in(state: &AppState, jar: &CookieJar) -> bool {
    if state.password_hash.is_none() {
        return true;
    }
    let Some(token) = jar.get(auth::COOKIE).map(Cookie::value) else {
        return false;
    };
    match auth::is_live(&state.pool, token).await {
        Ok(live) => live,
        Err(error) => {
            tracing::error!(error = format!("{error:#}"), "the session could not be checked");
            false
        }
    }
}

/// What `GET /api/session` answers: whether the door is locked, and whether
/// this client is through it.
#[derive(Serialize)]
struct Session {
    required: bool,
    signed_in: bool,
}

async fn session(State(state): State<AppState>, jar: CookieJar) -> Response {
    let signed_in = signed_in(&state, &jar).await;
    Json(Session {
        required: state.password_hash.is_some(),
        signed_in,
    })
    .into_response()
}

#[derive(Deserialize)]
struct Credentials {
    password: String,
}

async fn log_in(State(state): State<AppState>, jar: CookieJar, Json(credentials): Json<Credentials>) -> Response {
    let Some(hash) = state.password_hash.as_deref() else {
        // Nothing to sign into, and nothing to hand out either.
        return Json(Session {
            required: false,
            signed_in: true,
        })
        .into_response();
    };

    match auth::verify(&credentials.password, hash) {
        // No detail about why: a wrong password and an unknown one are the
        // same answer.
        Ok(false) => {
            return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "that is not the password" }))).into_response();
        }
        // A malformed hash is the deployment's mistake, not the learner's,
        // and telling them "wrong password" would send them hunting for a
        // password that was never going to work.
        Err(error) => {
            tracing::error!(error = format!("{error:#}"), "the configured password hash is unusable");
            return failed("the stand's password is misconfigured", &error);
        }
        Ok(true) => {}
    }

    let token = auth::new_token();
    if let Err(error) = auth::start(&state.pool, &token).await {
        return failed("the session could not be stored", &error);
    }

    let cookie = Cookie::build((auth::COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        // Not `secure`: the stand is reached over plain HTTP on a home
        // network, and a cookie the browser refuses to send is a login screen
        // that never goes away.
        .max_age(time::Duration::days(auth::SESSION_DAYS))
        .build();

    (
        jar.add(cookie),
        Json(Session {
            required: true,
            signed_in: true,
        }),
    )
        .into_response()
}

async fn log_out(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Some(token) = jar.get(auth::COOKIE).map(Cookie::value)
        && let Err(error) = auth::end(&state.pool, token).await
    {
        // The cookie is cleared regardless: a session row that outlives the
        // browser is a stale row, not an open door on this device.
        tracing::error!(error = format!("{error:#}"), "the session could not be ended");
    }
    let jar = jar.remove(Cookie::from(auth::COOKIE));
    (
        jar,
        Json(Session {
            required: state.password_hash.is_some(),
            signed_in: false,
        }),
    )
        .into_response()
}

async fn today(State(state): State<AppState>) -> Response {
    match study::today(&state.pool, Utc::now()).await {
        Ok(today) => Json(today).into_response(),
        Err(error) => failed("today's queue could not be read", &error),
    }
}

async fn formula(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Response {
    match study::formula(&state.pool, &id).await {
        Ok(formula) => Json(formula).into_response(),
        // A formula that is not there is a client asking for the wrong id,
        // not a server that broke.
        Err(error) => (StatusCode::NOT_FOUND, Json(json!({ "error": error.to_string() }))).into_response(),
    }
}

async fn review(State(state): State<AppState>, UrlPath(id): UrlPath<String>, Json(answer): Json<Answer>) -> Response {
    match study::review(&state.pool, &id, answer, Utc::now()).await {
        Ok(reviewed) => Json(reviewed).into_response(),
        Err(error) => (StatusCode::NOT_FOUND, Json(json!({ "error": error.to_string() }))).into_response(),
    }
}

/// What `GET /api/speech` is asked.
#[derive(Deserialize)]
struct Say {
    /// ISO 639-1 code of the language the text is in.
    language: String,
    text: String,
    #[serde(default)]
    tempo: Tempo,
}

/// One sentence of the material, spoken.
///
/// A GET, so the drill can hand the URL straight to an `<audio>` element and
/// the cookie goes with it. The `ETag` is the cache key: the browser keeps
/// the sound and revalidates, and a voice changed on the voices screen is a
/// new key, so it is never served the old one.
async fn speech(State(state): State<AppState>, headers: HeaderMap, Query(say): Query<Say>) -> Response {
    match state.voices.say(&state.pool, &say.language, &say.text, say.tempo).await {
        Ok(sound) => {
            let etag = format!("\"{}\"", sound.key);
            if headers.get(header::IF_NONE_MATCH).and_then(|value| value.to_str().ok()) == Some(etag.as_str()) {
                return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag), (header::CACHE_CONTROL, NO_CACHE.to_string())]).into_response();
            }
            (
                [
                    (header::CONTENT_TYPE, sound.mime),
                    (header::ETAG, etag),
                    (header::CACHE_CONTROL, NO_CACHE.to_string()),
                ],
                sound.bytes,
            )
                .into_response()
        }
        Err(error) => speak_failed(&error),
    }
}

/// Kept by the browser, asked about every time: a changed voice is a changed
/// `ETag`, and the answer to an unchanged one is an empty 304.
const NO_CACHE: &str = "private, no-cache";

fn speak_failed(error: &SpeakError) -> Response {
    let status = match error {
        SpeakError::NotInMaterial(_) => StatusCode::UNPROCESSABLE_ENTITY,
        SpeakError::NoVoice(_) => StatusCode::NOT_FOUND,
        SpeakError::Engine(_) => {
            tracing::warn!(error = %error, "a voice engine failed");
            StatusCode::BAD_GATEWAY
        }
        SpeakError::Internal(error) => return failed("the sentence could not be spoken", error),
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

async fn listen(State(state): State<AppState>) -> Response {
    match voice::material::listening(&state.pool).await {
        Ok(heard) => Json(json!({ "sentences": heard })).into_response(),
        Err(error) => failed("the sentences to listen to could not be read", &error),
    }
}

/// One language as the voices screen shows it.
#[derive(Serialize)]
struct LanguageVoices {
    code: String,
    /// `native` for the learner's own language, `target` for one being learnt.
    role: &'static str,
    /// The voice it is spoken in now; `None` when nothing speaks it.
    spoken: Option<voice::Spoken>,
    options: Vec<voice::Voice>,
}

#[derive(Serialize)]
struct VoicesScreen {
    languages: Vec<LanguageVoices>,
    piper: voice::State,
    elevenlabs: voice::State,
    budget: Option<voice::budget::Budget>,
}

async fn voices_screen(State(state): State<AppState>) -> Response {
    let languages: Vec<(String, i64)> = match sqlx::query_as("SELECT code, is_native FROM language ORDER BY is_native, code")
        .fetch_all(&state.pool)
        .await
    {
        Ok(languages) => languages,
        Err(error) => return failed("the languages could not be read", &error.into()),
    };
    let mut screen = Vec::with_capacity(languages.len());
    for (code, is_native) in languages {
        let native = is_native == 1;
        let spoken = match state.voices.spoken(&state.pool, &code).await {
            Ok(spoken) => spoken,
            Err(error) => return failed("the voice of a language could not be read", &error),
        };
        screen.push(LanguageVoices {
            options: state.voices.options(&code, native).await,
            role: if native { "native" } else { "target" },
            spoken,
            code,
        });
    }
    let (piper, elevenlabs) = state.voices.states().await;
    Json(VoicesScreen {
        languages: screen,
        piper,
        elevenlabs,
        budget: state.voices.budget(&state.pool).await,
    })
    .into_response()
}

#[derive(Deserialize)]
struct Choice {
    engine: Engine,
    voice: String,
}

async fn choose_voice(State(state): State<AppState>, UrlPath(language): UrlPath<String>, Json(choice): Json<Choice>) -> Response {
    match state.voices.choose(&state.pool, &language, choice.engine, &choice.voice).await {
        Ok(voice) => Json(voice).into_response(),
        Err(error) => speak_failed(&error),
    }
}

/// A failure that is the server's own: logged in full, answered without
/// detail, because the detail is a database error the browser cannot act on.
fn failed(message: &str, error: &anyhow::Error) -> Response {
    tracing::error!(error = format!("{error:#}"), "{message}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::pack;

    /// An in-memory database with the schema applied: the real thing, minus
    /// the file.
    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        pool
    }

    /// A database holding one small pack, so the study endpoints have
    /// something to answer with.
    async fn taught() -> SqlitePool {
        let pool = pool().await;
        let toml = r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be-present"
name = "n"
pattern = "<pronoun> + am/is/are"
say = "<pronoun> is here."
explanation = "e"
order = 10

  [[formula.sample]]
  native = "a"
  target = "I am at home."

  [[formula.slot]]
  name = "pronoun"
  values = [{ native = "one", target = "I" }]
"#;
        pack::load(&pool, &toml::from_str(toml).unwrap()).await.unwrap();
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

    /// The router as a locked stand runs it.
    fn locked(pool: SqlitePool, dir: &tempfile::TempDir) -> Router {
        router(pool, Some(auth::hash("hunter2").unwrap()), dir.path(), Voices::none())
    }

    /// The router as a developer's machine runs it: no password.
    fn open(pool: SqlitePool, dir: &tempfile::TempDir) -> Router {
        router(pool, None, dir.path(), Voices::none())
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let body = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    fn json_request(method: &str, uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    /// Signs in and returns the cookie to send with later requests.
    async fn sign_in(app: &Router) -> String {
        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/session", r#"{"password":"hunter2"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response
            .headers()
            .get(header::SET_COOKIE)
            .expect("signing in should set a cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn health_reports_ok_with_the_version() {
        let dir = web_root();
        let response = open(pool().await, &dir)
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = body_json(response).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn health_is_reachable_without_signing_in() {
        // The stand is watched by something that cannot hold a session.
        let dir = web_root();
        let response = locked(pool().await, &dir)
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn an_existing_file_is_served_as_it_is() {
        let dir = web_root();
        let response = open(pool().await, &dir)
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
        let response = open(pool().await, &dir)
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
        let response = open(pool().await, &dir)
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
        let response = router(pool().await, None, dir.path(), Voices::none())
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

    #[tokio::test]
    async fn the_learners_own_data_is_behind_the_door() {
        // The one thing the password is actually for.
        let dir = web_root();
        let app = locked(taught().await, &dir);
        for uri in ["/api/today", "/api/formulas/be-present"] {
            let response = app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri} was readable without signing in");
        }

        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/formulas/be-present/review", r#"{"rating":"good"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a stranger could grade the learner's formulas");
    }

    #[tokio::test]
    async fn signing_in_opens_the_study_endpoints() {
        let dir = web_root();
        let app = locked(taught().await, &dir);
        let cookie = sign_in(&app).await;

        let response = app
            .clone()
            .oneshot(Request::get("/api/today").header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["queue"][0]["formula"]["id"], "be-present");
    }

    #[tokio::test]
    async fn a_wrong_password_is_refused_and_sets_no_cookie() {
        let dir = web_root();
        let response = locked(pool().await, &dir)
            .oneshot(json_request("POST", "/api/session", r#"{"password":"guess"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().get(header::SET_COOKIE).is_none(), "a failed login handed out a session");
    }

    #[tokio::test]
    async fn signing_out_closes_the_door_again() {
        let dir = web_root();
        let app = locked(taught().await, &dir);
        let cookie = sign_in(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/session")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // The token is forgotten server-side, so a client that kept the
        // cookie is still out.
        let response = app
            .oneshot(Request::get("/api/today").header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn an_open_stand_needs_no_session() {
        let dir = web_root();
        let response = open(taught().await, &dir)
            .oneshot(Request::get("/api/today").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn the_session_endpoint_says_whether_the_door_is_locked() {
        let dir = web_root();
        let response = locked(pool().await, &dir)
            .oneshot(Request::get("/api/session").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = body_json(response).await;
        assert_eq!(body["required"], true);
        assert_eq!(body["signed_in"], false);
    }

    #[tokio::test]
    async fn answering_a_formula_schedules_it() {
        let dir = web_root();
        let app = open(taught().await, &dir);
        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/formulas/be-present/review", r#"{"rating":"good"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = body_json(response).await;
        assert_ne!(body["stitch"], "new", "an answered formula should have left the new pile");
        assert!(body["due"].is_string());
    }

    #[tokio::test]
    async fn answering_a_formula_that_does_not_exist_is_a_404() {
        let dir = web_root();
        let response = open(taught().await, &dir)
            .oneshot(json_request("POST", "/api/formulas/nonsense/review", r#"{"rating":"good"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_session_outlives_a_restart_of_the_server() {
        // The stand is updated with `docker compose up -d` every week. If
        // that logged the learner's phone out, the login screen would stand
        // between them and the drill on exactly the days a new version lands.
        let dir = web_root();
        let pool = taught().await;
        let cookie = sign_in(&locked(pool.clone(), &dir)).await;

        // A brand-new router over the same database is what a restart is.
        let restarted = locked(pool, &dir);
        let response = restarted
            .oneshot(Request::get("/api/today").header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "restarting the server logged the learner out");
    }

    #[tokio::test]
    async fn a_misconfigured_hash_is_not_reported_as_a_wrong_password() {
        // Telling the learner "wrong password" would send them hunting for a
        // password that was never going to work.
        let dir = web_root();
        let response = router(pool().await, Some("not-a-hash".to_string()), dir.path(), Voices::none())
            .oneshot(json_request("POST", "/api/session", r#"{"password":"anything"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn a_formula_comes_back_with_its_samples_and_slots() {
        let dir = web_root();
        let response = open(taught().await, &dir)
            .oneshot(Request::get("/api/formulas/be-present").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = body_json(response).await;
        assert_eq!(body["samples"][0]["target"], "I am at home.");
        assert_eq!(body["slots"][0]["name"], "pronoun");
    }
}
