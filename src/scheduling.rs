//! When something comes back.
//!
//! The memory model is FSRS through `rs-fsrs` (ADR 0003). This module is the
//! only place that knows it: everywhere else works with [`Card`], [`Rating`]
//! and [`Stitch`], which are hilvan's own words for what the learner sees.

use chrono::{DateTime, Duration, Utc};
use rs_fsrs::{FSRS, Parameters, Rating as FsrsRating, State as FsrsState};
use serde::{Deserialize, Serialize};

/// How well the learner produced the formula, in the four answers the drill
/// offers. The numbers are FSRS's and are stored, so they must not be
/// renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    /// Nothing came. The formula goes back to the start.
    Again = 1,
    /// It came, but slowly and with effort.
    Hard = 2,
    /// It came.
    Good = 3,
    /// It came without thinking about it.
    Easy = 4,
}

impl Rating {
    /// The number stored in the review history.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self as i64
    }

    /// Reads a rating back from the review history.
    ///
    /// # Errors
    ///
    /// Fails when the number is not one of the four ratings, which means the
    /// row was written by something other than this module.
    pub fn from_i64(value: i64) -> Result<Self, InvalidRating> {
        match value {
            1 => Ok(Self::Again),
            2 => Ok(Self::Hard),
            3 => Ok(Self::Good),
            4 => Ok(Self::Easy),
            other => Err(InvalidRating(other)),
        }
    }

    const fn to_fsrs(self) -> FsrsRating {
        match self {
            Self::Again => FsrsRating::Again,
            Self::Hard => FsrsRating::Hard,
            Self::Good => FsrsRating::Good,
            Self::Easy => FsrsRating::Easy,
        }
    }
}

/// A rating that is not one of the four.
#[derive(Debug, thiserror::Error)]
#[error("{0} is not a rating: expected 1 (again), 2 (hard), 3 (good) or 4 (easy)")]
pub struct InvalidRating(i64);

/// How far along a formula is, in the product's own vocabulary: the stitch the
/// name comes from. FSRS's four internal states collapse to three because the
/// learner has no use for the difference between learning something new and
/// learning it again after a lapse - both mean "not yet holding".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stitch {
    /// Never answered.
    New,
    /// Tacked in place: answered, but the thread is loose.
    Basted,
    /// Sewn for good: holding over long intervals.
    Sewn,
}

impl Stitch {
    /// The word stored in the card row.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Basted => "basted",
            Self::Sewn => "sewn",
        }
    }

    /// Reads a stitch back from a card row.
    ///
    /// # Errors
    ///
    /// Fails when the word is not one of the three.
    pub fn parse(value: &str) -> Result<Self, UnknownStitch> {
        match value {
            "new" => Ok(Self::New),
            "basted" => Ok(Self::Basted),
            "sewn" => Ok(Self::Sewn),
            other => Err(UnknownStitch(other.to_string())),
        }
    }

    const fn from_fsrs(state: FsrsState) -> Self {
        match state {
            FsrsState::New => Self::New,
            // Relearning is a lapse being repaired: loose again, not sewn.
            FsrsState::Learning | FsrsState::Relearning => Self::Basted,
            FsrsState::Review => Self::Sewn,
        }
    }
}

/// A stitch that is not one of the three.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a stitch: expected new, basted or sewn")]
pub struct UnknownStitch(String);

/// The schedulable state of one thing being learnt.
///
/// A formula today; a word from v0.4.0. The card does not know what it is
/// about - that is the `(kind, subject_id)` pair on the row.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub stitch: Stitch,
    /// FSRS: days until recall of this card is predicted to fall to 90%.
    pub stability: f64,
    /// FSRS: how hard this card is for this learner, 1 to 10.
    pub difficulty: f64,
    /// When it surfaces again.
    pub due: DateTime<Utc>,
    pub reps: i64,
    pub lapses: i64,
    pub last_reviewed: Option<DateTime<Utc>>,
}

impl Card {
    /// A card for something never answered, due immediately.
    #[must_use]
    pub const fn new(now: DateTime<Utc>) -> Self {
        Self {
            stitch: Stitch::New,
            stability: 0.0,
            difficulty: 0.0,
            due: now,
            reps: 0,
            lapses: 0,
            last_reviewed: None,
        }
    }

    /// Whether the card is waiting to be answered at `now`.
    #[must_use]
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        self.due <= now
    }

    fn to_fsrs(&self) -> rs_fsrs::Card {
        let last_review = self.last_reviewed.unwrap_or(self.due);
        rs_fsrs::Card {
            due: self.due,
            stability: self.stability,
            difficulty: self.difficulty,
            elapsed_days: 0,
            scheduled_days: 0,
            reps: i32::try_from(self.reps).unwrap_or(i32::MAX),
            lapses: i32::try_from(self.lapses).unwrap_or(i32::MAX),
            state: match self.stitch {
                Stitch::New => FsrsState::New,
                // A card that lapsed back to Basted is scheduled as Learning:
                // FSRS treats the two the same on the way forward, and the
                // difference is only in how the lapse was counted, which the
                // card already carries in `lapses`.
                Stitch::Basted => FsrsState::Learning,
                Stitch::Sewn => FsrsState::Review,
            },
            last_review,
        }
    }
}

/// What an answer did to a card.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheduled {
    /// The card as it now stands.
    pub card: Card,
    /// Days between this answer and the one before it, as FSRS counted them.
    pub elapsed_days: f64,
}

/// The scheduler, holding the FSRS parameters in use.
///
/// Default parameters to begin with (ADR 0003): they are the average of many
/// learners and wrong for any one of them, but nothing better can be said
/// before there is a review history to fit against.
#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    parameters: Parameters,
}

impl Scheduler {
    /// The scheduler with FSRS's default parameters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an answer and returns the card as it now stands.
    #[must_use]
    pub fn review(&self, card: &Card, rating: Rating, now: DateTime<Utc>) -> Scheduled {
        let fsrs = FSRS::new(self.parameters.clone());
        let scheduled = fsrs.next(card.to_fsrs(), now, rating.to_fsrs());
        let next = scheduled.card;

        Scheduled {
            card: Card {
                stitch: Stitch::from_fsrs(next.state),
                stability: next.stability,
                difficulty: next.difficulty,
                due: next.due,
                reps: i64::from(next.reps),
                lapses: i64::from(next.lapses),
                last_reviewed: Some(now),
            },
            #[allow(clippy::cast_precision_loss)]
            elapsed_days: scheduled.review_log.elapsed_days as f64,
        }
    }

    /// What each of the four answers would do, without applying any of them.
    ///
    /// The drill shows the learner how far each button pushes the formula out,
    /// which is the one honest way to make "easy" mean something.
    #[must_use]
    pub fn preview(&self, card: &Card, now: DateTime<Utc>) -> [(Rating, Duration); 4] {
        [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy].map(|rating| {
            let scheduled = self.review(card, rating, now);
            (rating, scheduled.card.due - now)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A morning `day` days after the fixed start, so a test can walk months
    /// forward without composing a date string that does not exist.
    fn at(day: i64) -> DateTime<Utc> {
        let start = DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z").unwrap().with_timezone(&Utc);
        start + Duration::days(day)
    }

    #[test]
    fn a_new_card_is_due_at_once() {
        let now = at(1);
        let card = Card::new(now);
        assert_eq!(card.stitch, Stitch::New);
        assert!(card.is_due(now), "a formula never answered should be waiting");
    }

    #[test]
    fn answering_moves_the_card_off_new() {
        let now = at(1);
        let scheduled = Scheduler::new().review(&Card::new(now), Rating::Good, now);
        assert_ne!(scheduled.card.stitch, Stitch::New, "an answered formula is no longer new");
        assert_eq!(scheduled.card.reps, 1);
        assert!(scheduled.card.due > now, "an answered formula should be pushed into the future");
    }

    #[test]
    fn a_better_answer_pushes_the_card_further_out() {
        // The four buttons have to be ordered, or the learner is grading into
        // a black box. This is the property the drill's promise rests on.
        let now = at(1);
        let card = Scheduler::new().review(&Card::new(now), Rating::Good, now).card;

        let intervals = Scheduler::new().preview(&card, at(5));
        let due: Vec<_> = intervals.iter().map(|(_, interval)| *interval).collect();
        assert!(
            due[0] <= due[1] && due[1] <= due[2] && due[2] <= due[3],
            "again/hard/good/easy should schedule in that order, got {due:?}"
        );
    }

    #[test]
    fn forgetting_counts_a_lapse_and_loosens_the_stitch() {
        // A formula that was holding and then would not come has to go back
        // to being loose, or "sewn" stops meaning anything.
        let scheduler = Scheduler::new();
        let mut card = Card::new(at(0));
        let mut day = 0;
        for _ in 0..4 {
            card = scheduler.review(&card, Rating::Easy, at(day)).card;
            // Answer each time it comes back, the way a learner would.
            day = card.due.signed_duration_since(at(0)).num_days();
        }
        assert_eq!(card.stitch, Stitch::Sewn, "four easy answers should sew a formula");

        let lapsed = scheduler.review(&card, Rating::Again, at(day)).card;
        assert_eq!(lapsed.stitch, Stitch::Basted, "a forgotten formula should come loose again");
        assert_eq!(lapsed.lapses, card.lapses + 1, "the lapse should be counted");
    }

    #[test]
    fn ratings_survive_the_round_trip_through_the_database() {
        for rating in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
            assert_eq!(Rating::from_i64(rating.as_i64()).unwrap(), rating);
        }
        assert!(Rating::from_i64(0).is_err(), "0 is not a rating");
        assert!(Rating::from_i64(5).is_err(), "5 is not a rating");
    }

    #[test]
    fn stitches_survive_the_round_trip_through_the_database() {
        for stitch in [Stitch::New, Stitch::Basted, Stitch::Sewn] {
            assert_eq!(Stitch::parse(stitch.as_str()).unwrap(), stitch);
        }
        assert!(Stitch::parse("review").is_err(), "FSRS's own words are not the product's");
    }
}
