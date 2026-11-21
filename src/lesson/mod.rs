//! Audio lessons: the day's queue, said aloud the way Pimsleur's courses are.
//!
//! "Today" comes in two forms: cards, or a lesson for the ears - one queue,
//! answered either way. A lesson is made on the stand when the learner asks
//! for one: the phrases come from the queue in its own order (what is due
//! first) and then from what is about to come back; the plan - which lines,
//! in what order, with how much silence - is the core's
//! ([`hilvan_core::lesson`]); every line is said through the audio cache,
//! the learner's own language by the teacher's voice and the language being
//! learnt by the native one; and [`splice`] makes one MP3 of it, with a
//! chapter for every phrase. Before a credit of the paid voice is spent, the
//! lesson is priced: one that the month's credits cannot pay for is not
//! started.
//!
//! The learner takes the file on the road and grades what they heard
//! afterwards; those answers come back like any others ([`crate::study::review_heard`]).

pub mod splice;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Datelike, Utc};
use hilvan_core::lesson::{self as plan, Asks, Chapter, Item, Line, Part, Voice};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tokio::sync::Mutex;

use crate::scheduling::Direction;
use crate::study::{self, CardRef, Due, Subject};
use crate::voice::{Tempo, Voices};

/// The lengths a lesson can be asked for, in minutes: a short walk, the
/// usual one, a long one.
pub const LENGTHS: [u32; 3] = [15, 25, 40];

/// The length a learner who has not chosen gets.
pub const DEFAULT_MINUTES: u32 = 25;

/// Where the chosen length is kept.
const SETTING: &str = "lesson.minutes";

/// How many finished lessons are kept: the one on the phone, the one
/// before it, and one more. Each is a few megabytes in the database.
const KEPT: i64 = 3;

/// How long a line not yet said is guessed to last, a character at a time,
/// before its sound exists to measure.
const GUESS_MS_PER_CHAR: i64 = 70;
const GUESS_SLOW_MS_PER_CHAR: i64 = 90;
const GUESS_MIN_MS: i64 = 500;

/// How long the learner asks a lesson to be, when they have chosen.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn chosen_minutes<'e, E: sqlx::SqliteExecutor<'e>>(executor: E) -> Result<Option<u32>> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(SETTING)
        .fetch_optional(executor)
        .await
        .context("failed to read the length a lesson is asked for")?;
    Ok(value.and_then(|value| value.parse().ok()).filter(|minutes| LENGTHS.contains(minutes)))
}

/// Keeps the length the learner chose, or forgets it for `None`.
///
/// # Errors
///
/// Fails on a length that is not one of [`LENGTHS`], or when the database
/// rejects the write.
pub async fn choose_minutes<'e, E: sqlx::SqliteExecutor<'e>>(executor: E, minutes: Option<u32>) -> Result<()> {
    match minutes {
        Some(minutes) => {
            anyhow::ensure!(LENGTHS.contains(&minutes), "a lesson is 15, 25 or 40 minutes, not {minutes}");
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
                .bind(SETTING)
                .bind(minutes.to_string())
                .execute(executor)
                .await
                .context("failed to keep the length a lesson is asked for")?;
        }
        None => {
            sqlx::query("DELETE FROM settings WHERE key = ?")
                .bind(SETTING)
                .execute(executor)
                .await
                .context("failed to forget the length a lesson is asked for")?;
        }
    }
    Ok(())
}

/// One phrase of a lesson, as the app is told it: the card it answers, and
/// what is said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phrase {
    pub card: CardRef,
    /// What it means, in the learner's own language.
    pub native: String,
    /// The phrase, in the language being learnt.
    pub target: String,
    pub asks: Asks,
    /// Heard for the first time in this lesson.
    pub fresh: bool,
}

/// What a lesson is made from: a phrase the queue offers, and the
/// languages it is said in.
#[derive(Debug, Clone)]
pub struct Offered {
    pub card: CardRef,
    pub item: Item,
    pub native: String,
    pub target: String,
}

/// What one item of the queue offers a lesson, if anything.
///
/// A formula is heard in one of its samples - another each day, so a week
/// of lessons goes through them - and a new formula is led in by its
/// explanation. A word is heard in its anchor, with the sentence's
/// translation as its meaning; a word from a text, whose sentence has none,
/// is heard alone with its gloss; a word with neither is left to its card.
/// A pair is left to its card too: choosing a word for a gap is done with
/// the eyes.
#[must_use]
pub fn offer(due: &Due, day: i64) -> Option<Offered> {
    match &due.subject {
        Subject::Formula { formula, direction } => {
            let samples = &formula.samples;
            if samples.is_empty() {
                return None;
            }
            let at = usize::try_from(day.rem_euclid(i64::try_from(samples.len()).ok()?)).ok()?;
            let sample = &samples[at];
            let fresh = due.is_new && *direction == Direction::Produce;
            let explanation = formula.explanation.trim();
            Some(Offered {
                card: CardRef::Formula {
                    id: formula.id.clone(),
                    direction: *direction,
                },
                item: Item {
                    native: sample.native.clone(),
                    target: sample.target.clone(),
                    asks: match direction {
                        Direction::Produce => Asks::Produce,
                        Direction::Recognise => Asks::Recognise,
                    },
                    fresh,
                    lead: (fresh && !explanation.is_empty()).then(|| explanation.to_string()),
                },
                native: formula.languages.native.clone(),
                target: formula.languages.target.clone(),
            })
        }
        Subject::Word { word } => {
            let anchor = word.contexts.first();
            let (native, target) = match (anchor.and_then(|context| context.translation.as_ref()), &word.gloss) {
                (Some(translation), _) => (translation.clone(), anchor?.text.clone()),
                (None, Some(gloss)) => (gloss.clone(), word.lemma.clone()),
                (None, None) => return None,
            };
            Some(Offered {
                card: CardRef::Word { id: word.id.clone() },
                item: Item {
                    native,
                    target,
                    asks: Asks::Recognise,
                    fresh: due.is_new,
                    lead: None,
                },
                native: word.languages.native.clone(),
                target: word.languages.target.clone(),
            })
        }
        Subject::Pair { .. } => None,
    }
}

/// Everything the queue offers a lesson now, in one pair of languages: the
/// one the queue starts with, so a learner of two languages hears one at a
/// time.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn offered(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Vec<Offered>> {
    let day = i64::from(now.date_naive().num_days_from_ce());
    let offered: Vec<Offered> = study::by_ear(pool, now).await?.iter().filter_map(|due| offer(due, day)).collect();
    let Some(first) = offered.first() else { return Ok(offered) };
    let languages = (first.native.clone(), first.target.clone());
    Ok(offered
        .into_iter()
        .filter(|offer| (offer.native.clone(), offer.target.clone()) == languages)
        .collect())
}

/// Where a lesson stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Building,
    Ready,
    Failed,
}

impl State {
    fn parse(word: &str) -> Result<Self> {
        match word {
            "building" => Ok(Self::Building),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            other => Err(anyhow!("{other:?} is not a state a lesson is in")),
        }
    }
}

/// A chapter as the app reads it: the phrase by its place in the lesson's
/// own list, and the stretch of the file it is heard in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stretch {
    pub phrase: usize,
    pub turn: plan::Turn,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// What a finished lesson holds besides its sound.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Contents {
    phrases: Vec<Phrase>,
    chapters: Vec<Stretch>,
}

/// A lesson as the app is told it.
#[derive(Debug, Clone, Serialize)]
pub struct Lesson {
    pub id: i64,
    pub asked_at: DateTime<Utc>,
    pub minutes: u32,
    pub state: State,
    /// Why it failed, in words the learner can act on.
    pub error: Option<String>,
    /// While it is being made: the lines said so far, and how many it needs.
    pub done: i64,
    pub total: i64,
    pub built_at: Option<DateTime<Utc>>,
    pub length_ms: Option<i64>,
    /// What its new lines cost the paid voice.
    pub credits: i64,
    /// Its phrases, in the order they are first heard; empty until ready.
    pub phrases: Vec<Phrase>,
    /// Every time a phrase is heard or asked, in order.
    pub chapters: Vec<Stretch>,
}

/// One lesson by id.
///
/// # Errors
///
/// Fails when the database rejects the query, or a row cannot be read.
pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Lesson>> {
    let row = sqlx::query("SELECT id, asked_at, minutes, state, error, done, total, built_at, length_ms, credits, plan FROM lesson WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a lesson")?;
    row.map(|row| lesson_of(&row)).transpose()
}

/// The lessons kept, the latest first.
///
/// # Errors
///
/// Fails when the database rejects the query, or a row cannot be read.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Lesson>> {
    sqlx::query("SELECT id, asked_at, minutes, state, error, done, total, built_at, length_ms, credits, plan FROM lesson ORDER BY id DESC")
        .fetch_all(pool)
        .await
        .context("failed to read the lessons")?
        .iter()
        .map(lesson_of)
        .collect()
}

fn lesson_of(row: &sqlx::sqlite::SqliteRow) -> Result<Lesson> {
    let contents: Contents = row
        .get::<Option<String>, _>("plan")
        .map(|plan| serde_json::from_str(&plan))
        .transpose()
        .context("a lesson's plan cannot be read")?
        .unwrap_or_default();
    Ok(Lesson {
        id: row.get("id"),
        asked_at: study::parse_stamp(&row.get::<String, _>("asked_at"))?,
        minutes: u32::try_from(row.get::<i64, _>("minutes")).context("a lesson's length is not a number of minutes")?,
        state: State::parse(&row.get::<String, _>("state"))?,
        error: row.get("error"),
        done: row.get("done"),
        total: row.get("total"),
        built_at: row.get::<Option<String>, _>("built_at").as_deref().map(study::parse_stamp).transpose()?,
        length_ms: row.get("length_ms"),
        credits: row.get("credits"),
        phrases: contents.phrases,
        chapters: contents.chapters,
    })
}

/// A lesson's sound, when it is ready.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn audio(pool: &SqlitePool, id: i64) -> Result<Option<Vec<u8>>> {
    sqlx::query_scalar("SELECT bytes FROM lesson WHERE id = ? AND state = 'ready'")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a lesson's sound")
        .map(Option::flatten)
}

/// Marks the lessons that were being made when the server stopped as
/// failed: nothing is making them any more.
///
/// # Errors
///
/// Fails when the database rejects the write.
pub async fn abandon(pool: &SqlitePool) -> Result<u64> {
    let abandoned = sqlx::query("UPDATE lesson SET state = 'failed', error = ? WHERE state = 'building'")
        .bind("The tutor restarted while this lesson was being made. Ask for it again.")
        .execute(pool)
        .await
        .context("failed to give up the lessons left half made")?;
    Ok(abandoned.rows_affected())
}

/// Why a lesson could not be asked for.
#[derive(Debug, thiserror::Error)]
pub enum AskError {
    #[error("a lesson is 15, 25 or 40 minutes, not {0}")]
    Length(u32),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// What makes lessons: ffmpeg, and the one lesson being made at a time.
#[derive(Clone)]
pub struct Lessons {
    ffmpeg: Arc<PathBuf>,
    /// The lesson being made, if one is: a stand makes one at a time, and a
    /// second ask while it does is told about the first.
    making: Arc<Mutex<Option<i64>>>,
}

impl Lessons {
    /// Lessons made with this ffmpeg.
    #[must_use]
    pub fn new(ffmpeg: PathBuf) -> Self {
        Self {
            ffmpeg: Arc::new(ffmpeg),
            making: Arc::default(),
        }
    }

    /// The ffmpeg lessons are made with.
    #[must_use]
    pub fn ffmpeg(&self) -> &std::path::Path {
        &self.ffmpeg
    }

    /// Asks for a lesson of `minutes`, and keeps that as the length the
    /// learner likes. The lesson is made in the background; the answer is
    /// its id, to follow it by. While one is being made, asking again is
    /// answered with that one.
    ///
    /// # Errors
    ///
    /// [`AskError::Length`] for a length not offered, `Internal` when the
    /// database rejects a write.
    pub async fn ask(&self, pool: &SqlitePool, voices: &Voices, minutes: u32, now: DateTime<Utc>) -> Result<i64, AskError> {
        if !LENGTHS.contains(&minutes) {
            return Err(AskError::Length(minutes));
        }
        choose_minutes(pool, Some(minutes)).await?;
        let mut making = self.making.lock().await;
        if let Some(id) = *making {
            return Ok(id);
        }
        let id: i64 = sqlx::query_scalar("INSERT INTO lesson (asked_at, minutes, state) VALUES (?, ?, 'building') RETURNING id")
            .bind(now.to_rfc3339())
            .bind(i64::from(minutes))
            .fetch_one(pool)
            .await
            .context("failed to start a lesson")?;
        *making = Some(id);
        drop(making);

        let (lessons, pool, voices) = (self.clone(), pool.clone(), voices.clone());
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            match make(&pool, &voices, &lessons.ffmpeg, id, minutes, now).await {
                Ok(()) => tracing::info!(lesson = id, minutes, elapsed_ms = started.elapsed().as_millis(), "a lesson is ready"),
                Err(error) => {
                    tracing::warn!(lesson = id, error = format!("{error:#}"), "a lesson could not be made");
                    let said = sqlx::query("UPDATE lesson SET state = 'failed', error = ? WHERE id = ?")
                        .bind(format!("{error:#}"))
                        .bind(id)
                        .execute(&pool)
                        .await;
                    if let Err(error) = said {
                        tracing::error!(lesson = id, %error, "a failed lesson could not be marked failed");
                    }
                }
            }
            *lessons.making.lock().await = None;
        });
        Ok(id)
    }

    /// Makes a lesson where the caller waits for it: what the tests and the
    /// command line do.
    ///
    /// # Errors
    ///
    /// Whatever stopped the lesson, as [`Lessons::ask`] would record it.
    pub async fn make_now(&self, pool: &SqlitePool, voices: &Voices, minutes: u32, now: DateTime<Utc>) -> Result<i64> {
        anyhow::ensure!(LENGTHS.contains(&minutes), "a lesson is 15, 25 or 40 minutes, not {minutes}");
        let id: i64 = sqlx::query_scalar("INSERT INTO lesson (asked_at, minutes, state) VALUES (?, ?, 'building') RETURNING id")
            .bind(now.to_rfc3339())
            .bind(i64::from(minutes))
            .fetch_one(pool)
            .await
            .context("failed to start a lesson")?;
        if let Err(error) = make(pool, voices, &self.ffmpeg, id, minutes, now).await {
            sqlx::query("UPDATE lesson SET state = 'failed', error = ? WHERE id = ?")
                .bind(format!("{error:#}"))
                .bind(id)
                .execute(pool)
                .await
                .context("failed to mark a lesson failed")?;
            return Err(error);
        }
        Ok(id)
    }
}

/// The language and the tempo a line is said in.
fn voice_of<'a>(line: &Line, native: &'a str, target: &'a str) -> (&'a str, Tempo) {
    match line.voice {
        Voice::Teacher => (native, Tempo::Normal),
        Voice::Speaker => (target, if line.slow { Tempo::Slow } else { Tempo::Normal }),
    }
}

/// How long a line not yet said is guessed to last.
fn guess(line: &Line) -> i64 {
    let chars = i64::try_from(line.text.chars().count()).unwrap_or(i64::MAX / 1_000);
    (chars * if line.slow { GUESS_SLOW_MS_PER_CHAR } else { GUESS_MS_PER_CHAR }).max(GUESS_MIN_MS)
}

/// The lines a plan says, each once.
fn lines_of(parts: &[Part]) -> Vec<Line> {
    parts
        .iter()
        .filter_map(|part| match part {
            Part::Say { line } => Some(line.clone()),
            Part::Silence { .. } => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Makes lesson `id`: offered, planned on guesses, priced, said, decoded,
/// planned again on what was said, spliced and kept.
async fn make(pool: &SqlitePool, voices: &Voices, ffmpeg: &std::path::Path, id: i64, minutes: u32, now: DateTime<Utc>) -> Result<()> {
    let offered = offered(pool, now).await?;
    anyhow::ensure!(
        !offered.is_empty(),
        "Nothing is waiting to be heard: today's cards are done and nothing comes back this week."
    );
    let (native, target) = (offered[0].native.clone(), offered[0].target.clone());
    let length_ms = i64::from(minutes) * 60_000;
    let items: Vec<Item> = offered.iter().map(|offer| offer.item.clone()).collect();

    // Planned first on what the cache knows and guesses for the rest, so
    // what is said and paid for is what the lesson will use.
    let mut quotes = HashMap::new();
    let lines: BTreeSet<Line> = items.iter().flat_map(Item::lines).collect();
    for line in lines {
        let (language, tempo) = voice_of(&line, &native, &target);
        let quote = voices.quote(pool, language, &line.text, tempo).await.map_err(|error| anyhow!("{error}"))?;
        quotes.insert(line, quote);
    }
    let guessed = plan::plan(&items, length_ms, |line| {
        quotes.get(line).map(|quote| quote.cached_ms.unwrap_or_else(|| guess(line)))
    });
    anyhow::ensure!(!guessed.items.is_empty(), "No phrase fits in a lesson of {minutes} minutes.");
    let needed = lines_of(&guessed.parts);

    let credits: i64 = needed.iter().filter_map(|line| quotes.get(line)).map(|quote| quote.credits).sum();
    if credits > 0 {
        let budget = voices
            .budget(pool)
            .await
            .context("The native voice's account did not say how many credits are left, so the lesson was not started.")?;
        if credits > budget.remaining {
            let reset = budget.resets_at.map_or_else(String::new, |at| format!(" until {}", at.format("%-d %B")));
            anyhow::bail!(
                "This lesson needs about {credits} credits of the native voice, and {} are left{reset}. A shorter lesson needs fewer; so does one made of what has been heard before.",
                budget.remaining
            );
        }
    }

    let total = i64::try_from(needed.len()).unwrap_or(i64::MAX);
    sqlx::query("UPDATE lesson SET total = ?, done = 0 WHERE id = ?")
        .bind(total)
        .bind(id)
        .execute(pool)
        .await
        .context("failed to say how much of the lesson there is to make")?;
    let mut decoded = HashMap::new();
    for (done, line) in needed.into_iter().enumerate() {
        let (language, tempo) = voice_of(&line, &native, &target);
        let sound = voices
            .line(pool, language, &line.text, tempo)
            .await
            .map_err(|error| anyhow!("{:?} could not be said: {error}", line.text))?;
        decoded.insert(line, splice::decode(ffmpeg, &sound.bytes).await?);
        sqlx::query("UPDATE lesson SET done = ? WHERE id = ?")
            .bind(i64::try_from(done + 1).unwrap_or(i64::MAX))
            .bind(id)
            .execute(pool)
            .await
            .context("failed to say how far the lesson has got")?;
    }

    // Planned again on the real lengths, from the phrases the guess chose.
    let chosen: Vec<&Offered> = guessed.items.iter().map(|index| &offered[*index]).collect();
    let chosen_items: Vec<Item> = chosen.iter().map(|offer| offer.item.clone()).collect();
    let real = plan::plan(&chosen_items, length_ms, |line| decoded.get(line).map(splice::Decoded::ms));
    anyhow::ensure!(!real.items.is_empty(), "No phrase fits in a lesson of {minutes} minutes.");

    let (contents, marks) = contents_of(&real, &chosen);
    let title = format!("hilvan · {}", now.format("%-d %B %Y"));
    let bytes = splice::encode(ffmpeg, &real.parts, &decoded, &marks, &title).await?;
    let spent: i64 = lines_of(&real.parts)
        .iter()
        .filter_map(|line| quotes.get(line))
        .map(|quote| quote.credits)
        .sum();

    sqlx::query("UPDATE lesson SET state = 'ready', built_at = ?, length_ms = ?, plan = ?, credits = ?, bytes = ?, done = total WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(real.length_ms)
        .bind(serde_json::to_string(&contents).context("failed to write a lesson's plan")?)
        .bind(spent)
        .bind(bytes)
        .bind(id)
        .execute(pool)
        .await
        .context("failed to keep the lesson")?;
    prune(pool).await
}

/// The phrases and chapters of a plan, as the app and the file read them.
fn contents_of(real: &plan::Plan, chosen: &[&Offered]) -> (Contents, Vec<splice::Mark>) {
    // A phrase's place in the lesson is the order it is first heard in.
    let place: HashMap<usize, usize> = real.items.iter().enumerate().map(|(place, item)| (*item, place)).collect();
    let phrases: Vec<Phrase> = real
        .items
        .iter()
        .map(|item| {
            let offer = chosen[*item];
            Phrase {
                card: offer.card.clone(),
                native: offer.item.native.clone(),
                target: offer.item.target.clone(),
                asks: offer.item.asks,
                fresh: offer.item.fresh,
            }
        })
        .collect();
    let chapters: Vec<Stretch> = real
        .chapters
        .iter()
        .map(|chapter: &Chapter| Stretch {
            phrase: place[&chapter.item],
            turn: chapter.turn,
            start_ms: chapter.start_ms,
            end_ms: chapter.end_ms,
        })
        .collect();
    let marks = chapters
        .iter()
        .map(|stretch| splice::Mark {
            start_ms: stretch.start_ms,
            end_ms: stretch.end_ms,
            title: phrases[stretch.phrase].target.clone(),
        })
        .collect();
    (Contents { phrases, chapters }, marks)
}

/// Drops all but the [`KEPT`] latest lessons that are done being made.
async fn prune(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "DELETE FROM lesson WHERE state != 'building'
         AND id NOT IN (SELECT id FROM lesson WHERE state != 'building' ORDER BY id DESC LIMIT ?)",
    )
    .bind(KEPT)
    .execute(pool)
    .await
    .context("failed to let go of old lessons")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guess_grows_with_the_line_and_slows_with_it() {
        let line = |text: &str, slow| Line {
            voice: Voice::Speaker,
            text: text.to_string(),
            slow,
        };
        assert_eq!(guess(&line("Hi.", false)), GUESS_MIN_MS, "a word is not said in a blink");
        assert!(guess(&line("I am at home today.", true)) > guess(&line("I am at home today.", false)));
    }

    #[test]
    fn a_line_is_said_by_the_teacher_in_the_learners_language_and_the_native_voice_in_the_other() {
        let teacher = Line {
            voice: Voice::Teacher,
            text: "Я дома.".to_string(),
            slow: false,
        };
        let slow = Line {
            voice: Voice::Speaker,
            text: "I am at home.".to_string(),
            slow: true,
        };
        assert_eq!(voice_of(&teacher, "ru", "en"), ("ru", Tempo::Normal));
        assert_eq!(voice_of(&slow, "ru", "en"), ("en", Tempo::Slow));
    }
}
