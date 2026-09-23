//! Every sound made once.
//!
//! A drill says the same sentence dozens of times, and a sentence from a paid
//! engine costs credits every time it is made. The cache keys a sound on
//! everything that changes it - engine, voice, model, settings, language,
//! text - so a slower tempo or another voice is a different entry rather
//! than the wrong sound served from here.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use super::{Engine, Speech};

/// What a sound is keyed on.
pub struct Key<'a> {
    pub engine: Engine,
    pub voice: &'a str,
    pub model: &'a str,
    pub settings: &'a serde_json::Value,
    pub language: &'a str,
    pub text: &'a str,
}

impl Key<'_> {
    /// The hash of every field, each ended by a zero byte so that two fields
    /// cannot run together into the same bytes ("ab" + "c" vs "a" + "bc").
    #[must_use]
    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        for part in [
            self.engine.as_str(),
            self.voice,
            self.model,
            &self.settings.to_string(),
            self.language,
            self.text,
        ] {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }
        hasher.finalize().iter().fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
    }
}

/// A sound as the endpoint serves it.
#[derive(Debug, Clone)]
pub struct Sound {
    /// The cache key, which doubles as the sound's `ETag`.
    pub key: String,
    pub mime: String,
    pub bytes: Vec<u8>,
    /// Whether it came from the cache rather than from an engine just now.
    pub cached: bool,
}

/// A sound made before, marked as used again.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub async fn get(pool: &SqlitePool, key: &Key<'_>, now: DateTime<Utc>) -> Result<Option<Sound>> {
    let hash = key.hash();
    let row = sqlx::query("UPDATE audio SET used_at = ?, uses = uses + 1 WHERE key = ? RETURNING mime, bytes")
        .bind(now.to_rfc3339())
        .bind(&hash)
        .fetch_optional(pool)
        .await
        .context("failed to read the audio cache")?;
    Ok(row.map(|row| Sound {
        key: hash,
        mime: row.get("mime"),
        bytes: row.get("bytes"),
        cached: true,
    }))
}

/// Keeps a sound an engine has just made.
///
/// Two requests for the same new sentence can both miss and both make it;
/// the second write keeps the first row rather than failing, because the
/// sound is the same and the learner is waiting for it.
///
/// # Errors
///
/// Fails when the database rejects the statement.
pub async fn put(pool: &SqlitePool, key: &Key<'_>, speech: Speech, now: DateTime<Utc>) -> Result<Sound> {
    let hash = key.hash();
    sqlx::query(
        "INSERT INTO audio (key, engine, voice, model, settings, language, text, mime, bytes, duration_ms, credits, created_at, used_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (key) DO NOTHING",
    )
    .bind(&hash)
    .bind(key.engine.as_str())
    .bind(key.voice)
    .bind(key.model)
    .bind(key.settings.to_string())
    .bind(key.language)
    .bind(key.text)
    .bind(&speech.mime)
    .bind(&speech.bytes)
    .bind(speech.duration_ms)
    .bind(speech.credits)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await
    .context("failed to keep a sound in the cache")?;
    Ok(Sound {
        key: hash,
        mime: speech.mime,
        bytes: speech.bytes,
        cached: false,
    })
}

/// What the cache holds, as health reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    /// Sounds kept.
    pub sounds: i64,
    /// Bytes they take in the database.
    pub bytes: i64,
    /// Minutes of speech they add up to.
    pub minutes: i64,
    /// Credits spent making the ones still kept.
    pub credits: i64,
}

/// What the cache holds.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn stats(pool: &SqlitePool) -> Result<Stats> {
    let row = sqlx::query(
        "SELECT count(*) AS sounds, coalesce(sum(length(bytes)), 0) AS bytes,
                coalesce(sum(duration_ms), 0) AS ms, coalesce(sum(credits), 0) AS credits
         FROM audio",
    )
    .fetch_one(pool)
    .await
    .context("failed to measure the audio cache")?;
    Ok(Stats {
        sounds: row.get("sounds"),
        bytes: row.get("bytes"),
        minutes: row.get::<i64, _>("ms") / 60_000,
        credits: row.get("credits"),
    })
}

/// Credits one engine has spent since a moment: what a forecast is made of.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn credits_since(pool: &SqlitePool, engine: Engine, since: DateTime<Utc>) -> Result<i64> {
    sqlx::query_scalar("SELECT coalesce(sum(credits), 0) FROM audio WHERE engine = ? AND created_at >= ?")
        .bind(engine.as_str())
        .bind(since.to_rfc3339())
        .fetch_one(pool)
        .await
        .context("failed to count the credits spent")
}

/// What a prune removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pruned {
    pub sounds: i64,
    pub bytes: i64,
}

/// Drops sounds nobody has asked for in `unused` - material that left the
/// pack, a voice the learner moved away from, a tempo no longer used.
///
/// Explicit rather than automatic: a paid sound dropped is a sound paid for
/// again, and that is the learner's call to make, not a timer's.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub async fn prune(pool: &SqlitePool, unused: Duration, now: DateTime<Utc>) -> Result<Pruned> {
    let cutoff = (now - unused).to_rfc3339();
    let row = sqlx::query("SELECT count(*) AS sounds, coalesce(sum(length(bytes)), 0) AS bytes FROM audio WHERE used_at < ?")
        .bind(&cutoff)
        .fetch_one(pool)
        .await
        .context("failed to measure what a prune would remove")?;
    sqlx::query("DELETE FROM audio WHERE used_at < ?")
        .bind(&cutoff)
        .execute(pool)
        .await
        .context("failed to prune the audio cache")?;
    Ok(Pruned {
        sounds: row.get("sounds"),
        bytes: row.get("bytes"),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        pool
    }

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z").unwrap().with_timezone(&Utc) + Duration::days(day)
    }

    fn speech(credits: i64) -> Speech {
        Speech {
            bytes: vec![1, 2, 3, 4],
            mime: "audio/mpeg".into(),
            duration_ms: 1500,
            credits,
        }
    }

    const SLOW: fn() -> serde_json::Value = || json!({ "speed": 0.8 });
    const NORMAL: fn() -> serde_json::Value = || json!({ "speed": 1.0 });

    fn key<'a>(settings: &'a serde_json::Value, voice: &'a str, text: &'a str) -> Key<'a> {
        Key {
            engine: Engine::ElevenLabs,
            voice,
            model: "eleven_flash_v2_5",
            settings,
            language: "en",
            text,
        }
    }

    #[tokio::test]
    async fn a_sound_made_once_is_served_from_then_on() {
        let pool = pool().await;
        let settings = NORMAL();
        let key = key(&settings, "rachel", "I am at home.");
        assert!(get(&pool, &key, at(0)).await.unwrap().is_none(), "an empty cache served something");

        let made = put(&pool, &key, speech(7), at(0)).await.unwrap();
        assert!(!made.cached);
        let again = get(&pool, &key, at(1)).await.unwrap().expect("the sound just made should be served");
        assert!(again.cached);
        assert_eq!(again.bytes, vec![1, 2, 3, 4]);
        assert_eq!(again.key, made.key);

        let (uses, used_at): (i64, String) = sqlx::query_as("SELECT uses, used_at FROM audio").fetch_one(&pool).await.unwrap();
        assert_eq!(uses, 2, "serving a sound should count as using it");
        assert_eq!(used_at, at(1).to_rfc3339());
    }

    #[tokio::test]
    async fn everything_that_changes_the_sound_changes_the_key() {
        // A slower tempo served from the entry of the normal one would be the
        // wrong sound with nobody the wiser.
        let (slow, normal) = (SLOW(), NORMAL());
        let base = key(&normal, "rachel", "I am at home.").hash();
        assert_ne!(key(&slow, "rachel", "I am at home.").hash(), base, "the tempo");
        assert_ne!(key(&normal, "adam", "I am at home.").hash(), base, "the voice");
        assert_ne!(key(&normal, "rachel", "He is at home.").hash(), base, "the text");
        let other_model = Key {
            model: "eleven_multilingual_v2",
            ..key(&normal, "rachel", "I am at home.")
        };
        assert_ne!(other_model.hash(), base, "the model");
        let other_engine = Key {
            engine: Engine::Piper,
            ..key(&normal, "rachel", "I am at home.")
        };
        assert_ne!(other_engine.hash(), base, "the engine");
        let other_language = Key {
            language: "es",
            ..key(&normal, "rachel", "I am at home.")
        };
        assert_ne!(other_language.hash(), base, "the language");
    }

    #[tokio::test]
    async fn fields_cannot_run_into_each_other() {
        let settings = NORMAL();
        assert_ne!(key(&settings, "ab", "c").hash(), key(&settings, "a", "bc").hash());
    }

    #[tokio::test]
    async fn making_the_same_sound_twice_keeps_one() {
        let pool = pool().await;
        let settings = NORMAL();
        let key = key(&settings, "rachel", "I am at home.");
        put(&pool, &key, speech(7), at(0)).await.unwrap();
        put(&pool, &key, speech(7), at(0)).await.expect("a race to make the same sound must not fail");
        assert_eq!(stats(&pool).await.unwrap().sounds, 1);
    }

    #[tokio::test]
    async fn the_credits_spent_are_counted_from_a_moment() {
        let pool = pool().await;
        let settings = NORMAL();
        put(&pool, &key(&settings, "rachel", "one"), speech(10), at(0)).await.unwrap();
        put(&pool, &key(&settings, "rachel", "two"), speech(20), at(5)).await.unwrap();
        assert_eq!(credits_since(&pool, Engine::ElevenLabs, at(3)).await.unwrap(), 20);
        assert_eq!(
            credits_since(&pool, Engine::Piper, at(0)).await.unwrap(),
            0,
            "credits belong to the engine that spent them"
        );
        assert_eq!(stats(&pool).await.unwrap().credits, 30);
    }

    #[tokio::test]
    async fn a_prune_drops_only_what_nobody_used() {
        let pool = pool().await;
        let settings = NORMAL();
        let old = key(&settings, "rachel", "left the pack");
        let fresh = key(&settings, "rachel", "said yesterday");
        put(&pool, &old, speech(10), at(0)).await.unwrap();
        put(&pool, &fresh, speech(10), at(0)).await.unwrap();
        // Used on day 99: made long ago, but in use.
        get(&pool, &fresh, at(99)).await.unwrap();

        let pruned = prune(&pool, Duration::days(90), at(100)).await.unwrap();
        assert_eq!(pruned, Pruned { sounds: 1, bytes: 4 });
        assert!(get(&pool, &fresh, at(100)).await.unwrap().is_some(), "a sound in use was pruned for being old");
        assert!(get(&pool, &old, at(100)).await.unwrap().is_none());
    }
}
