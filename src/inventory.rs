//! The inventory: starting from what the learner already knows.
//!
//! A learner who arrives with a few years of school English knows hundreds of
//! the commonest words already, and a deck that opens "house" as new in its
//! third week spends a sitting on nothing. The inventory walks the commonest
//! words of the language being learnt, commonest first, and asks one thing of
//! each: do you know it? A yes is a declared answer on the word's card - sewn
//! at once, and checked when a sentence brings the word (see
//! [`crate::scheduling::Source`]). A no is kept as well, so the run never
//! asks it again. A thousand words is an evening.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::lexicon::{self, LEVELS};
use crate::pack::{self, word_id};
use crate::scheduling::Kind;
use crate::study;

/// How far down the frequency list the inventory goes: the last level. Past
/// it a word is a matter for the reader, not for placing a learner.
pub const REACH: u32 = LEVELS[LEVELS.len() - 1];

/// How many words one read of the run hands out: enough for the screen to
/// stay a few dozen ahead of the learner's thumb.
pub const BATCH: usize = 50;

/// A word the run asks about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    /// `en:house`: what the answer is sent back under.
    pub word: String,
    /// As written: "house", "I", "Monday".
    pub lemma: String,
    /// How it sounds, IPA, when the lexicon has it. The inventory shows it
    /// rather than saying the word: a word outside the material is not sent
    /// to a paid voice a thousand times in an evening.
    pub ipa: Option<String>,
    /// Its place in the frequency list, 1 for the commonest.
    pub rank: u32,
}

/// One level, and how far the run has gone through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Reach {
    /// How many lemmas the level is: 1000, 2000, 5000.
    pub size: u32,
    /// Words of the level still to be asked.
    pub left: u32,
    /// Words of the level answered "I know it".
    pub known: u32,
}

/// Where the inventory stands, and what it asks next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    /// The language being learnt; `None` before a pack is loaded.
    pub language: Option<String>,
    /// The next words to ask, commonest first. Empty once the run reaches
    /// [`REACH`], or when the language has no lexicon to walk.
    pub next: Vec<Item>,
    /// The levels, and how far the run has gone through each.
    pub levels: Vec<Reach>,
    /// Answers given, either way.
    pub answered: i64,
    /// Answers that were "I know it".
    pub known: i64,
}

/// One answer, as the screen sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct Answer {
    /// `en:house`.
    pub word: String,
    pub known: bool,
}

/// Why an answer was not taken: the client's to fix, said in a sentence.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("{0:?} is not a word of the language being learnt")]
    NotAWord(String),
    #[error("{0:?} has been answered in the drill since, so it is the drill's now")]
    Asked(String),
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

/// The run as it stands: the next `limit` words to ask and how far it has
/// gone.
///
/// A word the learner is already learning - started in the drill - is not
/// asked: the drill knows more about it than a yes would say.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn run(pool: &SqlitePool, limit: usize) -> Result<Run> {
    let Some(language) = pack::language_being_learnt(pool).await? else {
        return Ok(Run {
            language: None,
            next: Vec::new(),
            levels: Vec::new(),
            answered: 0,
            known: 0,
        });
    };
    let prefix = format!("{language}:");

    let answers: HashMap<String, bool> = sqlx::query_as::<_, (String, bool)>("SELECT word_id, known FROM inventory WHERE substr(word_id, 1, ?) = ?")
        .bind(i64::try_from(prefix.len()).unwrap_or(i64::MAX))
        .bind(&prefix)
        .fetch_all(pool)
        .await
        .context("failed to read the inventory")?
        .into_iter()
        .collect();
    let learning: HashSet<String> = sqlx::query_scalar("SELECT subject_id FROM card WHERE kind = 'word' AND state != 'new' AND substr(subject_id, 1, ?) = ?")
        .bind(i64::try_from(prefix.len()).unwrap_or(i64::MAX))
        .bind(&prefix)
        .fetch_all(pool)
        .await
        .context("failed to read the words being learnt")?
        .into_iter()
        .collect();

    let mut next = Vec::new();
    let mut levels: Vec<Reach> = LEVELS.iter().map(|&size| Reach { size, left: 0, known: 0 }).collect();
    for entry in lexicon::of(&language).into_iter().flat_map(lexicon::Lexicon::entries) {
        if entry.rank > REACH {
            break;
        }
        let id = word_id(&language, &entry.lemma);
        let answer = answers.get(&id).copied();
        let asked = answer.is_some() || learning.contains(&id);
        for level in levels.iter_mut().filter(|level| entry.rank <= level.size) {
            if !asked {
                level.left += 1;
            }
            if answer == Some(true) {
                level.known += 1;
            }
        }
        if !asked && next.len() < limit {
            next.push(Item {
                word: id,
                lemma: entry.lemma.clone(),
                ipa: entry.ipa.clone(),
                rank: entry.rank,
            });
        }
    }

    Ok(Run {
        // A language without a lexicon has no frequency list to walk.
        levels: if lexicon::of(&language).is_some() { levels } else { Vec::new() },
        language: Some(language),
        next,
        answered: i64::try_from(answers.len()).unwrap_or(i64::MAX),
        known: i64::try_from(answers.values().filter(|known| **known).count()).unwrap_or(i64::MAX),
    })
}

/// Takes answers, in the order given: a yes declares the word known on its
/// card, a no is only kept. One transaction for all of them.
///
/// An answer to a word already answered, or already being learnt, changes
/// nothing - a screen that sends the same answer twice after a dropped
/// connection is not asking twice.
///
/// # Errors
///
/// Refuses a word that is not a lemma of the language being learnt, and
/// fails when the database rejects a statement.
pub async fn answer(pool: &SqlitePool, answers: &[Answer], now: DateTime<Utc>) -> Result<(), Refused> {
    let language = pack::language_being_learnt(pool).await?;
    for answer in answers {
        let lemma = answer
            .word
            .split_once(':')
            .filter(|(code, _)| Some(*code) == language.as_deref())
            .and_then(|(code, lemma)| lexicon::of(code)?.entry(lemma));
        if lemma.is_none_or(|entry| word_id(language.as_deref().unwrap_or_default(), &entry.lemma) != answer.word) {
            return Err(Refused::NotAWord(answer.word.clone()));
        }
    }

    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    for answer in answers {
        let taken = sqlx::query(
            "INSERT INTO inventory (word_id, known, answered_at)
             SELECT ?1, ?2, ?3
             WHERE NOT EXISTS (SELECT 1 FROM card WHERE kind = 'word' AND subject_id = ?1 AND state != 'new')
             ON CONFLICT (word_id) DO NOTHING",
        )
        .bind(&answer.word)
        .bind(answer.known)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await
        .context("failed to keep an answer of the inventory")?
        .rows_affected()
            == 1;
        if taken && answer.known {
            study::declare(&mut tx, &answer.word, now).await?;
        }
    }
    tx.commit().await.context("failed to commit the answers of the inventory")?;
    Ok(())
}

/// Takes an answer back: the run asks the word again, and a yes leaves the
/// word's card as it was before it - new for a word a pack teaches, gone for
/// one only the declaration had made a card for.
///
/// Taking back an answer never given is not an error: the learner wanted it
/// gone, and it is.
///
/// # Errors
///
/// Refuses a yes whose word has been answered in the drill since - that
/// history stands on the declaration - and fails when the database rejects a
/// statement.
pub async fn undo(pool: &SqlitePool, word: &str, now: DateTime<Utc>) -> Result<(), Refused> {
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let known: Option<bool> = sqlx::query_scalar("SELECT known FROM inventory WHERE word_id = ?")
        .bind(word)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to read an answer of the inventory")?;
    let Some(known) = known else { return Ok(()) };

    if known {
        let asked: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM review r JOIN card c ON c.id = r.card_id
                            WHERE c.kind = 'word' AND c.subject_id = ? AND r.source = 'asked')",
        )
        .bind(word)
        .fetch_one(&mut *tx)
        .await
        .context("failed to ask whether a word has been answered")?;
        if asked {
            return Err(Refused::Asked(word.to_string()));
        }
        sqlx::query(
            "DELETE FROM review WHERE source = 'declared'
               AND card_id IN (SELECT id FROM card WHERE kind = 'word' AND subject_id = ?)",
        )
        .bind(word)
        .execute(&mut *tx)
        .await
        .context("failed to take a declaration back")?;
        study::rebuild(&mut tx, Kind::Word, word, now).await?;
    }

    sqlx::query("DELETE FROM inventory WHERE word_id = ?")
        .bind(word)
        .execute(&mut *tx)
        .await
        .context("failed to take an answer of the inventory back")?;
    tx.commit().await.context("failed to commit taking an answer back")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduling::Rating;
    use crate::study::{self, Answer as Said, Asked, Subject, Turn};

    /// A pack with one formula whose two sentences teach "doctor" and "home".
    const PACK: &str = r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[word]]
lemma = "doctor"
gloss = "d"

[[word]]
lemma = "home"
gloss = "h"

[[formula]]
id = "be"
name = "n"
pattern = "x"
explanation = "e"
order = 1

  [[formula.sample]]
  native = "a"
  target = "He is a doctor."
  words = ["doctor"]

  [[formula.sample]]
  native = "b"
  target = "I am at home."
  words = ["home"]
"#;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pack::load(&pool, &toml::from_str(PACK).unwrap()).await.unwrap();
        pool
    }

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T18:00:00Z").unwrap().with_timezone(&Utc) + chrono::Duration::days(day)
    }

    fn said(word: &str, known: bool) -> Answer {
        Answer { word: word.to_string(), known }
    }

    async fn stitch(pool: &SqlitePool, word: &str) -> Option<String> {
        sqlx::query_scalar("SELECT state FROM card WHERE kind = 'word' AND subject_id = ?")
            .bind(word)
            .fetch_optional(pool)
            .await
            .unwrap()
    }

    /// Drills the formula once, both sentences typed right.
    async fn drill(pool: &SqlitePool, now: DateTime<Utc>) {
        let formula = study::formula(pool, "be").await.unwrap();
        let turns = formula
            .samples
            .iter()
            .map(|sample| Turn {
                asked: Asked::Sample(sample.sentence),
                typed: sample.target.clone(),
            })
            .collect();
        study::review(
            pool,
            "be",
            Said::Produce {
                turns,
                easy: false,
                duration_ms: Some(4000),
            },
            now,
        )
        .await
        .unwrap();
    }

    fn words_in(today: &study::Today) -> Vec<&str> {
        today
            .queue
            .iter()
            .filter_map(|due| match &due.subject {
                Subject::Word { word } => Some(word.id.as_str()),
                Subject::Formula { .. } => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn the_run_starts_with_the_commonest_words() {
        let pool = pool().await;
        let run = run(&pool, 5).await.unwrap();
        assert_eq!(run.language.as_deref(), Some("en"));
        let ranks: Vec<u32> = run.next.iter().map(|item| item.rank).collect();
        assert_eq!(ranks, vec![1, 2, 3, 4, 5]);
        assert_eq!(run.levels.iter().map(|level| level.left).collect::<Vec<_>>(), vec![1000, 2000, 5000]);
    }

    #[tokio::test]
    async fn a_yes_sews_the_word_and_neither_answer_is_asked_again() {
        let pool = pool().await;
        let first = run(&pool, 2).await.unwrap().next;
        answer(&pool, &[said(&first[0].word, true), said(&first[1].word, false)], at(0)).await.unwrap();

        let after = run(&pool, 2).await.unwrap();
        assert!(
            after.next.iter().all(|item| item.rank > 2),
            "answered words should not be asked again: {:?}",
            after.next
        );
        assert_eq!((after.answered, after.known), (2, 1));
        assert_eq!(
            after.levels[0],
            Reach {
                size: 1000,
                left: 998,
                known: 1
            }
        );

        assert_eq!(
            stitch(&pool, &first[0].word).await.as_deref(),
            Some("sewn"),
            "a yes should sew the word at once"
        );
        assert_eq!(stitch(&pool, &first[1].word).await, None, "a no should leave no card behind");
        let source: String = sqlx::query_scalar("SELECT r.source FROM review r JOIN card c ON c.id = r.card_id WHERE c.subject_id = ?")
            .bind(&first[0].word)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(source, "declared");

        let standing = crate::words::standing(&pool).await.unwrap();
        assert_eq!(standing.levels[0].sewn, 1, "a declared word should count toward its level");
        assert_eq!(standing.declared, 1);
        assert_eq!(standing.counts.sewn, 0, "a word no pack teaches is not one of the pack's words");
    }

    #[tokio::test]
    async fn declaring_is_not_answering() {
        // An evening of the inventory is not an evening of answers: the
        // day's tally stays at what was asked, and the day's room for new
        // words stays open for the words the drill opens.
        let pool = pool().await;
        let words: Vec<Answer> = run(&pool, 40).await.unwrap().next.iter().map(|item| said(&item.word, true)).collect();
        answer(&pool, &words, at(0)).await.unwrap();

        let today = study::today(&pool, at(0)).await.unwrap();
        assert_eq!(today.reviewed_today, 0, "declarations should not count as answers");
        drill(&pool, at(0)).await;
        let today = study::today(&pool, at(0)).await.unwrap();
        assert_eq!(today.reviewed_today, 1);
        assert!(
            words_in(&today).contains(&"en:home"),
            "the day's room for new words should not be spent by declarations: {:?}",
            words_in(&today)
        );
    }

    #[tokio::test]
    async fn a_declared_word_of_the_pack_waits_for_its_sentence() {
        let pool = pool().await;
        answer(&pool, &[said("en:doctor", true)], at(0)).await.unwrap();
        assert_eq!(stitch(&pool, "en:doctor").await.as_deref(), Some("sewn"));

        // A month on the word is due, but no sentence holding it has been met.
        let today = study::today(&pool, at(30)).await.unwrap();
        assert!(!words_in(&today).contains(&"en:doctor"), "a word should not be asked in a sentence never met");

        drill(&pool, at(30)).await;
        let today = study::today(&pool, at(30)).await.unwrap();
        let words = words_in(&today);
        assert!(words.contains(&"en:doctor"), "once its sentence is met, a due word comes back: {words:?}");
        let opened: Vec<bool> = today
            .queue
            .iter()
            .filter(|due| matches!(&due.subject, Subject::Word { word } if word.id == "en:doctor"))
            .map(|due| due.is_new)
            .collect();
        assert_eq!(opened, vec![false], "a declared word comes back as a review, not as a new word");
    }

    #[tokio::test]
    async fn an_answer_taken_back_is_asked_again_and_leaves_the_card_as_it_was() {
        let pool = pool().await;
        answer(&pool, &[said("en:doctor", true), said("en:house", true)], at(0)).await.unwrap();

        undo(&pool, "en:doctor", at(0)).await.unwrap();
        undo(&pool, "en:house", at(0)).await.unwrap();
        assert_eq!(stitch(&pool, "en:doctor").await.as_deref(), Some("new"), "a pack's word goes back to new");
        assert_eq!(stitch(&pool, "en:house").await, None, "a card only the declaration made goes with it");
        let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review").fetch_one(&pool).await.unwrap();
        assert_eq!(reviews, 0);
        let run = run(&pool, 5000).await.unwrap();
        assert!(run.next.iter().any(|item| item.word == "en:house"), "a word taken back should be asked again");
        assert_eq!(run.answered, 0);

        undo(&pool, "en:house", at(0)).await.expect("taking back what is not there is fine");
    }

    #[tokio::test]
    async fn a_yes_answered_in_the_drill_since_cannot_be_taken_back() {
        let pool = pool().await;
        answer(&pool, &[said("en:doctor", true)], at(0)).await.unwrap();
        drill(&pool, at(30)).await;
        study::review_word(
            &pool,
            "en:doctor",
            study::Graded {
                rating: Rating::Good,
                duration_ms: None,
            },
            at(30),
        )
        .await
        .unwrap();
        assert!(matches!(undo(&pool, "en:doctor", at(30)).await, Err(Refused::Asked(_))));
    }

    #[tokio::test]
    async fn only_words_of_the_language_are_taken_and_twice_is_once() {
        let pool = pool().await;
        for word in ["en:qwzx", "es:casa", "house", "en:House"] {
            assert!(matches!(answer(&pool, &[said(word, true)], at(0)).await, Err(Refused::NotAWord(_))), "{word}");
        }
        answer(&pool, &[said("en:house", true)], at(0)).await.unwrap();
        answer(&pool, &[said("en:house", false)], at(1)).await.unwrap();
        let known: bool = sqlx::query_scalar("SELECT known FROM inventory WHERE word_id = 'en:house'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(known, "a second answer to the same word should change nothing");
        let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review").fetch_one(&pool).await.unwrap();
        assert_eq!(reviews, 1);
    }

    #[tokio::test]
    async fn a_word_already_being_learnt_is_not_asked() {
        let pool = pool().await;
        drill(&pool, at(0)).await;
        let words: Vec<String> = words_in(&study::today(&pool, at(0)).await.unwrap()).into_iter().map(str::to_string).collect();
        for word in &words {
            study::review_word(
                &pool,
                word,
                study::Graded {
                    rating: Rating::Good,
                    duration_ms: None,
                },
                at(0),
            )
            .await
            .unwrap();
        }
        let run = run(&pool, 5000).await.unwrap();
        assert!(
            !run.next.iter().any(|item| item.word == "en:home"),
            "a word started in the drill is the drill's"
        );
        answer(&pool, &[said("en:home", true)], at(0)).await.unwrap();
        let inventoried: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory").fetch_one(&pool).await.unwrap();
        assert_eq!(inventoried, 0, "an answer about a word being learnt should change nothing");
    }
}
