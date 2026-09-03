//! The lock on the door.
//!
//! hilvan serves one learner on a home network, and what it protects is not a
//! secret so much as a learner model: nobody else should be able to answer
//! "easy" on someone else's formulas, and nothing here should be readable by
//! whoever finds the stand from outside.
//!
//! So the whole of it is: one password, held in the environment rather than
//! the database, and a session cookie holding a random token the server
//! remembers until it expires. No accounts, no registration, no reset flow -
//! those need a second person to exist.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use rand::RngExt;
use subtle::ConstantTimeEq;

/// The name of the session cookie.
pub const COOKIE: &str = "hilvan_session";

/// How long a session lasts before the learner has to type the password again.
///
/// Thirty days: the tutor is opened from a phone every day, and a login screen
/// between the learner and a five-minute drill is the surest way to skip the
/// drill.
const SESSION_DAYS: i64 = 30;

/// Sessions the server currently honours.
///
/// In memory, not in the database: a restart asking for the password once is
/// a fair price for tokens that cannot leak from a file that gets backed up.
#[derive(Clone)]
pub struct Sessions {
    /// The password to be let in with; `None` leaves the door open.
    password: Option<Arc<str>>,
    live: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl std::fmt::Debug for Sessions {
    /// Written by hand so a stray `{:?}` in a log line cannot print the
    /// password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sessions").field("required", &self.password.is_some()).finish_non_exhaustive()
    }
}

impl Sessions {
    /// A door with `password` on it, or an open one when it is `None`.
    #[must_use]
    pub fn new(password: Option<String>) -> Self {
        Self {
            password: password.filter(|p| !p.is_empty()).map(Into::into),
            live: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Whether a password is required at all.
    ///
    /// Unset means an open stand, which is what a developer's machine wants
    /// and what a home network can choose. The server says so at startup.
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.password.is_some()
    }

    /// Checks a password and issues a session token, or returns `None`.
    ///
    /// The comparison is constant-time: the difference between a wrong first
    /// character and a wrong last one should not be measurable.
    #[must_use]
    pub fn log_in(&self, attempt: &str, now: DateTime<Utc>) -> Option<String> {
        let password = self.password.as_ref()?;
        if !bool::from(attempt.as_bytes().ct_eq(password.as_bytes())) {
            return None;
        }
        Some(self.issue(now))
    }

    /// Issues a session without checking anything. Used when no password is
    /// set, and by `log_in` once the password matched.
    fn issue(&self, now: DateTime<Utc>) -> String {
        // 32 bytes of randomness, hex-encoded: not guessable, and safe to put
        // in a cookie without escaping.
        let token = {
            let mut rng = rand::rng();
            let bytes: [u8; 32] = rng.random();
            let mut token = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(token, "{byte:02x}");
            }
            token
        };
        let expires = now + Duration::days(SESSION_DAYS);
        if let Ok(mut live) = self.live.lock() {
            live.retain(|_, at| *at > now);
            live.insert(token.clone(), expires);
        }
        token
    }

    /// Whether a request carrying `token` is allowed in.
    #[must_use]
    pub fn is_valid(&self, token: Option<&str>, now: DateTime<Utc>) -> bool {
        if self.password.is_none() {
            return true;
        }
        let Some(token) = token else { return false };
        let Ok(live) = self.live.lock() else { return false };
        live.get(token).is_some_and(|expires| *expires > now)
    }

    /// Forgets a session.
    pub fn log_out(&self, token: Option<&str>) {
        let Some(token) = token else { return };
        if let Ok(mut live) = self.live.lock() {
            live.remove(token);
        }
    }

    /// How long a freshly issued session lasts, for the cookie's own expiry.
    #[must_use]
    pub const fn lifetime_days() -> i64 {
        SESSION_DAYS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-03T09:00:00Z").unwrap().with_timezone(&Utc)
    }

    #[test]
    fn the_right_password_opens_the_door_and_a_wrong_one_does_not() {
        let sessions = Sessions::new(Some("hunter2".into()));
        assert!(sessions.log_in("wrong", now()).is_none());

        let token = sessions.log_in("hunter2", now()).expect("the password should be accepted");
        assert!(sessions.is_valid(Some(&token), now()));
    }

    #[test]
    fn a_token_nobody_issued_is_refused() {
        let sessions = Sessions::new(Some("hunter2".into()));
        assert!(!sessions.is_valid(Some("0".repeat(64).as_str()), now()));
        assert!(!sessions.is_valid(None, now()));
    }

    #[test]
    fn a_session_expires() {
        let sessions = Sessions::new(Some("hunter2".into()));
        let token = sessions.log_in("hunter2", now()).unwrap();
        assert!(!sessions.is_valid(Some(&token), now() + Duration::days(SESSION_DAYS + 1)));
    }

    #[test]
    fn logging_out_ends_the_session_at_once() {
        let sessions = Sessions::new(Some("hunter2".into()));
        let token = sessions.log_in("hunter2", now()).unwrap();
        sessions.log_out(Some(&token));
        assert!(!sessions.is_valid(Some(&token), now()));
    }

    #[test]
    fn two_sessions_get_different_tokens() {
        // The phone and the desktop are two sessions; one must not be able to
        // guess or share the other's token.
        let sessions = Sessions::new(Some("hunter2".into()));
        let phone = sessions.log_in("hunter2", now()).unwrap();
        let desktop = sessions.log_in("hunter2", now()).unwrap();
        assert_ne!(phone, desktop);
        assert!(sessions.is_valid(Some(&phone), now()) && sessions.is_valid(Some(&desktop), now()));
    }

    #[test]
    fn no_password_means_no_door() {
        // A developer's machine, or a stand the learner deliberately leaves
        // open on the home network.
        let sessions = Sessions::new(None);
        assert!(!sessions.is_required());
        assert!(sessions.is_valid(None, now()));
    }

    #[test]
    fn an_empty_password_is_no_password() {
        // An .env with `HILVAN_PASSWORD=` means unset, not "the empty string
        // lets you in".
        let sessions = Sessions::new(Some(String::new()));
        assert!(!sessions.is_required());
    }

    #[test]
    fn the_password_never_reaches_a_log_line() {
        let sessions = Sessions::new(Some("hunter2".into()));
        assert!(!format!("{sessions:?}").contains("hunter2"), "the password is printable: {sessions:?}");
    }
}
