//! What slipped: what the learner had come up with and then could not.
//!
//! A slip is an asked "again" on a card whose answer before it was not one -
//! it had come - given at least a day earlier. Not every "again": a formula
//! failed twice in its first sitting was never there to slip, and the
//! learning steps of one day are learning, not forgetting. A slip is the
//! formula that was sewn and would not come on Tuesday, the word met last
//! week and gone, the word declared known in the inventory and not known
//! after all.
//!
//! The list is told by week, Monday to Sunday, and last week's opens the
//! first sitting of a new one: a note on Today, while the week has no
//! answer yet. It keeps no record of having been shown - the week's first
//! answer is what closes it.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::scheduling::{Direction, Kind, Stitch};
use crate::study::parse_stamp;

/// How many names the note on Today carries; the rest are on the list.
const NAMED: usize = 3;

/// One thing that slipped, the first time it slipped in the week.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slip {
    /// `formula` or `word`.
    pub kind: &'static str,
    /// A formula's id, or a word's: `en:house`.
    pub id: String,
    /// What the learner calls it: the formula's name, the word's lemma.
    pub name: String,
    /// Which way round a formula slipped; `None` for a word.
    pub direction: Option<Direction>,
    /// What a word means; `None` for a formula.
    pub gloss: Option<String>,
    pub at: DateTime<Utc>,
    /// Where it stands now: a slip mended since reads sewn again.
    pub stitch: Stitch,
    /// Whether the answer it slipped from was the learner's declaration
    /// that they knew it, rather than an answer.
    pub declared: bool,
}

/// One week's slips, the earliest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Week {
    /// Its Monday.
    pub starts_on: NaiveDate,
    pub slips: Vec<Slip>,
}

/// This week and last week.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Drift {
    pub this_week: Week,
    pub last_week: Week,
}

/// Last week's slips, as the note on Today tells them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    /// Its Monday.
    pub starts_on: NaiveDate,
    /// How many slipped.
    pub slipped: i64,
    /// The first few, by name.
    pub names: Vec<String>,
}

/// Midnight UTC on the Monday of the week `now` falls in - the day the
/// tutor's weeks start, on the UTC days its days are counted in.
#[must_use]
pub fn start_of_week(now: DateTime<Utc>) -> DateTime<Utc> {
    let day = now.date_naive() - Duration::days(i64::from(now.weekday().num_days_from_monday()));
    day.and_hms_opt(0, 0, 0).map_or(now, |midnight| midnight.and_utc())
}

/// What slipped this week and last week.
///
/// # Errors
///
/// Fails when the database rejects a query or holds a moment that is not
/// one.
pub async fn weeks(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Drift> {
    let monday = start_of_week(now);
    let before = monday - Duration::weeks(1);
    Ok(Drift {
        this_week: week(pool, monday, now).await?,
        last_week: week(pool, before, monday).await?,
    })
}

/// Last week's slips, while this week has had no asked answer yet and
/// something did slip: what the first sitting of the week opens on.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn note(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Option<Note>> {
    let monday = start_of_week(now);
    let answered: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM review WHERE source = 'asked' AND reviewed_at >= ?)")
        .bind(monday.to_rfc3339())
        .fetch_one(pool)
        .await
        .context("failed to ask whether the week has begun")?;
    if answered {
        return Ok(None);
    }
    let last = week(pool, monday - Duration::weeks(1), monday).await?;
    if last.slips.is_empty() {
        return Ok(None);
    }
    Ok(Some(Note {
        starts_on: last.starts_on,
        slipped: i64::try_from(last.slips.len()).unwrap_or(i64::MAX),
        names: last.slips.iter().take(NAMED).map(|slip| slip.name.clone()).collect(),
    }))
}

/// The slips from `from` up to `to`, each card once, at its first.
async fn week(pool: &SqlitePool, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Week> {
    // The answer before each answer, by card, in the order they were given:
    // a window over the whole log, then the week cut from it - cutting
    // first would lose the answer a Monday slip slipped from.
    let rows = sqlx::query(
        "SELECT c.kind, c.subject_id, c.state, x.reviewed_at, x.before_source,
                f.name AS formula_name, w.lemma, w.gloss
         FROM (
             SELECT r.card_id, r.rating, r.source, r.reviewed_at, r.elapsed_days, r.id,
                    LAG(r.rating) OVER answers AS before_rating,
                    LAG(r.source) OVER answers AS before_source
             FROM review r
             WINDOW answers AS (PARTITION BY r.card_id ORDER BY r.reviewed_at, r.id)
         ) x
         JOIN card c ON c.id = x.card_id
         LEFT JOIN formula f ON f.id = c.subject_id AND c.kind != 'word'
         LEFT JOIN word w ON w.id = c.subject_id AND c.kind = 'word'
         WHERE x.rating = 1 AND x.source = 'asked'
           AND x.before_rating IS NOT NULL AND x.before_rating != 1
           AND x.elapsed_days >= 1
           AND x.reviewed_at >= ? AND x.reviewed_at < ?
         ORDER BY x.reviewed_at, x.id",
    )
    .bind(from.to_rfc3339())
    .bind(to.to_rfc3339())
    .fetch_all(pool)
    .await
    .context("failed to read what slipped")?;

    let mut seen = HashSet::new();
    let mut slips = Vec::new();
    for row in rows {
        let kind_text: String = row.get("kind");
        let id: String = row.get("subject_id");
        if !seen.insert((kind_text.clone(), id.clone())) {
            continue;
        }
        let kind = Kind::parse(&kind_text)?;
        let (name, direction, gloss) = match kind {
            // A formula a reload of the pack dropped is still the learner's
            // slip: named by its id rather than left out.
            Kind::Formula(direction) => (
                row.get::<Option<String>, _>("formula_name").unwrap_or_else(|| id.clone()),
                Some(direction),
                None,
            ),
            Kind::Word => (
                row.get::<Option<String>, _>("lemma")
                    .unwrap_or_else(|| id.split_once(':').map_or_else(|| id.clone(), |(_, lemma)| lemma.to_string())),
                None,
                row.get::<Option<String>, _>("gloss"),
            ),
        };
        slips.push(Slip {
            kind: if kind == Kind::Word { "word" } else { "formula" },
            name,
            direction,
            gloss,
            at: parse_stamp(&row.get::<String, _>("reviewed_at"))?,
            stitch: Stitch::parse(&row.get::<String, _>("state"))?,
            declared: row.get::<Option<String>, _>("before_source").as_deref() == Some("declared"),
            id,
        });
    }
    Ok(Week {
        starts_on: from.date_naive(),
        slips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory;
    use crate::pack;
    use crate::scheduling::Rating;
    use crate::study::{self, Answer, Graded};

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        let pack: pack::Pack = toml::from_str(
            "id = \"t\"\nversion = 1\nnative = \"ru\"\ntarget = \"en\"\n\n[[formula]]\nid = \"f0\"\nname = \"to be\"\npattern = \"x\"\nexplanation = \"e\"\norder = 10\n\n  [[formula.sample]]\n  native = \"Он врач.\"\n  target = \"He is a doctor.\"\n  words = [\"doctor\"]\n\n[[word]]\nlemma = \"doctor\"\ngloss = \"врач\"\n",
        )
        .expect("the fixture should parse");
        pack::load(&pool, &pack).await.expect("the fixture should load");
        pool
    }

    /// Monday 7 September 2026, nine in the morning, and the days after it.
    fn at(day: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-07T09:00:00Z").unwrap().with_timezone(&Utc) + Duration::days(day)
    }

    async fn heard(pool: &SqlitePool, rating: Rating, now: DateTime<Utc>) {
        let answer = Answer::Recognise { rating, duration_ms: None };
        study::review(pool, "f0", answer, now).await.unwrap();
    }

    async fn word(pool: &SqlitePool, rating: Rating, now: DateTime<Utc>) {
        study::review_word(pool, "en:doctor", Graded { rating, duration_ms: None }, now).await.unwrap();
    }

    #[test]
    fn a_week_starts_on_monday() {
        let monday = at(0) - Duration::hours(9);
        for day in 0..7 {
            assert_eq!(start_of_week(at(day)), monday, "day {day}");
        }
        assert_eq!(start_of_week(at(7)), monday + Duration::weeks(1));
    }

    #[tokio::test]
    async fn what_had_come_and_then_did_not_slipped() {
        let pool = pool().await;
        heard(&pool, Rating::Good, at(0)).await;
        heard(&pool, Rating::Again, at(3)).await;

        let drift = weeks(&pool, at(4)).await.unwrap();
        let slips = &drift.this_week.slips;
        assert_eq!(slips.len(), 1);
        assert_eq!(
            (slips[0].name.as_str(), slips[0].direction, slips[0].at),
            ("to be", Some(Direction::Recognise), at(3))
        );
        assert!(!slips[0].declared);
        assert_eq!(drift.this_week.starts_on, at(0).date_naive());
        assert_eq!(drift.last_week.slips, Vec::new());
    }

    #[tokio::test]
    async fn learning_is_not_slipping() {
        let pool = pool().await;
        // Not there the first time, and not there again the same day: it was
        // never there to slip.
        heard(&pool, Rating::Again, at(0)).await;
        heard(&pool, Rating::Again, at(0) + Duration::minutes(10)).await;
        // There, then gone within the hour: the steps of one sitting.
        heard(&pool, Rating::Good, at(0) + Duration::minutes(20)).await;
        heard(&pool, Rating::Again, at(0) + Duration::minutes(40)).await;
        // Not there two days on, after a not-there: still not a slip.
        heard(&pool, Rating::Again, at(2)).await;
        assert_eq!(weeks(&pool, at(3)).await.unwrap().this_week.slips, Vec::new());
    }

    #[tokio::test]
    async fn a_word_declared_known_and_not_known_slipped_from_the_declaration() {
        let pool = pool().await;
        let yes = inventory::Answer {
            word: "en:doctor".to_string(),
            known: true,
        };
        inventory::answer(&pool, &[yes], at(0)).await.unwrap();
        word(&pool, Rating::Again, at(2)).await;
        let slips = weeks(&pool, at(3)).await.unwrap().this_week.slips;
        assert_eq!(slips.len(), 1);
        assert_eq!(
            (slips[0].kind, slips[0].name.as_str(), slips[0].gloss.as_deref()),
            ("word", "doctor", Some("врач"))
        );
        assert!(slips[0].declared, "the answer it slipped from was a declaration");
    }

    #[tokio::test]
    async fn a_card_that_slipped_twice_in_a_week_is_listed_once() {
        let pool = pool().await;
        heard(&pool, Rating::Good, at(0)).await;
        heard(&pool, Rating::Again, at(1)).await;
        heard(&pool, Rating::Good, at(2)).await;
        heard(&pool, Rating::Again, at(4)).await;
        let slips = weeks(&pool, at(5)).await.unwrap().this_week.slips;
        assert_eq!(slips.len(), 1);
        assert_eq!(slips[0].at, at(1), "the first slip of the week");
    }

    #[tokio::test]
    async fn last_weeks_slips_open_the_week_until_its_first_answer() {
        let pool = pool().await;
        heard(&pool, Rating::Good, at(0)).await;
        word(&pool, Rating::Good, at(0)).await;
        heard(&pool, Rating::Again, at(3)).await;
        word(&pool, Rating::Again, at(4)).await;
        assert_eq!(note(&pool, at(5)).await.unwrap(), None, "the note is about last week, not this one");

        let next_monday = at(7) + Duration::hours(1);
        let note_on_monday = note(&pool, next_monday).await.unwrap().expect("a week that slipped should be told");
        assert_eq!(note_on_monday.slipped, 2);
        assert_eq!(note_on_monday.names, vec!["to be".to_string(), "doctor".to_string()]);
        assert_eq!(note_on_monday.starts_on, at(0).date_naive());
        // Wednesday, still nothing answered this week: the week has not begun.
        assert!(note(&pool, at(9)).await.unwrap().is_some());

        heard(&pool, Rating::Good, at(9)).await;
        assert_eq!(
            note(&pool, at(9) + Duration::hours(1)).await.unwrap(),
            None,
            "the week's first answer closes it"
        );
    }

    #[tokio::test]
    async fn a_week_where_nothing_slipped_says_nothing() {
        let pool = pool().await;
        heard(&pool, Rating::Good, at(0)).await;
        heard(&pool, Rating::Good, at(3)).await;
        assert_eq!(note(&pool, at(8)).await.unwrap(), None);
    }
}
