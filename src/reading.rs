//! The reader: texts the learner brings, read as theirs.
//!
//! A text comes in pasted, as a file or by a link ([`crate::import`]) and is
//! kept as chapters of paragraphs of sentences. Its sentences live in the
//! same table as the pack's, so a word taken from one is a word met in a
//! sentence like any other, asked in it, and heard in it. A book has the
//! chapters its source gives; every other text is one chapter.
//!
//! Every running word is read through the lexicon of the language into the
//! lemmas it can be (*went* is *go*, *don't* is *do* and *not*), and the text
//! says how much of it the learner holds: the coverage, of the whole and of
//! each chapter, counted by the same arithmetic the phone counts it with
//! after every tap ([`hilvan_core::cover`]).
//!
//! Nothing derived is stored. The reading of a text is kept in memory while
//! the server runs ([`Library`]): a text never changes once added and the
//! lexicon is built into the binary, so the process is the reading's
//! version. A book of a hundred thousand words is read in a fraction of a
//! second on a desktop and in most of one on a Pi - once, not on every list.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use hilvan_core::cover::{self, Coverage, Hold, Piece};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::import::{self, Chapter, Document, Format};
use crate::lexicon::{self, LEVELS};
use crate::pack::{self, word_id};
use crate::scheduling::Stitch;

/// One running word, where it stands and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Token {
    /// The character it starts at in its sentence.
    pub start: u32,
    /// How many characters it is.
    pub length: u32,
    #[serde(flatten)]
    pub piece: Piece,
}

/// A text read through a lexicon: the lemmas its words can be, and every
/// sentence's words as indices into them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Analysis {
    /// Word ids, `en:go`, in the order they were first met.
    pub lemmas: Vec<String>,
    /// The running words of each sentence given, in order.
    pub tokens: Vec<Vec<Token>>,
}

impl Analysis {
    /// Every running word, sentence after sentence.
    pub fn pieces(&self) -> impl Iterator<Item = &Piece> {
        self.tokens.iter().flatten().map(|token| &token.piece)
    }
}

/// A running word as the first pass of [`analyse`] reads it.
struct Seen<'a> {
    start: u32,
    length: u32,
    parts: Vec<Vec<&'a str>>,
    capital: bool,
    spelling: String,
}

/// Reads the sentences of a text into running words and the lemmas they can
/// be.
///
/// A name is not a word of the language and is marked so: a capitalised word
/// the lexicon does not have, and a capitalised word inside a sentence - not
/// opening it - that the lexicon has only as a rare lowercase word ("Harry",
/// "Rose" past the last level). Such a word is a name wherever else in the
/// text it is capitalised, the opening of a sentence included.
///
/// Without a lexicon for the language every word is past it: there is
/// nothing to read it with.
#[must_use]
pub fn analyse(language: &str, sentences: &[&str]) -> Analysis {
    let lexicon = lexicon::of(language);
    let mut lemmas: Vec<String> = Vec::new();
    let mut index: HashMap<String, u32> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let reach = LEVELS[LEVELS.len() - 1];

    // First pass: read every word, and find the names.
    let mut read: Vec<Vec<Seen>> = Vec::with_capacity(sentences.len());
    for sentence in sentences {
        let chars: Vec<char> = sentence.chars().collect();
        let mut words = Vec::new();
        for (position, token) in lexicon::tokens(sentence).into_iter().enumerate() {
            let length = token.text.chars().count();
            // A piece of a number - the "rd" of "3rd", the "s" of "1990s" -
            // is no word.
            let digit = |at: Option<&char>| at.is_some_and(char::is_ascii_digit);
            if digit(token.start.checked_sub(1).and_then(|at| chars.get(at))) || digit(chars.get(token.start + length)) {
                continue;
            }
            let parts = lexicon.map(|lexicon| lexicon.parts(token.text)).unwrap_or_default();
            let capital = token.text.chars().next().is_some_and(char::is_uppercase);
            let spelling = token.text.to_lowercase();
            if capital && position > 0 && parts.len() == 1 {
                let rare = parts[0].iter().all(|lemma| {
                    lemma.chars().next().is_some_and(char::is_lowercase)
                        && lexicon.and_then(|lexicon| lexicon.entry(lemma)).is_none_or(|entry| entry.rank > reach)
                });
                if rare {
                    names.insert(spelling.clone());
                }
            }
            let (Ok(start), Ok(length)) = (u32::try_from(token.start), u32::try_from(length)) else {
                continue;
            };
            words.push(Seen {
                start,
                length,
                parts,
                capital,
                spelling,
            });
        }
        read.push(words);
    }

    // Second pass: number the lemmas, and mark the names.
    let mut tokens = Vec::with_capacity(read.len());
    for words in read {
        let mut line = Vec::with_capacity(words.len());
        for Seen {
            start,
            length,
            parts,
            capital,
            spelling,
        } in words
        {
            let name = capital && (parts.is_empty() || names.contains(&spelling));
            let parts = if name {
                Vec::new()
            } else {
                parts
                    .into_iter()
                    .map(|part| {
                        part.into_iter()
                            .map(|lemma| {
                                let id = word_id(language, lemma);
                                *index.entry(id.clone()).or_insert_with(|| {
                                    lemmas.push(id);
                                    u32::try_from(lemmas.len() - 1).unwrap_or(u32::MAX)
                                })
                            })
                            .collect()
                    })
                    .collect()
            };
            line.push(Token {
                start,
                length,
                piece: Piece { parts, name },
            });
        }
        tokens.push(line);
    }
    Analysis { lemmas, tokens }
}

/// What the learner holds of each word that has a card: the cards of words
/// in hand, and of words in the deck - met in a sentence or taken from one,
/// and not started. A word with no card, or with a card nobody has met it
/// in, is not here.
///
/// # Errors
///
/// Fails when the database rejects the query or holds a state the product
/// does not know.
pub async fn holds(pool: &SqlitePool) -> Result<HashMap<String, Hold>> {
    let rows = sqlx::query(
        "SELECT c.subject_id, c.state, EXISTS (SELECT 1 FROM met m WHERE m.word_id = c.subject_id) AS met
         FROM card c WHERE c.kind = 'word'",
    )
    .fetch_all(pool)
    .await
    .context("failed to read what the learner holds of words")?;
    let mut holds = HashMap::with_capacity(rows.len());
    for row in rows {
        let hold = match Stitch::parse(row.get::<String, _>("state").as_str())? {
            Stitch::Sewn => Hold::Sewn,
            Stitch::Basted => Hold::Basted,
            Stitch::New if row.get::<bool, _>("met") => Hold::Deck,
            Stitch::New => continue,
        };
        holds.insert(row.get("subject_id"), hold);
    }
    Ok(holds)
}

/// What the learner holds of one word, as [`holds`] would say it.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn hold(pool: &SqlitePool, word: &str) -> Result<Option<Hold>> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT c.state, EXISTS (SELECT 1 FROM met m WHERE m.word_id = c.subject_id)
         FROM card c WHERE c.kind = 'word' AND c.subject_id = ?",
    )
    .bind(word)
    .fetch_optional(pool)
    .await
    .context("failed to read what the learner holds of a word")?;
    let Some((state, met)) = row else { return Ok(None) };
    Ok(match Stitch::parse(&state)? {
        Stitch::Sewn => Some(Hold::Sewn),
        Stitch::Basted => Some(Hold::Basted),
        Stitch::New => met.then_some(Hold::Deck),
    })
}

/// A running word of a [`Reading`]: where it stands, and which of the
/// text's distinct pieces it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spot {
    start: u32,
    length: u32,
    piece: u32,
}

/// A chapter of a [`Reading`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub title: Option<String>,
    /// The paragraph it starts at, through the whole text, from 0.
    pub paragraph: i64,
    /// How many paragraphs it has.
    pub paragraphs: i64,
    /// Its sentences, by position through the whole text.
    sentences: Range<usize>,
    /// Its running words as `(piece, times)`: what its coverage is counted
    /// from.
    tally: Vec<(u32, u32)>,
}

/// A text read through the lexicon, every chapter tallied: what the
/// [`Library`] keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// Every lemma its words can be, `en:go`.
    lemmas: Vec<String>,
    /// Its distinct running words, the parts indexing `lemmas`.
    pieces: Vec<Piece>,
    /// Each sentence's running words, by position through the text.
    sentences: Vec<Vec<Spot>>,
    pub chapters: Vec<Part>,
    /// How many paragraphs the whole text has.
    pub paragraphs: i64,
}

/// A text's sentences and chapters as the database keeps them: what a
/// [`Reading`] is made from.
struct Rows {
    language: String,
    /// `(sentence id, paragraph, text)` in order.
    sentences: Vec<(i64, i64, String)>,
    /// `(title, first paragraph)` in order.
    chapters: Vec<(Option<String>, i64)>,
}

impl Reading {
    /// Reads a text's rows.
    fn of(rows: &Rows) -> Self {
        let texts: Vec<&str> = rows.sentences.iter().map(|(_, _, text)| text.as_str()).collect();
        let analysis = analyse(&rows.language, &texts);
        let mut index: HashMap<&Piece, u32> = HashMap::new();
        let mut pieces: Vec<Piece> = Vec::new();
        let mut sentences: Vec<Vec<Spot>> = Vec::with_capacity(analysis.tokens.len());
        for line in &analysis.tokens {
            sentences.push(
                line.iter()
                    .map(|token| {
                        let piece = *index.entry(&token.piece).or_insert_with(|| {
                            pieces.push(token.piece.clone());
                            u32::try_from(pieces.len() - 1).unwrap_or(u32::MAX)
                        });
                        Spot {
                            start: token.start,
                            length: token.length,
                            piece,
                        }
                    })
                    .collect(),
            );
        }
        let paragraphs = rows.sentences.last().map_or(0, |(_, paragraph, _)| paragraph + 1);
        let first_sentence = |paragraph: i64| rows.sentences.iter().position(|(_, at, _)| *at >= paragraph).unwrap_or(rows.sentences.len());
        let mut chapters = Vec::with_capacity(rows.chapters.len());
        for (at, (title, paragraph)) in rows.chapters.iter().enumerate() {
            let next = rows.chapters.get(at + 1).map_or(paragraphs, |(_, next)| *next);
            let range = first_sentence(*paragraph)..first_sentence(next);
            let mut counts: HashMap<u32, u32> = HashMap::new();
            for spot in sentences[range.clone()].iter().flatten() {
                *counts.entry(spot.piece).or_default() += 1;
            }
            let mut tally: Vec<(u32, u32)> = counts.into_iter().collect();
            tally.sort_unstable();
            chapters.push(Part {
                title: title.clone(),
                paragraph: *paragraph,
                paragraphs: next - paragraph,
                sentences: range,
                tally,
            });
        }
        Self {
            lemmas: analysis.lemmas,
            pieces,
            sentences,
            chapters,
            paragraphs,
        }
    }

    /// What the learner holds of each lemma, by the reading's index.
    fn held(&self, holds: &HashMap<String, Hold>) -> Vec<Option<Hold>> {
        self.lemmas.iter().map(|id| holds.get(id).copied()).collect()
    }

    /// How much of one chapter the learner holds.
    fn chapter_coverage(&self, chapter: usize, held: &[Option<Hold>]) -> Coverage {
        self.chapters
            .get(chapter)
            .map(|part| cover::weigh(&self.pieces, &part.tally, held))
            .unwrap_or_default()
    }

    /// The chapter a paragraph is in.
    #[must_use]
    pub fn chapter_at(&self, paragraph: i64) -> usize {
        self.chapters.iter().rposition(|part| part.paragraph <= paragraph).unwrap_or(0)
    }

    /// Every lemma the running words of some sentences can be, and the
    /// sentences' words numbered by that list alone: what one chapter's page
    /// needs, without the rest of the book's ten thousand lemmas.
    fn lines(&self, sentences: Range<usize>) -> (Vec<String>, Vec<Vec<Token>>) {
        let mut local: HashMap<u32, u32> = HashMap::new();
        let mut lemmas: Vec<String> = Vec::new();
        let mut lines = Vec::with_capacity(sentences.len());
        for spots in &self.sentences[sentences] {
            let mut tokens = Vec::with_capacity(spots.len());
            for spot in spots {
                let piece = &self.pieces[spot.piece as usize];
                let parts = piece
                    .parts
                    .iter()
                    .map(|part| {
                        part.iter()
                            .map(|&lemma| {
                                *local.entry(lemma).or_insert_with(|| {
                                    lemmas.push(self.lemmas[lemma as usize].clone());
                                    u32::try_from(lemmas.len() - 1).unwrap_or(u32::MAX)
                                })
                            })
                            .collect()
                    })
                    .collect();
                tokens.push(Token {
                    start: spot.start,
                    length: spot.length,
                    piece: Piece { parts, name: piece.name },
                });
            }
            lines.push(tokens);
        }
        (lemmas, lines)
    }
}

/// The readings of the texts, kept while the server runs.
///
/// A text is keyed by its id, which is never given to another text: the
/// table counts its ids up and does not reuse them. A text never changes
/// once added, so a reading kept is a reading right until the text goes.
#[derive(Debug, Clone, Default)]
pub struct Library {
    kept: Arc<Mutex<HashMap<i64, Arc<Reading>>>>,
}

impl Library {
    /// The reading of a text: kept, or read now and kept. `None` for no such
    /// text.
    ///
    /// # Errors
    ///
    /// Fails when the database rejects a query.
    pub async fn reading(&self, pool: &SqlitePool, id: i64) -> Result<Option<Arc<Reading>>> {
        if let Some(kept) = self.kept.lock().unwrap_or_else(PoisonError::into_inner).get(&id) {
            return Ok(Some(Arc::clone(kept)));
        }
        let Some(rows) = rows(pool, id).await? else {
            return Ok(None);
        };
        // Reading a book is work for a processor, not for the runtime that
        // answers requests.
        let reading = Arc::new(
            tokio::task::spawn_blocking(move || Reading::of(&rows))
                .await
                .context("the reading of a text failed")?,
        );
        self.kept.lock().unwrap_or_else(PoisonError::into_inner).insert(id, Arc::clone(&reading));
        Ok(Some(reading))
    }

    /// Lets the reading of a text go: the text has gone.
    pub fn forget(&self, id: i64) {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner).remove(&id);
    }

    /// Reads every text not read yet, so the first list after a start does
    /// not wait for a library of books.
    ///
    /// # Errors
    ///
    /// Fails when the database rejects a query.
    pub async fn read_all(&self, pool: &SqlitePool) -> Result<usize> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM text ORDER BY opened_at IS NULL, opened_at DESC")
            .fetch_all(pool)
            .await
            .context("failed to list the texts")?;
        for &id in &ids {
            self.reading(pool, id).await?;
        }
        Ok(ids.len())
    }
}

/// The rows a reading is made of; `None` for no such text.
async fn rows(pool: &SqlitePool, id: i64) -> Result<Option<Rows>> {
    let Some(language) = sqlx::query_scalar::<_, String>("SELECT language FROM text WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a text")?
    else {
        return Ok(None);
    };
    let sentences = lines_of(pool, id).await?;
    let chapters = sqlx::query_as("SELECT title, paragraph FROM chapter WHERE text_id = ? ORDER BY position")
        .bind(id)
        .fetch_all(pool)
        .await
        .context("failed to read the chapters of a text")?;
    Ok(Some(Rows { language, sentences, chapters }))
}

/// A chapter as a list or the contents show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChapterSummary {
    /// Its place among the chapters, from 0.
    pub position: u32,
    pub title: Option<String>,
    /// The paragraph it starts at, through the whole text.
    pub paragraph: i64,
    pub paragraphs: i64,
    /// How much of it the learner holds, now.
    pub coverage: Coverage,
}

/// A text as the list shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    /// The link it came from; `None` for a text pasted or opened from a file.
    pub source: Option<String>,
    pub language: String,
    pub format: Format,
    pub added_at: DateTime<Utc>,
    /// When it was last opened; `None` for a text never opened.
    pub opened_at: Option<DateTime<Utc>>,
    /// The paragraph the learner stopped at, through the whole text, from 0.
    pub paragraph: i64,
    /// How many paragraphs it has.
    pub paragraphs: i64,
    /// When the learner said they had read it.
    pub finished_at: Option<DateTime<Utc>>,
    /// How much of it the learner holds, now.
    pub coverage: Coverage,
    /// How many chapters it has: 1 for anything but a book.
    pub chapters: u32,
    /// The chapter the learner stopped in.
    pub chapter: ChapterSummary,
}

/// One sentence of a text, read.
#[derive(Debug, Clone, Serialize)]
pub struct Line {
    /// The sentence's id: what a word is taken from it under.
    pub sentence: i64,
    pub text: String,
    pub tokens: Vec<Token>,
}

/// A chapter of a text as the reader shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Text {
    #[serde(flatten)]
    pub summary: Summary,
    /// Every chapter, with how much of each is the learner's.
    pub contents: Vec<ChapterSummary>,
    /// The chapter shown: its position among `contents`.
    pub shown: u32,
    /// The shown chapter's paragraphs, each the sentences in it.
    pub lines: Vec<Vec<Line>>,
    /// Every lemma the shown chapter's words can be, `en:go`: what the
    /// tokens' parts index.
    pub lemmas: Vec<String>,
    /// What the learner holds of each, by the same index.
    pub holds: Vec<Option<Hold>>,
}

/// A text the learner pastes.
#[derive(Debug, Clone, Deserialize)]
pub struct Addition {
    /// Its title; taken from the text when it carries one, or from its first
    /// sentence, when not given.
    #[serde(default)]
    pub title: Option<String>,
    pub body: String,
    /// How it is written; guessed from the text when not said.
    #[serde(default)]
    pub format: Option<Format>,
}

/// A text taken apart, ready to keep.
#[derive(Debug, Clone)]
pub struct Arrival {
    pub format: Format,
    pub document: Document,
    /// The link it came from.
    pub source: Option<String>,
    /// A title that wins over what the document says.
    pub title: Option<String>,
    /// An author that wins over what the document says.
    pub author: Option<String>,
}

/// Why a text was not taken or changed: the client's to fix.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("load a pack first: the reader reads the language a pack teaches")]
    NoLanguage,
    #[error("the text holds no sentence to read")]
    Empty,
    #[error("there is no text {0}")]
    NotFound(i64),
    #[error("text {0} has no chapter {1}")]
    NoChapter(i64, u32),
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

/// How long a title made from a first sentence may be, in characters.
const TITLE_CHARS: usize = 60;

/// A title from a first sentence: whole when short, cut at a word and
/// ended with an ellipsis when not.
fn title_from(sentence: &str) -> String {
    if sentence.chars().count() <= TITLE_CHARS {
        return sentence.to_string();
    }
    let cut: String = sentence.chars().take(TITLE_CHARS).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}

/// Keeps a pasted text.
///
/// # Errors
///
/// As [`keep`].
pub async fn add(pool: &SqlitePool, library: &Library, addition: &Addition, now: DateTime<Utc>) -> Result<Summary, Refused> {
    let language = pack::language_being_learnt(pool).await?.ok_or(Refused::NoLanguage)?;
    let format = addition.format.unwrap_or_else(|| Format::guess(&addition.body));
    let document = import::written(format, &addition.body, lexicon::of(&language));
    let arrival = Arrival {
        format,
        document,
        source: None,
        title: addition.title.clone(),
        author: None,
    };
    keep(pool, library, arrival, now).await
}

/// The language being learnt: what every text is read in.
///
/// # Errors
///
/// [`Refused::NoLanguage`] before a pack is loaded.
pub async fn language(pool: &SqlitePool) -> Result<String, Refused> {
    pack::language_being_learnt(pool).await?.ok_or(Refused::NoLanguage)
}

/// The columns of a text's row, as it is written.
pub(crate) struct Written<'a> {
    pub language: &'a str,
    pub title: &'a str,
    pub author: Option<&'a str>,
    pub source: Option<&'a str>,
    pub format: Format,
    pub added_at: DateTime<Utc>,
    pub opened_at: Option<DateTime<Utc>>,
    pub paragraph: i64,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Writes a text: its row, its chapters, and its sentences, each kept once
/// among the sentences of its language. The one way a text is written - by
/// the reader and by the import of a learner model alike.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub(crate) async fn write(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, text: &Written<'_>, chapters: &[Chapter]) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO text (language, title, author, source, format, added_at, opened_at, paragraph, finished_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(text.language)
    .bind(text.title)
    .bind(text.author)
    .bind(text.source)
    .bind(text.format.as_str())
    .bind(text.added_at.to_rfc3339())
    .bind(text.opened_at.map(|at| at.to_rfc3339()))
    .bind(text.paragraph)
    .bind(text.finished_at.map(|at| at.to_rfc3339()))
    .fetch_one(&mut **tx)
    .await
    .with_context(|| format!("failed to keep the text {:?}", text.title))?;
    let mut position = 0i64;
    let mut paragraph = 0i64;
    for (number, chapter) in chapters.iter().enumerate() {
        sqlx::query("INSERT INTO chapter (text_id, position, title, paragraph) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(i64::try_from(number).unwrap_or(i64::MAX))
            .bind(chapter.title.as_deref())
            .bind(paragraph)
            .execute(&mut **tx)
            .await
            .context("failed to keep a chapter of a text")?;
        for sentences in &chapter.paragraphs {
            for sentence in sentences {
                let sentence_id = keep_sentence(tx, text.language, sentence).await?;
                sqlx::query("INSERT INTO text_sentence (text_id, position, paragraph, sentence_id) VALUES (?, ?, ?, ?)")
                    .bind(id)
                    .bind(position)
                    .bind(paragraph)
                    .bind(sentence_id)
                    .execute(&mut **tx)
                    .await
                    .context("failed to keep a sentence of a text")?;
                position += 1;
            }
            paragraph += 1;
        }
    }
    Ok(id)
}

/// Keeps a text taken apart: pasted, from a file, by a link.
///
/// # Errors
///
/// [`Refused::NoLanguage`] before a pack is loaded, [`Refused::Empty`] for a
/// text with no sentence in it, and [`Refused::Database`] when the database
/// rejects a statement.
pub async fn keep(pool: &SqlitePool, library: &Library, arrival: Arrival, now: DateTime<Utc>) -> Result<Summary, Refused> {
    let language = language(pool).await?;
    let first = arrival.document.first_sentence().ok_or(Refused::Empty)?;
    let given = |text: Option<&String>| text.map(|text| import::squeeze(text)).filter(|text| !text.is_empty());
    let title = given(arrival.title.as_ref())
        .or_else(|| given(arrival.document.title.as_ref()))
        .unwrap_or_else(|| title_from(first));
    let author = given(arrival.author.as_ref()).or_else(|| given(arrival.document.author.as_ref()));

    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let id = write(
        &mut tx,
        &Written {
            language: &language,
            title: &title,
            author: author.as_deref(),
            source: arrival.source.as_deref(),
            format: arrival.format,
            added_at: now,
            opened_at: None,
            paragraph: 0,
            finished_at: None,
        },
        &arrival.document.chapters,
    )
    .await?;
    tx.commit().await.context("failed to commit a text")?;

    let holds = holds(pool).await?;
    summary(pool, library, id, &holds).await?.ok_or(Refused::NotFound(id))
}

/// The id of a sentence, kept once by what it says.
///
/// # Errors
///
/// Fails when the database rejects the statement.
pub async fn keep_sentence(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, language: &str, text: &str) -> Result<i64> {
    // An update that changes nothing, so the existing row hands its id back
    // as a new one would.
    sqlx::query_scalar(
        "INSERT INTO sentence (language, text) VALUES (?, ?)
         ON CONFLICT (language, text) DO UPDATE SET text = excluded.text RETURNING id",
    )
    .bind(language)
    .bind(text)
    .fetch_one(&mut **tx)
    .await
    .context("failed to keep a sentence")
}

/// Drops the sentences nothing shows any more: no formula's example, no
/// text's line, and not the sentence a word was taken from. Their anchors go
/// with them.
///
/// # Errors
///
/// Fails when the database rejects the statement.
pub async fn drop_unshown_sentences(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    sqlx::query(
        "DELETE FROM sentence
         WHERE id NOT IN (SELECT sentence_id FROM formula_sentence)
           AND id NOT IN (SELECT sentence_id FROM text_sentence)
           AND id NOT IN (SELECT sentence_id FROM pick)",
    )
    .execute(&mut **tx)
    .await
    .context("failed to drop the sentences nothing shows any more")?;
    Ok(())
}

/// The sentences of a text in order: id, paragraph and what it says.
async fn lines_of(pool: &SqlitePool, id: i64) -> Result<Vec<(i64, i64, String)>> {
    sqlx::query_as(
        "SELECT s.id, ts.paragraph, s.text FROM text_sentence ts JOIN sentence s ON s.id = ts.sentence_id
         WHERE ts.text_id = ? ORDER BY ts.position",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .context("failed to read the sentences of a text")
}

fn moment(text: Option<String>) -> Result<Option<DateTime<Utc>>> {
    text.map(|text| {
        DateTime::parse_from_rfc3339(&text)
            .map(|at| at.with_timezone(&Utc))
            .with_context(|| format!("{text:?} is not a moment"))
    })
    .transpose()
}

/// Every chapter of a reading, counted for the learner.
fn contents(reading: &Reading, held: &[Option<Hold>]) -> Vec<ChapterSummary> {
    reading
        .chapters
        .iter()
        .enumerate()
        .map(|(at, part)| ChapterSummary {
            position: u32::try_from(at).unwrap_or(u32::MAX),
            title: part.title.clone(),
            paragraph: part.paragraph,
            paragraphs: part.paragraphs,
            coverage: reading.chapter_coverage(at, held),
        })
        .collect()
}

/// One text's summary, and the reading and every chapter it was counted
/// from.
async fn summary_and_contents(
    pool: &SqlitePool,
    library: &Library,
    id: i64,
    holds: &HashMap<String, Hold>,
) -> Result<Option<(Summary, Arc<Reading>, Vec<ChapterSummary>, Vec<Option<Hold>>)>> {
    let Some(row) = sqlx::query("SELECT id, language, title, author, source, format, added_at, opened_at, paragraph, finished_at FROM text WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a text")?
    else {
        return Ok(None);
    };
    let Some(reading) = library.reading(pool, id).await? else {
        return Ok(None);
    };
    let held = reading.held(holds);
    let contents = contents(&reading, &held);
    let mut coverage = Coverage::default();
    for chapter in &contents {
        coverage.merge(&chapter.coverage);
    }
    let paragraph = row.get::<i64, _>("paragraph").min((reading.paragraphs - 1).max(0));
    let chapter = contents.get(reading.chapter_at(paragraph)).cloned().context("a text without a chapter")?;
    let summary = Summary {
        id: row.get("id"),
        title: row.get("title"),
        author: row.get("author"),
        source: row.get("source"),
        format: Format::parse(row.get::<String, _>("format").as_str())?,
        added_at: moment(Some(row.get("added_at")))?.context("a text without the moment it was added")?,
        opened_at: moment(row.get("opened_at"))?,
        paragraph,
        paragraphs: reading.paragraphs,
        finished_at: moment(row.get("finished_at"))?,
        language: row.get("language"),
        coverage,
        chapters: u32::try_from(contents.len()).unwrap_or(u32::MAX),
        chapter,
    };
    Ok(Some((summary, reading, contents, held)))
}

async fn summary(pool: &SqlitePool, library: &Library, id: i64, holds: &HashMap<String, Hold>) -> Result<Option<Summary>> {
    Ok(summary_and_contents(pool, library, id, holds).await?.map(|(summary, ..)| summary))
}

/// Every text, the one opened last first and those never opened after them,
/// newest first.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn list(pool: &SqlitePool, library: &Library) -> Result<Vec<Summary>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM text ORDER BY opened_at IS NULL, opened_at DESC, added_at DESC, id DESC")
        .fetch_all(pool)
        .await
        .context("failed to list the texts")?;
    let holds = holds(pool).await?;
    let mut texts = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(summary) = summary(pool, library, id, &holds).await? {
            texts.push(summary);
        }
    }
    Ok(texts)
}

/// One chapter of a text, every word read: the chapter asked for, or the
/// one the learner stopped in.
///
/// # Errors
///
/// [`Refused::NotFound`] for no such text, [`Refused::NoChapter`] for a
/// chapter past its last, [`Refused::Database`] when the database rejects a
/// query.
pub async fn read(pool: &SqlitePool, library: &Library, id: i64, chapter: Option<u32>) -> Result<Text, Refused> {
    let holds = holds(pool).await?;
    let (summary, reading, contents, _) = summary_and_contents(pool, library, id, &holds).await?.ok_or(Refused::NotFound(id))?;
    let shown = chapter.unwrap_or(summary.chapter.position);
    let part = reading.chapters.get(shown as usize).ok_or(Refused::NoChapter(id, shown))?;
    let (lemmas, tokens) = reading.lines(part.sentences.clone());
    let texts = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT s.id, ts.paragraph, s.text FROM text_sentence ts JOIN sentence s ON s.id = ts.sentence_id
         WHERE ts.text_id = ? AND ts.position >= ? AND ts.position < ? ORDER BY ts.position",
    )
    .bind(id)
    .bind(i64::try_from(part.sentences.start).unwrap_or(i64::MAX))
    .bind(i64::try_from(part.sentences.end).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await
    .context("failed to read the sentences of a chapter")?;
    let mut paragraphs: Vec<Vec<Line>> = Vec::new();
    for ((sentence, paragraph, text), tokens) in texts.into_iter().zip(tokens) {
        let paragraph = usize::try_from(paragraph - part.paragraph).unwrap_or(0);
        if paragraphs.len() <= paragraph {
            paragraphs.resize_with(paragraph + 1, Vec::new);
        }
        paragraphs[paragraph].push(Line { sentence, text, tokens });
    }
    let held = lemmas.iter().map(|id| holds.get(id).copied()).collect();
    Ok(Text {
        summary,
        contents,
        shown,
        lines: paragraphs,
        lemmas,
        holds: held,
    })
}

/// Where the learner is in a text, as the reader reports it.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Place {
    /// The paragraph being read, through the whole text, from 0.
    #[serde(default)]
    pub paragraph: Option<i64>,
    /// Whether the learner has read it all.
    #[serde(default)]
    pub finished: Option<bool>,
}

/// Records where the learner is in a text, and that it is open now.
///
/// A paragraph past the end is the last one. Finished is said by the learner
/// and taken back by them: reaching the bottom of a text to see how long it
/// is does not finish it.
///
/// # Errors
///
/// [`Refused::NotFound`] for no such text, [`Refused::Database`] when the
/// database rejects a statement.
pub async fn place(pool: &SqlitePool, library: &Library, id: i64, place: Place, now: DateTime<Utc>) -> Result<Summary, Refused> {
    let updated = sqlx::query(
        "UPDATE text SET opened_at = ?1,
             paragraph = COALESCE(max(0, ?2), paragraph),
             finished_at = CASE WHEN ?3 IS NULL THEN finished_at WHEN ?3 THEN COALESCE(finished_at, ?1) ELSE NULL END
         WHERE id = ?4",
    )
    .bind(now.to_rfc3339())
    .bind(place.paragraph)
    .bind(place.finished)
    .bind(id)
    .execute(pool)
    .await
    .context("failed to record where the learner is in a text")?;
    if updated.rows_affected() == 0 {
        return Err(Refused::NotFound(id));
    }
    let holds = holds(pool).await?;
    summary(pool, library, id, &holds).await?.ok_or(Refused::NotFound(id))
}

/// Lets a text go. The words taken from it stay in the deck with the
/// sentences they were taken from.
///
/// # Errors
///
/// [`Refused::NotFound`] for no such text, [`Refused::Database`] when the
/// database rejects a statement.
pub async fn remove(pool: &SqlitePool, library: &Library, id: i64) -> Result<(), Refused> {
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let removed = sqlx::query("DELETE FROM text WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("failed to let a text go")?;
    if removed.rows_affected() == 0 {
        return Err(Refused::NotFound(id));
    }
    drop_unshown_sentences(&mut tx).await?;
    tx.commit().await.context("failed to commit letting a text go")?;
    library.forget(id);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn a_long_first_sentence_is_cut_into_a_title_at_a_word() {
        assert_eq!(title_from("Short one."), "Short one.");
        let long = title_from("This is a very long first sentence that goes on and on, well past the sixty characters a title can hold.");
        assert!(long.ends_with('…') && long.chars().count() <= TITLE_CHARS + 1, "{long}");
        assert!(!long.contains(" …"), "{long}");
    }

    #[test]
    fn running_words_are_read_into_lemmas_and_names_are_kept_out() {
        let analysis = analyse("en", &["Harry went to London.", "Harry didn't go.", "Rose bought a rose."]);
        let words = |sentence: usize| -> Vec<(u32, Vec<Vec<&str>>, bool)> {
            analysis.tokens[sentence]
                .iter()
                .map(|token| {
                    let parts = token
                        .piece
                        .parts
                        .iter()
                        .map(|part| part.iter().map(|&index| analysis.lemmas[index as usize].as_str()).collect())
                        .collect();
                    (token.start, parts, token.piece.name)
                })
                .collect()
        };
        let first = words(0);
        assert!(first[0].2, "Harry is a name the lexicon has only as a rare word: {first:?}");
        assert_eq!(first[1], (6, vec![vec!["en:go"]], false));
        assert!(first[3].2, "London is not in the lexicon, and capitalised");
        let second = words(1);
        assert!(second[0].2, "a name opening a sentence is a name once it was one inside a sentence");
        assert_eq!(second[1].1, vec![vec!["en:do"], vec!["en:not"]]);
        let third = words(2);
        assert!(!third[0].2, "Rose opening a sentence, and common: a word, not a name");
        assert!(!third[3].2);
    }

    #[test]
    fn a_rare_word_capitalised_inside_a_sentence_is_a_name_all_through_the_text() {
        // The lexicon has "john" - a rare word, past the last level - so it
        // is no spelling past the lexicon: only the capital inside a
        // sentence says it is a name, and that holds where it opens one.
        let analysis = analyse("en", &["Then John came home.", "John was late."]);
        let names: Vec<bool> = analysis.tokens.iter().map(|line| line.iter().any(|token| token.piece.name)).collect();
        assert_eq!(names, vec![true, true], "{analysis:?}");
        assert!(!analysis.lemmas.contains(&"en:john".to_string()), "a name brings no lemma to count");
        // A common word capitalised inside a sentence is still the word.
        let common = analyse("en", &["We saw Rose."]);
        assert!(!common.tokens[0][2].piece.name, "rose is among the levels");
    }

    #[test]
    fn a_piece_of_a_number_is_no_word() {
        let analysis = analyse("en", &["The 3rd time in the 1990s."]);
        let spelled: Vec<u32> = analysis.tokens[0].iter().map(|token| token.start).collect();
        assert_eq!(spelled, vec![0, 8, 13, 16], "the, time, in, the - no rd, no s");
    }

    #[test]
    fn without_a_lexicon_every_word_is_past_it() {
        let analysis = analyse("xx", &["una casa"]);
        assert_eq!(analysis.lemmas, Vec::<String>::new());
        assert!(analysis.pieces().all(|piece| piece.parts.is_empty() && !piece.name));
    }

    /// A pack with one formula whose sample teaches "doctor", and a glossary.
    pub(crate) const PACK: &str = r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[word]]
lemma = "doctor"
gloss = "врач"

[[formula]]
id = "be"
name = "n"
pattern = "x"
explanation = "e"
order = 1

  [[formula.sample]]
  native = "Он врач."
  target = "He is a doctor."
  words = ["doctor"]
"#;

    pub(crate) fn pack(version: i64) -> crate::pack::Pack {
        let mut pack: crate::pack::Pack = toml::from_str(PACK).unwrap();
        pack.version = version;
        pack.glossary = Some("glossary.tsv".to_string());
        pack.glosses = [("house", "дом"), ("doctor", "доктор"), ("go", "идти"), ("suddenly", "вдруг")]
            .map(|(lemma, gloss)| (lemma.to_string(), gloss.to_string()))
            .to_vec();
        pack
    }

    pub(crate) async fn tutor() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        crate::pack::load(&pool, &pack(1)).await.unwrap();
        pool
    }

    pub(crate) fn at(minute: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-20T18:00:00Z").unwrap().with_timezone(&Utc) + chrono::Duration::minutes(minute)
    }

    pub(crate) fn plain(title: &str, body: &str) -> Addition {
        Addition {
            title: Some(title.to_string()),
            body: body.to_string(),
            format: Some(Format::Plain),
        }
    }

    /// A book of three chapters, the last two in one paragraph each; "Harry"
    /// is a name only the first chapter shows to be one.
    pub(crate) fn book() -> Arrival {
        let chapter = |title: &str, paragraphs: &[&[&str]]| Chapter {
            title: Some(title.to_string()),
            paragraphs: paragraphs
                .iter()
                .map(|sentences| sentences.iter().map(|sentence| (*sentence).to_string()).collect())
                .collect(),
        };
        Arrival {
            format: Format::Epub,
            document: Document {
                title: Some("The Book".to_string()),
                author: Some("Ann Writer".to_string()),
                chapters: vec![
                    chapter("One", &[&["Then Harry went to the house."], &["He is a doctor."]]),
                    chapter("Two", &[&["Harry went home.", "It was late."]]),
                    chapter("Three", &[&["Suddenly it rained."]]),
                ],
            },
            source: Some("https://example.org/book".to_string()),
            title: None,
            author: None,
        }
    }

    #[tokio::test]
    async fn a_text_is_kept_as_its_sentences_and_shares_them_with_the_pack() {
        let pool = tutor().await;
        let library = Library::default();
        let added = add(
            &pool,
            &library,
            &plain("Visit", "She went to the house.\n\nHe is a doctor. It was late."),
            at(0),
        )
        .await
        .unwrap();
        assert_eq!(
            (added.title.as_str(), added.paragraphs, added.format, added.chapters),
            ("Visit", 2, Format::Plain, 1)
        );

        let text = read(&pool, &library, added.id, None).await.unwrap();
        let said: Vec<Vec<&str>> = text.lines.iter().map(|lines| lines.iter().map(|line| line.text.as_str()).collect()).collect();
        assert_eq!(said, vec![vec!["She went to the house."], vec!["He is a doctor.", "It was late."]]);
        let shared: i64 = sqlx::query_scalar("SELECT count(*) FROM sentence WHERE text = 'He is a doctor.'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(shared, 1, "the pack's sentence and the text's are one sentence");
        assert_eq!(text.lemmas.len(), text.holds.len());
    }

    #[tokio::test]
    async fn the_coverage_moves_with_what_the_learner_holds() {
        let pool = tutor().await;
        let library = Library::default();
        let added = add(&pool, &library, &plain("t", "She went to the house, Harry."), at(0)).await.unwrap();
        assert_eq!(
            added.coverage,
            Coverage {
                words: 5,
                unknown: 5,
                names: 1,
                ..Coverage::default()
            }
        );

        crate::words::declare(&pool, &["en:she".to_string(), "en:go".to_string()], at(1)).await.unwrap();
        let sentence = read(&pool, &library, added.id, None).await.unwrap().lines[0][0].sentence;
        crate::words::pick(&pool, "en:house", sentence, 16, at(2)).await.unwrap();

        let text = read(&pool, &library, added.id, None).await.unwrap();
        assert_eq!(
            (text.summary.coverage.sewn, text.summary.coverage.deck, text.summary.coverage.unknown),
            (2, 1, 2)
        );
        assert_eq!(text.summary.coverage.yours(), 2, "a word put in the deck is not in hand yet");
        let go = text.lemmas.iter().position(|id| id == "en:go").unwrap();
        assert_eq!(text.holds[go], Some(Hold::Sewn));
        assert_eq!(
            list(&pool, &library).await.unwrap()[0].coverage,
            text.summary.coverage,
            "the list counts as the reader does"
        );
    }

    #[tokio::test]
    async fn a_book_is_kept_in_chapters_and_each_is_counted_and_read_alone() {
        let pool = tutor().await;
        let library = Library::default();
        let added = keep(&pool, &library, book(), at(0)).await.unwrap();
        assert_eq!(
            (
                added.title.as_str(),
                added.author.as_deref(),
                added.source.as_deref(),
                added.chapters,
                added.paragraphs
            ),
            ("The Book", Some("Ann Writer"), Some("https://example.org/book"), 3, 4)
        );
        crate::words::declare(&pool, &["en:go".to_string(), "en:home".to_string()], at(1))
            .await
            .unwrap();

        let text = read(&pool, &library, added.id, Some(1)).await.unwrap();
        assert_eq!(text.shown, 1);
        assert_eq!(text.contents.iter().map(|chapter| chapter.paragraph).collect::<Vec<_>>(), vec![0, 2, 3]);
        assert_eq!(text.contents.iter().map(|chapter| chapter.paragraphs).collect::<Vec<_>>(), vec![2, 1, 1]);
        let said: Vec<Vec<&str>> = text.lines.iter().map(|lines| lines.iter().map(|line| line.text.as_str()).collect()).collect();
        assert_eq!(
            said,
            vec![vec!["Harry went home.", "It was late."]],
            "a chapter's paragraphs from its own first"
        );
        // "Harry" opens the sentence here; the first chapter shows it is a
        // name, and the reading is of the whole book.
        assert!(text.lines[0][0].tokens[0].piece.name, "a name is a name through the whole book");
        let two = &text.contents[1].coverage;
        assert_eq!((two.words, two.names, two.sewn), (5, 1, 2), "went and home are held");
        // Every lemma the shown chapter's words can be, and no other.
        let mut lemmas = text.lemmas.clone();
        lemmas.sort();
        assert_eq!(lemmas, vec!["en:be", "en:go", "en:home", "en:it", "en:late"]);
        assert!(
            text.lines
                .iter()
                .flatten()
                .flat_map(|line| &line.tokens)
                .flat_map(|token| token.piece.parts.iter().flatten())
                .all(|&index| (index as usize) < text.lemmas.len())
        );

        let mut whole = Coverage::default();
        for chapter in &text.contents {
            whole.merge(&chapter.coverage);
        }
        assert_eq!(text.summary.coverage, whole, "the book is its chapters");
        assert!(matches!(read(&pool, &library, added.id, Some(3)).await, Err(Refused::NoChapter(_, 3))));
    }

    #[tokio::test]
    async fn every_running_word_counts_however_often_it_repeats() {
        let pool = tutor().await;
        let library = Library::default();
        let added = add(&pool, &library, &plain("t", "It was late. It was late, so late."), at(0)).await.unwrap();
        assert_eq!(added.coverage.words, 8, "a word met three times is three running words");
        crate::words::declare(&pool, &["en:late".to_string()], at(1)).await.unwrap();
        let text = read(&pool, &library, added.id, None).await.unwrap();
        assert_eq!((text.summary.coverage.sewn, text.contents[0].coverage.sewn), (3, 3));
    }

    #[tokio::test]
    async fn the_place_names_the_chapter_it_is_in_and_the_reader_opens_there() {
        let pool = tutor().await;
        let library = Library::default();
        let added = keep(&pool, &library, book(), at(0)).await.unwrap();
        assert_eq!(added.chapter.position, 0);
        let placed = place(
            &pool,
            &library,
            added.id,
            Place {
                paragraph: Some(3),
                finished: None,
            },
            at(1),
        )
        .await
        .unwrap();
        assert_eq!((placed.chapter.position, placed.chapter.title.as_deref()), (2, Some("Three")));
        let text = read(&pool, &library, added.id, None).await.unwrap();
        assert_eq!(text.shown, 2, "the reader opens on the chapter the learner stopped in");
        assert_eq!(text.lines[0][0].text, "Suddenly it rained.");
    }

    #[tokio::test]
    async fn a_reload_of_the_pack_keeps_the_sentences_of_texts() {
        let pool = tutor().await;
        let library = Library::default();
        let added = add(&pool, &library, &plain("t", "It was late."), at(0)).await.unwrap();
        crate::pack::load(&pool, &pack(2)).await.unwrap();
        assert_eq!(read(&pool, &Library::default(), added.id, None).await.unwrap().lines[0][0].text, "It was late.");
    }

    #[tokio::test]
    async fn letting_a_text_go_keeps_the_sentence_a_word_was_taken_from() {
        let pool = tutor().await;
        let library = Library::default();
        let added = add(&pool, &library, &plain("t", "Suddenly it rained. It was late."), at(0)).await.unwrap();
        let sentence = read(&pool, &library, added.id, None).await.unwrap().lines[0][0].sentence;
        crate::words::pick(&pool, "en:suddenly", sentence, 0, at(1)).await.unwrap();

        remove(&pool, &library, added.id).await.unwrap();
        crate::pack::load(&pool, &pack(2)).await.unwrap();
        let kept: Vec<String> = sqlx::query_scalar("SELECT text FROM sentence ORDER BY id").fetch_all(&pool).await.unwrap();
        assert!(kept.contains(&"Suddenly it rained.".to_string()), "the word's sentence stays: {kept:?}");
        assert!(!kept.contains(&"It was late.".to_string()), "a sentence nothing shows goes: {kept:?}");
        let word = crate::words::word(&pool, "en:suddenly").await.unwrap();
        assert_eq!(word.contexts[0].text, "Suddenly it rained.");
        assert_eq!(word.contexts[0].source, crate::words::Source::Text { id: None, title: None });
        assert!(matches!(read(&pool, &library, added.id, None).await, Err(Refused::NotFound(_))));
    }

    #[tokio::test]
    async fn a_text_let_go_never_lends_its_number_to_the_next() {
        let pool = tutor().await;
        let library = Library::default();
        let first = add(&pool, &library, &plain("first", "It was late."), at(0)).await.unwrap();
        // Kept while it stands; the reading has to go with it.
        library.reading(&pool, first.id).await.unwrap();
        remove(&pool, &library, first.id).await.unwrap();
        let second = add(&pool, &library, &plain("second", "Suddenly it rained."), at(1)).await.unwrap();
        assert!(second.id > first.id, "the table counts on past a text let go");
        assert_eq!(read(&pool, &library, second.id, None).await.unwrap().lines[0][0].text, "Suddenly it rained.");
    }

    #[tokio::test]
    async fn the_migration_to_chapters_keeps_every_text_and_its_sentences_where_they_were() {
        // A tutor as v0.9 left it: two texts, a word taken from one, the
        // second text the last one added.
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let migrator = sqlx::migrate!();
        let (before, books): (Vec<_>, Vec<_>) = migrator.iter().partition(|migration| migration.version < 12);
        for migration in before {
            sqlx::raw_sql(migration.sql.clone()).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql(
            "INSERT INTO language (code, name, is_native) VALUES ('en', 'en', 0);
             INSERT INTO sentence (id, language, text) VALUES (1, 'en', 'It was late.'), (2, 'en', 'Suddenly it rained.');
             INSERT INTO text (id, language, title, format, added_at, opened_at, paragraph, finished_at)
             VALUES (4, 'en', 'First', 'plain', '2026-10-20T18:00:00+00:00', '2026-10-20T18:05:00+00:00', 1, NULL),
                    (7, 'en', 'Second', 'subtitles', '2026-10-21T18:00:00+00:00', NULL, 0, NULL);
             INSERT INTO text_sentence (text_id, position, paragraph, sentence_id) VALUES (4, 0, 0, 1), (4, 1, 1, 2), (7, 0, 0, 2);
             INSERT INTO pick (word_id, sentence_id, form, start, picked_at) VALUES ('en:suddenly', 2, 'Suddenly', 0, '2026-10-20T18:06:00+00:00');",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        for migration in books {
            sqlx::raw_sql(migration.sql.clone()).execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();

        let texts: Vec<(i64, String, i64, Option<String>)> = sqlx::query_as("SELECT id, title, paragraph, author FROM text ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(texts, vec![(4, "First".to_string(), 1, None), (7, "Second".to_string(), 0, None)]);
        let sentences: Vec<(i64, i64, i64)> = sqlx::query_as("SELECT text_id, paragraph, sentence_id FROM text_sentence ORDER BY text_id, position")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(sentences, vec![(4, 0, 1), (4, 1, 2), (7, 0, 2)], "the sentences survive the table under them");
        let chapters: Vec<(i64, i64, Option<String>, i64)> = sqlx::query_as("SELECT text_id, position, title, paragraph FROM chapter ORDER BY text_id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(chapters, vec![(4, 0, None, 0), (7, 0, None, 0)], "every text is one chapter");

        // The last text let go: its number is not given again.
        sqlx::query("DELETE FROM text WHERE id = 7").execute(&pool).await.unwrap();
        let next: i64 =
            sqlx::query_scalar("INSERT INTO text (language, title, format, added_at) VALUES ('en', 'Third', 'epub', '2026-11-01T00:00:00+00:00') RETURNING id")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(next, 8);
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM text_sentence WHERE text_id = 7")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0, "a text let go takes its sentences with it, as before");
        let bad = sqlx::query("INSERT INTO chapter (text_id, position, title, paragraph) VALUES (4, 0, NULL, 3)")
            .execute(&pool)
            .await;
        assert!(bad.is_err(), "the first chapter starts at the first paragraph");
    }

    #[tokio::test]
    async fn the_place_is_kept_and_finishing_is_the_learners_to_say() {
        let pool = tutor().await;
        let library = Library::default();
        let first = add(&pool, &library, &plain("first", "One.\n\nTwo."), at(0)).await.unwrap();
        let second = add(&pool, &library, &plain("second", "Three."), at(1)).await.unwrap();
        assert_eq!(list(&pool, &library).await.unwrap()[0].id, second.id, "never opened: the newest first");

        let to = |paragraph, finished| Place { paragraph, finished };
        let placed = place(&pool, &library, first.id, to(Some(9), None), at(2)).await.unwrap();
        assert_eq!(placed.paragraph, 1, "a paragraph past the end is the last");
        assert_eq!(placed.opened_at, Some(at(2)));
        assert_eq!(list(&pool, &library).await.unwrap()[0].id, first.id, "the one opened last first");

        let finished = place(&pool, &library, first.id, to(None, Some(true)), at(3)).await.unwrap();
        assert_eq!((finished.paragraph, finished.finished_at), (1, Some(at(3))));
        let again = place(&pool, &library, first.id, to(Some(0), Some(true)), at(4)).await.unwrap();
        assert_eq!(again.finished_at, Some(at(3)), "finished once is finished when it was");
        let unfinished = place(&pool, &library, first.id, to(None, Some(false)), at(5)).await.unwrap();
        assert_eq!(unfinished.finished_at, None);
        assert!(matches!(place(&pool, &library, 99, Place::default(), at(6)).await, Err(Refused::NotFound(99))));
    }

    #[tokio::test]
    async fn an_empty_text_and_a_tutor_without_a_language_are_refused() {
        let pool = tutor().await;
        let library = Library::default();
        assert!(matches!(add(&pool, &library, &plain("t", " 42 \n\n -- "), at(0)).await, Err(Refused::Empty)));
        let bare = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&bare).await.unwrap();
        assert!(matches!(add(&bare, &library, &plain("t", "Hello."), at(0)).await, Err(Refused::NoLanguage)));
    }

    #[tokio::test]
    async fn a_text_without_a_title_is_named_by_what_it_carries() {
        let pool = tutor().await;
        let library = Library::default();
        let markdown = Addition {
            title: None,
            body: "# A day out\n\nWe went home.".to_string(),
            format: None,
        };
        assert_eq!(add(&pool, &library, &markdown, at(0)).await.unwrap().title, "A day out");
        let pasted = Addition {
            title: Some("  ".to_string()),
            body: "We went home. Then we slept.".to_string(),
            format: None,
        };
        let pasted = add(&pool, &library, &pasted, at(1)).await.unwrap();
        assert_eq!((pasted.title.as_str(), pasted.format), ("We went home.", Format::Plain));
    }
}
