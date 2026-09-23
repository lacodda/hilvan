//! Runtime configuration, read from the environment and nothing else.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

/// Default bind address when `HILVAN_ADDR` is not set.
const DEFAULT_ADDR: &str = "0.0.0.0:8086";

/// Default database when `HILVAN_DATABASE_URL` is not set. `mode=rwc`
/// creates the file on first start; the directory is created by `db::connect`.
const DEFAULT_DATABASE_URL: &str = "sqlite://data/hilvan.db?mode=rwc";

/// Default SPA directory when `HILVAN_WEB_DIR` is not set: what `pnpm build`
/// in `web/` produces, relative to the working directory.
const DEFAULT_WEB_DIR: &str = "web/dist";

/// Runtime configuration, read from the environment.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server binds to (`HILVAN_ADDR`).
    pub addr: SocketAddr,
    /// `SQLite` connection string (`HILVAN_DATABASE_URL`): the learner model,
    /// schedules, generated material and everything else the tutor
    /// remembers lives in this one file.
    pub database_url: String,
    /// Directory holding the built SPA (`HILVAN_WEB_DIR`).
    pub web_dir: PathBuf,
    /// Argon2 hash of the password the learner signs in with
    /// (`HILVAN_PASSWORD_HASH`, produced by `hilvan hash`). Unset leaves the
    /// stand open, which is what a developer's machine wants.
    ///
    /// A hash rather than the password itself: an .env file travels into
    /// backups and shows up in `docker inspect`, and neither should hand
    /// anyone the password.
    pub password_hash: Option<String>,
    /// The Piper service that speaks the learner's own language
    /// (`HILVAN_PIPER_URL`, e.g. `http://piper:5000`). Unset leaves the
    /// tutor without a local voice, which health and the voices screen say.
    pub piper_url: Option<String>,
    /// The `ElevenLabs` key the language being learnt is spoken with
    /// (`HILVAN_ELEVENLABS_KEY`). Unset means Piper speaks it too.
    pub elevenlabs_key: Option<String>,
}

impl Config {
    /// Reads the configuration from the process environment.
    ///
    /// # Errors
    ///
    /// Fails when `HILVAN_ADDR` is not a socket address; the message names
    /// the variable.
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// The environment is passed in as a lookup so tests can supply their own.
    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let addr = lookup("HILVAN_ADDR").unwrap_or_else(|| DEFAULT_ADDR.to_string());
        let addr = addr.parse().with_context(|| format!("HILVAN_ADDR is not a valid socket address: {addr}"))?;
        let database_url = lookup("HILVAN_DATABASE_URL")
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_DATABASE_URL.to_string());
        let web_dir = lookup("HILVAN_WEB_DIR")
            .filter(|path| !path.trim().is_empty())
            .map_or_else(|| PathBuf::from(DEFAULT_WEB_DIR), PathBuf::from);
        let password_hash = lookup("HILVAN_PASSWORD_HASH").filter(|hash| !hash.trim().is_empty());
        let piper_url = lookup("HILVAN_PIPER_URL").map(|url| url.trim().to_string()).filter(|url| !url.is_empty());
        if let Some(url) = &piper_url {
            anyhow::ensure!(
                url.starts_with("http://") || url.starts_with("https://"),
                "HILVAN_PIPER_URL is not an http(s) URL: {url}"
            );
        }
        let elevenlabs_key = lookup("HILVAN_ELEVENLABS_KEY").map(|key| key.trim().to_string()).filter(|key| !key.is_empty());
        Ok(Self {
            addr,
            database_url,
            web_dir,
            password_hash,
            piper_url,
            elevenlabs_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_string())
    }

    #[test]
    fn defaults_everything() {
        let config = Config::from_lookup(env(&[])).expect("config should build with nothing set");
        assert_eq!(config.addr, DEFAULT_ADDR.parse().unwrap());
        assert_eq!(config.database_url, DEFAULT_DATABASE_URL);
        assert_eq!(config.web_dir, PathBuf::from(DEFAULT_WEB_DIR));
        assert!(config.password_hash.is_none(), "an unset password should leave the stand open");
        assert!(
            config.piper_url.is_none() && config.elevenlabs_key.is_none(),
            "no voice is configured by default"
        );
    }

    #[test]
    fn reads_the_voices() {
        let config = Config::from_lookup(env(&[("HILVAN_PIPER_URL", " http://piper:5000 "), ("HILVAN_ELEVENLABS_KEY", "sk_test")])).unwrap();
        assert_eq!(config.piper_url.as_deref(), Some("http://piper:5000"));
        assert_eq!(config.elevenlabs_key.as_deref(), Some("sk_test"));

        // `HILVAN_ELEVENLABS_KEY=` in an .env means no key, not an empty one
        // sent to the API on every sentence.
        let config = Config::from_lookup(env(&[("HILVAN_PIPER_URL", ""), ("HILVAN_ELEVENLABS_KEY", " ")])).unwrap();
        assert!(config.piper_url.is_none() && config.elevenlabs_key.is_none());
    }

    #[test]
    fn rejects_a_piper_address_that_is_not_a_url() {
        let error = Config::from_lookup(env(&[("HILVAN_PIPER_URL", "piper:5000")])).unwrap_err();
        assert!(error.to_string().contains("HILVAN_PIPER_URL"));
    }

    #[test]
    fn reads_the_overrides() {
        let config = Config::from_lookup(env(&[
            ("HILVAN_ADDR", "127.0.0.1:9090"),
            ("HILVAN_DATABASE_URL", "sqlite:///data/tutor.db?mode=rwc"),
            ("HILVAN_WEB_DIR", "/app/web"),
        ]))
        .expect("config should accept valid overrides");
        assert_eq!(config.addr, "127.0.0.1:9090".parse().unwrap());
        assert_eq!(config.database_url, "sqlite:///data/tutor.db?mode=rwc");
        assert_eq!(config.web_dir, PathBuf::from("/app/web"));
    }

    #[test]
    fn treats_blank_optionals_as_unset() {
        // A compose file that leaves a variable empty means "the default",
        // not "a database with no name" or "serve the working directory".
        let config = Config::from_lookup(env(&[("HILVAN_DATABASE_URL", ""), ("HILVAN_WEB_DIR", " "), ("HILVAN_PASSWORD_HASH", "  ")])).unwrap();
        assert_eq!(config.database_url, DEFAULT_DATABASE_URL);
        assert_eq!(config.web_dir, PathBuf::from(DEFAULT_WEB_DIR));
        // An .env with `HILVAN_PASSWORD_HASH=` means unset, not "a hash of
        // nothing lets you in".
        assert!(config.password_hash.is_none());
    }

    #[test]
    fn rejects_a_malformed_bind_address() {
        let error = Config::from_lookup(env(&[("HILVAN_ADDR", "not-an-address")])).unwrap_err();
        assert!(error.to_string().contains("HILVAN_ADDR"));
    }
}
