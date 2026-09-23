//! What is left of the `ElevenLabs` budget, and how long it will last.
//!
//! The account says what is left; the cache says how fast it goes, because
//! every paid sentence is in there with what it cost. Together they answer
//! the one question worth asking before a lesson: will this month's credits
//! last until they start over?

use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use super::elevenlabs::Subscription;
use super::{ElevenLabs, Engine, cache};

/// The window the spending rate is measured over. Two weeks: long enough
/// that one heavy evening of new material does not read as a trend, short
/// enough that a change of habit shows within the month.
const RATE_WINDOW_DAYS: i64 = 14;

/// How long the account's answer is trusted. Health is asked every thirty
/// seconds by the container runtime, and the account does not need to be.
const SUBSCRIPTION_TTL: StdDuration = StdDuration::from_secs(600);

/// The budget as the voices screen and health show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Budget {
    /// Credits left this period.
    pub remaining: i64,
    /// Credits the period holds.
    pub limit: i64,
    /// When the period starts over.
    pub resets_at: Option<DateTime<Utc>>,
    /// Credits spent a day, averaged over the last two weeks.
    pub per_day: f64,
    /// When the credits run out at that rate. `None` when nothing is being
    /// spent - they never do.
    pub runs_out_at: Option<DateTime<Utc>>,
    /// Whether they last until the period starts over.
    pub lasts: bool,
}

/// Works the forecast out from what the account says and what was spent.
#[must_use]
pub fn forecast(subscription: &Subscription, spent_in_window: i64, now: DateTime<Utc>) -> Budget {
    let remaining = (subscription.character_limit - subscription.character_count).max(0);
    #[allow(clippy::cast_precision_loss)]
    let per_day = spent_in_window.max(0) as f64 / RATE_WINDOW_DAYS as f64;
    let runs_out_at = (per_day > 0.0).then(|| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let minutes = (remaining as f64 / per_day * 24.0 * 60.0) as i64;
        now + Duration::minutes(minutes)
    });
    let resets_at = subscription.resets_at();
    let lasts = match (runs_out_at, resets_at) {
        (None, _) => true,
        (Some(out), Some(reset)) => out >= reset,
        // No reset date: the only honest answer is whether anything is left.
        (Some(_), None) => remaining > 0,
    };
    Budget {
        remaining,
        limit: subscription.character_limit,
        resets_at,
        per_day,
        runs_out_at,
        lasts,
    }
}

/// The account's answer, remembered for a while.
#[derive(Clone, Default)]
pub struct Accountant {
    remembered: Arc<Mutex<Option<(Instant, Subscription)>>>,
}

impl Accountant {
    /// The budget now.
    ///
    /// # Errors
    ///
    /// Fails when the account cannot be asked and nothing is remembered, or
    /// when the database rejects the query for what was spent.
    pub async fn budget(&self, elevenlabs: &ElevenLabs, pool: &SqlitePool, now: DateTime<Utc>) -> Result<Budget> {
        let subscription = {
            let mut remembered = self.remembered.lock().await;
            match remembered.as_ref() {
                Some((at, subscription)) if at.elapsed() < SUBSCRIPTION_TTL => subscription.clone(),
                _ => {
                    let subscription = elevenlabs.subscription().await?;
                    *remembered = Some((Instant::now(), subscription.clone()));
                    subscription
                }
            }
        };
        let spent = cache::credits_since(pool, Engine::ElevenLabs, now - Duration::days(RATE_WINDOW_DAYS)).await?;
        Ok(forecast(&subscription, spent, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-10T00:00:00Z").unwrap().with_timezone(&Utc)
    }

    fn subscription(used: i64, reset_in_days: i64) -> Subscription {
        Subscription {
            character_count: used,
            character_limit: 30_000,
            next_character_count_reset_unix: Some((now() + Duration::days(reset_in_days)).timestamp()),
        }
    }

    #[test]
    fn a_slow_month_lasts_until_the_reset() {
        // 1400 over two weeks is 100 a day; 20 000 left is 200 days.
        let budget = forecast(&subscription(10_000, 20), 1400, now());
        assert_eq!(budget.remaining, 20_000);
        assert!((budget.per_day - 100.0).abs() < f64::EPSILON);
        assert_eq!(budget.runs_out_at, Some(now() + Duration::days(200)));
        assert!(budget.lasts);
    }

    #[test]
    fn a_heavy_month_runs_out_before_the_reset() {
        // 14 000 over two weeks is 1000 a day; 5000 left is five days, and
        // the reset is twenty away.
        let budget = forecast(&subscription(25_000, 20), 14_000, now());
        assert_eq!(budget.runs_out_at, Some(now() + Duration::days(5)));
        assert!(!budget.lasts, "five days of credits were said to last twenty");
    }

    #[test]
    fn nothing_spent_never_runs_out() {
        let budget = forecast(&subscription(0, 20), 0, now());
        assert_eq!(budget.runs_out_at, None);
        assert!(budget.lasts);
    }

    #[test]
    fn an_overdrawn_account_has_nothing_left_rather_than_less_than_nothing() {
        let budget = forecast(&subscription(31_000, 20), 1400, now());
        assert_eq!(budget.remaining, 0);
        assert!(!budget.lasts);
    }
}
