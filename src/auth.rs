//! The lock on the door.
//!
//! hilvan serves one learner on a home network, and what it protects is not a
//! secret so much as a learner model: nobody else should be able to answer
//! "easy" on someone else's formulas, and nothing here should be readable by
//! whoever finds the stand from outside.
//!
//! So the whole of it is: one password, kept as an Argon2 hash in the
//! environment, and sessions as rows in the database. No accounts, no
//! registration, no reset flow - those need a second person to exist.

use anyhow::{Context, Result};
use argon2::password_hash::phc::PasswordHash;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use sqlx::SqlitePool;

/// The cookie the session token travels in.
pub const COOKIE: &str = "hilvan_session";

/// How long a session lives without being used.
///
/// Ninety days, refreshed on every request: the learner is one person on
/// their own phone, opening the tutor daily, and a login screen between them
/// and a five-minute drill is the surest way to skip the drill.
pub const SESSION_DAYS: i64 = 90;

/// Hashes a password for `HILVAN_PASSWORD_HASH`.
///
/// Argon2id with the crate's defaults, which are the OWASP-recommended
/// parameters. The salt is random per password and travels inside the PHC
/// string, so nothing else has to be stored beside it.
///
/// # Errors
///
/// Fails when the hasher rejects the password.
pub fn hash(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!("failed to hash the password: {error}"))
}

/// Checks a password against the configured hash.
///
/// # Errors
///
/// Fails when the configured hash is not a valid PHC string, which is a
/// deployment error worth naming rather than reading as a wrong password.
pub fn verify(password: &str, hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash).map_err(|error| anyhow::anyhow!("HILVAN_PASSWORD_HASH is not a valid Argon2 hash: {error}"))?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// A fresh session token: 256 bits from the operating system's generator,
/// hex-encoded so it survives a cookie header unescaped.
#[must_use]
pub fn new_token() -> String {
    use std::fmt::Write as _;

    let bytes: [u8; 32] = rand::random();
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    token
}

/// Stores a new session.
///
/// # Errors
///
/// Fails when the database rejects the insert.
pub async fn start(pool: &SqlitePool, token: &str) -> Result<()> {
    sqlx::query("INSERT INTO session (token) VALUES (?)")
        .bind(token)
        .execute(pool)
        .await
        .context("failed to store the session")?;
    Ok(())
}

/// Ends a session.
///
/// # Errors
///
/// Fails when the database rejects the delete.
pub async fn end(pool: &SqlitePool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM session WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await
        .context("failed to end the session")?;
    Ok(())
}

/// Whether a token names a live session, refreshing it if it does.
///
/// # Errors
///
/// Fails when the database cannot be read.
pub async fn is_live(pool: &SqlitePool, token: &str) -> Result<bool> {
    // The lifetime is enforced in the query rather than by a sweep: a session
    // that has not been used in ninety days is dead the moment it is asked
    // about, whether or not anything has cleaned it up.
    let refreshed = sqlx::query(
        "UPDATE session
            SET seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE token = ?
            AND seen_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?)",
    )
    .bind(token)
    .bind(format!("-{SESSION_DAYS} days"))
    .execute(pool)
    .await
    .context("failed to check the session")?;
    Ok(refreshed.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        pool
    }

    #[test]
    fn the_right_password_verifies_and_a_wrong_one_does_not() {
        let stored = hash("hunter2").expect("a password should hash");
        assert!(verify("hunter2", &stored).unwrap());
        assert!(!verify("hunter3", &stored).unwrap());
    }

    #[test]
    fn the_hash_does_not_contain_the_password() {
        // The whole point of storing a hash: an .env that leaks into a backup
        // does not hand over the password.
        let stored = hash("hunter2").unwrap();
        assert!(!stored.contains("hunter2"), "the hash carries the password: {stored}");
        assert!(stored.starts_with("$argon2"), "not a PHC string: {stored}");
    }

    #[test]
    fn the_same_password_hashes_differently_every_time() {
        // A random salt per hash, so two stands with the same password do not
        // share a hash anyone could recognise.
        assert_ne!(hash("hunter2").unwrap(), hash("hunter2").unwrap());
    }

    #[test]
    fn a_configured_hash_that_is_not_a_hash_is_named_as_such() {
        // A deployment error, not a wrong password: the message has to say
        // which variable is wrong.
        let error = verify("hunter2", "not-a-hash").unwrap_err().to_string();
        assert!(error.contains("HILVAN_PASSWORD_HASH"), "{error}");
    }

    #[test]
    fn two_tokens_are_never_the_same() {
        assert_ne!(new_token(), new_token());
        assert_eq!(new_token().len(), 64, "256 bits, hex-encoded");
    }

    #[tokio::test]
    async fn a_stored_session_is_live_and_an_unknown_token_is_not() {
        let pool = pool().await;
        let token = new_token();
        start(&pool, &token).await.unwrap();

        assert!(is_live(&pool, &token).await.unwrap());
        assert!(!is_live(&pool, &new_token()).await.unwrap(), "a token nobody issued was accepted");
    }

    #[tokio::test]
    async fn ending_a_session_takes_effect_at_once() {
        let pool = pool().await;
        let token = new_token();
        start(&pool, &token).await.unwrap();
        end(&pool, &token).await.unwrap();
        assert!(!is_live(&pool, &token).await.unwrap());
    }

    #[tokio::test]
    async fn a_session_unused_for_too_long_is_dead() {
        let pool = pool().await;
        let token = new_token();
        start(&pool, &token).await.unwrap();

        // Pushed past the lifetime the way time would.
        sqlx::query("UPDATE session SET seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-91 days')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!is_live(&pool, &token).await.unwrap());
    }

    #[tokio::test]
    async fn using_a_session_pushes_its_expiry_out() {
        // The reason a phone opened every few days never sees the login
        // screen again.
        let pool = pool().await;
        let token = new_token();
        start(&pool, &token).await.unwrap();
        sqlx::query("UPDATE session SET seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-89 days')")
            .execute(&pool)
            .await
            .unwrap();

        assert!(is_live(&pool, &token).await.unwrap(), "a session inside its lifetime was refused");

        // Refreshed by that check, so it survives well past the original
        // ninety days.
        let stale: i64 = sqlx::query_scalar("SELECT count(*) FROM session WHERE seen_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-1 day')")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stale, 0, "the session was not refreshed on use");
    }

    #[tokio::test]
    async fn two_devices_hold_two_sessions() {
        // The phone and the desktop: signing out of one must not sign out of
        // the other.
        let pool = pool().await;
        let (phone, desktop) = (new_token(), new_token());
        start(&pool, &phone).await.unwrap();
        start(&pool, &desktop).await.unwrap();

        end(&pool, &phone).await.unwrap();
        assert!(!is_live(&pool, &phone).await.unwrap());
        assert!(is_live(&pool, &desktop).await.unwrap(), "signing out of one device ended the other");
    }
}
