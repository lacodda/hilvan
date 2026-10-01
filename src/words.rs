//! Words: lemmas the learner meets inside sentences.
//!
//! A word is never learnt alone. It enters the deck only once a sentence that
//! holds it has been met - the formula that sentence belongs to has been
//! drilled - and it is asked in that sentence: heard, read with the word
//! marked, understood. One word met in three sentences is one card with three
//! contexts, and one of them is its anchor, the sentence it is heard in every
//! time it comes back.
//!
//! How common a word is and how it sounds come from the lexicon of its
//! language ([`crate::lexicon`]); what it means comes from the pack.

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::lexicon::{self, LEVELS};
use crate::scheduling::Stitch;
use crate::study::{Counts, Languages};

/// A word with everything its card needs.
#[derive(Debug, Clone, Serialize)]
pub struct Word {
    /// `en:doctor`: what its card and its anchor key on.
    pub id: String,
    /// As written: "doctor", "I", "Monday".
    pub lemma: String,
    /// What it means, in the learner's own language.
    pub gloss: String,
    /// How it sounds, IPA; `None` when the language has no lexicon or the
    /// lexicon no transcription.
    pub ipa: Option<String>,
    /// Its place in the language's frequency list, 1 for the commonest;
    /// `None` past the end of the lexicon.
    pub rank: Option<u32>,
    /// The smallest level that holds it - 1000, 2000 or 5000 - or `None`
    /// past the last.
    pub level: Option<u32>,
    /// The sentences it has been met in, its anchor first. Before the word
    /// has been met anywhere, every sentence that holds it.
    pub contexts: Vec<Context>,
    pub languages: Languages,
}

/// One sentence a word stands in.
#[derive(Debug, Clone, Serialize)]
pub struct Context {
    pub sentence: i64,
    /// The sentence, in the language being learnt.
    pub text: String,
    /// What it means, in the learner's own.
    pub translation: String,
    /// How the word is spelt here: "went" for go.
    pub form: String,
    /// The character the word starts at in `text`.
    pub start: i64,
    /// The formula whose example this is.
    pub formula: String,
    /// Whether this is the sentence the word is heard in.
    pub anchor: bool,
}

/// Where the learner stands with words.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Standing {
    /// Every word of the loaded packs, by state.
    pub counts: Counts,
    /// The levels a learner aims at - the commonest 1000, 2000 and 5000
    /// lemmas of the language - and how many of each are in hand.
    pub levels: Vec<Level>,
    /// Words met in a sentence and not started yet. Told on Today without
    /// those already in the day's queue: what later days will open, at most
    /// [`crate::study::NEW_WORDS_PER_DAY`] a day.
    pub waiting: i64,
}

/// One level, and the learner's words in it.
///
/// Cumulative: the 2000 level counts every word in the commonest two
/// thousand, so "sewn" is how many of those two thousand the learner holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Level {
    /// How many lemmas the level is: 1000, 2000, 5000.
    pub size: u32,
    pub basted: i64,
    pub sewn: i64,
}

/// One word, with its contexts and its anchor.
///
/// # Errors
///
/// Fails when there is no such word, or when the database rejects a query.
pub async fn word(pool: &SqlitePool, id: &str) -> Result<Word> {
    let row = sqlx::query(
        "SELECT w.id, w.lemma, w.gloss, w.language, p.native
         FROM word w JOIN pack p ON p.id = w.pack_id WHERE w.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("failed to read a word")?
    .with_context(|| format!("there is no word called {id}"))?;

    let lemma: String = row.get("lemma");
    let language: String = row.get("language");
    let native: String = row.get("native");
    let entry = lexicon::of(&language).and_then(|lexicon| lexicon.entry(&lemma));

    Ok(Word {
        contexts: contexts(pool, id, &native).await?,
        id: row.get("id"),
        gloss: row.get("gloss"),
        ipa: entry.and_then(|entry| entry.ipa.clone()),
        rank: entry.map(|entry| entry.rank),
        level: entry.and_then(lexicon::Entry::level),
        lemma,
        languages: Languages { native, target: language },
    })
}

/// The sentences a word stands in, its anchor first.
///
/// Only the sentences already met: a sentence of a formula not yet drilled
/// would spoil it, and a word is learnt from what has been seen. A word not
/// met anywhere yet - asked for by id before it is due - shows them all
/// rather than nothing.
async fn contexts(pool: &SqlitePool, id: &str, native: &str) -> Result<Vec<Context>> {
    let rows = sqlx::query(
        "SELECT s.id, s.text, COALESCE(t.text, '') AS translation, sw.form, sw.start, f.id AS formula,
                EXISTS (SELECT 1 FROM card c WHERE c.kind = 'formula' AND c.subject_id = f.id AND c.state != 'new') AS met
         FROM sentence_word sw
         JOIN sentence s ON s.id = sw.sentence_id
         JOIN formula_sentence fs ON fs.sentence_id = s.id
         JOIN formula f ON f.id = fs.formula_id
         LEFT JOIN sentence_translation t ON t.sentence_id = s.id AND t.language = ?
         WHERE sw.word_id = ?
         ORDER BY f.position, fs.position",
    )
    .bind(native)
    .bind(id)
    .fetch_all(pool)
    .await
    .context("failed to read the sentences of a word")?;

    // A sentence two formulas share is one context, placed where it is first
    // shown and met if either formula has been.
    let mut found: Vec<(Context, bool)> = Vec::new();
    let mut at: HashMap<i64, usize> = HashMap::new();
    for row in rows {
        let sentence: i64 = row.get("id");
        let met: bool = row.get("met");
        if let Some(&index) = at.get(&sentence) {
            found[index].1 |= met;
            continue;
        }
        at.insert(sentence, found.len());
        found.push((
            Context {
                sentence,
                text: row.get("text"),
                translation: row.get("translation"),
                form: row.get("form"),
                start: row.get("start"),
                formula: row.get("formula"),
                anchor: false,
            },
            met,
        ));
    }

    let any_met = found.iter().any(|(_, met)| *met);
    let mut shown: Vec<Context> = found.into_iter().filter(|(_, met)| *met || !any_met).map(|(context, _)| context).collect();

    let anchor: Option<i64> = sqlx::query_scalar("SELECT sentence_id FROM anchor WHERE word_id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read the anchor of a word")?;
    // The stored anchor when it is still among the sentences shown; the first
    // sentence the word was met in otherwise.
    let index = anchor
        .and_then(|anchor| shown.iter().position(|context| context.sentence == anchor))
        .unwrap_or(0);
    if index < shown.len() {
        let mut first = shown.remove(index);
        first.anchor = true;
        shown.insert(0, first);
    }
    Ok(shown)
}

/// Fixes the sentence a word is heard in, the first time it is answered.
///
/// The sentence it was shown in, which is the first it was met in; a word
/// answered again keeps the anchor it has.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub async fn anchor(pool: &SqlitePool, id: &str) -> Result<()> {
    let native: Option<String> = sqlx::query_scalar("SELECT p.native FROM word w JOIN pack p ON p.id = w.pack_id WHERE w.id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read the language of a word")?;
    let Some(native) = native else { return Ok(()) };
    let Some(first) = contexts(pool, id, &native).await?.into_iter().next() else {
        return Ok(());
    };
    sqlx::query("INSERT INTO anchor (word_id, sentence_id) VALUES (?, ?) ON CONFLICT (word_id) DO NOTHING")
        .bind(id)
        .bind(first.sentence)
        .execute(pool)
        .await
        .context("failed to fix the anchor of a word")?;
    Ok(())
}

/// Words that have come due by `now`, oldest first.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn due(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT c.subject_id FROM card c JOIN word w ON w.id = c.subject_id
         WHERE c.kind = 'word' AND c.state != 'new' AND c.due <= ?
         ORDER BY c.due, c.subject_id",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await
    .context("failed to read the words that are due")
}

/// Words met in a sentence and never answered, in the order they open.
///
/// Commonest first: a learner aims at the 1k level before the 2k one, and a
/// word from the first thousand is worth more in every text they will read.
/// Words the lexicon does not rank come last; a tie goes to the word met
/// first.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn waiting(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT w.id, w.lemma, w.language FROM word w
         JOIN card c ON c.kind = 'word' AND c.subject_id = w.id AND c.state = 'new'
         JOIN sentence_word sw ON sw.word_id = w.id
         JOIN formula_sentence fs ON fs.sentence_id = sw.sentence_id
         JOIN formula f ON f.id = fs.formula_id
         JOIN card fc ON fc.kind = 'formula' AND fc.subject_id = f.id AND fc.state != 'new'
         ORDER BY f.position, fs.position, w.id",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the words waiting to start")?;

    let mut words: Vec<(String, u32, usize)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let id: String = row.get("id");
        if !seen.insert(id.clone()) {
            continue;
        }
        let lemma: String = row.get("lemma");
        let language: String = row.get("language");
        let rank = lexicon::of(&language)
            .and_then(|lexicon| lexicon.entry(&lemma))
            .map_or(u32::MAX, |entry| entry.rank);
        let met = words.len();
        words.push((id, rank, met));
    }
    words.sort_by_key(|(_, rank, met)| (*rank, *met));
    Ok(words.into_iter().map(|(id, _, _)| id).collect())
}

/// Where the learner stands with words: by state, and by level.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn standing(pool: &SqlitePool) -> Result<Standing> {
    let rows = sqlx::query("SELECT c.state, w.lemma, w.language FROM card c JOIN word w ON w.id = c.subject_id WHERE c.kind = 'word'")
        .fetch_all(pool)
        .await
        .context("failed to count the words")?;

    let mut counts = Counts::default();
    let mut levels: Vec<Level> = LEVELS.iter().map(|&size| Level { size, basted: 0, sewn: 0 }).collect();
    for row in rows {
        let stitch = match Stitch::parse(row.get::<String, _>("state").as_str()) {
            Ok(stitch) => stitch,
            Err(error) => {
                // Counted nowhere rather than counted wrong, and said out loud.
                tracing::warn!(%error, "a word's card holds a state the product does not know");
                continue;
            }
        };
        match stitch {
            Stitch::New => counts.new += 1,
            Stitch::Basted => counts.basted += 1,
            Stitch::Sewn => counts.sewn += 1,
        }
        let lemma: String = row.get("lemma");
        let language: String = row.get("language");
        let Some(rank) = lexicon::of(&language).and_then(|lexicon| lexicon.entry(&lemma)).map(|entry| entry.rank) else {
            continue;
        };
        for level in levels.iter_mut().filter(|level| rank <= level.size) {
            match stitch {
                Stitch::New => {}
                Stitch::Basted => level.basted += 1,
                Stitch::Sewn => level.sewn += 1,
            }
        }
    }

    Ok(Standing {
        counts,
        levels,
        waiting: i64::try_from(waiting(pool).await?.len()).unwrap_or(i64::MAX),
    })
}
