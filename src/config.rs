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
        Ok(Self { addr, database_url, web_dir })
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
        let config = Config::from_lookup(env(&[("HILVAN_DATABASE_URL", ""), ("HILVAN_WEB_DIR", " ")])).unwrap();
        assert_eq!(config.database_url, DEFAULT_DATABASE_URL);
        assert_eq!(config.web_dir, PathBuf::from(DEFAULT_WEB_DIR));
    }

    #[test]
    fn rejects_a_malformed_bind_address() {
        let error = Config::from_lookup(env(&[("HILVAN_ADDR", "not-an-address")])).unwrap_err();
        assert!(error.to_string().contains("HILVAN_ADDR"));
    }
}
