//! The learner model as one file.
//!
//! Everything the tutor knows about its learner - each card and every answer
//! behind it with the turns typed for it, the words they brought, the voices
//! they chose, the inventory they went through - as one JSON document. It
//! moves a learner to another stand, shows a fictional one in the docs, and
//! tells the model as it stood on any day: a snapshot is the review log up
//! to that day played through the scheduler again, never a stored copy.
//!
//! Material is not in it. Packs are files of their own and the lexicon is
//! built into the binary; the model names the packs it was learnt on, and
//! keys everything on what outlives a reload - a formula's id, a word's
//! lemma, a sentence's text.
//!
//! The format is a published contract: `docs/site/public/schema/`, served by
//! the docs site at [`SCHEMA`]. What the schema refuses, [`import`] refuses,
//! and import checks what a schema cannot say besides - nothing listed
//! twice, every language named one the file lists. A change that breaks a
//! file of this format is a new [`FORMAT`].

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::answer::Failure;
use crate::pack::{self, OwnKind};
use crate::scheduling::{Direction, Kind, Rating, Scheduler, Source, Stitch};
use crate::study::{self, Counts, Progress};
use crate::voice::Engine;
use crate::words::{Held, Standing};

/// The version of the file's shape. Read files of this one; a file of
/// another is refused rather than guessed at.
pub const FORMAT: u32 = 1;

/// Where the JSON Schema of [`FORMAT`] is published, and what a file says
/// it follows.
pub const SCHEMA: &str = "https://lacodda.github.io/hilvan/schema/learner-model-1.json";

/// A fictional learner, three weeks into English from Russian on the shipped
/// pack: what `hilvan load-demo` loads, what the docs are shown with. Made by
/// `cargo test --test demo -- --ignored`, which drives the tutor's own code
/// through those weeks.
pub const DEMO: &str = include_str!("../demo/learner.json");

/// The learner model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// [`SCHEMA`]: what the file follows.
    #[serde(rename = "$schema")]
    pub schema: String,
    /// [`FORMAT`].
    pub format: u32,
    /// When the file was written.
    pub exported_at: DateTime<Utc>,
    /// The moment the model is told as of: when it was written, or an
    /// earlier one for a snapshot.
    pub as_of: DateTime<Utc>,
    /// What wrote it.
    pub app: App,
    pub languages: Vec<Language>,
    /// The packs the learner was learning from, and their versions.
    pub packs: Vec<PackRef>,
    /// Every card that has been answered, with its answers; a card never
    /// answered is new, and a loaded pack makes those.
    pub cards: Vec<CardRecord>,
    /// The sentence each word is heard in.
    pub anchors: Vec<Anchor>,
    pub own_words: Vec<OwnWord>,
    /// The voice chosen for a language, where one was.
    pub voices: Vec<VoiceChoice>,
    /// Every answer of the inventory, either way.
    pub inventory: Vec<Inventoried>,
}

/// The program that wrote a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct App {
    pub name: String,
    pub version: String,
}

/// A language the learner has to do with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Language {
    /// ISO 639-1.
    pub code: String,
    /// Whether it is the learner's own.
    pub native: bool,
}

/// A pack, by id and version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRef {
    pub id: String,
    pub version: i64,
}

/// One card: what it schedules, where it stands, and the answers that put
/// it there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardRecord {
    /// `formula`, `formula-recognise` or `word`.
    pub kind: String,
    /// A formula's id, or a word's: `en:house`.
    pub subject: String,
    pub state: Stitch,
    pub stability: f64,
    pub difficulty: f64,
    pub due: DateTime<Utc>,
    pub reps: i64,
    pub lapses: i64,
    pub last_reviewed: Option<DateTime<Utc>>,
    pub introduced_at: Option<DateTime<Utc>>,
    /// Oldest first; never empty.
    pub reviews: Vec<ReviewRecord>,
}

/// One answer, and the card it left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRecord {
    pub at: DateTime<Utc>,
    pub rating: Rating,
    pub source: Source,
    pub elapsed_days: f64,
    pub stability: f64,
    pub difficulty: f64,
    pub duration_ms: Option<i64>,
    /// The turns typed for it, in order; empty for an answer not typed.
    pub attempts: Vec<AttemptRecord>,
}

/// One typed turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub prompt: String,
    pub expected: String,
    pub typed: String,
    pub correct: bool,
    pub failure: Option<Failure>,
    pub missed: Vec<String>,
}

/// The sentence a word is heard in, by what it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub word: String,
    pub sentence: SentenceRef,
}

/// A sentence, by language and text: what outlives a reload of its pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentenceRef {
    pub language: String,
    pub text: String,
}

/// One of the learner's own words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnWord {
    pub language: String,
    pub kind: OwnKind,
    pub native: String,
    pub target: String,
    pub created_at: DateTime<Utc>,
}

/// The voice a language is spoken in, as the learner chose it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceChoice {
    pub language: String,
    pub engine: Engine,
    pub voice: String,
}

/// One answer of the inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventoried {
    pub word: String,
    pub known: bool,
    pub at: DateTime<Utc>,
}

impl Model {
    /// Reads a model file.
    ///
    /// # Errors
    ///
    /// Fails on text that is not a model of [`FORMAT`]: JSON of another
    /// shape, a field the format does not have, or what [`Model::check`]
    /// refuses.
    pub fn parse(text: &str) -> Result<Self> {
        // The format is read first, so a file of another one is named as
        // such rather than as a field that does not fit.
        let probe: serde_json::Value = serde_json::from_str(text).context("the file is not JSON")?;
        let format = probe.get("format").and_then(serde_json::Value::as_u64);
        ensure!(
            format == Some(u64::from(FORMAT)),
            "the file is not a learner model of format {FORMAT} (its format: {})",
            format.map_or_else(|| "none".to_string(), |format| format.to_string())
        );
        let model: Self = serde_json::from_value(probe).context("the file does not have the shape of a learner model")?;
        model.check()?;
        Ok(model)
    }

    /// Whether the model holds what the format promises: nothing twice,
    /// every name one the tutor knows, every answer in order.
    ///
    /// # Errors
    ///
    /// Names the first thing that does not hold.
    pub fn check(&self) -> Result<()> {
        ensure!(self.schema == SCHEMA, "the file follows {:?}, not {SCHEMA}", self.schema);
        ensure!(self.format == FORMAT, "the file is of format {}, not {FORMAT}", self.format);

        let mut languages = BTreeSet::new();
        for language in &self.languages {
            ensure!(is_language(&language.code), "{:?} is not a language code", language.code);
            ensure!(languages.insert(language.code.as_str()), "language {} is listed twice", language.code);
        }
        ensure!(
            self.languages.iter().filter(|language| language.native).count() <= 1,
            "more than one language is the learner's own"
        );
        let known_language = |code: &str, what: &str| listed(&languages, code, what);

        let mut cards = BTreeSet::new();
        for card in &self.cards {
            ensure!(
                cards.insert((card.kind.as_str(), card.subject.as_str())),
                "card {} {} is listed twice",
                card.kind,
                card.subject
            );
            card.check(&languages)?;
        }

        let mut anchored = BTreeSet::new();
        for anchor in &self.anchors {
            ensure!(anchored.insert(anchor.word.as_str()), "word {} has two anchors", anchor.word);
            known_language(&anchor.sentence.language, &format!("the anchor of {}", anchor.word))?;
        }

        let mut own = BTreeSet::new();
        for word in &self.own_words {
            known_language(&word.language, &format!("own word {:?}", word.target))?;
            ensure!(
                !word.native.trim().is_empty() && !word.target.trim().is_empty(),
                "an own word is empty on one side"
            );
            ensure!(
                own.insert((word.language.as_str(), word.kind.as_str(), word.target.as_str())),
                "own word {:?} is listed twice",
                word.target
            );
        }

        let mut voiced = BTreeSet::new();
        for voice in &self.voices {
            known_language(&voice.language, &format!("the voice {:?}", voice.voice))?;
            ensure!(voiced.insert(voice.language.as_str()), "language {} has two voices", voice.language);
        }

        let mut inventoried = BTreeSet::new();
        for answer in &self.inventory {
            let (language, lemma) = answer
                .word
                .split_once(':')
                .with_context(|| format!("inventory word {:?} is not language:lemma", answer.word))?;
            ensure!(is_lemma(lemma), "inventory word {:?} is not language:lemma, lowercase", answer.word);
            known_language(language, &format!("inventory word {}", answer.word))?;
            ensure!(inventoried.insert(answer.word.as_str()), "inventory word {} is answered twice", answer.word);
        }
        Ok(())
    }

    /// The same model moved in time by `by`: every moment in it, kept apart
    /// as they were. What puts the demo learner's last day on today.
    #[must_use]
    pub fn shifted(mut self, by: Duration) -> Self {
        self.exported_at += by;
        self.as_of += by;
        for card in &mut self.cards {
            card.due += by;
            card.last_reviewed = card.last_reviewed.map(|at| at + by);
            card.introduced_at = card.introduced_at.map(|at| at + by);
            for review in &mut card.reviews {
                review.at += by;
            }
        }
        for word in &mut self.own_words {
            word.created_at += by;
        }
        for answer in &mut self.inventory {
            answer.at += by;
        }
        self
    }
}

impl CardRecord {
    /// Whether one card holds what the format promises, its languages among
    /// those the file lists.
    fn check(&self, languages: &BTreeSet<&str>) -> Result<()> {
        let what = format!("card {} {}", self.kind, self.subject);
        let kind = Kind::parse(&self.kind).with_context(|| what.clone())?;
        ensure!(!self.subject.trim().is_empty(), "a {} card has no subject", self.kind);
        if kind == Kind::Word {
            let (language, lemma) = self
                .subject
                .split_once(':')
                .with_context(|| format!("word {:?} is not language:lemma", self.subject))?;
            ensure!(is_lemma(lemma), "word {:?} is not language:lemma, lowercase", self.subject);
            listed(languages, language, &format!("word {}", self.subject))?;
        }
        ensure!(self.state != Stitch::New, "{what} has answers and still says it is new");
        ensure!(!self.reviews.is_empty(), "{what} has no answers");
        ensure!(self.reps >= 0 && self.lapses >= 0, "{what} counts below zero");
        for number in [self.stability, self.difficulty] {
            ensure!(number.is_finite() && number >= 0.0, "{what} holds {number}");
        }
        for pair in self.reviews.windows(2) {
            ensure!(pair[0].at <= pair[1].at, "the answers of {what} are out of order");
        }
        for review in &self.reviews {
            for number in [review.elapsed_days, review.stability, review.difficulty] {
                ensure!(number.is_finite() && number >= 0.0, "an answer of {what} holds {number}");
            }
            ensure!(review.duration_ms.is_none_or(|ms| ms >= 0), "an answer of {what} took less than no time");
            for attempt in &review.attempts {
                ensure!(
                    attempt.correct == attempt.failure.is_none(),
                    "a turn of {what} is right and failed at once, or neither"
                );
            }
        }
        Ok(())
    }
}

/// Whether a language named somewhere in the file is one it lists.
fn listed(languages: &BTreeSet<&str>, code: &str, what: &str) -> Result<()> {
    ensure!(
        languages.contains(code),
        "{what} is in {code:?}, which the file does not list among its languages"
    );
    Ok(())
}

fn is_language(code: &str) -> bool {
    (2..=3).contains(&code.len()) && code.bytes().all(|byte| byte.is_ascii_lowercase())
}

/// The lemma half of a word id, as `pack::word_id` writes it: lowercase.
/// Checked for ASCII capitals only, as the schema's pattern does.
fn is_lemma(lemma: &str) -> bool {
    !lemma.is_empty() && !lemma.bytes().any(|byte| byte.is_ascii_uppercase())
}

/// The model as it stands.
///
/// # Errors
///
/// Fails when the database rejects a query or holds what the model cannot
/// say - a state, a rating or a moment written by something else.
pub async fn export(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Model> {
    read(pool, None, now).await
}

/// The model as it stood at `at`: the answers given by then, played through
/// the scheduler again, and what else the learner had by then.
///
/// A voice choice keeps no date, so a snapshot carries the voices chosen now.
///
/// # Errors
///
/// As [`export`].
pub async fn as_of(pool: &SqlitePool, at: DateTime<Utc>, now: DateTime<Utc>) -> Result<Model> {
    read(pool, Some(at), now).await
}

async fn read(pool: &SqlitePool, at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Result<Model> {
    let languages = sqlx::query("SELECT code, is_native FROM language ORDER BY is_native DESC, code")
        .fetch_all(pool)
        .await
        .context("failed to read the languages")?
        .iter()
        .map(|row| Language {
            code: row.get("code"),
            native: row.get::<i64, _>("is_native") == 1,
        })
        .collect();
    let packs = sqlx::query("SELECT id, version FROM pack ORDER BY id")
        .fetch_all(pool)
        .await
        .context("failed to read the packs")?
        .iter()
        .map(|row| PackRef {
            id: row.get("id"),
            version: row.get("version"),
        })
        .collect();

    let answers = read_answers(pool, at).await?;
    let cards = read_cards(pool, at, answers).await?;
    let anchors = read_anchors(pool, &cards).await?;
    let Belongings { own_words, voices, inventory } = read_belongings(pool, at).await?;

    Ok(Model {
        schema: SCHEMA.to_string(),
        format: FORMAT,
        exported_at: now,
        as_of: at.unwrap_or(now),
        app: App {
            name: "hilvan".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        languages,
        packs,
        cards,
        anchors,
        own_words,
        voices,
        inventory,
    })
}

/// Every answer given by `at`, with the turns typed for it, by card.
async fn read_answers(pool: &SqlitePool, at: Option<DateTime<Utc>>) -> Result<HashMap<i64, Vec<ReviewRecord>>> {
    let by_then = |moment: DateTime<Utc>| at.is_none_or(|at| moment <= at);
    let mut attempts: HashMap<i64, Vec<AttemptRecord>> = HashMap::new();
    for row in sqlx::query("SELECT review_id, prompt, expected, typed, correct, failure, missed FROM attempt ORDER BY review_id, position")
        .fetch_all(pool)
        .await
        .context("failed to read the typed turns")?
    {
        let failure = row.get::<Option<String>, _>("failure").map(|word| failure(&word)).transpose()?;
        let missed: String = row.get("missed");
        attempts.entry(row.get("review_id")).or_default().push(AttemptRecord {
            prompt: row.get("prompt"),
            expected: row.get("expected"),
            typed: row.get("typed"),
            correct: row.get::<i64, _>("correct") == 1,
            failure,
            missed: serde_json::from_str(&missed).with_context(|| format!("the missed words of a turn are not JSON: {missed}"))?,
        });
    }

    let mut reviews: HashMap<i64, Vec<ReviewRecord>> = HashMap::new();
    for row in sqlx::query(
        "SELECT id, card_id, rating, reviewed_at, elapsed_days, stability, difficulty, duration_ms, source
         FROM review ORDER BY card_id, reviewed_at, id",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the answers")?
    {
        let moment = study::parse_stamp(&row.get::<String, _>("reviewed_at"))?;
        if !by_then(moment) {
            continue;
        }
        reviews.entry(row.get("card_id")).or_default().push(ReviewRecord {
            at: moment,
            rating: Rating::from_i64(row.get("rating"))?,
            source: Source::parse(&row.get::<String, _>("source"))?,
            elapsed_days: row.get("elapsed_days"),
            stability: row.get("stability"),
            difficulty: row.get("difficulty"),
            duration_ms: row.get("duration_ms"),
            attempts: attempts.remove(&row.get::<i64, _>("id")).unwrap_or_default(),
        });
    }

    Ok(reviews)
}

/// The cards answered by `at`: as they stand, or - for a moment before now -
/// as their answers by then leave them.
async fn read_cards(pool: &SqlitePool, at: Option<DateTime<Utc>>, mut reviews: HashMap<i64, Vec<ReviewRecord>>) -> Result<Vec<CardRecord>> {
    let mut cards = Vec::new();
    for row in sqlx::query(
        "SELECT id, kind, subject_id, state, stability, difficulty, due, reps, lapses, last_reviewed, introduced_at
         FROM card ORDER BY kind, subject_id",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the cards")?
    {
        let Some(answers) = reviews.remove(&row.get::<i64, _>("id")) else { continue };
        let kind: String = row.get("kind");
        let subject: String = row.get("subject_id");
        let card = if at.is_some() {
            // The card as these answers left it: the log is the truth, and
            // the row only its last step.
            let card = Scheduler::new()
                .replay(answers.iter().map(|review| (review.rating, review.at)))
                .with_context(|| format!("card {kind} {subject} has answers and nothing to replay"))?;
            CardRecord {
                kind,
                subject,
                state: card.stitch,
                stability: card.stability,
                difficulty: card.difficulty,
                due: card.due,
                reps: card.reps,
                lapses: card.lapses,
                last_reviewed: card.last_reviewed,
                introduced_at: answers.first().map(|review| review.at),
                reviews: answers,
            }
        } else {
            let stamp =
                |column: &str| -> Result<Option<DateTime<Utc>>> { row.get::<Option<String>, _>(column).map(|text| study::parse_stamp(&text)).transpose() };
            CardRecord {
                state: Stitch::parse(&row.get::<String, _>("state"))?,
                stability: row.get("stability"),
                difficulty: row.get("difficulty"),
                due: stamp("due")?.with_context(|| format!("card {kind} {subject} has no due date"))?,
                reps: row.get("reps"),
                lapses: row.get("lapses"),
                last_reviewed: stamp("last_reviewed")?,
                introduced_at: stamp("introduced_at")?,
                kind,
                subject,
                reviews: answers,
            }
        };
        cards.push(card);
    }

    Ok(cards)
}

/// The anchors of the words among `cards`.
async fn read_anchors(pool: &SqlitePool, cards: &[CardRecord]) -> Result<Vec<Anchor>> {
    let answered: BTreeSet<&str> = cards
        .iter()
        .filter(|card| card.kind == Kind::Word.as_str())
        .map(|card| card.subject.as_str())
        .collect();
    let anchors = sqlx::query("SELECT a.word_id, s.language, s.text FROM anchor a JOIN sentence s ON s.id = a.sentence_id ORDER BY a.word_id")
        .fetch_all(pool)
        .await
        .context("failed to read the anchors")?
        .iter()
        // An anchor is fixed by a word's first answer: a snapshot from
        // before it has none.
        .filter(|row| answered.contains(row.get::<&str, _>("word_id")))
        .map(|row| Anchor {
            word: row.get("word_id"),
            sentence: SentenceRef {
                language: row.get("language"),
                text: row.get("text"),
            },
        })
        .collect();

    Ok(anchors)
}

/// What else the learner has, besides cards: their own words, the voices
/// they chose, their answers in the inventory.
struct Belongings {
    own_words: Vec<OwnWord>,
    voices: Vec<VoiceChoice>,
    inventory: Vec<Inventoried>,
}

/// What the learner had by `at`, besides cards.
async fn read_belongings(pool: &SqlitePool, at: Option<DateTime<Utc>>) -> Result<Belongings> {
    let by_then = |moment: DateTime<Utc>| at.is_none_or(|at| moment <= at);
    let mut own_words = Vec::new();
    for row in sqlx::query("SELECT language, kind, native, target, created_at FROM own_word ORDER BY created_at, id")
        .fetch_all(pool)
        .await
        .context("failed to read the learner's own words")?
    {
        let created_at = study::parse_stamp(&row.get::<String, _>("created_at"))?;
        if by_then(created_at) {
            own_words.push(OwnWord {
                language: row.get("language"),
                kind: OwnKind::parse(&row.get::<String, _>("kind"))?,
                native: row.get("native"),
                target: row.get("target"),
                created_at,
            });
        }
    }

    let voices = sqlx::query("SELECT language, engine, voice FROM voice_choice ORDER BY language")
        .fetch_all(pool)
        .await
        .context("failed to read the chosen voices")?
        .iter()
        .map(|row| {
            Ok(VoiceChoice {
                language: row.get("language"),
                engine: Engine::parse(&row.get::<String, _>("engine"))?,
                voice: row.get("voice"),
            })
        })
        .collect::<Result<_>>()?;

    let mut inventory = Vec::new();
    for row in sqlx::query("SELECT word_id, known, answered_at FROM inventory ORDER BY answered_at, word_id")
        .fetch_all(pool)
        .await
        .context("failed to read the inventory")?
    {
        let answered_at = study::parse_stamp(&row.get::<String, _>("answered_at"))?;
        if by_then(answered_at) {
            inventory.push(Inventoried {
                word: row.get("word_id"),
                known: row.get::<i64, _>("known") == 1,
                at: answered_at,
            });
        }
    }

    Ok(Belongings { own_words, voices, inventory })
}

fn failure(word: &str) -> Result<Failure> {
    Ok(match word {
        "formula" => Failure::Formula,
        "order" => Failure::Order,
        "word" => Failure::Word,
        other => bail!("{other:?} is not a failure of a turn"),
    })
}

/// Whether the tutor holds anything of a learner: an answer, an own word, a
/// chosen voice, an answer of the inventory. A stand with only packs loaded
/// holds nothing.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn holds_a_learner(pool: &SqlitePool) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM review) OR EXISTS (SELECT 1 FROM own_word)
             OR EXISTS (SELECT 1 FROM voice_choice) OR EXISTS (SELECT 1 FROM inventory)",
    )
    .fetch_one(pool)
    .await
    .context("failed to ask whether the tutor holds a learner")
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub cards: usize,
    pub reviews: usize,
    /// Anchors whose sentence no loaded pack shows: the word is heard in the
    /// first sentence it is met in until it is answered again.
    pub anchors_dropped: usize,
    /// Packs the model was learnt on that are not loaded here, or loaded at
    /// another version: `en-from-ru v5`.
    pub packs_missing: Vec<String>,
}

/// Replaces the learner model with `model`.
///
/// Everything of the learner goes - cards, answers, anchors, own words,
/// voices, the inventory - and the model's takes its place, in one
/// transaction; the loaded material stays, and every formula and word of it
/// the model never answered gets a new card. What was there is the caller's
/// to keep first: the command line writes it to a file before calling this.
///
/// # Errors
///
/// Fails when the model does not pass [`Model::check`], or when the database
/// rejects a statement.
pub async fn import(pool: &SqlitePool, model: &Model, now: DateTime<Utc>) -> Result<Imported> {
    model.check()?;
    let mut tx = pool.begin().await.context("failed to open a transaction")?;

    for statement in [
        "DELETE FROM attempt",
        "DELETE FROM review",
        "DELETE FROM anchor",
        "DELETE FROM card",
        "DELETE FROM own_word",
        "DELETE FROM voice_choice",
        "DELETE FROM inventory",
    ] {
        sqlx::query(statement)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to clear the learner: {statement}"))?;
    }

    for language in &model.languages {
        // The name stays the code until a language needs one, as a pack
        // load leaves it.
        sqlx::query("INSERT INTO language (code, name, is_native) VALUES (?, ?, ?) ON CONFLICT (code) DO NOTHING")
            .bind(&language.code)
            .bind(&language.code)
            .bind(i64::from(language.native))
            .execute(&mut *tx)
            .await
            .context("failed to record a language")?;
    }

    let reviews = write_cards(&mut tx, &model.cards).await?;
    let anchors_dropped = write_belongings(&mut tx, model).await?;

    pack::give_every_subject_a_card(&mut tx, now).await?;

    let loaded: BTreeMap<String, i64> = sqlx::query_as::<_, (String, i64)>("SELECT id, version FROM pack")
        .fetch_all(&mut *tx)
        .await
        .context("failed to read the loaded packs")?
        .into_iter()
        .collect();
    tx.commit().await.context("failed to commit the learner model")?;

    Ok(Imported {
        cards: model.cards.len(),
        reviews,
        anchors_dropped,
        packs_missing: model
            .packs
            .iter()
            .filter(|pack| loaded.get(&pack.id) != Some(&pack.version))
            .map(|pack| format!("{} v{}", pack.id, pack.version))
            .collect(),
    })
}

/// Writes the cards with their answers and turns; how many answers.
async fn write_cards(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, cards: &[CardRecord]) -> Result<usize> {
    let mut reviews = 0;
    for card in cards {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO card (kind, subject_id, state, stability, difficulty, due, reps, lapses, last_reviewed, introduced_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(&card.kind)
        .bind(&card.subject)
        .bind(card.state.as_str())
        .bind(card.stability)
        .bind(card.difficulty)
        .bind(stamp(card.due))
        .bind(card.reps)
        .bind(card.lapses)
        .bind(card.last_reviewed.map(stamp))
        .bind(card.introduced_at.map(stamp))
        .fetch_one(&mut **tx)
        .await
        .with_context(|| format!("failed to write card {} {}", card.kind, card.subject))?;

        for review in &card.reviews {
            let review_id: i64 = sqlx::query_scalar(
                "INSERT INTO review (card_id, rating, reviewed_at, elapsed_days, stability, difficulty, duration_ms, source)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(id)
            .bind(review.rating.as_i64())
            .bind(stamp(review.at))
            .bind(review.elapsed_days)
            .bind(review.stability)
            .bind(review.difficulty)
            .bind(review.duration_ms)
            .bind(review.source.as_str())
            .fetch_one(&mut **tx)
            .await
            .with_context(|| format!("failed to write an answer of {} {}", card.kind, card.subject))?;
            reviews += 1;

            for (position, attempt) in review.attempts.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO attempt (review_id, position, prompt, expected, typed, correct, failure, missed)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(review_id)
                .bind(i64::try_from(position).unwrap_or(i64::MAX))
                .bind(&attempt.prompt)
                .bind(&attempt.expected)
                .bind(&attempt.typed)
                .bind(i64::from(attempt.correct))
                .bind(attempt.failure.map(Failure::as_str))
                .bind(serde_json::to_string(&attempt.missed).context("failed to write the missed words of a turn")?)
                .execute(&mut **tx)
                .await
                .with_context(|| format!("failed to write a typed turn of {}", card.subject))?;
            }
        }
    }

    Ok(reviews)
}

/// Writes the anchors, own words, voices and inventory; how many anchors
/// found no sentence to stand in.
async fn write_belongings(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, model: &Model) -> Result<usize> {
    let mut anchors_dropped = 0;
    for anchor in &model.anchors {
        let written = sqlx::query(
            "INSERT INTO anchor (word_id, sentence_id)
             SELECT ?, id FROM sentence WHERE language = ? AND text = ?",
        )
        .bind(&anchor.word)
        .bind(&anchor.sentence.language)
        .bind(&anchor.sentence.text)
        .execute(&mut **tx)
        .await
        .with_context(|| format!("failed to write the anchor of {}", anchor.word))?;
        if written.rows_affected() == 0 {
            anchors_dropped += 1;
        }
    }

    for word in &model.own_words {
        sqlx::query("INSERT INTO own_word (language, kind, native, target, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&word.language)
            .bind(word.kind.as_str())
            .bind(&word.native)
            .bind(&word.target)
            .bind(stamp(word.created_at))
            .execute(&mut **tx)
            .await
            .with_context(|| format!("failed to write own word {:?}", word.target))?;
    }
    for voice in &model.voices {
        sqlx::query("INSERT INTO voice_choice (language, engine, voice) VALUES (?, ?, ?)")
            .bind(&voice.language)
            .bind(voice.engine.as_str())
            .bind(&voice.voice)
            .execute(&mut **tx)
            .await
            .with_context(|| format!("failed to write the voice of {}", voice.language))?;
    }
    for answer in &model.inventory {
        sqlx::query("INSERT INTO inventory (word_id, known, answered_at) VALUES (?, ?, ?)")
            .bind(&answer.word)
            .bind(answer.known)
            .bind(stamp(answer.at))
            .execute(&mut **tx)
            .await
            .with_context(|| format!("failed to write the inventory answer for {}", answer.word))?;
    }

    Ok(anchors_dropped)
}

/// A moment, written the way the tutor writes it, `to_rfc3339`: the queries
/// compare moments as text, and "...Z" against "...+00:00" would order a
/// review wrongly by a character, not by a second.
fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339()
}

/// Loads the demo learner over a tutor that holds no learner, moved in time
/// so that the day it is told as of is today.
///
/// # Errors
///
/// Refuses a tutor that holds a learner already - a demo is never what is
/// meant over someone's own history - and fails as [`import`] does.
pub async fn load_demo(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Imported> {
    ensure!(
        !holds_a_learner(pool).await?,
        "this tutor already holds a learner; the demo goes onto a database of its own"
    );
    ensure!(
        !sqlx::query_scalar::<_, bool>("SELECT NOT EXISTS (SELECT 1 FROM pack)").fetch_one(pool).await?,
        "no pack is loaded: run `hilvan load-pack packs/en-from-ru/pack.toml` first, the demo learner learns from it"
    );
    let demo = Model::parse(DEMO).context("the demo learner built into hilvan does not read")?;
    // Whole days: its evenings stay evenings, and its last one is the
    // evening before today.
    let by = Duration::days((now.date_naive() - demo.as_of.date_naive()).num_days());
    import(pool, &demo.shifted(by), now).await
}

/// Where the learner stood at a moment, counted the way Today counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub at: DateTime<Utc>,
    /// Formulas by state, both directions.
    pub progress: Progress,
    /// Words by state and by level; `waiting` is not told of the past.
    pub words: Standing,
    /// Answers given by then, asked ones.
    pub answers: i64,
}

/// Where the learner stood at `at`, from the model as of then and the
/// material loaded now.
///
/// # Errors
///
/// Fails as [`as_of`] does.
pub async fn standing_at(pool: &SqlitePool, at: DateTime<Utc>) -> Result<Snapshot> {
    let model = as_of(pool, at, at).await?;
    let held: HashMap<(&str, &str), (Stitch, bool)> = model
        .cards
        .iter()
        .map(|card| {
            let declared = card.reviews.iter().all(|review| review.source == Source::Declared);
            ((card.kind.as_str(), card.subject.as_str()), (card.state, declared))
        })
        .collect();

    let mut progress = Progress::default();
    for id in sqlx::query_scalar::<_, String>("SELECT id FROM formula ORDER BY id")
        .fetch_all(pool)
        .await
        .context("failed to read the formulas")?
    {
        for direction in Direction::ALL {
            let stitch = held.get(&(direction.kind(), id.as_str())).map_or(Stitch::New, |(stitch, _)| *stitch);
            let counts: &mut Counts = match direction {
                Direction::Produce => &mut progress.produce,
                Direction::Recognise => &mut progress.recognise,
            };
            counts.add(stitch);
        }
    }

    let taught: BTreeSet<String> = sqlx::query_scalar("SELECT id FROM word")
        .fetch_all(pool)
        .await
        .context("failed to read the words")?
        .into_iter()
        .collect();
    let mut words: Vec<Held> = taught
        .iter()
        .map(|id| {
            let (stitch, declared) = held.get(&(Kind::Word.as_str(), id.as_str())).copied().unwrap_or((Stitch::New, false));
            Held {
                id: id.clone(),
                stitch,
                taught: true,
                declared,
            }
        })
        .collect();
    words.extend(
        model
            .cards
            .iter()
            .filter(|card| card.kind == Kind::Word.as_str() && !taught.contains(&card.subject))
            .map(|card| Held {
                id: card.subject.clone(),
                stitch: card.state,
                taught: false,
                declared: card.reviews.iter().all(|review| review.source == Source::Declared),
            }),
    );

    Ok(Snapshot {
        at,
        progress,
        words: Standing::tally(&words),
        answers: i64::try_from(
            model
                .cards
                .iter()
                .flat_map(|card| &card.reviews)
                .filter(|review| review.source == Source::Asked)
                .count(),
        )
        .unwrap_or(i64::MAX),
    })
}

/// When the learner first answered anything the tutor asked, if they have.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn began(pool: &SqlitePool) -> Result<Option<DateTime<Utc>>> {
    let first: Option<String> = sqlx::query_scalar("SELECT reviewed_at FROM review WHERE source = 'asked' ORDER BY reviewed_at LIMIT 1")
        .fetch_optional(pool)
        .await
        .context("failed to read the first answer")?;
    first.map(|text| study::parse_stamp(&text)).transpose()
}
