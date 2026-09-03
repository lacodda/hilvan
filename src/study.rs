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

use crate::scheduling::{Card, Rating, Scheduled, Scheduler, Stitch};

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

/// One item in today's queue: a formula and the state it is in.
#[derive(Debug, Clone, Serialize)]
pub struct Due {
    pub formula: Formula,
    pub stitch: Stitch,
    /// True when this formula is being seen for the first time.
    pub is_new: bool,
    pub due: DateTime<Utc>,
}

/// How many formulas stand in each state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub new: i64,
    pub basted: i64,
    pub sewn: i64,
}

/// The answer to "what am I doing today".
#[derive(Debug, Clone, Serialize)]
pub struct Today {
    /// Formulas waiting to come back, plus at most one new one.
    pub queue: Vec<Due>,
    /// Formulas already reviewed since midnight UTC.
    pub reviewed_today: i64,
    pub counts: Counts,
}

/// What the learner sends back after answering.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Answer {
    pub rating: Rating,
    /// How long the answer took, when the client measured it. Stored now,
    /// used as the second dimension of a card in v0.2.0.
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

/// What an answer changed.
#[derive(Debug, Clone, Serialize)]
pub struct Reviewed {
    pub stitch: Stitch,
    pub due: DateTime<Utc>,
    /// Days until this formula comes back.
    pub interval_days: i64,
}

/// Today's queue: everything due, then one new formula if the day has room.
///
/// Reviews come first and are never crowded out by new material - a day where
/// the learner starts something new while forgetting yesterday's is the
/// failure mode every course product has.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn today(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Today> {
    let counts = counts(pool).await?;
    let mut queue = Vec::new();

    let due_rows = sqlx::query(
        "SELECT f.id FROM card c
         JOIN formula f ON f.id = c.subject_id
         WHERE c.kind = 'formula' AND c.state != 'new' AND c.due <= ?
         ORDER BY c.due, f.position",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await
    .context("failed to read the review queue")?;

    for row in due_rows {
        let id: String = row.get("id");
        queue.push(Due {
            formula: formula(pool, &id).await?,
            stitch: card(pool, &id).await?.map_or(Stitch::New, |card| card.stitch),
            is_new: false,
            due: now,
        });
    }

    let started_today = introduced_since(pool, start_of_day(now)).await?;
    if usize::try_from(started_today).unwrap_or(usize::MAX) < NEW_FORMULAS_PER_DAY {
        // The next formula is the first one in the pack's own sequence that
        // has never been answered: the pack decides the order, not the clock.
        if let Some(id) = next_new(pool).await? {
            queue.push(Due {
                formula: formula(pool, &id).await?,
                stitch: Stitch::New,
                is_new: true,
                due: now,
            });
        }
    }

    Ok(Today {
        queue,
        reviewed_today: reviewed_since(pool, start_of_day(now)).await?,
        counts,
    })
}

/// Records an answer and reschedules the formula.
///
/// # Errors
///
/// Fails when the formula has no card - which means it is not in any loaded
/// pack - or when the database rejects a statement.
pub async fn review(pool: &SqlitePool, formula_id: &str, answer: Answer, now: DateTime<Utc>) -> Result<Reviewed> {
    let card = card(pool, formula_id)
        .await?
        .with_context(|| format!("there is no formula called {formula_id}"))?;

    let Scheduled { card: next, elapsed_days } = Scheduler::new().review(&card, answer.rating, now);

    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    sqlx::query(
        "UPDATE card SET state = ?, stability = ?, difficulty = ?, due = ?, reps = ?, lapses = ?,
             last_reviewed = ?, introduced_at = COALESCE(introduced_at, ?)
         WHERE kind = 'formula' AND subject_id = ?",
    )
    .bind(next.stitch.as_str())
    .bind(next.stability)
    .bind(next.difficulty)
    .bind(next.due.to_rfc3339())
    .bind(next.reps)
    .bind(next.lapses)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(formula_id)
    .execute(&mut *tx)
    .await
    .context("failed to save the card")?;

    // The history is kept in full so the schedule can be recomputed when the
    // memory model changes (ADR 0003).
    sqlx::query(
        "INSERT INTO review (card_id, rating, reviewed_at, elapsed_days, stability, difficulty, duration_ms)
         SELECT id, ?, ?, ?, ?, ?, ? FROM card WHERE kind = 'formula' AND subject_id = ?",
    )
    .bind(answer.rating.as_i64())
    .bind(now.to_rfc3339())
    .bind(elapsed_days)
    .bind(next.stability)
    .bind(next.difficulty)
    .bind(answer.duration_ms)
    .bind(formula_id)
    .execute(&mut *tx)
    .await
    .context("failed to record the review")?;
    tx.commit().await.context("failed to commit the review")?;

    Ok(Reviewed {
        stitch: next.stitch,
        due: next.due,
        interval_days: (next.due - now).num_days(),
    })
}

/// One formula with its samples and slots.
///
/// # Errors
///
/// Fails when there is no such formula, or when the database rejects a query.
pub async fn formula(pool: &SqlitePool, id: &str) -> Result<Formula> {
    let row = sqlx::query("SELECT id, name, pattern, explanation FROM formula WHERE id = ?")
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

    Ok(Formula {
        id: row.get("id"),
        name: row.get("name"),
        pattern: row.get("pattern"),
        explanation: row.get("explanation"),
        samples,
        slots,
    })
}

/// How many formulas stand in each state.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn counts(pool: &SqlitePool) -> Result<Counts> {
    let rows = sqlx::query("SELECT state, count(*) AS n FROM card WHERE kind = 'formula' GROUP BY state")
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

async fn card(pool: &SqlitePool, formula_id: &str) -> Result<Option<Card>> {
    let row = sqlx::query(
        "SELECT state, stability, difficulty, due, reps, lapses, last_reviewed
         FROM card WHERE kind = 'formula' AND subject_id = ?",
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;

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
            let _ = write!(
                toml,
                "
[[formula]]
id = \"f{index}\"
name = \"n{index}\"
pattern = \"<pronoun> + x\"
                 explanation = \"e\"
order = {}

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
        review(
            &pool,
            "f0",
            Answer {
                rating: Rating::Good,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap();

        let today = today(&pool, at(0)).await.unwrap();
        assert!(
            today.queue.iter().all(|due| !due.is_new),
            "a second new formula was offered on the same day: {:?}",
            today.queue.iter().map(|due| &due.formula.id).collect::<Vec<_>>()
        );
        assert_eq!(today.reviewed_today, 1);
    }

    #[tokio::test]
    async fn tomorrow_brings_the_next_formula() {
        let pool = loaded(5).await;
        review(
            &pool,
            "f0",
            Answer {
                rating: Rating::Good,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap();

        let tomorrow = today(&pool, at(1)).await.unwrap();
        let new: Vec<_> = tomorrow.queue.iter().filter(|due| due.is_new).map(|due| due.formula.id.as_str()).collect();
        assert_eq!(new, vec!["f1"], "a new day should open exactly the next formula in the pack");
    }

    #[tokio::test]
    async fn a_formula_answered_comes_back_and_one_forgotten_comes_back_sooner() {
        let pool = loaded(3).await;
        let good = review(
            &pool,
            "f0",
            Answer {
                rating: Rating::Good,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap();
        let again = review(
            &pool,
            "f1",
            Answer {
                rating: Rating::Again,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap();

        assert!(again.due <= good.due, "a formula that would not come should return sooner than one that did");
        assert_eq!(again.stitch, Stitch::Basted);
    }

    #[tokio::test]
    async fn a_review_is_kept_in_full() {
        // The history is the asset: the schedule can be recomputed from it,
        // and nothing else can.
        let pool = loaded(2).await;
        review(
            &pool,
            "f0",
            Answer {
                rating: Rating::Hard,
                duration_ms: Some(4200),
            },
            at(0),
        )
        .await
        .unwrap();

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
        review(
            &pool,
            "f0",
            Answer {
                rating: Rating::Again,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap();

        let later = today(&pool, at(2)).await.unwrap();
        assert_eq!(later.queue.first().map(|due| due.formula.id.as_str()), Some("f0"));
        assert!(!later.queue[0].is_new);
    }

    #[tokio::test]
    async fn a_formula_not_yet_due_stays_out_of_the_queue() {
        let pool = loaded(3).await;
        let reviewed = review(
            &pool,
            "f0",
            Answer {
                rating: Rating::Easy,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap();
        assert!(reviewed.interval_days >= 1, "an easy answer should push a formula past today");

        let same_day = today(&pool, at(0)).await.unwrap();
        assert!(
            same_day.queue.iter().all(|due| due.formula.id != "f0"),
            "an answered formula came back the same day"
        );
    }

    #[tokio::test]
    async fn answering_something_that_is_not_a_formula_fails() {
        let pool = loaded(1).await;
        let error = review(
            &pool,
            "not-a-formula",
            Answer {
                rating: Rating::Good,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap_err();
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
    async fn the_counts_follow_the_learner() {
        let pool = loaded(4).await;
        review(
            &pool,
            "f0",
            Answer {
                rating: Rating::Good,
                duration_ms: None,
            },
            at(0),
        )
        .await
        .unwrap();

        let counts = counts(&pool).await.unwrap();
        assert_eq!(counts.new, 3, "the formula answered should have left the new pile");
        assert_eq!(counts.basted + counts.sewn, 1);
    }
}
