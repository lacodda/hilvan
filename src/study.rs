//! What the learner does today.
//!
//! Two questions, and the whole of v0.1.0 is the honest answer to them: what
//! is waiting to be reviewed, and what one new thing is worth starting. The
//! scheduling itself lives in [`crate::scheduling`]; this module is the part
//! that knows about formulas, days and the size of a sitting.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::pack::Form;
use crate::scheduling::{Card, Direction, Rating, Scheduled, Scheduler, Stitch};

/// How many formulas start on any one day.
///
/// One (the plan's "formula of the day"): a formula is a shape you assemble
/// until it stops needing thought, and two new ones in a day means neither
/// gets that. This is a product decision, not a tuning knob.
pub const NEW_FORMULAS_PER_DAY: usize = 1;

/// A formula with everything the drill needs to run it.
#[derive(Debug, Clone, Serialize)]
pub struct Formula {
    pub id: String,
    pub name: String,
    pub pattern: String,
    /// In the learner's native language: the pack carries it, the code does not.
    pub explanation: String,
    pub samples: Vec<Sample>,
    pub slots: Vec<Slot>,
    /// The shape this is one form of, when it is one of several.
    pub family: Option<String>,
    pub form: Option<Form>,
    /// The other forms of the same shape, in statement-negation-question
    /// order: what the switch on the card offers. Empty when the formula
    /// stands alone.
    pub sisters: Vec<Sister>,
}

/// One other form of the same shape, as the switch shows it.
///
/// Enough to draw the switch and to move to that form without a second round
/// trip; the drill fetches the full formula when the learner actually
/// switches.
#[derive(Debug, Clone, Serialize)]
pub struct Sister {
    pub id: String,
    pub form: Form,
    pub name: String,
    pub pattern: String,
    /// Where that form stands - so the switch can show that the question is
    /// still new while the statement is sewn.
    pub stitch: Stitch,
}

/// A worked example of a formula.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    /// The prompt, in the learner's native language.
    pub native: String,
    /// The answer, in the language being learnt.
    pub target: String,
}

/// A hole in the pattern, with what can go in it.
#[derive(Debug, Clone, Serialize)]
pub struct Slot {
    pub name: String,
    pub values: Vec<Sample>,
}

/// One item in today's queue: a formula, a direction, and the state that
/// pair is in.
#[derive(Debug, Clone, Serialize)]
pub struct Due {
    pub formula: Formula,
    /// Which way round this turn asks it.
    pub direction: Direction,
    pub stitch: Stitch,
    /// True when this formula is being seen in this direction for the first
    /// time.
    pub is_new: bool,
    pub due: DateTime<Utc>,
    /// How fast this card usually comes, when it has come often enough to
    /// say. Shown on the card; the scheduler does not read it.
    pub pace: Option<Pace>,
}

/// How quickly a card is answered, as the second dimension of knowing it.
///
/// Stability says whether the formula is still there; pace says whether it
/// still costs thought. A formula answered right after eight seconds of
/// assembling it is not the same as one that arrives - and FSRS cannot see
/// the difference, because both are "good".
///
/// Reported, not scheduled on (owner's decision, 2026-09-03): a home-made
/// correction on top of FSRS would move every interval with no way to tell
/// what moved them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pace {
    /// The median of the last few answers, in milliseconds.
    pub typical_ms: i64,
    /// The most recent answer, in milliseconds.
    pub last_ms: i64,
    /// How many timed answers the median rests on.
    pub answers: i64,
}

/// How many timed answers a pace needs before it is shown.
///
/// Three: one is noise, two cannot have a middle, and a number the learner
/// cannot trust is worse than no number.
const PACE_MIN_ANSWERS: usize = 3;

/// How many recent answers the typical pace is taken over.
const PACE_WINDOW: i64 = 8;

/// How many formulas stand in each state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub new: i64,
    pub basted: i64,
    pub sewn: i64,
}

/// The same counts told twice, once per direction.
///
/// Recognition runs ahead of production - it always does - and the two
/// columns side by side are the honest picture of where a learner stands.
/// One averaged number would hide exactly the gap worth seeing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    /// Saying it: native prompt, target answer.
    pub produce: Counts,
    /// Understanding it: target prompt, native answer.
    pub recognise: Counts,
}

/// The answer to "what am I doing today".
#[derive(Debug, Clone, Serialize)]
pub struct Today {
    /// Formulas waiting to come back, plus at most one new one.
    pub queue: Vec<Due>,
    /// Formulas already reviewed since midnight UTC.
    pub reviewed_today: i64,
    pub counts: Counts,
    /// The same standing, split by direction.
    pub progress: Progress,
}

/// What the learner sends back after answering.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Answer {
    pub rating: Rating,
    /// Which direction was answered. Defaults to producing, which is what a
    /// client written before directions existed meant.
    #[serde(default = "produce")]
    pub direction: Direction,
    /// How long the answer took, when the client measured it. The second
    /// dimension of a card: shown to the learner, not fed to the scheduler.
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

const fn produce() -> Direction {
    Direction::Produce
}

/// What an answer changed.
#[derive(Debug, Clone, Serialize)]
pub struct Reviewed {
    pub stitch: Stitch,
    pub due: DateTime<Utc>,
    /// Days until this formula comes back.
    pub interval_days: i64,
    /// How this answer compared with the usual pace of this card, when there
    /// is a usual pace to compare with.
    pub pace: Option<Pace>,
}

/// Today's queue: everything due, then one new formula if the day has room.
///
/// Reviews come first and are never crowded out by new material - a day where
/// the learner starts something new while forgetting yesterday's is the
/// failure mode every course product has.
///
/// Both directions of a formula queue independently: understanding a shape
/// and producing it come back on their own schedules, and the recognising
/// card of a formula the learner has never produced is not offered - you
/// cannot be asked to understand what you were never shown.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn today(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Today> {
    let counts = counts(pool).await?;
    let progress = progress(pool).await?;
    let mut queue = Vec::new();

    let due_rows = sqlx::query(
        "SELECT c.kind, f.id FROM card c
         JOIN formula f ON f.id = c.subject_id
         WHERE c.kind IN ('formula', 'formula-recognise') AND c.state != 'new' AND c.due <= ?
         ORDER BY c.due, f.position, c.kind",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await
    .context("failed to read the review queue")?;

    for row in due_rows {
        let id: String = row.get("id");
        let direction = Direction::parse(row.get::<String, _>("kind").as_str())?;
        queue.push(due(pool, &id, direction, false, now).await?);
    }

    // The recognising side of a formula already being produced: it opens as
    // soon as the formula is introduced, and never before, so it costs no
    // slot in the day's budget of new material.
    for id in unopened_recognitions(pool).await? {
        queue.push(due(pool, &id, Direction::Recognise, true, now).await?);
    }

    let started_today = introduced_since(pool, start_of_day(now)).await?;
    if usize::try_from(started_today).unwrap_or(usize::MAX) < NEW_FORMULAS_PER_DAY {
        // The next formula is the first one in the pack's own sequence that
        // has never been answered: the pack decides the order, not the clock.
        if let Some(id) = next_new(pool).await? {
            queue.push(due(pool, &id, Direction::Produce, true, now).await?);
        }
    }

    Ok(Today {
        queue,
        reviewed_today: reviewed_since(pool, start_of_day(now)).await?,
        counts,
        progress,
    })
}

/// One queue entry, with the formula, its sisters and its pace.
async fn due(pool: &SqlitePool, id: &str, direction: Direction, is_new: bool, now: DateTime<Utc>) -> Result<Due> {
    Ok(Due {
        formula: formula(pool, id).await?,
        direction,
        stitch: card(pool, id, direction).await?.map_or(Stitch::New, |card| card.stitch),
        is_new,
        due: now,
        pace: pace(pool, id, direction).await?,
    })
}

/// Records an answer and reschedules the formula in the direction answered.
///
/// # Errors
///
/// Fails when the formula has no card in that direction - which means it is
/// not in any loaded pack - or when the database rejects a statement.
pub async fn review(pool: &SqlitePool, formula_id: &str, answer: Answer, now: DateTime<Utc>) -> Result<Reviewed> {
    let direction = answer.direction;
    let card = card(pool, formula_id, direction)
        .await?
        .with_context(|| format!("there is no formula called {formula_id}"))?;

    let Scheduled { card: next, elapsed_days } = Scheduler::new().review(&card, answer.rating, now);

    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    sqlx::query(
        "UPDATE card SET state = ?, stability = ?, difficulty = ?, due = ?, reps = ?, lapses = ?,
             last_reviewed = ?, introduced_at = COALESCE(introduced_at, ?)
         WHERE kind = ? AND subject_id = ?",
    )
    .bind(next.stitch.as_str())
    .bind(next.stability)
    .bind(next.difficulty)
    .bind(next.due.to_rfc3339())
    .bind(next.reps)
    .bind(next.lapses)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(direction.kind())
    .bind(formula_id)
    .execute(&mut *tx)
    .await
    .context("failed to save the card")?;

    // The history is kept in full so the schedule can be recomputed when the
    // memory model changes (ADR 0003).
    sqlx::query(
        "INSERT INTO review (card_id, rating, reviewed_at, elapsed_days, stability, difficulty, duration_ms)
         SELECT id, ?, ?, ?, ?, ?, ? FROM card WHERE kind = ? AND subject_id = ?",
    )
    .bind(answer.rating.as_i64())
    .bind(now.to_rfc3339())
    .bind(elapsed_days)
    .bind(next.stability)
    .bind(next.difficulty)
    .bind(answer.duration_ms)
    .bind(direction.kind())
    .bind(formula_id)
    .execute(&mut *tx)
    .await
    .context("failed to record the review")?;
    tx.commit().await.context("failed to commit the review")?;

    Ok(Reviewed {
        stitch: next.stitch,
        due: next.due,
        interval_days: (next.due - now).num_days(),
        pace: pace(pool, formula_id, direction).await?,
    })
}

/// One formula with its samples and slots.
///
/// # Errors
///
/// Fails when there is no such formula, or when the database rejects a query.
pub async fn formula(pool: &SqlitePool, id: &str) -> Result<Formula> {
    let row = sqlx::query("SELECT id, name, pattern, explanation, family, form FROM formula WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a formula")?
        .with_context(|| format!("there is no formula called {id}"))?;

    let samples = sqlx::query("SELECT native, target FROM sample WHERE formula_id = ? ORDER BY position")
        .bind(id)
        .fetch_all(pool)
        .await
        .context("failed to read the samples of a formula")?
        .iter()
        .map(sample)
        .collect();

    let slot_rows = sqlx::query("SELECT id, name FROM slot WHERE formula_id = ? ORDER BY position")
        .bind(id)
        .fetch_all(pool)
        .await
        .context("failed to read the slots of a formula")?;

    let mut slots = Vec::with_capacity(slot_rows.len());
    for slot in slot_rows {
        let slot_id: i64 = slot.get("id");
        let values = sqlx::query("SELECT native, target FROM slot_value WHERE slot_id = ? ORDER BY position")
            .bind(slot_id)
            .fetch_all(pool)
            .await
            .context("failed to read the values of a slot")?
            .iter()
            .map(sample)
            .collect();
        slots.push(Slot {
            name: slot.get("name"),
            values,
        });
    }

    let family: Option<String> = row.get("family");
    let form = row.get::<Option<String>, _>("form").map(|word| Form::parse(&word)).transpose()?;

    Ok(Formula {
        id: row.get("id"),
        name: row.get("name"),
        pattern: row.get("pattern"),
        explanation: row.get("explanation"),
        sisters: match family.as_deref() {
            Some(family) => sisters(pool, family, id).await?,
            None => Vec::new(),
        },
        family,
        form,
        samples,
        slots,
    })
}

/// The other forms of the same shape, in the order the switch shows them.
///
/// Statement, negation, question - the order they are learnt in and the order
/// they are asked in, taken from the pack's own sequence rather than from the
/// enum, so a pack that introduces the question first is shown that way.
async fn sisters(pool: &SqlitePool, family: &str, without: &str) -> Result<Vec<Sister>> {
    let rows = sqlx::query(
        "SELECT f.id, f.form, f.name, f.pattern, c.state FROM formula f
         LEFT JOIN card c ON c.subject_id = f.id AND c.kind = 'formula'
         WHERE f.family = ? AND f.id != ?
         ORDER BY f.position, f.id",
    )
    .bind(family)
    .bind(without)
    .fetch_all(pool)
    .await
    .context("failed to read the other forms of a formula")?;

    let mut sisters = Vec::with_capacity(rows.len());
    for row in rows {
        // A row in a family without a form is rejected by the pack validator,
        // so this can only be a database edited by hand.
        let Some(word) = row.get::<Option<String>, _>("form") else { continue };
        sisters.push(Sister {
            id: row.get("id"),
            form: Form::parse(&word)?,
            name: row.get("name"),
            pattern: row.get("pattern"),
            stitch: row.get::<Option<String>, _>("state").map_or(Ok(Stitch::New), |state| Stitch::parse(&state))?,
        });
    }
    Ok(sisters)
}

/// How many formulas stand in each state, counting the producing side.
///
/// Producing is the side the product is about, and the one number a learner
/// quotes about themselves. The split by direction is [`progress`].
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn counts(pool: &SqlitePool) -> Result<Counts> {
    counts_for(pool, Direction::Produce).await
}

/// The same standing told once per direction.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn progress(pool: &SqlitePool) -> Result<Progress> {
    Ok(Progress {
        produce: counts_for(pool, Direction::Produce).await?,
        recognise: counts_for(pool, Direction::Recognise).await?,
    })
}

async fn counts_for(pool: &SqlitePool, direction: Direction) -> Result<Counts> {
    let rows = sqlx::query("SELECT state, count(*) AS n FROM card WHERE kind = ? GROUP BY state")
        .bind(direction.kind())
        .fetch_all(pool)
        .await
        .context("failed to count the formulas")?;

    let mut counts = Counts::default();
    for row in rows {
        let n: i64 = row.get("n");
        match Stitch::parse(row.get::<String, _>("state").as_str()) {
            Ok(Stitch::New) => counts.new = n,
            Ok(Stitch::Basted) => counts.basted = n,
            Ok(Stitch::Sewn) => counts.sewn = n,
            Err(error) => {
                // A state nobody wrote on purpose: counted nowhere rather
                // than counted wrong, and said out loud.
                tracing::warn!(%error, "a card holds a state the product does not know");
            }
        }
    }
    Ok(counts)
}

fn sample(row: &SqliteRow) -> Sample {
    Sample {
        native: row.get("native"),
        target: row.get("target"),
    }
}

/// Midnight UTC of the day `now` falls in.
///
/// UTC rather than a local timezone: a single learner in one place, and a day
/// boundary that does not move is worth more than one that lands at midnight
/// exactly. When the day starts mattering - streaks, in v0.21.0 - it becomes
/// a setting.
fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive().and_hms_opt(0, 0, 0).map_or(now, |naive| naive.and_utc())
}

async fn card(pool: &SqlitePool, formula_id: &str, direction: Direction) -> Result<Option<Card>> {
    let row = sqlx::query(
        "SELECT state, stability, difficulty, due, reps, lapses, last_reviewed
         FROM card WHERE kind = ? AND subject_id = ?",
    )
    .bind(direction.kind())
    .bind(formula_id)
    .fetch_optional(pool)
    .await
    .context("failed to read a card")?;

    let Some(row) = row else { return Ok(None) };
    Ok(Some(Card {
        stitch: Stitch::parse(row.get::<String, _>("state").as_str())?,
        stability: row.get("stability"),
        difficulty: row.get("difficulty"),
        due: timestamp(&row, "due")?.context("a card has no due date")?,
        reps: row.get("reps"),
        lapses: row.get("lapses"),
        last_reviewed: timestamp(&row, "last_reviewed")?,
    }))
}

fn timestamp(row: &SqliteRow, column: &str) -> Result<Option<DateTime<Utc>>> {
    let raw: Option<String> = row.get(column);
    raw.map(|text| {
        DateTime::parse_from_rfc3339(&text)
            .map(|at| at.with_timezone(&Utc))
            .with_context(|| format!("{column} is not a timestamp: {text}"))
    })
    .transpose()
}

async fn next_new(pool: &SqlitePool) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT f.id FROM card c
         JOIN formula f ON f.id = c.subject_id
         WHERE c.kind = 'formula' AND c.state = 'new'
         ORDER BY f.position, f.id
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .context("failed to look for the next new formula")
}

/// How many formulas were started today.
///
/// The producing side only: the day's budget of one new formula is about new
/// shapes, and the recognising card of a shape already being drilled is not a
/// new shape.
async fn introduced_since(pool: &SqlitePool, since: DateTime<Utc>) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM card WHERE kind = 'formula' AND introduced_at >= ?")
        .bind(since.to_rfc3339())
        .fetch_one(pool)
        .await
        .context("failed to count the formulas started today")
}

async fn reviewed_since(pool: &SqlitePool, since: DateTime<Utc>) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM review WHERE reviewed_at >= ?")
        .bind(since.to_rfc3339())
        .fetch_one(pool)
        .await
        .context("failed to count today's reviews")
}

/// Formulas being produced whose recognising side has never been asked.
///
/// A shape is shown in the drill before it is asked backwards: understanding
/// a sentence you have never been taught to build is a guess, not a review.
async fn unopened_recognitions(pool: &SqlitePool) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT p.subject_id FROM card p
         JOIN card r ON r.subject_id = p.subject_id AND r.kind = 'formula-recognise'
         JOIN formula f ON f.id = p.subject_id
         WHERE p.kind = 'formula' AND p.state != 'new' AND r.state = 'new'
         ORDER BY f.position, f.id",
    )
    .fetch_all(pool)
    .await
    .context("failed to look for formulas ready to be asked backwards")
}

/// How fast this card usually comes.
///
/// The median of the last few timed answers rather than the mean: one
/// interrupted turn - a phone put down mid-drill - would drag a mean for
/// weeks, and the number is there to be trusted at a glance.
///
/// `None` until there are enough answers to have a middle worth showing.
async fn pace(pool: &SqlitePool, formula_id: &str, direction: Direction) -> Result<Option<Pace>> {
    let mut recent: Vec<i64> = sqlx::query_scalar(
        "SELECT r.duration_ms FROM review r
         JOIN card c ON c.id = r.card_id
         WHERE c.kind = ? AND c.subject_id = ? AND r.duration_ms IS NOT NULL
         ORDER BY r.reviewed_at DESC, r.id DESC
         LIMIT ?",
    )
    .bind(direction.kind())
    .bind(formula_id)
    .bind(PACE_WINDOW)
    .fetch_all(pool)
    .await
    .context("failed to read how long a formula usually takes")?;

    if recent.len() < PACE_MIN_ANSWERS {
        return Ok(None);
    }

    let last_ms = recent[0];
    let answers = i64::try_from(recent.len()).unwrap_or(i64::MAX);
    recent.sort_unstable();
    let typical_ms = recent[recent.len() / 2];

    Ok(Some(Pace { typical_ms, last_ms, answers }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;

    /// The first three fixture formulas are the three forms of one shape, so
    /// the switch has something to switch between; the rest stand alone.
    const FORMS: [&str; 3] = ["statement", "negation", "question"];

    fn fixture(formulas: usize) -> pack::Pack {
        use std::fmt::Write as _;

        let mut toml = String::from(
            "id = \"t\"
version = 1
native = \"ru\"
target = \"en\"
",
        );
        for index in 0..formulas {
            let family = FORMS
                .get(index)
                .map(|form| {
                    format!(
                        "family = \"shape\"
form = \"{form}\"
"
                    )
                })
                .unwrap_or_default();
            let _ = write!(
                toml,
                "
[[formula]]
id = \"f{index}\"
name = \"n{index}\"
pattern = \"<pronoun> + x\"
                 explanation = \"e\"
order = {}
{family}

  [[formula.sample]]
  native = \"a\"
  target = \"b\"

                   [[formula.slot]]
  name = \"pronoun\"
  values = [{{ native = \"one\", target = \"I\" }}]
",
                (index + 1) * 10
            );
        }
        toml::from_str(&toml).expect("the fixture should parse")
    }

    async fn loaded(formulas: usize) -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        pack::load(&pool, &fixture(formulas)).await.expect("the fixture should load");
        pool
    }

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z").unwrap().with_timezone(&Utc) + chrono::Duration::days(day)
    }

    /// An answer in the producing direction, which is what most of these
    /// tests are about.
    const fn said(rating: Rating) -> Answer {
        Answer {
            rating,
            direction: Direction::Produce,
            duration_ms: None,
        }
    }

    impl Answer {
        /// The same answer, with how long it took.
        const fn taking(self, duration_ms: i64) -> Self {
            Self {
                duration_ms: Some(duration_ms),
                ..self
            }
        }

        /// The same answer, asked the other way round.
        const fn backwards(self) -> Self {
            Self {
                direction: Direction::Recognise,
                ..self
            }
        }
    }

    #[tokio::test]
    async fn a_fresh_learner_gets_exactly_one_formula() {
        // The whole promise of "one new formula a day": a first sitting shows
        // one shape, not a syllabus.
        let pool = loaded(5).await;
        let today = today(&pool, at(0)).await.unwrap();

        assert_eq!(today.queue.len(), 1);
        assert!(today.queue[0].is_new);
        assert_eq!(today.queue[0].formula.id, "f0", "the pack's own order should decide what comes first");
        assert_eq!(today.counts, Counts { new: 5, basted: 0, sewn: 0 });
    }

    #[tokio::test]
    async fn the_second_formula_of_the_day_is_not_offered() {
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let today = today(&pool, at(0)).await.unwrap();
        assert!(
            today.queue.iter().all(|due| !(due.is_new && due.direction == Direction::Produce)),
            "a second new formula was offered on the same day: {:?}",
            today.queue.iter().map(|due| &due.formula.id).collect::<Vec<_>>()
        );
        assert_eq!(today.reviewed_today, 1);
    }

    #[tokio::test]
    async fn tomorrow_brings_the_next_formula() {
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let tomorrow = today(&pool, at(1)).await.unwrap();
        let new: Vec<_> = tomorrow
            .queue
            .iter()
            .filter(|due| due.is_new && due.direction == Direction::Produce)
            .map(|due| due.formula.id.as_str())
            .collect();
        assert_eq!(new, vec!["f1"], "a new day should open exactly the next formula in the pack");
    }

    #[tokio::test]
    async fn a_formula_answered_comes_back_and_one_forgotten_comes_back_sooner() {
        let pool = loaded(3).await;
        let good = review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let again = review(&pool, "f1", said(Rating::Again), at(0)).await.unwrap();

        assert!(again.due <= good.due, "a formula that would not come should return sooner than one that did");
        assert_eq!(again.stitch, Stitch::Basted);
    }

    #[tokio::test]
    async fn a_review_is_kept_in_full() {
        // The history is the asset: the schedule can be recomputed from it,
        // and nothing else can.
        let pool = loaded(2).await;
        review(&pool, "f0", said(Rating::Hard).taking(4200), at(0)).await.unwrap();

        let (rating, duration, stability): (i64, Option<i64>, f64) = sqlx::query_as("SELECT rating, duration_ms, stability FROM review")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((rating, duration), (2, Some(4200)));
        assert!(stability > 0.0, "the review should record the memory state it produced");
    }

    #[tokio::test]
    async fn a_due_formula_is_queued_before_anything_new() {
        // Reviews must never be crowded out by new material.
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Again), at(0)).await.unwrap();

        let later = today(&pool, at(2)).await.unwrap();
        assert_eq!(later.queue.first().map(|due| due.formula.id.as_str()), Some("f0"));
        assert!(!later.queue[0].is_new);
    }

    #[tokio::test]
    async fn a_formula_not_yet_due_stays_out_of_the_queue() {
        let pool = loaded(3).await;
        let reviewed = review(&pool, "f0", said(Rating::Easy), at(0)).await.unwrap();
        assert!(reviewed.interval_days >= 1, "an easy answer should push a formula past today");

        let same_day = today(&pool, at(0)).await.unwrap();
        assert!(
            same_day
                .queue
                .iter()
                .all(|due| !(due.formula.id == "f0" && due.direction == Direction::Produce)),
            "an answered formula came back the same day"
        );
    }

    #[tokio::test]
    async fn answering_something_that_is_not_a_formula_fails() {
        let pool = loaded(1).await;
        let error = review(&pool, "not-a-formula", said(Rating::Good), at(0)).await.unwrap_err();
        assert!(error.to_string().contains("not-a-formula"), "{error:#}");
    }

    #[tokio::test]
    async fn a_formula_carries_its_samples_and_slots() {
        let pool = loaded(1).await;
        let formula = formula(&pool, "f0").await.unwrap();
        assert_eq!(formula.samples.len(), 1);
        assert_eq!(formula.slots.len(), 1);
        assert_eq!(formula.slots[0].name, "pronoun");
        assert_eq!(formula.slots[0].values[0].target, "I");
    }

    #[tokio::test]
    async fn a_formula_carries_the_other_forms_of_its_shape() {
        // What the switch on the card is drawn from.
        let pool = loaded(5).await;
        let formula = formula(&pool, "f0").await.unwrap();

        assert_eq!(formula.family.as_deref(), Some("shape"));
        assert_eq!(formula.form, Some(Form::Statement));
        let forms: Vec<_> = formula.sisters.iter().map(|sister| sister.form).collect();
        assert_eq!(forms, vec![Form::Negation, Form::Question], "the switch should offer the other two forms");
        assert!(
            formula.sisters.iter().all(|sister| sister.id != "f0"),
            "a formula should not be offered as its own sister"
        );
    }

    #[tokio::test]
    async fn a_formula_that_stands_alone_offers_no_switch() {
        let pool = loaded(5).await;
        let formula = formula(&pool, "f4").await.unwrap();
        assert_eq!(formula.family, None);
        assert!(formula.sisters.is_empty(), "a formula with no family should offer nothing to switch to");
    }

    #[tokio::test]
    async fn the_switch_shows_where_each_form_stands() {
        // The point of putting the stitch on the switch: a learner who has
        // sewn the statement and never touched the question can see it.
        let pool = loaded(3).await;
        review(&pool, "f1", said(Rating::Good), at(0)).await.unwrap();

        let formula = formula(&pool, "f0").await.unwrap();
        let negation = formula.sisters.iter().find(|sister| sister.form == Form::Negation).unwrap();
        let question = formula.sisters.iter().find(|sister| sister.form == Form::Question).unwrap();
        assert_ne!(negation.stitch, Stitch::New, "the form that was answered should not read as new");
        assert_eq!(question.stitch, Stitch::New, "the form never answered should read as new");
    }

    #[tokio::test]
    async fn the_two_directions_of_a_formula_are_scheduled_apart() {
        // The whole of the reverse drill: understanding runs ahead of
        // producing, and the two must not be averaged into one number.
        let pool = loaded(3).await;
        let produced = review(&pool, "f0", said(Rating::Again), at(0)).await.unwrap();
        let recognised = review(&pool, "f0", said(Rating::Easy).backwards(), at(0)).await.unwrap();

        assert!(
            recognised.due > produced.due,
            "an easy recognition and a failed production should not land on the same day"
        );

        let progress = progress(&pool).await.unwrap();
        assert_eq!(progress.produce.new, 2, "answering one direction should not move the other count");
        assert_eq!(progress.recognise.new, 2);
        assert_eq!(progress.produce.basted + progress.produce.sewn, 1);
        assert_eq!(progress.recognise.basted + progress.recognise.sewn, 1);
    }

    #[tokio::test]
    async fn answering_one_direction_leaves_the_other_untouched() {
        let pool = loaded(2).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let (state, reps): (String, i64) = sqlx::query_as("SELECT state, reps FROM card WHERE kind = 'formula-recognise' AND subject_id = 'f0'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((state.as_str(), reps), ("new", 0), "producing an answer graded the recognising card too");
    }

    #[tokio::test]
    async fn a_formula_is_asked_backwards_only_after_it_has_been_shown() {
        // Asking someone to understand a shape nobody has taught them to
        // build is a guess, not a review.
        let pool = loaded(4).await;
        let first = today(&pool, at(0)).await.unwrap();
        assert!(
            first.queue.iter().all(|due| due.direction == Direction::Produce),
            "a fresh learner was asked to recognise something never shown"
        );

        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let after = today(&pool, at(0)).await.unwrap();
        let backwards: Vec<_> = after
            .queue
            .iter()
            .filter(|due| due.direction == Direction::Recognise)
            .map(|due| due.formula.id.as_str())
            .collect();
        assert_eq!(backwards, vec!["f0"], "the formula just shown should open its recognising side");
    }

    #[tokio::test]
    async fn the_reverse_side_costs_no_slot_in_the_day_budget() {
        // One new formula a day is about new shapes. If the recognising card
        // counted against it, the learner would meet a new shape every other
        // day instead of every day.
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        review(&pool, "f0", said(Rating::Good).backwards(), at(0)).await.unwrap();

        // The budget counts new shapes, and a shape is the producing side.
        // Asserted on the count itself: with a budget of one, a count of two
        // and a count of one behave identically in the queue, so the queue
        // cannot show the difference and this is the only place it is
        // visible.
        assert_eq!(
            introduced_since(&pool, start_of_day(at(0))).await.unwrap(),
            1,
            "the reverse card was counted as a new shape"
        );

        let same_day = today(&pool, at(0)).await.unwrap();
        assert_eq!(
            same_day.queue.iter().filter(|due| due.is_new && due.direction == Direction::Produce).count(),
            0,
            "the formula of the day was already answered, so no second one is due"
        );

        let tomorrow = today(&pool, at(1)).await.unwrap();
        let fresh: Vec<_> = tomorrow
            .queue
            .iter()
            .filter(|due| due.is_new && due.direction == Direction::Produce)
            .map(|due| due.formula.id.as_str())
            .collect();
        assert_eq!(fresh, vec!["f1"], "the reverse drill of yesterday ate the new formula of today");

        // And the day after: the reverse card answered on day 1 must not
        // count against the shape opening on day 2 either.
        review(&pool, "f1", said(Rating::Good), at(1)).await.unwrap();
        review(&pool, "f1", said(Rating::Good).backwards(), at(1)).await.unwrap();
        let later = today(&pool, at(2)).await.unwrap();
        let fresh: Vec<_> = later
            .queue
            .iter()
            .filter(|due| due.is_new && due.direction == Direction::Produce)
            .map(|due| due.formula.id.as_str())
            .collect();
        assert_eq!(fresh, vec!["f2"], "a reverse card counted against the budget of new shapes");
    }

    #[tokio::test]
    async fn pace_appears_once_there_is_a_middle_worth_showing() {
        let pool = loaded(2).await;
        let mut day = 0;
        for took in [4000, 30_000, 5000] {
            let reviewed = review(&pool, "f0", said(Rating::Good).taking(took), at(day)).await.unwrap();
            day = reviewed.due.signed_duration_since(at(0)).num_days();
            // Only the last of the three has enough history behind it.
            if took == 5000 {
                let pace = reviewed.pace.expect("three timed answers should be enough for a pace");
                assert_eq!(pace.answers, 3);
                assert_eq!(pace.last_ms, 5000);
                assert_eq!(
                    pace.typical_ms, 5000,
                    "one interrupted turn should not drag the usual pace: the median of 4000, 5000 and 30000 is 5000"
                );
            } else {
                assert!(reviewed.pace.is_none(), "a pace was reported from {took}ms and too little history");
            }
        }
    }

    #[tokio::test]
    async fn an_untimed_answer_never_invents_a_pace() {
        let pool = loaded(2).await;
        let mut day = 0;
        for _ in 0..4 {
            let reviewed = review(&pool, "f0", said(Rating::Good), at(day)).await.unwrap();
            day = reviewed.due.signed_duration_since(at(0)).num_days();
            assert!(reviewed.pace.is_none(), "a pace was reported for answers nobody timed");
        }
    }

    #[tokio::test]
    async fn pace_does_not_touch_the_schedule() {
        // The decision of the owner (2026-09-03): speed is shown, not
        // scheduled on. A home-made correction over FSRS would move every
        // interval with no way to tell what moved it.
        let quick = loaded(2).await;
        let slow = loaded(2).await;

        let fast = review(&quick, "f0", said(Rating::Good).taking(900), at(0)).await.unwrap();
        let crawl = review(&slow, "f0", said(Rating::Good).taking(45_000), at(0)).await.unwrap();
        assert_eq!(fast.due, crawl.due, "how long the answer took changed when the formula comes back");
        assert_eq!(fast.stitch, crawl.stitch);
    }

    #[tokio::test]
    async fn a_queued_card_carries_its_pace() {
        let pool = loaded(2).await;
        let mut day = 0;
        for took in [3000, 3500, 4000] {
            let reviewed = review(&pool, "f0", said(Rating::Good).taking(took), at(day)).await.unwrap();
            day = reviewed.due.signed_duration_since(at(0)).num_days();
        }

        let due = today(&pool, at(day))
            .await
            .unwrap()
            .queue
            .into_iter()
            .find(|due| due.formula.id == "f0" && due.direction == Direction::Produce)
            .expect("the formula should be waiting on the day it is due");
        assert_eq!(due.pace.expect("a drilled card should carry its pace").typical_ms, 3500);
    }

    #[tokio::test]
    async fn the_counts_follow_the_learner() {
        let pool = loaded(4).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let counts = counts(&pool).await.unwrap();
        assert_eq!(counts.new, 3, "the formula answered should have left the new pile");
        assert_eq!(counts.basted + counts.sewn, 1);
    }
}
