//! Breaks: the learner away, and the schedule's clock stopped while they are.
//!
//! What a break does to a card is [`crate::scheduling::comes_back`]; this
//! module is where breaks are kept. A break is taken now - "I am off for a
//! fortnight", with or without a day back - or counted afterwards, for an
//! absence nobody declared: the time since the last answer, in one tap on
//! the day the learner returns. Both are the same row, and the schedule
//! cannot tell them apart.
//!
//! A break holds no asked answer: one cannot be counted back past the last
//! answer, and an answer given while one is going on ends it - answering is
//! being back.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::scheduling::Break;
use crate::study::parse_stamp;

/// How long without an answer before the tutor offers to count the absence
/// as a break.
///
/// Three days: a weekend off is two, and a weekend is not an absence worth
/// stopping the clock for - what came due over it is a sitting's worth,
/// and it should be owed. Three days and more is the trip, the illness, the
/// week the tutor was forgotten, and that is what the stretch is for.
pub const AWAY_DAYS: i64 = 3;

/// A break as it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Taken {
    pub id: i64,
    pub starts_at: DateTime<Utc>,
    /// `None` until the learner says they are back.
    pub ends_at: Option<DateTime<Utc>>,
}

impl Taken {
    /// The break as the schedule counts it.
    #[must_use]
    pub const fn as_break(&self) -> Break {
        Break {
            starts_at: self.starts_at,
            ends_at: self.ends_at,
        }
    }
}

/// Why a break could not be taken, ended or removed: the learner's to fix,
/// said in a sentence.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("there is no break {0}")]
    NotFound(i64),
    #[error("you are already on a break")]
    AlreadyAway,
    #[error("the day you are back has to come after today")]
    BackTooSoon,
    #[error("there is no absence to count: you answered within the last {AWAY_DAYS} days, or the time away is already a break")]
    NotAway,
    #[error("break {0} is not going on")]
    NotGoingOn(i64),
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

/// Every break, the earliest first.
///
/// # Errors
///
/// Fails when the database rejects the query or holds a moment that is not
/// one.
pub async fn all<'e, E: sqlx::SqliteExecutor<'e>>(executor: E) -> Result<Vec<Taken>> {
    sqlx::query("SELECT id, starts_at, ends_at FROM break ORDER BY starts_at, id")
        .fetch_all(executor)
        .await
        .context("failed to read the breaks")?
        .iter()
        .map(taken)
        .collect()
}

/// The breaks as the schedule counts them.
#[must_use]
pub fn of(taken: &[Taken]) -> Vec<Break> {
    taken.iter().map(Taken::as_break).collect()
}

/// The break going on at `now`, if one is.
#[must_use]
pub fn in_force(taken: &[Taken], now: DateTime<Utc>) -> Option<Taken> {
    taken.iter().find(|taken| taken.as_break().holds(now)).copied()
}

/// Starts a break now, until the learner is back or until `until`.
///
/// # Errors
///
/// `AlreadyAway` while another is going on, `BackTooSoon` for a day back
/// that is not after now, `Database` when a statement fails.
pub async fn start(pool: &SqlitePool, until: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Result<Taken, Refused> {
    if until.is_some_and(|until| until <= now) {
        return Err(Refused::BackTooSoon);
    }
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    if in_force(&all(&mut *tx).await?, now).is_some() {
        return Err(Refused::AlreadyAway);
    }
    let taken = insert(&mut tx, now, until).await?;
    tx.commit().await.context("failed to commit the break")?;
    Ok(taken)
}

/// The last answer the tutor asked for, when the learner has been away
/// since it for [`AWAY_DAYS`] or more and no break covers any of that time:
/// the absence the tutor can offer to count as a break.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn away(pool: &SqlitePool, taken: &[Taken], now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
    Ok(absence(last_answer(pool).await?, taken, now))
}

/// The absence a last answer and the breaks leave at `now`, if there is one.
fn absence(last: Option<DateTime<Utc>>, taken: &[Taken], now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let last = last?;
    // Any break reaching past the last answer means the learner has already
    // said something about this absence; the tutor does not second-guess it.
    let declared = taken.iter().any(|taken| taken.ends_at.is_none_or(|end| end > last));
    (now - last >= Duration::days(AWAY_DAYS) && !declared).then_some(last)
}

/// Counts the time since the last answer as a break that ended now: the
/// learner was away and is back, and what came due meanwhile comes back
/// over the days ahead rather than all of it today.
///
/// # Errors
///
/// `NotAway` when there is no such absence (see [`away`]), `Database` when a
/// statement fails.
pub async fn count_away(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Taken, Refused> {
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let taken = all(&mut *tx).await?;
    let Some(since) = absence(last_answer(&mut *tx).await?, &taken, now) else {
        return Err(Refused::NotAway);
    };
    let counted = insert(&mut tx, since, Some(now)).await?;
    tx.commit().await.context("failed to commit the break")?;
    Ok(counted)
}

/// Ends a break now: the learner is back.
///
/// # Errors
///
/// `NotFound` for no such break, `NotGoingOn` for one that is over,
/// `Database` when a statement fails.
pub async fn end(pool: &SqlitePool, id: i64, now: DateTime<Utc>) -> Result<Taken, Refused> {
    let taken = one(pool, id).await?;
    if !taken.as_break().holds(now) {
        return Err(Refused::NotGoingOn(id));
    }
    if taken.starts_at == now {
        // Begun and ended in the same instant: no time stood still, and a
        // break of no length is not a break.
        sqlx::query("DELETE FROM break WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .context("failed to remove a break of no length")?;
    } else {
        sqlx::query("UPDATE break SET ends_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(id)
            .execute(pool)
            .await
            .context("failed to end the break")?;
    }
    Ok(Taken { ends_at: Some(now), ..taken })
}

/// Removes a break as if it had never been taken: what it held comes due
/// again where FSRS put it.
///
/// # Errors
///
/// `NotFound` for no such break, `Database` when the statement fails.
pub async fn remove(pool: &SqlitePool, id: i64) -> Result<(), Refused> {
    let removed = sqlx::query("DELETE FROM break WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("failed to remove the break")?
        .rows_affected();
    if removed == 0 {
        return Err(Refused::NotFound(id));
    }
    Ok(())
}

/// Ends whatever break is going on, inside the transaction that records an
/// asked answer: a learner who answers is back, and a break never holds an
/// answer.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub(crate) async fn end_by_answer(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, now: DateTime<Utc>) -> Result<()> {
    let stamp = now.to_rfc3339();
    for taken in all(&mut **tx).await?.into_iter().filter(|taken| taken.as_break().holds(now)) {
        let query = if taken.starts_at == now {
            sqlx::query("DELETE FROM break WHERE id = ?").bind(taken.id)
        } else {
            sqlx::query("UPDATE break SET ends_at = ? WHERE id = ?").bind(stamp.clone()).bind(taken.id)
        };
        query.execute(&mut **tx).await.context("failed to end a break with an answer")?;
    }
    Ok(())
}

/// The moment of the last answer the tutor asked for.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn last_answer<'e, E: sqlx::SqliteExecutor<'e>>(executor: E) -> Result<Option<DateTime<Utc>>> {
    // Every moment is written by `to_rfc3339`, so the text sorts as time.
    let last: Option<String> = sqlx::query_scalar("SELECT max(reviewed_at) FROM review WHERE source = 'asked'")
        .fetch_one(executor)
        .await
        .context("failed to find the last answer")?;
    last.map(|text| parse_stamp(&text)).transpose()
}

async fn one(pool: &SqlitePool, id: i64) -> Result<Taken, Refused> {
    let row = sqlx::query("SELECT id, starts_at, ends_at FROM break WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a break")?
        .ok_or(Refused::NotFound(id))?;
    Ok(taken(&row)?)
}

async fn insert(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, starts_at: DateTime<Utc>, ends_at: Option<DateTime<Utc>>) -> Result<Taken> {
    let id: i64 = sqlx::query_scalar("INSERT INTO break (starts_at, ends_at) VALUES (?, ?) RETURNING id")
        .bind(starts_at.to_rfc3339())
        .bind(ends_at.map(|at| at.to_rfc3339()))
        .fetch_one(&mut **tx)
        .await
        .context("failed to record the break")?;
    Ok(Taken { id, starts_at, ends_at })
}

fn taken(row: &SqliteRow) -> Result<Taken> {
    Ok(Taken {
        id: row.get("id"),
        starts_at: parse_stamp(&row.get::<String, _>("starts_at"))?,
        ends_at: row.get::<Option<String>, _>("ends_at").map(|text| parse_stamp(&text)).transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;
    use crate::scheduling::Rating;
    use crate::study::{self, Answer};

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        let pack: pack::Pack = toml::from_str(
            "id = \"t\"\nversion = 1\nnative = \"ru\"\ntarget = \"en\"\n\n[[formula]]\nid = \"f0\"\nname = \"n\"\npattern = \"x\"\nexplanation = \"e\"\norder = 10\n\n  [[formula.sample]]\n  native = \"a\"\n  target = \"b\"\n",
        )
        .expect("the fixture should parse");
        pack::load(&pool, &pack).await.expect("the fixture should load");
        pool
    }

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z").unwrap().with_timezone(&Utc) + Duration::days(day)
    }

    async fn answer(pool: &SqlitePool, now: DateTime<Utc>) {
        let heard = Answer::Recognise {
            rating: Rating::Good,
            duration_ms: None,
        };
        study::review(pool, "f0", heard, now).await.expect("the answer should be taken");
    }

    #[tokio::test]
    async fn a_break_is_one_at_a_time() {
        let pool = pool().await;
        let taken = start(&pool, None, at(0)).await.unwrap();
        assert_eq!(in_force(&all(&pool).await.unwrap(), at(5)), Some(taken));
        assert!(matches!(start(&pool, None, at(1)).await, Err(Refused::AlreadyAway)));
    }

    #[tokio::test]
    async fn the_day_back_comes_after_the_day_away() {
        let pool = pool().await;
        assert!(matches!(start(&pool, Some(at(0)), at(0)).await, Err(Refused::BackTooSoon)));
        assert!(matches!(start(&pool, Some(at(-1)), at(0)).await, Err(Refused::BackTooSoon)));
        let planned = start(&pool, Some(at(7)), at(0)).await.unwrap();
        assert_eq!(planned.ends_at, Some(at(7)));
        // Past the day back the break is over by itself.
        assert_eq!(in_force(&all(&pool).await.unwrap(), at(7)), None);
        start(&pool, None, at(8)).await.expect("a new break can start once the last has ended");
    }

    #[tokio::test]
    async fn being_back_ends_the_break_now() {
        let pool = pool().await;
        let taken = start(&pool, None, at(0)).await.unwrap();
        let ended = end(&pool, taken.id, at(3)).await.unwrap();
        assert_eq!(ended.ends_at, Some(at(3)));
        assert_eq!(all(&pool).await.unwrap(), vec![ended]);
        assert!(
            matches!(end(&pool, taken.id, at(4)).await, Err(Refused::NotGoingOn(_))),
            "a break that is over cannot end again"
        );
        assert!(matches!(end(&pool, 99, at(4)).await, Err(Refused::NotFound(99))));
    }

    #[tokio::test]
    async fn a_break_ended_as_it_began_leaves_nothing() {
        let pool = pool().await;
        let taken = start(&pool, None, at(0)).await.unwrap();
        end(&pool, taken.id, at(0)).await.unwrap();
        assert!(all(&pool).await.unwrap().is_empty(), "a break of no length was kept");
    }

    #[tokio::test]
    async fn a_break_removed_is_gone() {
        let pool = pool().await;
        let taken = start(&pool, Some(at(2)), at(0)).await.unwrap();
        remove(&pool, taken.id).await.unwrap();
        assert_eq!(all(&pool).await.unwrap(), Vec::new());
        assert!(matches!(remove(&pool, taken.id).await, Err(Refused::NotFound(_))));
    }

    #[tokio::test]
    async fn an_absence_is_counted_from_the_last_answer_to_now() {
        let pool = pool().await;
        assert_eq!(away(&pool, &[], at(10)).await.unwrap(), None, "a learner who never answered was never away");

        answer(&pool, at(0)).await;
        assert_eq!(away(&pool, &[], at(0) + Duration::days(AWAY_DAYS) - Duration::seconds(1)).await.unwrap(), None);
        assert_eq!(away(&pool, &[], at(AWAY_DAYS)).await.unwrap(), Some(at(0)));

        let counted = count_away(&pool, at(6)).await.unwrap();
        assert_eq!((counted.starts_at, counted.ends_at), (at(0), Some(at(6))));
        let taken = all(&pool).await.unwrap();
        assert_eq!(away(&pool, &taken, at(7)).await.unwrap(), None, "the same absence is not counted twice");
        assert!(matches!(count_away(&pool, at(7)).await, Err(Refused::NotAway)));
    }

    #[tokio::test]
    async fn an_absence_the_learner_already_spoke_of_is_not_second_guessed() {
        let pool = pool().await;
        answer(&pool, at(0)).await;
        let taken = start(&pool, Some(at(2)), at(1)).await.unwrap();
        assert_eq!(away(&pool, &[taken], at(9)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_answer_ends_the_break_it_falls_in_and_no_other() {
        let pool = pool().await;
        let earlier = start(&pool, Some(at(2)), at(0)).await.unwrap();
        let open = start(&pool, None, at(3)).await.unwrap();
        answer(&pool, at(5)).await;
        let taken = all(&pool).await.unwrap();
        assert_eq!(taken[0], earlier, "a break that was over is left as it was");
        assert_eq!(taken[1], Taken { ends_at: Some(at(5)), ..open });
    }
}
