//! The four answers the drill offers.

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
    /// row was written by something other than hilvan.
    pub const fn from_i64(value: i64) -> Result<Self, InvalidRating> {
        match value {
            1 => Ok(Self::Again),
            2 => Ok(Self::Hard),
            3 => Ok(Self::Good),
            4 => Ok(Self::Easy),
            other => Err(InvalidRating(other)),
        }
    }
}

/// A rating that is not one of the four.
#[derive(Debug, thiserror::Error)]
#[error("{0} is not a rating: expected 1 (again), 2 (hard), 3 (good) or 4 (easy)")]
pub struct InvalidRating(i64);
