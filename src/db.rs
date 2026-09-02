//! The one file the tutor remembers things in.
//!
//! `SQLite` was chosen over a database server on purpose (ADR 0001): one user,
//! one Pi, and a backup that is a copy of a file. Everything below is what it
//! takes to treat that file well.

use std::str::FromStr;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

/// Opens the database and brings the schema up to date.
///
/// Every entry point does this: a server against an outdated schema fails in
/// less obvious ways than one that migrates first.
///
/// # Errors
///
/// Fails when the URL is not a `SQLite` URL, when the directory the file lives
/// in cannot be created, when the file cannot be opened, or when a migration
/// does not apply.
pub async fn connect(url: &str) -> Result<SqlitePool> {
    // sqlx reads anything without the scheme as a bare file name, so a URL
    // for another database would become a file called `postgres:`; the
    // scheme is checked here so the message names the actual mistake.
    anyhow::ensure!(
        url.starts_with("sqlite:"),
        "HILVAN_DATABASE_URL is not a SQLite URL (expected sqlite://path/to/file.db?mode=rwc): {url}"
    );
    let options = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("HILVAN_DATABASE_URL is not a valid SQLite URL: {url}"))?
        // WAL lets the tutor app keep reading while a review is being saved,
        // and foreign keys are off by default in SQLite, which is not a
        // default anyone wants.
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    // `mode=rwc` creates the file but not the directory it lives in. On a
    // fresh stand `/data` is a mounted volume and exists; on a developer's
    // machine `data/` usually does not.
    if let Some(parent) = options.get_filename().parent().filter(|parent| !parent.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create the database directory {}", parent.display()))?;
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to open the database at {url}"))?;
    sqlx::migrate!().run(&pool).await.context("failed to apply database migrations")?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_the_directory_the_file_lives_in() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let file = dir.path().join("nested/deeper/tutor.db");
        let url = format!("sqlite://{}?mode=rwc", file.display());

        let pool = connect(&url).await.expect("the database should open in a directory that did not exist");
        assert!(file.is_file(), "the database file was not created at {}", file.display());

        // Migrations ran: the settings table is there to be queried.
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM settings").fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn rejects_a_url_that_is_not_sqlite() {
        let error = connect("postgres://nobody@localhost/hilvan").await.unwrap_err();
        assert!(error.to_string().contains("HILVAN_DATABASE_URL"), "{error:#}");
    }
}
