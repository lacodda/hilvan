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
    async fn the_worked_examples_of_a_stand_survive_the_move_into_sentences() {
        // A stand is upgraded before its pack is reloaded: between the two,
        // the drill has to keep its examples. The schema as v0.3 left it,
        // filled the way v0.3 filled it, then the migrations that came after.
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let mut before = sqlx::migrate!();
        before.migrations = before.migrations.iter().filter(|migration| migration.version <= 5).cloned().collect();
        before.run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO language (code, name, is_native) VALUES ('ru', 'ru', 1), ('en', 'en', 0)",
            "INSERT INTO pack (id, version, native, target, loaded_at) VALUES ('p', 3, 'ru', 'en', '2026-09-23T00:00:00Z')",
            "INSERT INTO formula (id, pack_id, target, name, pattern, explanation, position) VALUES
                 ('be', 'p', 'en', 'n', 'x', 'e', 10), ('have', 'p', 'en', 'n', 'y', 'e', 20)",
            "INSERT INTO sample (formula_id, native, target, position) VALUES
                 ('be', 'Я дома.', 'I am at home.', 0), ('be', 'Он врач.', 'He is a doctor.', 1),
                 ('have', 'Я дома.', 'I am at home.', 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        sqlx::migrate!().run(&pool).await.expect("the move into sentences should apply over v0.3 data");

        let sentences: i64 = sqlx::query_scalar("SELECT count(*) FROM sentence").fetch_one(&pool).await.unwrap();
        assert_eq!(sentences, 2, "a sentence two formulas showed should be one sentence");
        let shown: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT fs.formula_id, s.text, t.text FROM formula_sentence fs
             JOIN sentence s ON s.id = fs.sentence_id
             JOIN sentence_translation t ON t.sentence_id = s.id AND t.language = 'ru'
             ORDER BY fs.formula_id, fs.position",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            shown,
            vec![
                ("be".into(), "I am at home.".into(), "Я дома.".into()),
                ("be".into(), "He is a doctor.".into(), "Он врач.".into()),
                ("have".into(), "I am at home.".into(), "Я дома.".into()),
            ]
        );
    }

    #[tokio::test]
    async fn rejects_a_url_that_is_not_sqlite() {
        let error = connect("postgres://nobody@localhost/hilvan").await.unwrap_err();
        assert!(error.to_string().contains("HILVAN_DATABASE_URL"), "{error:#}");
    }
}
