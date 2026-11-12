//! The router: what the browser asks the server for.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as UrlPath, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::anki;
use crate::auth;
use crate::breaks;
use crate::drift;
use crate::import::{self, web};
use crate::inventory;
use crate::lexicon;
use crate::model;
use crate::own::{self, NewWord, Refused};
use crate::reading::{self, Addition, Arrival, Library, Place};
use crate::shelf;
use crate::study::{self, Answer, AnswerError, Graded, Sent};
use crate::voice::{self, Engine, SpeakError, Tempo, Voices};
use crate::words;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// Argon2 hash of the learner's password; `None` leaves the stand open.
    pub password_hash: Option<Arc<str>>,
    /// The engines that speak, and the cache behind them.
    pub voices: Voices,
    /// The texts as they read, kept while the server runs.
    pub library: Library,
    /// What a text by a link is fetched with.
    pub web: reqwest::Client,
}

impl AppState {
    /// The state of a server over a database.
    #[must_use]
    pub fn new(pool: SqlitePool, password_hash: Option<String>, voices: Voices) -> Self {
        Self {
            pool,
            password_hash: password_hash.filter(|hash| !hash.trim().is_empty()).map(Into::into),
            voices,
            library: Library::default(),
            web: web::client().unwrap_or_else(|error| {
                tracing::error!(%error, "the client for links could not start; links will not be followed");
                reqwest::Client::new()
            }),
        }
    }
}

/// The router: the API under `/api`, the tutor app everywhere else.
///
/// The SPA is served from `web_dir` as real files when they exist and as
/// `index.html` otherwise, so a deep link into a lesson loads the app rather
/// than a 404. The API never falls through to it: a misspelled endpoint has to
/// look like a mistake, not like a page.
pub fn router(pool: SqlitePool, password_hash: Option<String>, web_dir: &Path, voices: Voices) -> Router {
    routes(AppState::new(pool, password_hash, voices), web_dir)
}

/// The router over a state made beforehand: what the server runs, so it can
/// read the library while the router answers.
pub fn routes(state: AppState, web_dir: &Path) -> Router {
    // Everything the learner's own data flows through sits behind the door;
    // health and the session endpoints are what a locked-out client is still
    // allowed to ask.
    let study = Router::new()
        .route("/today", get(today))
        .route("/formulas/{id}", get(formula))
        .route("/formulas/{id}/review", post(review))
        .route("/own-words", get(own_words).post(add_own_word))
        .route("/own-words/{id}", delete(remove_own_word))
        .route("/pairs/{id}", get(pair))
        .route("/pairs/{id}/review", post(review_pair))
        .route("/words/{id}", get(word))
        .route("/words/{id}/review", post(review_word))
        .route("/words/{id}/examples", get(word_examples))
        .route("/words/{id}/picks", post(pick_word))
        .route("/words/{id}/picks/{sentence}", delete(unpick_word))
        .route("/declarations", post(declare_words))
        .route("/declarations/take-back", post(take_back_words))
        .route("/texts", get(texts).post(add_text))
        .route("/texts/file", post(add_file).layer(DefaultBodyLimit::max(web::LIMIT)))
        .route("/texts/link", post(add_link))
        .route("/texts/{id}", get(text).patch(place_in_text).delete(remove_text))
        .route("/shelf", get(shelf_items))
        .route("/shelf/{id}", post(take_from_shelf))
        .route("/picks/anki", get(anki_file))
        .route("/speech", get(speech))
        .route("/listen", get(listen))
        .route("/voices", get(voices_screen))
        .route("/voices/{language}", put(choose_voice))
        .route("/inventory", get(inventory_run).post(inventory_answer))
        .route("/inventory/{word}", delete(inventory_undo))
        .route("/model", get(model_file))
        .route("/model/standing", get(model_standing))
        .route("/breaks", post(start_break))
        .route("/breaks/away", post(count_away))
        .route("/breaks/{id}", delete(remove_break))
        .route("/breaks/{id}/end", post(end_break))
        .route("/drift", get(drift_weeks))
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

async fn review(State(state): State<AppState>, UrlPath(id): UrlPath<String>, Json(sent): Json<Sent<Answer>>) -> Response {
    match study::review(&state.pool, &id, sent, Utc::now()).await {
        Ok(reviewed) => Json(reviewed).into_response(),
        Err(error) => answer_failed(error),
    }
}

/// An answer that could not be taken: a formula that is not there is the
/// client asking for the wrong id, a turn naming something the formula does
/// not ask is a client out of step with the pack, and a database failure is
/// the server's own.
fn answer_failed(error: AnswerError) -> Response {
    match error {
        AnswerError::NotFound(_) => (StatusCode::NOT_FOUND, Json(json!({ "error": error.to_string() }))).into_response(),
        AnswerError::Unanswerable(_) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": error.to_string() }))).into_response(),
        AnswerError::Database(error) => failed("the answer could not be recorded", &error),
    }
}

async fn own_words(State(state): State<AppState>) -> Response {
    match own::list(&state.pool).await {
        Ok(words) => Json(json!({ "words": words })).into_response(),
        Err(error) => failed("your words could not be read", &error),
    }
}

async fn add_own_word(State(state): State<AppState>, Json(word): Json<NewWord>) -> Response {
    match own::add(&state.pool, &word, Utc::now()).await {
        Ok(word) => (StatusCode::CREATED, Json(word)).into_response(),
        Err(Refused::Database(error)) => failed("the word could not be added", &error),
        // Said to the learner as it is: each of these is theirs to fix.
        Err(refused) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": refused.to_string() }))).into_response(),
    }
}

async fn remove_own_word(State(state): State<AppState>, UrlPath(id): UrlPath<i64>) -> Response {
    match own::remove(&state.pool, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => failed("the word could not be removed", &error),
    }
}

async fn word(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Response {
    match words::word(&state.pool, &id).await {
        Ok(word) => Json(word).into_response(),
        // A word that is not there is a client asking for the wrong id.
        Err(error) => (StatusCode::NOT_FOUND, Json(json!({ "error": error.to_string() }))).into_response(),
    }
}

async fn review_word(State(state): State<AppState>, UrlPath(id): UrlPath<String>, Json(sent): Json<Sent<Graded>>) -> Response {
    match study::review_word(&state.pool, &id, sent, Utc::now()).await {
        Ok(reviewed) => Json(reviewed).into_response(),
        Err(error) => answer_failed(error),
    }
}

async fn pair(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Response {
    match crate::pairs::pair(&state.pool, &id).await {
        Ok(pair) => Json(pair).into_response(),
        // A pair that is not there is a client asking for the wrong id.
        Err(error) => (StatusCode::NOT_FOUND, Json(json!({ "error": error.to_string() }))).into_response(),
    }
}

async fn review_pair(State(state): State<AppState>, UrlPath(id): UrlPath<String>, Json(sent): Json<Sent<crate::pairs::Chosen>>) -> Response {
    match study::review_pair(&state.pool, &id, sent, Utc::now()).await {
        Ok(reviewed) => Json(reviewed).into_response(),
        Err(error) => answer_failed(error),
    }
}

/// How many sentences of their own material the reader shows for a word.
const EXAMPLES: usize = 4;

async fn word_examples(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Response {
    match words::examples(&state.pool, &id, EXAMPLES).await {
        Ok(examples) => Json(examples).into_response(),
        Err(error) => failed("the examples of a word could not be read", &error),
    }
}

/// What the learner holds of some words after the reader changed them: by
/// id, `null` for a word they hold nothing of.
async fn holds_after(pool: &SqlitePool, ids: &[String]) -> Result<serde_json::Map<String, serde_json::Value>, anyhow::Error> {
    let holds = words::holds_of(pool, ids).await?;
    Ok(ids.iter().map(|id| (id.clone(), json!(holds.get(id)))).collect())
}

fn word_refused(error: words::Refused) -> Response {
    let status = match error {
        words::Refused::NotAWord(_) | words::Refused::NotThere { .. } => StatusCode::UNPROCESSABLE_ENTITY,
        words::Refused::NoSentence(_) => StatusCode::NOT_FOUND,
        words::Refused::Database(error) => return failed("the word could not be saved", &error),
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

/// Where a word was taken from: the sentence, and the character it starts at.
#[derive(Deserialize)]
struct Taken {
    sentence: i64,
    start: i64,
}

async fn pick_word(State(state): State<AppState>, UrlPath(id): UrlPath<String>, Json(taken): Json<Taken>) -> Response {
    if let Err(error) = words::pick(&state.pool, &id, taken.sentence, taken.start, Utc::now()).await {
        return word_refused(error);
    }
    match holds_after(&state.pool, std::slice::from_ref(&id)).await {
        Ok(holds) => Json(json!({ "holds": holds })).into_response(),
        Err(error) => failed("the word could not be read back", &error),
    }
}

async fn unpick_word(State(state): State<AppState>, UrlPath((id, sentence)): UrlPath<(String, i64)>) -> Response {
    if let Err(error) = words::unpick(&state.pool, &id, sentence).await {
        return failed("the word could not be taken back", &error);
    }
    match holds_after(&state.pool, std::slice::from_ref(&id)).await {
        Ok(holds) => Json(json!({ "holds": holds })).into_response(),
        Err(error) => failed("the word could not be read back", &error),
    }
}

/// Words the reader says something of at once.
#[derive(Deserialize)]
struct Said {
    words: Vec<String>,
}

/// How many words one declaration may carry: a paragraph's worth many
/// times over, and not a dictionary.
const DECLARED_AT_ONCE: usize = 2_000;

async fn declare_words(State(state): State<AppState>, Json(said): Json<Said>) -> Response {
    if said.words.len() > DECLARED_AT_ONCE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": format!("at most {DECLARED_AT_ONCE} words at once") })),
        )
            .into_response();
    }
    let declared = match words::declare(&state.pool, &said.words, Utc::now()).await {
        Ok(declared) => declared,
        Err(error) => return word_refused(error),
    };
    match holds_after(&state.pool, &said.words).await {
        Ok(holds) => Json(json!({ "declared": declared, "holds": holds })).into_response(),
        Err(error) => failed("the words could not be read back", &error),
    }
}

async fn take_back_words(State(state): State<AppState>, Json(said): Json<Said>) -> Response {
    if said.words.len() > DECLARED_AT_ONCE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": format!("at most {DECLARED_AT_ONCE} words at once") })),
        )
            .into_response();
    }
    let taken = match words::take_back(&state.pool, &said.words, Utc::now()).await {
        Ok(taken) => taken,
        Err(error) => return failed("the declarations could not be taken back", &error),
    };
    match holds_after(&state.pool, &said.words).await {
        Ok(holds) => Json(json!({ "taken": taken, "holds": holds })).into_response(),
        Err(error) => failed("the words could not be read back", &error),
    }
}

fn text_refused(error: reading::Refused) -> Response {
    let status = match error {
        reading::Refused::NoLanguage => StatusCode::CONFLICT,
        reading::Refused::Empty => StatusCode::UNPROCESSABLE_ENTITY,
        reading::Refused::NotFound(_) | reading::Refused::NoChapter(..) => StatusCode::NOT_FOUND,
        reading::Refused::Database(error) => return failed("the text could not be read or saved", &error),
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

fn refused(status: StatusCode, error: &impl std::fmt::Display) -> Response {
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

/// The learner's texts, and what to read next among them and the shelf.
#[derive(Serialize)]
struct Texts {
    texts: Vec<reading::Summary>,
    next: Option<shelf::Next>,
}

async fn texts(State(state): State<AppState>) -> Response {
    let read = async {
        let texts = reading::list(&state.pool, &state.library).await?;
        let language = crate::pack::language_being_learnt(&state.pool).await?;
        let items = match language.as_deref().and_then(shelf::of) {
            Some(shelf) => shelf::items(shelf, &reading::holds(&state.pool).await?, &texts),
            None => Vec::new(),
        };
        let next = shelf::next(&texts, &items);
        Ok::<_, anyhow::Error>(Texts { texts, next })
    };
    match read.await {
        Ok(library) => Json(library).into_response(),
        Err(error) => failed("the texts could not be read", &error),
    }
}

async fn add_text(State(state): State<AppState>, Json(addition): Json<Addition>) -> Response {
    match reading::add(&state.pool, &state.library, &addition, Utc::now()).await {
        Ok(summary) => (StatusCode::CREATED, Json(summary)).into_response(),
        Err(error) => text_refused(error),
    }
}

/// The file a text is read from: its name says what it is.
#[derive(Deserialize)]
struct Named {
    #[serde(default)]
    name: Option<String>,
}

/// Keeps what came of an import.
async fn keep(state: &AppState, arrival: Arrival) -> Response {
    match reading::keep(&state.pool, &state.library, arrival, Utc::now()).await {
        Ok(summary) => (StatusCode::CREATED, Json(summary)).into_response(),
        Err(error) => text_refused(error),
    }
}

async fn add_file(State(state): State<AppState>, Query(named): Query<Named>, body: Bytes) -> Response {
    let language = match reading::language(&state.pool).await {
        Ok(language) => language,
        Err(error) => return text_refused(error),
    };
    let name = named.name.as_deref();
    // A book is read in a fraction of a second; a large one is work for a
    // processor, not for the runtime answering other requests.
    let read = tokio::task::spawn_blocking({
        let name = name.map(str::to_string);
        move || import::file(name.as_deref(), &body, lexicon::of(&language))
    })
    .await;
    let imported = match read {
        Ok(Ok(imported)) => imported,
        Ok(Err(error)) => return refused(StatusCode::UNPROCESSABLE_ENTITY, &error),
        Err(error) => return failed("the file could not be read", &anyhow::Error::from(error)),
    };
    // A file named after its text names it when the text does not.
    let stem = name
        .map(|name| name.rsplit(['/', '\\']).next().unwrap_or(name))
        .map(|name| name.split('.').next().unwrap_or(name).replace(['_', '-'], " "))
        .filter(|stem| !stem.trim().is_empty());
    let title = imported.document.title.is_none().then_some(stem).flatten();
    keep(
        &state,
        Arrival {
            format: imported.format,
            document: imported.document,
            source: None,
            title,
            author: None,
        },
    )
    .await
}

/// A text by its link.
#[derive(Deserialize)]
struct Link {
    url: String,
}

async fn fetch(state: &AppState, link: &str) -> Result<web::Fetched, Box<Response>> {
    let language = reading::language(&state.pool).await.map_err(|error| Box::new(text_refused(error)))?;
    web::fetch(&state.web, link, lexicon::of(&language)).await.map_err(|error| {
        let status = match error {
            web::Unfetched::NotALink(_) | web::Unfetched::Unreadable(_) | web::Unfetched::TooLarge | web::Unfetched::NoArticle => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            web::Unfetched::Unreachable(_) | web::Unfetched::Refused(_) => StatusCode::BAD_GATEWAY,
        };
        Box::new(refused(status, &error))
    })
}

async fn add_link(State(state): State<AppState>, Json(link): Json<Link>) -> Response {
    let fetched = match fetch(&state, &link.url).await {
        Ok(fetched) => fetched,
        Err(response) => return *response,
    };
    keep(
        &state,
        Arrival {
            format: fetched.format,
            document: fetched.document,
            source: Some(fetched.source),
            title: None,
            author: None,
        },
    )
    .await
}

/// Which chapter of a text to read.
#[derive(Deserialize)]
struct Which {
    #[serde(default)]
    chapter: Option<u32>,
}

async fn text(State(state): State<AppState>, UrlPath(id): UrlPath<i64>, Query(which): Query<Which>) -> Response {
    match reading::read(&state.pool, &state.library, id, which.chapter).await {
        Ok(text) => Json(text).into_response(),
        Err(error) => text_refused(error),
    }
}

async fn place_in_text(State(state): State<AppState>, UrlPath(id): UrlPath<i64>, Json(place): Json<Place>) -> Response {
    match reading::place(&state.pool, &state.library, id, place, Utc::now()).await {
        Ok(summary) => Json(summary).into_response(),
        Err(error) => text_refused(error),
    }
}

async fn remove_text(State(state): State<AppState>, UrlPath(id): UrlPath<i64>) -> Response {
    match reading::remove(&state.pool, &state.library, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => text_refused(error),
    }
}

/// The shelf of the language being learnt, each item with how much of it
/// is the learner's.
async fn shelf_items(State(state): State<AppState>) -> Response {
    let read = async {
        let Some(shelf) = crate::pack::language_being_learnt(&state.pool).await?.as_deref().and_then(shelf::of) else {
            return Ok(Vec::new());
        };
        let texts = reading::list(&state.pool, &state.library).await?;
        Ok::<_, anyhow::Error>(shelf::items(shelf, &reading::holds(&state.pool).await?, &texts))
    };
    match read.await {
        Ok(items) => Json(json!({ "items": items })).into_response(),
        Err(error) => failed("the shelf could not be read", &error),
    }
}

/// Takes an item of the shelf: fetched from its source and kept as a text,
/// under the title and author the shelf gives it.
async fn take_from_shelf(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Response {
    let language = match reading::language(&state.pool).await {
        Ok(language) => language,
        Err(error) => return text_refused(error),
    };
    let Some(entry) = shelf::of(&language)
        .and_then(|shelf| shelf.items.iter().find(|(entry, _)| entry.id == id))
        .map(|(entry, _)| entry.clone())
    else {
        return refused(StatusCode::NOT_FOUND, &format!("the shelf has no {id}"));
    };
    let fetched = match fetch(&state, &entry.source).await {
        Ok(fetched) => fetched,
        Err(response) => return *response,
    };
    keep(
        &state,
        Arrival {
            format: fetched.format,
            document: fetched.document,
            source: Some(entry.source),
            title: Some(entry.title),
            author: entry.author,
        },
    )
    .await
}

/// The words taken from texts as a file for Anki.
async fn anki_file(State(state): State<AppState>) -> Response {
    match anki::notes(&state.pool).await {
        Ok(notes) => (
            [
                (header::CONTENT_TYPE, "text/tab-separated-values; charset=utf-8"),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"hilvan-words.txt\""),
            ],
            anki::file(&notes),
        )
            .into_response(),
        Err(error) => failed("the words could not be read", &error),
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
    /// A sentence of the material in this language, to hear a voice with.
    sample: Option<String>,
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
        let sample = match voice::material::sample_in(&state.pool, &code).await {
            Ok(sample) => sample,
            Err(error) => return failed("a sample sentence could not be read", &error),
        };
        screen.push(LanguageVoices {
            sample,
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

/// What the inventory screen sends: its answers, oldest first. A list, so a
/// screen that lost its connection can send what it holds in one go.
#[derive(Deserialize)]
struct Answers {
    answers: Vec<inventory::Answer>,
}

async fn inventory_run(State(state): State<AppState>) -> Response {
    match inventory::run(&state.pool, inventory::BATCH).await {
        Ok(run) => Json(run).into_response(),
        Err(error) => failed("the inventory could not be read", &error),
    }
}

async fn inventory_answer(State(state): State<AppState>, Json(body): Json<Answers>) -> Response {
    match inventory::answer(&state.pool, &body.answers, Utc::now()).await {
        Ok(()) => inventory_run(State(state)).await,
        Err(error) => inventory_refused(error),
    }
}

async fn inventory_undo(State(state): State<AppState>, UrlPath(word): UrlPath<String>) -> Response {
    match inventory::undo(&state.pool, &word, Utc::now()).await {
        Ok(()) => inventory_run(State(state)).await,
        Err(error) => inventory_refused(error),
    }
}

fn inventory_refused(error: inventory::Refused) -> Response {
    let status = match error {
        inventory::Refused::NotAWord(_) => StatusCode::UNPROCESSABLE_ENTITY,
        inventory::Refused::Asked(_) => StatusCode::CONFLICT,
        inventory::Refused::Database(error) => return failed("the inventory could not be saved", &error),
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

/// What starting a break says: when the learner will be back, if they know.
#[derive(Deserialize)]
struct Leaving {
    #[serde(default)]
    until: Option<DateTime<Utc>>,
}

async fn start_break(State(state): State<AppState>, Json(leaving): Json<Leaving>) -> Response {
    match breaks::start(&state.pool, leaving.until, Utc::now()).await {
        Ok(taken) => (StatusCode::CREATED, Json(taken)).into_response(),
        Err(error) => break_refused(error),
    }
}

async fn count_away(State(state): State<AppState>) -> Response {
    match breaks::count_away(&state.pool, Utc::now()).await {
        Ok(taken) => (StatusCode::CREATED, Json(taken)).into_response(),
        Err(error) => break_refused(error),
    }
}

async fn end_break(State(state): State<AppState>, UrlPath(id): UrlPath<i64>) -> Response {
    match breaks::end(&state.pool, id, Utc::now()).await {
        Ok(taken) => Json(taken).into_response(),
        Err(error) => break_refused(error),
    }
}

async fn remove_break(State(state): State<AppState>, UrlPath(id): UrlPath<i64>) -> Response {
    match breaks::remove(&state.pool, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => break_refused(error),
    }
}

/// A break the learner cannot take, end or remove as asked: a sentence to
/// show them, under the status that says whose the mistake is.
fn break_refused(error: breaks::Refused) -> Response {
    let status = match error {
        breaks::Refused::NotFound(_) => StatusCode::NOT_FOUND,
        breaks::Refused::AlreadyAway | breaks::Refused::NotAway | breaks::Refused::NotGoingOn(_) => StatusCode::CONFLICT,
        breaks::Refused::BackTooSoon => StatusCode::UNPROCESSABLE_ENTITY,
        breaks::Refused::Database(error) => return failed("the break could not be saved", &error),
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

async fn drift_weeks(State(state): State<AppState>) -> Response {
    match drift::weeks(&state.pool, Utc::now()).await {
        Ok(drift) => Json(drift).into_response(),
        Err(error) => failed("what slipped could not be read", &error),
    }
}

/// A moment the model is asked about; now when left out. A moment still to
/// come is now as well: the model cannot be told ahead of the answers.
#[derive(Deserialize)]
struct At {
    at: Option<DateTime<Utc>>,
}

impl At {
    fn or_now(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        self.at.map_or(now, |at| at.min(now))
    }
}

/// The learner model as a file to keep: everything the tutor knows of the
/// learner, as of now or of the moment asked.
async fn model_file(State(state): State<AppState>, Query(at): Query<At>) -> Response {
    let now = Utc::now();
    let read = match at.at {
        Some(_) => model::as_of(&state.pool, at.or_now(now), now).await,
        None => model::export(&state.pool, now).await,
    };
    match read {
        Ok(model) => {
            let name = format!("attachment; filename=\"hilvan-model-{}.json\"", model.as_of.format("%Y-%m-%d"));
            ([(header::CONTENT_DISPOSITION, name)], Json(model)).into_response()
        }
        Err(error) => failed("the learner model could not be read", &error),
    }
}

/// Where the learner stood at a moment, beside where they stand now.
#[derive(Serialize)]
struct Standings {
    then: model::Snapshot,
    now: model::Snapshot,
    /// The first answer the tutor asked for, if any: how far back there is
    /// anything to look at.
    began: Option<DateTime<Utc>>,
}

async fn model_standing(State(state): State<AppState>, Query(at): Query<At>) -> Response {
    let now = Utc::now();
    let read = async {
        Ok::<_, anyhow::Error>(Standings {
            then: model::standing_at(&state.pool, at.or_now(now)).await?,
            now: model::standing_at(&state.pool, now).await?,
            began: model::began(&state.pool).await?,
        })
    };
    match read.await {
        Ok(standings) => Json(standings).into_response(),
        Err(error) => failed("the standing could not be read", &error),
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
  words = ["home"]

  [[formula.slot]]
  name = "pronoun"
  values = [{ native = "one", target = "I" }]

[[word]]
lemma = "home"
gloss = "дом"
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

    /// An answer's body as a device sends it: under an identity minted here,
    /// given now.
    fn sent(body: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static MINTED: AtomicU64 = AtomicU64::new(0);
        let id = MINTED.fetch_add(1, Ordering::Relaxed);
        let rest = body.trim_start().strip_prefix('{').expect("an answer is a JSON object");
        format!(r#"{{"answer_id":"app-{id}","answered_at":"{}",{rest}"#, Utc::now().to_rfc3339())
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
        for uri in [
            "/api/today",
            "/api/formulas/be-present",
            "/api/words/en:home",
            "/api/inventory",
            "/api/model",
            "/api/model/standing",
            "/api/drift",
        ] {
            let response = app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri} was readable without signing in");
        }

        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/inventory", r#"{"answers":[{"word":"en:house","known":true}]}"#))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "a stranger could tell the tutor what the learner knows"
        );

        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/formulas/be-present/review", r#"{"rating":"good"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a stranger could grade the learner's formulas");

        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/words/en:home/review", r#"{"rating":"good"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a stranger could grade the learner's words");

        let response = app.clone().oneshot(json_request("POST", "/api/breaks", "{}")).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a stranger could stop the learner's schedule");
    }

    #[tokio::test]
    async fn a_break_is_taken_ended_and_removed() {
        let dir = web_root();
        let app = open(taught().await, &dir);

        let response = app.clone().oneshot(json_request("POST", "/api/breaks", "{}")).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let taken = body_json(response).await;
        let id = taken["id"].as_i64().unwrap();
        assert!(taken["ends_at"].is_null(), "a break with no day back lasts until the learner is back");

        let response = app.clone().oneshot(Request::get("/api/today").body(Body::empty()).unwrap()).await.unwrap();
        let today = body_json(response).await;
        assert_eq!(today["on_break"]["id"], id);
        assert_eq!(today["queue"].as_array().map(Vec::len), Some(0), "nothing is asked on a break");

        let response = app.clone().oneshot(json_request("POST", "/api/breaks", "{}")).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "a second break at once");

        let response = app
            .clone()
            .oneshot(Request::post(format!("/api/breaks/{id}/end")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_json(response).await["ends_at"].is_string());

        let response = app.clone().oneshot(Request::get("/api/today").body(Body::empty()).unwrap()).await.unwrap();
        assert!(body_json(response).await["on_break"].is_null());

        let response = app
            .clone()
            .oneshot(Request::delete(format!("/api/breaks/{id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = app
            .oneshot(Request::delete(format!("/api/breaks/{id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_break_that_cannot_be_is_refused_in_words() {
        let dir = web_root();
        let app = open(taught().await, &dir);

        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/breaks", r#"{"until":"2020-01-01T00:00:00Z"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "a day back already past");
        assert!(body_json(response).await["error"].as_str().is_some_and(|error| error.contains("after today")));

        let response = app
            .clone()
            .oneshot(Request::post("/api/breaks/away").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "a learner who never answered has no absence to count");

        let response = app.oneshot(Request::post("/api/breaks/7/end").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_drift_is_told_by_week() {
        let dir = web_root();
        let response = open(taught().await, &dir)
            .oneshot(Request::get("/api/drift").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let drift = body_json(response).await;
        assert!(drift["this_week"]["starts_on"].is_string());
        assert_eq!(drift["last_week"]["slips"].as_array().map(Vec::len), Some(0));
    }

    #[tokio::test]
    async fn a_text_is_read_and_its_words_are_known_taken_and_given_back() {
        let dir = web_root();
        let app = open(taught().await, &dir);
        let call = |method: &str, uri: &str, body: &str| app.clone().oneshot(json_request(method, uri, body));

        let response = call("POST", "/api/texts", r#"{"body":"She went to the house. Harry went home.\n\nIt was late."}"#)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let added = body_json(response).await;
        assert_eq!(
            (added["title"].as_str(), added["paragraphs"].as_i64()),
            (Some("She went to the house."), Some(2))
        );
        let id = added["id"].as_i64().unwrap();

        let text = body_json(call("GET", &format!("/api/texts/{id}"), "").await.unwrap()).await;
        assert_eq!(text["paragraphs"], 2, "the count, said once");
        let first = &text["lines"][0][0];
        assert_eq!(first["text"], "She went to the house.");
        let went = &first["tokens"][1];
        assert_eq!((went["start"].as_i64(), went["length"].as_i64()), (Some(4), Some(4)));
        let lemma = |token: &serde_json::Value| text["lemmas"][usize::try_from(token["parts"][0][0].as_u64().unwrap()).unwrap()].clone();
        assert_eq!(lemma(went), "en:go");
        assert_eq!(text["lines"][0][1]["tokens"][0]["name"], true, "Harry is a name");
        assert_eq!(text["coverage"]["names"], 1);

        let declared = body_json(call("POST", "/api/declarations", r#"{"words":["en:she","en:go"]}"#).await.unwrap()).await;
        assert_eq!(declared["declared"], json!(["en:she", "en:go"]));
        assert_eq!(declared["holds"]["en:go"], "sewn");
        let response = call("POST", "/api/declarations", r#"{"words":["en:zxqv"]}"#).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let sentence = first["sentence"].as_i64().unwrap();
        let taken = call("POST", "/api/words/en:house/picks", &format!(r#"{{"sentence":{sentence},"start":16}}"#))
            .await
            .unwrap();
        assert_eq!(taken.status(), StatusCode::OK);
        assert_eq!(body_json(taken).await["holds"]["en:house"], "deck");
        let wrong = call("POST", "/api/words/en:house/picks", &format!(r#"{{"sentence":{sentence},"start":0}}"#))
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let examples = body_json(call("GET", "/api/words/en:house/examples", "").await.unwrap()).await;
        assert_eq!(examples[0]["text"], "She went to the house.");
        assert_eq!(examples[0]["source"]["kind"], "text");

        let listed = body_json(call("GET", "/api/texts", "").await.unwrap()).await;
        assert_eq!(
            (listed["texts"][0]["coverage"]["sewn"].as_i64(), listed["texts"][0]["coverage"]["deck"].as_i64()),
            (Some(3), Some(1))
        );
        assert_eq!(listed["next"]["kind"], "text", "a text of the learner's or the shelf: {}", listed["next"]);

        let back = call("DELETE", &format!("/api/words/en:house/picks/{sentence}"), "").await.unwrap();
        assert_eq!(body_json(back).await["holds"]["en:house"], serde_json::Value::Null);
        let taken_back = body_json(call("POST", "/api/declarations/take-back", r#"{"words":["en:go"]}"#).await.unwrap()).await;
        assert_eq!(
            (taken_back["taken"].clone(), taken_back["holds"]["en:go"].clone()),
            (json!(["en:go"]), serde_json::Value::Null)
        );

        let placed = body_json(call("PATCH", &format!("/api/texts/{id}"), r#"{"paragraph":1,"finished":true}"#).await.unwrap()).await;
        assert_eq!(placed["paragraph"], 1);
        assert!(placed["finished_at"].is_string());
        assert_eq!(call("DELETE", &format!("/api/texts/{id}"), "").await.unwrap().status(), StatusCode::NO_CONTENT);
        assert_eq!(call("GET", &format!("/api/texts/{id}"), "").await.unwrap().status(), StatusCode::NOT_FOUND);
        let empty = call("POST", "/api/texts", r#"{"body":"  1 2 3  "}"#).await.unwrap();
        assert_eq!(empty.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_book_comes_in_as_a_file_and_is_read_a_chapter_at_a_time() {
        let dir = web_root();
        let app = open(taught().await, &dir);
        let upload = |name: &str, bytes: Vec<u8>| {
            Request::post(format!("/api/texts/file?name={name}"))
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(bytes))
                .unwrap()
        };
        let response = app
            .clone()
            .oneshot(upload("small-book.epub", crate::import::epub::tests::book()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let added = body_json(response).await;
        assert_eq!(
            (
                added["title"].as_str(),
                added["author"].as_str(),
                added["format"].as_str(),
                added["chapters"].as_i64()
            ),
            (Some("A Small Book"), Some("Ann Writer"), Some("epub"), Some(2))
        );
        assert_eq!(added["chapter"]["title"], "Chapter One", "the chapter the learner is in");
        let id = added["id"].as_i64().unwrap();

        let second = body_json(
            app.clone()
                .oneshot(json_request("GET", &format!("/api/texts/{id}?chapter=1"), ""))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!((second["shown"].as_i64(), second["contents"].as_array().map(Vec::len)), (Some(1), Some(2)));
        assert_eq!(second["lines"][0][0]["text"], "The second day was warm.");
        assert_eq!(second["contents"][1]["paragraph"], 2, "where the chapter starts through the whole book");
        let missing = app
            .clone()
            .oneshot(json_request("GET", &format!("/api/texts/{id}?chapter=7"), ""))
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        // A book with its pictures is larger than a request usually is.
        let picture = "x".repeat(3 << 20);
        let heavy = crate::import::epub::tests::book_with(&[("OEBPS/plate.jpg", &picture)]);
        assert!(heavy.len() > 3 << 20);
        let response = app.clone().oneshot(upload("illustrated.epub", heavy)).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "a book of three megabytes");

        // A file named after its text, with nothing in it saying its title.
        let response = app.clone().oneshot(upload("the_rain-today.txt", b"It rained all day.".to_vec())).await.unwrap();
        assert_eq!(body_json(response).await["title"], "the rain today");
        let junk = app
            .clone()
            .oneshot(upload("notes.zip", crate::import::epub::tests::zip(&[("a.txt", "x")])))
            .await
            .unwrap();
        assert_eq!(junk.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_page_comes_in_by_its_link() {
        let page = Router::new()
            .route(
                "/cats",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                        "<html><head><title>Cats | Site</title></head><body><nav><p>Home and news and more.</p></nav><article><h1>All about cats</h1><p>Cats are small animals.</p></article></body></html>",
                    )
                }),
            )
            .route("/gone", get(|| async { StatusCode::NOT_FOUND }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, page).await });

        let dir = web_root();
        let app = open(taught().await, &dir);
        let link = |url: String| json_request("POST", "/api/texts/link", &json!({ "url": url }).to_string());
        let response = app.clone().oneshot(link(format!("http://{at}/cats"))).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let added = body_json(response).await;
        assert_eq!((added["title"].as_str(), added["format"].as_str()), (Some("All about cats"), Some("html")));
        assert_eq!(added["source"], format!("http://{at}/cats"));
        assert_eq!(added["coverage"]["words"], 4, "the article, not the navigation");

        let gone = app.clone().oneshot(link(format!("http://{at}/gone"))).await.unwrap();
        assert_eq!(gone.status(), StatusCode::BAD_GATEWAY);
        let nonsense = app.clone().oneshot(link("file:///etc/passwd".to_string())).await.unwrap();
        assert_eq!(nonsense.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn the_shelf_says_how_much_of_each_item_is_yours_and_the_list_what_to_read_next() {
        let dir = web_root();
        let app = open(taught().await, &dir);
        let shelf = body_json(app.clone().oneshot(json_request("GET", "/api/shelf", "")).await.unwrap()).await;
        let items = shelf["items"].as_array().unwrap();
        assert!(items.len() >= 40);
        assert!(items.iter().all(|item| item["estimate"].is_u64() && item["taken"].is_null()));
        let unknown = app.clone().oneshot(json_request("POST", "/api/shelf/no-such-book", "")).await.unwrap();
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

        // Nothing of the learner's yet: the shelf's nearest item is next.
        let listed = body_json(app.clone().oneshot(json_request("GET", "/api/texts", "")).await.unwrap()).await;
        assert_eq!(listed["texts"], json!([]));
        assert_eq!(listed["next"]["kind"], "shelf");
    }

    #[tokio::test]
    async fn the_words_taken_go_to_anki_as_a_file() {
        let dir = web_root();
        let app = open(taught().await, &dir);
        let added = body_json(
            app.clone()
                .oneshot(json_request("POST", "/api/texts", r#"{"body":"She went to the house."}"#))
                .await
                .unwrap(),
        )
        .await;
        let text = body_json(
            app.clone()
                .oneshot(json_request("GET", &format!("/api/texts/{}", added["id"]), ""))
                .await
                .unwrap(),
        )
        .await;
        let sentence = text["lines"][0][0]["sentence"].as_i64().unwrap();
        app.clone()
            .oneshot(json_request(
                "POST",
                "/api/words/en:house/picks",
                &format!(r#"{{"sentence":{sentence},"start":16}}"#),
            ))
            .await
            .unwrap();
        let response = app.clone().oneshot(json_request("GET", "/api/picks/anki", "")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "text/tab-separated-values; charset=utf-8");
        assert!(response.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().contains("hilvan-words.txt"));
        let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(body.starts_with("#separator:tab\n"));
        assert!(body.contains("She went to the <b>house</b>."), "{body}");
    }

    #[tokio::test]
    async fn a_word_comes_back_with_its_sentence_and_answering_it_schedules_it() {
        let dir = web_root();
        let app = open(taught().await, &dir);
        let response = app
            .clone()
            .oneshot(Request::get("/api/words/en:home").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["lemma"], "home");
        assert_eq!(body["gloss"], "дом");
        assert_eq!(body["level"], 1000);
        assert_eq!(body["contexts"][0]["text"], "I am at home.");
        assert_eq!(body["contexts"][0]["start"], 8);

        let response = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/words/en:home/review",
                &sent(r#"{"rating":"good","duration_ms":2100}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_ne!(body_json(response).await["stitch"], "new");

        // A lemma of the lexicon no pack teaches is a word all the same - the
        // reader asks for any - and one with no card cannot be answered.
        let response = app.clone().oneshot(json_request("GET", "/api/words/en:house", "")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["lemma"], "house");
        for (method, uri) in [
            ("GET", "/api/words/en:zxqv"),
            ("POST", "/api/words/en:zxqv/review"),
            ("POST", "/api/words/en:house/review"),
        ] {
            let response = app.clone().oneshot(json_request(method, uri, &sent(r#"{"rating":"good"}"#))).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {uri}");
        }
    }

    #[tokio::test]
    async fn the_queue_says_what_each_item_is() {
        // The app tells a formula from a word by `kind`; a queue item without
        // it would be drawn as the wrong card.
        let dir = web_root();
        let app = open(taught().await, &dir);
        let response = app.clone().oneshot(Request::get("/api/today").body(Body::empty()).unwrap()).await.unwrap();
        let body = body_json(response).await;
        assert_eq!(body["queue"][0]["kind"], "formula");
        assert_eq!(body["queue"][0]["direction"], "produce");
        assert_eq!(body["words"]["levels"][0]["size"], 1000);

        app.clone()
            .oneshot(json_request(
                "POST",
                "/api/formulas/be-present/review",
                &sent(r#"{"direction":"produce","turns":[{"asked":{"values":{"pronoun":"I"}},"typed":"I is here."}]}"#),
            ))
            .await
            .unwrap();
        let response = app.oneshot(Request::get("/api/today").body(Body::empty()).unwrap()).await.unwrap();
        let body = body_json(response).await;
        let word = body["queue"]
            .as_array()
            .unwrap()
            .iter()
            .find(|due| due["kind"] == "word")
            .expect("the word of the drilled formula should be waiting");
        assert_eq!(word["word"]["id"], "en:home");
        assert_eq!(word["is_new"], true);
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
            .oneshot(json_request(
                "POST",
                "/api/formulas/be-present/review",
                &sent(r#"{"direction":"produce","turns":[{"asked":{"values":{"pronoun":"I"}},"typed":"I is here."}]}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = body_json(response).await;
        assert_ne!(body["stitch"], "new", "an answered formula should have left the new pile");
        assert!(body["due"].is_string());
    }

    #[tokio::test]
    async fn an_answer_delivered_twice_is_taken_once_and_told_the_same() {
        // The reply to the first delivery was lost on the way back, and the
        // phone sent the answer again: one review, one reply.
        let dir = web_root();
        let pool = taught().await;
        let app = open(pool.clone(), &dir);
        let body = sent(r#"{"direction":"produce","turns":[{"asked":{"values":{"pronoun":"I"}},"typed":"I is here."}],"duration_ms":4000}"#);
        let mut replies = Vec::new();
        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(json_request("POST", "/api/formulas/be-present/review", &body))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            replies.push(body_json(response).await);
        }
        assert_eq!(replies[0], replies[1], "the second delivery should be told what the first was");
        let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review").fetch_one(&pool).await.unwrap();
        assert_eq!(reviews, 1, "an answer delivered twice was recorded twice");
        let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM attempt").fetch_one(&pool).await.unwrap();
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn an_answer_is_taken_at_the_moment_its_device_gave_it() {
        let dir = web_root();
        let pool = taught().await;
        let app = open(pool.clone(), &dir);
        // Given on a train an hour ago, delivered now.
        let then = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let body = format!(
            r#"{{"answer_id":"train-1","answered_at":"{then}","direction":"produce","turns":[{{"asked":{{"values":{{"pronoun":"I"}}}},"typed":"I is here."}}]}}"#
        );
        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/formulas/be-present/review", &body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let at: String = sqlx::query_scalar("SELECT reviewed_at FROM review").fetch_one(&pool).await.unwrap();
        assert_eq!(at, then, "the answer should be where it happened, not where it arrived");

        // A clock that runs ahead cannot answer from the future.
        let body = r#"{"answer_id":"ahead-1","answered_at":"2999-01-01T00:00:00Z","direction":"recognise","rating":"good"}"#;
        let before = Utc::now();
        let response = app.oneshot(json_request("POST", "/api/formulas/be-present/review", body)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let at: String = sqlx::query_scalar("SELECT max(reviewed_at) FROM review").fetch_one(&pool).await.unwrap();
        let at = crate::study::parse_stamp(&at).unwrap();
        assert!(at >= before && at <= Utc::now(), "an answer from the future was not taken at its arrival: {at}");
    }

    #[tokio::test]
    async fn an_answer_without_its_identity_or_its_moment_is_refused() {
        let dir = web_root();
        let pool = taught().await;
        let app = open(pool.clone(), &dir);
        let now = Utc::now().to_rfc3339();
        for body in [
            r#"{"direction":"recognise","rating":"good"}"#.to_string(),
            r#"{"answer_id":"no-moment","direction":"recognise","rating":"good"}"#.to_string(),
            format!(r#"{{"answer_id":"  ","answered_at":"{now}","direction":"recognise","rating":"good"}}"#),
            format!(
                r#"{{"answer_id":"{}","answered_at":"{now}","direction":"recognise","rating":"good"}}"#,
                "x".repeat(65)
            ),
        ] {
            let response = app
                .clone()
                .oneshot(json_request("POST", "/api/formulas/be-present/review", &body))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        }
        let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review").fetch_one(&pool).await.unwrap();
        assert_eq!(reviews, 0, "a refused answer was recorded");
    }

    #[tokio::test]
    async fn a_typed_turn_is_no_longer_checked_by_the_server_alone() {
        // The check runs on the device, from the same code (crates/check);
        // the endpoint that did it is gone rather than kept as a second way.
        let dir = web_root();
        let response = open(taught().await, &dir)
            .oneshot(json_request(
                "POST",
                "/api/formulas/be-present/check",
                r#"{"asked":{"values":{"pronoun":"I"}},"typed":"I are here"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_said_answer_is_graded_by_its_turns_and_kept_with_them() {
        let dir = web_root();
        let pool = taught().await;
        let response = open(pool.clone(), &dir)
            .oneshot(json_request(
                "POST",
                "/api/formulas/be-present/review",
                &sent(
                    r#"{"direction":"produce","easy":true,"turns":[
                    {"asked":{"values":{"pronoun":"I"}},"typed":"I is here."},
                    {"asked":{"values":{"pronoun":"I"}},"typed":"I here is"}]}"#,
                ),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["rating"], "again", "one turn of two in the wrong order is half the turns slipping");

        let kept: Vec<(i64, String, Option<String>)> = sqlx::query_as("SELECT correct, typed, failure FROM attempt ORDER BY position")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(
            kept,
            vec![(1, "I is here.".to_string(), None), (0, "I here is".to_string(), Some("order".to_string()))]
        );
    }

    #[tokio::test]
    async fn a_said_answer_with_no_turns_is_refused() {
        let dir = web_root();
        let response = open(taught().await, &dir)
            .oneshot(json_request(
                "POST",
                "/api/formulas/be-present/review",
                &sent(r#"{"direction":"produce","turns":[]}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn own_words_are_added_listed_and_removed() {
        let dir = web_root();
        let app = open(taught().await, &dir);
        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/own-words", r#"{"kind":"verb","native":"плавать","target":"swim"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let id = body_json(response).await["id"].as_i64().unwrap();

        let response = app
            .clone()
            .oneshot(json_request("POST", "/api/own-words", r#"{"kind":"verb","native":"плыть","target":"swim"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "a word the learner already has");

        let response = app.clone().oneshot(Request::get("/api/own-words").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(body_json(response).await["words"][0]["target"], "swim");

        let response = app
            .clone()
            .oneshot(Request::delete(format!("/api/own-words/{id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = app.oneshot(Request::get("/api/own-words").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(body_json(response).await["words"].as_array().map(Vec::len), Some(0));
    }

    #[tokio::test]
    async fn answering_a_formula_that_does_not_exist_is_a_404() {
        let dir = web_root();
        let response = open(taught().await, &dir)
            .oneshot(json_request(
                "POST",
                "/api/formulas/nonsense/review",
                &sent(r#"{"direction":"recognise","rating":"good"}"#),
            ))
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
