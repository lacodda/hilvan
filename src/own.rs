//! Words from the learner's own life, substituted into the drill.
//!
//! A pack drills "I can swim" because its author chose "swim"; the learner
//! who cooks every evening and drives a truck learns faster saying those.
//! Own words are nouns and verbs the learner types in, each with what it
//! means in their own language, and the drill puts them into the slots of
//! the pack that take them - a slot that asks nothing of its value, so a
//! word typed in fits without the tutor knowing any grammar.
//!
//! They are the learner's, not the pack's: a reload of the pack leaves them
//! alone, and they belong to the language being learnt.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::lexicon;
use crate::pack::OwnKind;

/// The longest own word accepted, in characters: a phrase like "a cup of
/// tea" fits, a sentence pasted by mistake does not.
const LONGEST: usize = 40;

/// An own word, as the app shows it.
#[derive(Debug, Clone, Serialize)]
pub struct OwnWord {
    pub id: i64,
    pub kind: OwnKind,
    /// In the learner's own language.
    pub native: String,
    /// As it is said in a sentence: "a car", "swim".
    pub target: String,
    /// Whether every word of `target` is one the lexicon of the language
    /// knows. Not a refusal - a word past the lexicon's ten thousand can be
    /// right - but the one hint at a typo the learner gets before drilling it.
    pub known: bool,
}

/// What the learner sends to add a word.
#[derive(Debug, Clone, Deserialize)]
pub struct NewWord {
    pub kind: OwnKind,
    pub native: String,
    pub target: String,
}

/// Why a word was not added: the learner's to fix, said in a sentence.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("type the word in both languages")]
    Empty,
    #[error("that is longer than a word - at most {LONGEST} characters")]
    TooLong,
    #[error("{0:?} is already one of your words")]
    Duplicate(String),
    #[error("no pack is loaded, so there is no language to add words to")]
    NoLanguage,
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

/// The learner's own words in the language being learnt, newest first.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn list(pool: &SqlitePool) -> Result<Vec<OwnWord>> {
    let rows = sqlx::query(
        "SELECT o.id, o.kind, o.native, o.target, o.language FROM own_word o
         WHERE o.language IN (SELECT target FROM pack)
         ORDER BY o.created_at DESC, o.id DESC",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the learner's own words")?;
    rows.iter().map(row).collect()
}

/// The own words of a kind in a language, oldest first: what a slot that
/// takes them draws from.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn of_kind(pool: &SqlitePool, language: &str, kind: OwnKind) -> Result<Vec<OwnWord>> {
    let rows = sqlx::query("SELECT id, kind, native, target, language FROM own_word WHERE language = ? AND kind = ? ORDER BY created_at, id")
        .bind(language)
        .bind(kind.as_str())
        .fetch_all(pool)
        .await
        .context("failed to read the learner's own words of a kind")?;
    rows.iter().map(row).collect()
}

/// Adds a word to the language being learnt.
///
/// # Errors
///
/// Refuses an empty or overlong word and one the learner already has, and
/// fails when no pack names a language or the database rejects a statement.
pub async fn add(pool: &SqlitePool, word: &NewWord, now: DateTime<Utc>) -> Result<OwnWord, Refused> {
    let native = word.native.split_whitespace().collect::<Vec<_>>().join(" ");
    let target = word.target.split_whitespace().collect::<Vec<_>>().join(" ");
    if native.is_empty() || target.is_empty() {
        return Err(Refused::Empty);
    }
    if native.chars().count() > LONGEST || target.chars().count() > LONGEST {
        return Err(Refused::TooLong);
    }
    let Some(language) = crate::pack::language_being_learnt(pool).await? else {
        return Err(Refused::NoLanguage);
    };

    let inserted = sqlx::query(
        "INSERT INTO own_word (language, kind, native, target, created_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (language, kind, target) DO NOTHING
         RETURNING id, kind, native, target, language",
    )
    .bind(&language)
    .bind(word.kind.as_str())
    .bind(&native)
    .bind(&target)
    .bind(now.to_rfc3339())
    .fetch_optional(pool)
    .await
    .context("failed to add an own word")?;
    match inserted {
        Some(inserted) => Ok(row(&inserted)?),
        None => Err(Refused::Duplicate(target)),
    }
}

/// Removes an own word. Removing one that is not there is not an error: the
/// learner wanted it gone, and it is.
///
/// # Errors
///
/// Fails when the database rejects the statement.
pub async fn remove(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM own_word WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("failed to remove an own word")?;
    Ok(())
}

fn row(row: &sqlx::sqlite::SqliteRow) -> Result<OwnWord> {
    let target: String = row.get("target");
    let language: String = row.get("language");
    Ok(OwnWord {
        id: row.get("id"),
        kind: OwnKind::parse(row.get::<String, _>("kind").as_str())?,
        native: row.get("native"),
        known: known(&language, &target),
        target,
    })
}

/// Whether the lexicon knows every word of a phrase. A language without a
/// lexicon knows nothing to doubt, so everything passes.
fn known(language: &str, target: &str) -> bool {
    let Some(lexicon) = lexicon::of(language) else { return true };
    lexicon::tokens(target).iter().all(|token| !lexicon.lemmas_of(&token.spelling()).is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let pack: crate::pack::Pack = toml::from_str(
            r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "f"
name = "n"
pattern = "x"
explanation = "e"
order = 1
  [[formula.sample]]
  native = "a"
  target = "b"
"#,
        )
        .unwrap();
        crate::pack::load(&pool, &pack).await.unwrap();
        pool
    }

    fn verb(native: &str, target: &str) -> NewWord {
        NewWord {
            kind: OwnKind::Verb,
            native: native.to_string(),
            target: target.to_string(),
        }
    }

    #[tokio::test]
    async fn a_word_is_added_once_tidied_and_listed_newest_first() {
        let pool = pool().await;
        let now = Utc::now();
        let swim = add(&pool, &verb(" плавать ", "  swim "), now).await.unwrap();
        assert_eq!((swim.native.as_str(), swim.target.as_str()), ("плавать", "swim"));
        assert!(swim.known);
        add(&pool, &verb("водить", "drive"), now + chrono::Duration::seconds(1)).await.unwrap();

        let listed: Vec<_> = list(&pool).await.unwrap().into_iter().map(|word| word.target).collect();
        assert_eq!(listed, vec!["drive", "swim"]);
        assert!(matches!(add(&pool, &verb("плыть", "swim"), now).await, Err(Refused::Duplicate(_))));
        // The same spelling as a noun is another word.
        let noun = NewWord {
            kind: OwnKind::Noun,
            ..verb("заплыв", "swim")
        };
        add(&pool, &noun, now).await.expect("a noun and a verb spelt alike are two words");
    }

    #[tokio::test]
    async fn an_empty_or_overlong_word_is_refused() {
        let pool = pool().await;
        assert!(matches!(add(&pool, &verb("", "swim"), Utc::now()).await, Err(Refused::Empty)));
        assert!(matches!(add(&pool, &verb("плавать", "   "), Utc::now()).await, Err(Refused::Empty)));
        let long = "swim ".repeat(10);
        assert!(matches!(add(&pool, &verb("плавать", &long), Utc::now()).await, Err(Refused::TooLong)));
    }

    #[tokio::test]
    async fn a_word_the_lexicon_does_not_know_is_kept_and_flagged() {
        let pool = pool().await;
        let word = add(&pool, &verb("получать", "recieve"), Utc::now()).await.unwrap();
        assert!(!word.known, "a misspelling should be flagged");
        let word = add(&pool, &verb("ходить", "goes"), Utc::now()).await.unwrap();
        assert!(word.known, "a form of a lemma is a word the lexicon knows");
    }

    #[tokio::test]
    async fn without_a_pack_there_is_no_language_to_add_to() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        assert!(matches!(add(&pool, &verb("плавать", "swim"), Utc::now()).await, Err(Refused::NoLanguage)));
    }

    #[tokio::test]
    async fn a_word_removed_is_gone_and_removing_it_again_is_fine() {
        let pool = pool().await;
        let word = add(&pool, &verb("плавать", "swim"), Utc::now()).await.unwrap();
        remove(&pool, word.id).await.unwrap();
        remove(&pool, word.id).await.unwrap();
        assert!(list(&pool).await.unwrap().is_empty());
    }
}
