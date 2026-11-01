//! Words: lemmas the learner meets inside sentences.
//!
//! A word is never learnt alone. It enters the deck only once a sentence that
//! holds it has been met - the formula that sentence belongs to has been
//! drilled, or the learner took the word from a sentence of a text they were
//! reading - and it is asked in that sentence: heard, read with the word
//! marked, understood. One word met in three sentences is one card with three
//! contexts, and one of them is its anchor, the sentence it is heard in every
//! time it comes back.
//!
//! How common a word is and how it sounds come from the lexicon of its
//! language ([`crate::lexicon`]); what it means comes from the pack - its own
//! gloss for a word it teaches, its glossary for any other.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use hilvan_core::cover::Hold;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::lexicon::{self, LEVELS};
use crate::pack::{self, word_id};
use crate::reading;
use crate::scheduling::{Break, Stitch};
use crate::study::{self, Counts, Languages};

/// A word with everything its card needs.
#[derive(Debug, Clone, Serialize)]
pub struct Word {
    /// `en:doctor`: what its card and its anchor key on.
    pub id: String,
    /// As written: "doctor", "I", "Monday".
    pub lemma: String,
    /// What it means, in the learner's own language: the pack's gloss for a
    /// word it teaches, its glossary's for any other; `None` when neither
    /// has one.
    pub gloss: Option<String>,
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
    /// has been met anywhere, every sentence of a pack that holds it.
    pub contexts: Vec<Context>,
    pub languages: Languages,
}

/// One sentence a word stands in.
#[derive(Debug, Clone, Serialize)]
pub struct Context {
    pub sentence: i64,
    /// The sentence, in the language being learnt.
    pub text: String,
    /// What it means, in the learner's own; `None` for a sentence of a text,
    /// which comes without one.
    pub translation: Option<String>,
    /// How the word is spelt here: "went" for go.
    pub form: String,
    /// The character the word starts at in `text`.
    pub start: i64,
    /// Where the sentence comes from.
    pub source: Source,
    /// Whether this is the sentence the word is heard in.
    pub anchor: bool,
}

/// Where a sentence comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    /// A worked example of a formula.
    Formula { id: String },
    /// A text the learner reads; `None` once the text is gone and the
    /// sentence is kept for the word taken from it.
    Text { id: Option<i64>, title: Option<String> },
}

/// Where the learner stands with words.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Standing {
    /// Every word of the deck by state: the packs' and those taken from
    /// texts.
    pub counts: Counts,
    /// The levels a learner aims at - the commonest 1000, 2000 and 5000
    /// lemmas of the language - and how many of each are in hand: every word
    /// with a card, the deck's and those declared known.
    pub levels: Vec<Level>,
    /// Words met in a sentence and not started yet. Told on Today without
    /// those already in the day's queue: what later days will open, at most
    /// [`crate::study::NEW_WORDS_PER_DAY`] a day.
    pub waiting: i64,
    /// Words held on the learner's word alone: declared known and not asked
    /// since. Counted in the levels, and said apart, because a level is only
    /// as honest as the words in it.
    pub declared: i64,
}

/// One word's card, as the standing counts it.
#[derive(Debug, Clone)]
pub struct Held {
    /// `en:house`.
    pub id: String,
    pub stitch: Stitch,
    /// Whether it is in the deck: a loaded pack teaches it, or the learner
    /// took it from a text.
    pub taught: bool,
    /// Whether it was declared known and never asked.
    pub declared: bool,
}

impl Standing {
    /// The standing a set of word cards makes. `waiting` is left at zero for
    /// the caller: it is about the queue, not about the cards.
    #[must_use]
    pub fn tally(cards: &[Held]) -> Self {
        let mut counts = Counts::default();
        let mut levels: Vec<Level> = LEVELS.iter().map(|&size| Level { size, basted: 0, sewn: 0 }).collect();
        let mut declared = 0;
        for card in cards {
            if card.taught {
                counts.add(card.stitch);
            }
            if card.declared && card.stitch != Stitch::New {
                declared += 1;
            }
            let Some(rank) = rank(&card.id) else { continue };
            for level in levels.iter_mut().filter(|level| rank <= level.size) {
                match card.stitch {
                    Stitch::New => {}
                    Stitch::Basted => level.basted += 1,
                    Stitch::Sewn => level.sewn += 1,
                }
            }
        }
        Self {
            counts,
            levels,
            waiting: 0,
            declared,
        }
    }
}

/// Where a word stands in its language's frequency list, read off its id:
/// `en:house` is "house" in the English lexicon.
#[must_use]
pub fn rank(id: &str) -> Option<u32> {
    let (language, lemma) = id.split_once(':')?;
    lexicon::of(language)?.entry(lemma).map(|entry| entry.rank)
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

/// The learner's own language for the language being learnt: the native
/// language of the first pack loaded that teaches it.
async fn native_of(pool: &SqlitePool, language: &str) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT native FROM pack WHERE target = ? ORDER BY loaded_at, id LIMIT 1")
        .bind(language)
        .fetch_optional(pool)
        .await
        .context("failed to read the learner's own language")
}

/// One word, with its contexts and its anchor.
///
/// Any word of the language: one a pack teaches, and any lemma of its
/// lexicon - a word declared known, or met in a text.
///
/// # Errors
///
/// Fails when there is no such word - no pack teaches it and the lexicon of
/// its language does not have it, or no pack teaches its language - or when
/// the database rejects a query.
pub async fn word(pool: &SqlitePool, id: &str) -> Result<Word> {
    let (language, key) = id.split_once(':').with_context(|| format!("there is no word called {id}"))?;
    let taught: Option<(String, String)> = sqlx::query_as("SELECT lemma, gloss FROM word WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a word")?;
    let entry = lexicon::of(language).and_then(|lexicon| lexicon.entry(key));
    let (lemma, gloss) = match (taught, entry) {
        (Some((lemma, gloss)), _) => (lemma, Some(gloss)),
        (None, Some(entry)) => (entry.lemma.clone(), None),
        (None, None) => anyhow::bail!("there is no word called {id}"),
    };
    let native = native_of(pool, language)
        .await?
        .with_context(|| format!("there is no word called {id}: no pack teaches {language}"))?;
    let gloss = match gloss {
        Some(gloss) => Some(gloss),
        None => glossed(pool, id, &native).await?,
    };

    Ok(Word {
        contexts: contexts(pool, id, &native).await?,
        id: id.to_string(),
        gloss,
        ipa: entry.and_then(|entry| entry.ipa.clone()),
        rank: entry.map(|entry| entry.rank),
        level: entry.and_then(lexicon::Entry::level),
        lemma,
        languages: Languages {
            native,
            target: language.to_string(),
        },
    })
}

/// What a word means by the glossary of a pack into `native`.
async fn glossed(pool: &SqlitePool, id: &str, native: &str) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT g.text FROM gloss g JOIN pack p ON p.id = g.pack_id
         WHERE g.word_id = ? AND p.native = ? ORDER BY p.loaded_at, p.id LIMIT 1",
    )
    .bind(id)
    .bind(native)
    .fetch_optional(pool)
    .await
    .context("failed to read the gloss of a word")
}

/// The sentences a word stands in, its anchor first.
///
/// Only the sentences already met: a sentence of a formula not yet drilled
/// would spoil it, and a word is learnt from what has been seen. The
/// sentences it was taken from are met by being read. A word not met
/// anywhere yet - asked for by id before it is due - shows every sentence of
/// a pack that holds it rather than nothing.
async fn contexts(pool: &SqlitePool, id: &str, native: &str) -> Result<Vec<Context>> {
    let rows = sqlx::query(
        "SELECT s.id, s.text, t.text AS translation, sw.form, sw.start, f.id AS formula,
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
                source: Source::Formula { id: row.get("formula") },
                anchor: false,
            },
            met,
        ));
    }

    // The sentences the word was taken from, in the order they were: met,
    // and after the pack's. One a formula shows as well is that context.
    // The text a sentence stands in is the earliest of those that share it.
    let picked = sqlx::query(
        "SELECT s.id, s.text, t.text AS translation, p.form, p.start, x.id AS text_id, x.title
         FROM pick p
         JOIN sentence s ON s.id = p.sentence_id
         LEFT JOIN sentence_translation t ON t.sentence_id = s.id AND t.language = ?
         LEFT JOIN text x ON x.id = (SELECT ts.text_id FROM text_sentence ts JOIN text y ON y.id = ts.text_id
                                     WHERE ts.sentence_id = s.id ORDER BY y.added_at, y.id LIMIT 1)
         WHERE p.word_id = ?
         ORDER BY p.picked_at, s.id",
    )
    .bind(native)
    .bind(id)
    .fetch_all(pool)
    .await
    .context("failed to read the sentences a word was taken from")?;
    for row in picked {
        let sentence: i64 = row.get("id");
        if let Some(&index) = at.get(&sentence) {
            found[index].1 = true;
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
                source: Source::Text {
                    id: row.get("text_id"),
                    title: row.get("title"),
                },
                anchor: false,
            },
            true,
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
    let Some((language, _)) = id.split_once(':') else { return Ok(()) };
    let Some(native) = native_of(pool, language).await? else { return Ok(()) };
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

/// Words that have come due by `now`, oldest first, the breaks counted.
///
/// Only words met in a sentence: a word is asked in a sentence the learner
/// has seen. One answered in its sentence always has been; one declared
/// known waits, sewn, until the drill brings a sentence that holds it or the
/// learner takes it from one.
///
/// # Errors
///
/// Fails when the database rejects the query or holds a moment that is not
/// one.
pub async fn due(pool: &SqlitePool, now: DateTime<Utc>, breaks: &[Break]) -> Result<Vec<String>> {
    Ok(coming(pool, now, breaks).await?.into_iter().map(|(id, _)| id).collect())
}

/// The words that have come back by `until`, with the moment each does -
/// [`due`] at `now`, and what the next days bring when `until` is later.
///
/// # Errors
///
/// Fails when the database rejects the query or holds a moment that is not
/// one.
pub async fn coming(pool: &SqlitePool, until: DateTime<Utc>, breaks: &[Break]) -> Result<Vec<(String, DateTime<Utc>)>> {
    // FSRS's due is never later than the moment a card comes back, so the
    // query narrows to the candidates and the breaks decide among them.
    let rows = sqlx::query(
        "SELECT c.subject_id, c.due, c.last_reviewed FROM card c
         WHERE c.kind = 'word' AND c.state != 'new' AND c.due <= ?
           AND EXISTS (SELECT 1 FROM met m WHERE m.word_id = c.subject_id)
         ORDER BY c.due, c.subject_id",
    )
    .bind(until.to_rfc3339())
    .fetch_all(pool)
    .await
    .context("failed to read the words that are due")?;

    let mut coming = Vec::with_capacity(rows.len());
    for row in rows {
        if let Some(back) = study::back_at(&row.get::<String, _>("due"), row.get("last_reviewed"), breaks)?
            && back <= until
        {
            coming.push((row.get("subject_id"), back));
        }
    }
    Ok(coming)
}

/// Words met in a sentence and never answered, in the order they open.
///
/// The words the learner took from texts first, in the order they took
/// them: each is a word they asked for, in a sentence they chose. Then the
/// words the pack's sentences brought, commonest first: a learner aims at
/// the 1k level before the 2k one, and a word from the first thousand is
/// worth more in every text they will read. Words the lexicon does not rank
/// come last; a tie goes to the word met first.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn waiting(pool: &SqlitePool) -> Result<Vec<String>> {
    let mut words: Vec<String> = sqlx::query_scalar(
        "SELECT p.word_id FROM pick p
         JOIN card c ON c.kind = 'word' AND c.subject_id = p.word_id AND c.state = 'new'
         GROUP BY p.word_id ORDER BY min(p.picked_at), p.word_id",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the words taken from texts")?;
    let mut seen: HashSet<String> = words.iter().cloned().collect();

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

    let mut met: Vec<(String, u32, usize)> = Vec::new();
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
        let order = met.len();
        met.push((id, rank, order));
    }
    met.sort_by_key(|(_, rank, order)| (*rank, *order));
    words.extend(met.into_iter().map(|(id, _, _)| id));
    Ok(words)
}

/// Where the learner stands with words: by state, and by level.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn standing(pool: &SqlitePool) -> Result<Standing> {
    let rows = sqlx::query(
        "SELECT c.subject_id, c.state,
                EXISTS (SELECT 1 FROM word w WHERE w.id = c.subject_id)
                OR EXISTS (SELECT 1 FROM pick p WHERE p.word_id = c.subject_id) AS taught,
                EXISTS (SELECT 1 FROM review r WHERE r.card_id = c.id AND r.source = 'declared')
                AND NOT EXISTS (SELECT 1 FROM review r WHERE r.card_id = c.id AND r.source = 'asked') AS declared
         FROM card c
         WHERE c.kind = 'word'",
    )
    .fetch_all(pool)
    .await
    .context("failed to count the words")?;

    let mut held = Vec::with_capacity(rows.len());
    for row in rows {
        let stitch = match Stitch::parse(row.get::<String, _>("state").as_str()) {
            Ok(stitch) => stitch,
            Err(error) => {
                // Counted nowhere rather than counted wrong, and said out loud.
                tracing::warn!(%error, "a word's card holds a state the product does not know");
                continue;
            }
        };
        held.push(Held {
            id: row.get("subject_id"),
            stitch,
            taught: row.get("taught"),
            declared: row.get("declared"),
        });
    }

    Ok(Standing {
        waiting: i64::try_from(waiting(pool).await?.len()).unwrap_or(i64::MAX),
        ..Standing::tally(&held)
    })
}

/// Why something the reader asked of a word was not done: the client's to
/// fix, said in a sentence.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("{0:?} is not a word of the language being learnt")]
    NotAWord(String),
    #[error("there is no sentence {0}")]
    NoSentence(i64),
    #[error("{word:?} does not stand at character {start} of sentence {sentence}")]
    NotThere { word: String, sentence: i64, start: i64 },
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

/// Holds a word to being a lemma of the lexicon of the language being
/// learnt, as its id spells it: `en:house`, lowercase.
async fn of_the_language(pool: &SqlitePool, id: &str) -> Result<(), Refused> {
    let language = pack::language_being_learnt(pool).await?;
    let known = id
        .split_once(':')
        .filter(|(code, _)| Some(*code) == language.as_deref())
        .and_then(|(code, lemma)| lexicon::of(code)?.entry(lemma).map(|entry| word_id(code, &entry.lemma)))
        .is_some_and(|spelled| spelled == id);
    if known { Ok(()) } else { Err(Refused::NotAWord(id.to_string())) }
}

/// What the learner holds of some words now, by id; a word they hold
/// nothing of is not in it.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn holds_of(pool: &SqlitePool, ids: &[String]) -> Result<HashMap<String, Hold>> {
    let mut holds = HashMap::new();
    for id in ids {
        if let Some(hold) = reading::hold(pool, id).await? {
            holds.insert(id.clone(), hold);
        }
    }
    Ok(holds)
}

/// Declares words known, the reader's way: "I know these" said of a word, or
/// of every word of a paragraph not yet in hand. Each is a declaration on the
/// word's card, as a yes of the inventory is (ADR 0008). The words returned
/// are those declared; a word already in hand - answered, or declared
/// before - is left as it is. One transaction for all of them.
///
/// # Errors
///
/// [`Refused::NotAWord`] for a word that is not a lemma of the language being
/// learnt - nothing is declared then - and [`Refused::Database`] when the
/// database rejects a statement.
pub async fn declare(pool: &SqlitePool, ids: &[String], now: DateTime<Utc>) -> Result<Vec<String>, Refused> {
    for id in ids {
        of_the_language(pool, id).await?;
    }
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let mut declared = Vec::new();
    for id in ids {
        let held: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM card WHERE kind = 'word' AND subject_id = ? AND state != 'new')")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .context("failed to read a word's card")?;
        if held || declared.contains(id) {
            continue;
        }
        study::declare(&mut tx, id, now).await?;
        declared.push(id.clone());
    }
    tx.commit().await.context("failed to commit the declarations")?;
    Ok(declared)
}

/// Takes declarations back: each word's card is what its other answers
/// leave, and the inventory, if it had asked the word, asks it again. The
/// words returned are those taken back; a word the drill has asked since
/// keeps its declaration, because its history stands on it.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub async fn take_back(pool: &SqlitePool, ids: &[String], now: DateTime<Utc>) -> Result<Vec<String>> {
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let mut taken = Vec::new();
    for id in ids {
        let declared: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM review r JOIN card c ON c.id = r.card_id
                            WHERE c.kind = 'word' AND c.subject_id = ? AND r.source = 'declared')",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to read a word's declarations")?;
        if !declared || taken.contains(id) || !study::take_back(&mut tx, id, now).await? {
            continue;
        }
        sqlx::query("DELETE FROM inventory WHERE word_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("failed to let the inventory ask a word again")?;
        taken.push(id.clone());
    }
    tx.commit().await.context("failed to commit taking declarations back")?;
    Ok(taken)
}

/// Puts a word into the deck with the sentence it was read in: the word at
/// character `start` of sentence `sentence`. The sentence becomes one the
/// word has been met in - it opens among the first new words, and is asked
/// and heard there. A word already in hand gains the sentence as one more
/// context. Taking it from the same sentence again changes nothing.
///
/// # Errors
///
/// [`Refused::NotAWord`] for a word that is not a lemma of the language
/// being learnt, [`Refused::NoSentence`] for a sentence that is not there,
/// [`Refused::NotThere`] when the word at `start` is not this word, and
/// [`Refused::Database`] when the database rejects a statement.
pub async fn pick(pool: &SqlitePool, id: &str, sentence: i64, start: i64, now: DateTime<Utc>) -> Result<(), Refused> {
    of_the_language(pool, id).await?;
    let (language, text): (String, String) = sqlx::query_as("SELECT language, text FROM sentence WHERE id = ?")
        .bind(sentence)
        .fetch_optional(pool)
        .await
        .context("failed to read a sentence")?
        .ok_or(Refused::NoSentence(sentence))?;
    let not_there = || Refused::NotThere {
        word: id.to_string(),
        sentence,
        start,
    };
    let lexicon = lexicon::of(&language).ok_or_else(not_there)?;
    let token = lexicon::tokens(&text)
        .into_iter()
        .find(|token| i64::try_from(token.start).is_ok_and(|at| at == start))
        .ok_or_else(not_there)?;
    let holds_it = lexicon.parts(token.text).iter().flatten().any(|lemma| word_id(&language, lemma) == id);
    if !holds_it {
        return Err(not_there());
    }

    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    sqlx::query(
        "INSERT INTO card (kind, subject_id, state, stability, difficulty, due, reps, lapses)
         VALUES ('word', ?, 'new', 0.0, 0.0, ?, 0, 0)
         ON CONFLICT (kind, subject_id) DO NOTHING",
    )
    .bind(id)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await
    .context("failed to make a card for a word taken from a text")?;
    sqlx::query(
        "INSERT INTO pick (word_id, sentence_id, form, start, picked_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (word_id, sentence_id) DO NOTHING",
    )
    .bind(id)
    .bind(sentence)
    .bind(token.text)
    .bind(start)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await
    .context("failed to keep the sentence a word was taken from")?;
    tx.commit().await.context("failed to commit taking a word")?;
    Ok(())
}

/// Takes a word back out of the deck from a sentence: the sentence is no
/// longer one it was met in, and a word nobody asked yet, which no pack
/// teaches and no other sentence was taken for, leaves the deck.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub async fn unpick(pool: &SqlitePool, id: &str, sentence: i64) -> Result<()> {
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    sqlx::query("DELETE FROM pick WHERE word_id = ? AND sentence_id = ?")
        .bind(id)
        .bind(sentence)
        .execute(&mut *tx)
        .await
        .context("failed to take a word back out of the deck")?;
    sqlx::query(
        "DELETE FROM card WHERE kind = 'word' AND subject_id = ?1 AND state = 'new'
           AND NOT EXISTS (SELECT 1 FROM review r WHERE r.card_id = card.id)
           AND NOT EXISTS (SELECT 1 FROM word WHERE id = ?1)
           AND NOT EXISTS (SELECT 1 FROM pick WHERE word_id = ?1)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .context("failed to let a word leave the deck")?;
    // The anchor goes if it was this sentence: the word is heard in another.
    sqlx::query("DELETE FROM anchor WHERE word_id = ? AND sentence_id = ?")
        .bind(id)
        .bind(sentence)
        .execute(&mut *tx)
        .await
        .context("failed to let go of an anchor")?;
    reading::drop_unshown_sentences(&mut tx).await?;
    tx.commit().await.context("failed to commit taking a word back")?;
    Ok(())
}

/// A sentence of the learner's own material a word stands in.
#[derive(Debug, Clone, Serialize)]
pub struct Example {
    pub sentence: i64,
    pub text: String,
    /// What it means, when the material says: a pack's sentence does.
    pub translation: Option<String>,
    /// The character the word starts at, and how many it takes.
    pub start: u32,
    pub length: u32,
    pub source: Source,
}

/// How many sentences are looked at for examples before the search stops:
/// enough for any word to find a handful, few enough that "be" in a library
/// of books does not read the library.
const EXAMPLE_CANDIDATES: i64 = 400;

/// Sentences of the learner's own material that hold a word: the examples
/// of formulas they have started - which say what they mean - first, then
/// the sentences of the texts they read. At most `limit`.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn examples(pool: &SqlitePool, id: &str, limit: usize) -> Result<Vec<Example>> {
    let Some((language, key)) = id.split_once(':') else { return Ok(Vec::new()) };
    let Some(lexicon) = lexicon::of(language) else { return Ok(Vec::new()) };
    let Some(entry) = lexicon.entry(key) else { return Ok(Vec::new()) };
    let native = native_of(pool, language).await?.unwrap_or_default();

    // Narrowed by spelling in the database, then read word by word: "is"
    // stands in "this" too.
    let spellings: Vec<String> = std::iter::once(entry.lemma.to_lowercase()).chain(entry.forms.iter().cloned()).collect();
    let rows = sqlx::query(
        "SELECT s.id, s.text, t.text AS translation,
                (SELECT fs.formula_id FROM formula_sentence fs JOIN card fc ON fc.kind = 'formula' AND fc.subject_id = fs.formula_id
                 WHERE fs.sentence_id = s.id AND fc.state != 'new' ORDER BY fs.formula_id LIMIT 1) AS formula,
                x.id AS text_id, x.title
         FROM sentence s
         LEFT JOIN sentence_translation t ON t.sentence_id = s.id AND t.language = ?
         LEFT JOIN text x ON x.id = (SELECT ts.text_id FROM text_sentence ts JOIN text y ON y.id = ts.text_id
                                     WHERE ts.sentence_id = s.id ORDER BY y.added_at, y.id LIMIT 1)
         WHERE s.language = ? AND EXISTS (SELECT 1 FROM json_each(?) j WHERE s.text LIKE '%' || j.value || '%')
         ORDER BY s.id LIMIT ?",
    )
    .bind(&native)
    .bind(language)
    .bind(serde_json::to_string(&spellings).context("failed to write the spellings of a word")?)
    .bind(EXAMPLE_CANDIDATES)
    .fetch_all(pool)
    .await
    .context("failed to look for sentences that hold a word")?;

    let mut taught = Vec::new();
    let mut read = Vec::new();
    for row in rows {
        let formula: Option<String> = row.get("formula");
        let text_id: Option<i64> = row.get("text_id");
        let source = match (formula, text_id) {
            (Some(id), _) => Source::Formula { id },
            (None, Some(id)) => Source::Text {
                id: Some(id),
                title: row.get("title"),
            },
            // A sentence of a formula not started yet, or one kept only for
            // a word taken from it: not material the learner has read.
            (None, None) => continue,
        };
        let text: String = row.get("text");
        let Some(token) = lexicon::tokens(&text)
            .into_iter()
            .find(|token| lexicon.parts(token.text).iter().flatten().any(|lemma| word_id(language, lemma) == id))
        else {
            continue;
        };
        let (Ok(start), Ok(length)) = (u32::try_from(token.start), u32::try_from(token.text.chars().count())) else {
            continue;
        };
        let example = Example {
            sentence: row.get("id"),
            translation: row.get("translation"),
            start,
            length,
            source,
            text,
        };
        if matches!(example.source, Source::Formula { .. }) {
            taught.push(example);
        } else {
            read.push(example);
        }
    }
    taught.extend(read);
    taught.truncate(limit);
    Ok(taught)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reading::tests::{at, plain, tutor};
    use crate::scheduling::Rating;
    use crate::study::{Answer as Said, Asked, Graded, Subject, Turn};

    /// A text with one sentence; its id.
    async fn sentence(pool: &SqlitePool, body: &str) -> i64 {
        let library = reading::Library::default();
        let added = reading::add(pool, &library, &plain("Out", body), at(0)).await.unwrap();
        reading::read(pool, &library, added.id, None).await.unwrap().lines[0][0].sentence
    }

    /// Drills the formula once, its sample typed right: its sentence is met.
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
            study::arriving(Said::Produce {
                turns,
                easy: false,
                duration_ms: None,
            }),
            now,
        )
        .await
        .unwrap();
    }

    fn good() -> study::Sent<Graded> {
        study::arriving(Graded {
            rating: Rating::Good,
            duration_ms: None,
        })
    }

    fn ids(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    fn words_in(today: &study::Today) -> Vec<(&str, bool)> {
        today
            .queue
            .iter()
            .filter_map(|due| match &due.subject {
                Subject::Word { word } => Some((word.id.as_str(), due.is_new)),
                Subject::Formula { .. } => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_word_taken_from_a_text_opens_first_and_is_asked_in_its_sentence() {
        let pool = tutor().await;
        drill(&pool, at(0)).await;
        let taken = sentence(&pool, "She went to the house.").await;
        pick(&pool, "en:house", taken, 16, at(1)).await.unwrap();

        assert_eq!(
            waiting(&pool).await.unwrap(),
            ids(&["en:house", "en:doctor"]),
            "taken first, then what the pack brought"
        );
        let today = study::today(&pool, at(2)).await.unwrap();
        assert_eq!(words_in(&today), vec![("en:house", true), ("en:doctor", true)]);

        let house = word(&pool, "en:house").await.unwrap();
        assert_eq!(house.gloss.as_deref(), Some("дом"), "a word no pack teaches is glossed by the glossary");
        assert_eq!(house.contexts.len(), 1);
        let context = &house.contexts[0];
        assert_eq!(
            (context.text.as_str(), context.form.as_str(), context.start),
            ("She went to the house.", "house", 16)
        );
        assert_eq!(context.translation, None);
        assert!(matches!(&context.source, Source::Text { title: Some(title), .. } if title == "Out"));

        study::review_word(&pool, "en:house", good(), at(3)).await.unwrap();
        let anchor: i64 = sqlx::query_scalar("SELECT sentence_id FROM anchor WHERE word_id = 'en:house'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(anchor, taken, "heard in the sentence it was taken from");
        assert_eq!(
            standing(&pool).await.unwrap().counts.basted,
            1,
            "a word taken from a text is a word of the deck"
        );
    }

    #[tokio::test]
    async fn the_packs_gloss_says_more_than_the_glossary() {
        let pool = tutor().await;
        assert_eq!(word(&pool, "en:doctor").await.unwrap().gloss.as_deref(), Some("врач"));
        assert_eq!(word(&pool, "en:the").await.unwrap().gloss, None, "a word the glossary does not have");
        assert!(word(&pool, "en:zxqv").await.is_err());
    }

    #[tokio::test]
    async fn a_word_is_taken_only_from_where_it_stands() {
        let pool = tutor().await;
        let taken = sentence(&pool, "She went to the house.").await;
        assert!(matches!(pick(&pool, "en:house", taken, 4, at(1)).await, Err(Refused::NotThere { .. })));
        assert!(
            matches!(pick(&pool, "en:house", taken, 17, at(1)).await, Err(Refused::NotThere { .. })),
            "inside a word"
        );
        assert!(
            matches!(pick(&pool, "en:houses", taken, 16, at(1)).await, Err(Refused::NotAWord(_))),
            "a form is no lemma"
        );
        assert!(matches!(pick(&pool, "en:house", 999, 16, at(1)).await, Err(Refused::NoSentence(999))));
        pick(&pool, "en:go", taken, 4, at(1)).await.expect("went is go");
        pick(&pool, "en:go", taken, 4, at(2)).await.expect("taking it again changes nothing");
        let picks: i64 = sqlx::query_scalar("SELECT count(*) FROM pick").fetch_one(&pool).await.unwrap();
        assert_eq!(picks, 1);
    }

    #[tokio::test]
    async fn a_word_taken_back_leaves_the_deck_as_it_found_it() {
        let pool = tutor().await;
        let taken = sentence(&pool, "He saw a doctor and a house.").await;
        pick(&pool, "en:house", taken, 22, at(1)).await.unwrap();
        pick(&pool, "en:doctor", taken, 9, at(1)).await.unwrap();
        assert_eq!(reading::hold(&pool, "en:house").await.unwrap(), Some(Hold::Deck));

        unpick(&pool, "en:house", taken).await.unwrap();
        unpick(&pool, "en:doctor", taken).await.unwrap();
        let cards: Vec<String> = sqlx::query_scalar("SELECT subject_id FROM card WHERE kind = 'word'")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(cards, ids(&["en:doctor"]), "the pack's word keeps its card; one only the text brought loses it");
        assert_eq!(
            reading::hold(&pool, "en:doctor").await.unwrap(),
            None,
            "and is not in the deck until its sentence is met"
        );
    }

    #[tokio::test]
    async fn a_declaration_is_taken_back_unless_the_drill_asked_since() {
        let pool = tutor().await;
        assert_eq!(
            declare(&pool, &ids(&["en:the", "en:doctor", "en:the"]), at(0)).await.unwrap(),
            ids(&["en:the", "en:doctor"])
        );
        assert_eq!(declare(&pool, &ids(&["en:the"]), at(1)).await.unwrap(), Vec::<String>::new(), "held already");
        assert!(matches!(declare(&pool, &ids(&["en:the", "en:zxqv"]), at(1)).await, Err(Refused::NotAWord(_))));
        sqlx::query("INSERT INTO inventory (word_id, known, answered_at) VALUES ('en:the', 1, ?)")
            .bind(at(0).to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();

        drill(&pool, at(2)).await;
        study::review_word(&pool, "en:doctor", good(), at(3)).await.unwrap();
        assert_eq!(take_back(&pool, &ids(&["en:the", "en:doctor"]), at(4)).await.unwrap(), ids(&["en:the"]));
        assert_eq!(reading::hold(&pool, "en:the").await.unwrap(), None);
        let card: Option<String> = sqlx::query_scalar("SELECT state FROM card WHERE subject_id = 'en:the'")
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(card, None, "a word only the declaration had brought is nobody's");
        let asked: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory").fetch_one(&pool).await.unwrap();
        assert_eq!(asked, 0, "the inventory asks a word taken back again");
        assert_ne!(
            reading::hold(&pool, "en:doctor").await.unwrap(),
            None,
            "a word the drill asked keeps its declaration"
        );
    }

    #[tokio::test]
    async fn examples_come_from_the_learners_own_material_the_explained_first() {
        let pool = tutor().await;
        assert!(
            examples(&pool, "en:doctor", 4).await.unwrap().is_empty(),
            "a formula not started is not read yet"
        );

        sentence(&pool, "The doctor\u{2019}s car was red.").await;
        drill(&pool, at(1)).await;
        let found = examples(&pool, "en:doctor", 4).await.unwrap();
        let said: Vec<(&str, Option<&str>, u32, u32)> = found
            .iter()
            .map(|example| (example.text.as_str(), example.translation.as_deref(), example.start, example.length))
            .collect();
        assert_eq!(
            said,
            vec![("He is a doctor.", Some("Он врач."), 8, 6), ("The doctor\u{2019}s car was red.", None, 4, 8)]
        );
        assert_eq!(examples(&pool, "en:doctor", 1).await.unwrap().len(), 1);
        assert!(examples(&pool, "en:is", 4).await.unwrap().is_empty(), "a form is no word");
    }

    #[tokio::test]
    async fn a_taken_word_stays_in_the_deck_when_its_declaration_is_taken_back() {
        let pool = tutor().await;
        let taken = sentence(&pool, "She went to the house.").await;
        pick(&pool, "en:house", taken, 16, at(1)).await.unwrap();
        declare(&pool, &ids(&["en:house"]), at(2)).await.unwrap();
        assert_eq!(reading::hold(&pool, "en:house").await.unwrap(), Some(Hold::Sewn));

        take_back(&pool, &ids(&["en:house"]), at(3)).await.unwrap();
        assert_eq!(
            reading::hold(&pool, "en:house").await.unwrap(),
            Some(Hold::Deck),
            "taken from a text, it waits in the deck as before the declaration"
        );
    }

    #[tokio::test]
    async fn a_word_declared_known_waits_for_a_sentence_taken_or_met() {
        let pool = tutor().await;
        declare(&pool, &ids(&["en:house"]), at(0)).await.unwrap();
        let later = at(60 * 24 * 60);
        assert!(due(&pool, later, &[]).await.unwrap().is_empty(), "sewn, and never met");
        let taken = sentence(&pool, "She went to the house.").await;
        pick(&pool, "en:house", taken, 16, at(1)).await.unwrap();
        assert_eq!(due(&pool, later, &[]).await.unwrap(), ids(&["en:house"]));
    }
}
