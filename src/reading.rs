//! The reader: texts the learner brings, read as theirs.
//!
//! A text is pasted or imported - plain text, markdown, subtitles - and kept
//! as sentences in paragraphs. Its sentences live in the same table as the
//! pack's, so a word taken from one is a word met in a sentence like any
//! other, asked in it, and heard in it.
//!
//! Every running word is read through the lexicon of the language into the
//! lemmas it can be (*went* is *go*, *don't* is *do* and *not*), and the text
//! says how much of it the learner holds: the coverage, counted by the same
//! arithmetic the phone counts it with after every tap
//! ([`hilvan_core::cover`]). Nothing derived is stored - the analysis is done
//! again on every read, so a text read after hilvan's lexicon grew is read
//! with the lexicon it has now.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use hilvan_core::cover::{self, Coverage, Hold, Piece};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::lexicon::{self, LEVELS, Lexicon};
use crate::pack::{self, word_id};
use crate::scheduling::Stitch;

/// How a text came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Paragraphs apart by blank lines - or, with no blank line anywhere, one
    /// paragraph a line.
    Plain,
    /// Markdown: the text of its paragraphs, headings and list items, its
    /// marks and links taken off, its code left out.
    Markdown,
    /// `SubRip` or `WebVTT` subtitles: the lines of the cues, run together,
    /// a new paragraph where the speech pauses.
    Subtitles,
}

impl Format {
    /// The word stored in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Markdown => "markdown",
            Self::Subtitles => "subtitles",
        }
    }

    /// Reads a format back from the database.
    ///
    /// # Errors
    ///
    /// Fails on a word that names no format.
    pub fn parse(word: &str) -> Result<Self> {
        match word {
            "plain" => Ok(Self::Plain),
            "markdown" => Ok(Self::Markdown),
            "subtitles" => Ok(Self::Subtitles),
            other => anyhow::bail!("{other:?} is not a text format"),
        }
    }

    /// What a text pasted without saying looks like: subtitles when a line
    /// times a cue, markdown when a line is a heading or a mark of markdown
    /// stands in it, plain otherwise.
    #[must_use]
    pub fn guess(body: &str) -> Self {
        if body.lines().any(|line| cue_times(line).is_some()) {
            return Self::Subtitles;
        }
        let markdown = body.lines().any(|line| {
            let line = line.trim_start();
            let hashes = line.chars().take_while(|ch| *ch == '#').count();
            ((1..=6).contains(&hashes) && line[hashes..].starts_with(' ')) || line.starts_with("```")
        }) || body.contains("](")
            || body.contains("**");
        if markdown { Self::Markdown } else { Self::Plain }
    }
}

/// A text as it was taken apart: a title when it carries one, and its
/// paragraphs, each the sentences in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub title: Option<String>,
    pub paragraphs: Vec<Vec<String>>,
}

/// Takes a text apart into paragraphs of sentences.
#[must_use]
pub fn parse(format: Format, body: &str, lexicon: Option<&Lexicon>) -> Parsed {
    let body = body.replace("\r\n", "\n").replace('\r', "\n");
    let (title, paragraphs) = match format {
        Format::Plain => (None, plain(&body)),
        Format::Markdown => markdown(&body),
        Format::Subtitles => (None, subtitles(&body)),
    };
    Parsed {
        title,
        paragraphs: paragraphs
            .iter()
            .map(|paragraph| sentences(paragraph, lexicon))
            .filter(|sentences| !sentences.is_empty())
            .collect(),
    }
}

/// Runs of whitespace as one space, the ends trimmed.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn plain(body: &str) -> Vec<String> {
    // A text with blank lines between its paragraphs wraps them across
    // lines; one without a blank line anywhere is a paragraph a line - what
    // a paste from a chat or a page usually is.
    if body.lines().any(|line| line.trim().is_empty()) {
        body.split("\n\n").map(squeeze).filter(|paragraph| !paragraph.is_empty()).collect()
    } else {
        body.lines().map(squeeze).filter(|paragraph| !paragraph.is_empty()).collect()
    }
}

fn markdown(body: &str) -> (Option<String>, Vec<String>) {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    fn flush(current: &mut String, heading_one: bool, paragraphs: &mut Vec<String>, title: &mut Option<String>) {
        let text = squeeze(current);
        current.clear();
        if text.is_empty() {
            return;
        }
        // The first top heading before any prose is the title, not a line
        // of the text.
        if heading_one && title.is_none() && paragraphs.is_empty() {
            *title = Some(text);
        } else {
            paragraphs.push(text);
        }
    }

    let mut title = None;
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    // Inside a block of code, an image or raw HTML: nothing in them is prose.
    let mut skipping = 0usize;
    let mut heading_one = false;
    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::Image { .. } | Tag::HtmlBlock) => skipping += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::Image | TagEnd::HtmlBlock) => skipping = skipping.saturating_sub(1),
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut current, false, &mut paragraphs, &mut title);
                heading_one = level == pulldown_cmark::HeadingLevel::H1;
            }
            Event::End(TagEnd::Heading(_)) => {
                flush(&mut current, heading_one, &mut paragraphs, &mut title);
                heading_one = false;
            }
            Event::Start(Tag::Paragraph | Tag::Item) | Event::End(TagEnd::Paragraph | TagEnd::Item) => {
                flush(&mut current, false, &mut paragraphs, &mut title);
            }
            Event::Text(text) | Event::Code(text) if skipping == 0 => current.push_str(&text),
            Event::SoftBreak | Event::HardBreak => current.push(' '),
            _ => {}
        }
    }
    flush(&mut current, false, &mut paragraphs, &mut title);
    (title, paragraphs)
}

/// How long the speech has to stop for the subtitles to start a new
/// paragraph: a breath is shorter, a change of scene longer.
const PAUSE_MS: i64 = 2_000;

/// The start and end of a cue, in milliseconds, when the line times one:
/// `00:01:02,500 --> 00:01:04,000` (or with full stops, as `WebVTT` writes
/// them, and with an hour that may be left off).
fn cue_times(line: &str) -> Option<(i64, i64)> {
    let (from, to) = line.split_once("-->")?;
    let moment = |text: &str| -> Option<i64> {
        let text = text.split_whitespace().next()?;
        let (clock, fraction) = text.rsplit_once([',', '.'])?;
        let mut seconds = 0i64;
        let fields: Vec<&str> = clock.split(':').collect();
        if !(2..=3).contains(&fields.len()) {
            return None;
        }
        for field in fields {
            seconds = seconds * 60 + field.parse::<i64>().ok()?;
        }
        Some(seconds * 1000 + fraction.parse::<i64>().ok()?)
    };
    Some((moment(from.trim())?, moment(to.trim())?))
}

/// What a cue says, without what is not speech: markup (`<i>`, `{\an8}`),
/// sounds and speakers in brackets, the dash that opens a line of dialogue,
/// the notes of a song.
fn cue_text(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut depth: Vec<char> = Vec::new();
    for ch in line.chars() {
        match ch {
            '<' | '{' | '[' | '(' => depth.push(ch),
            '>' | '}' | ']' | ')' if !depth.is_empty() => {
                depth.pop();
            }
            '♪' | '♫' => {}
            _ if depth.is_empty() => text.push(ch),
            _ => {}
        }
    }
    let text = squeeze(&text);
    text.trim_start_matches(['-', '–', '—']).trim().to_string()
}

fn subtitles(body: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut last_end: Option<i64> = None;
    for block in body.split("\n\n") {
        let mut lines = block.lines().map(str::trim).filter(|line| !line.is_empty());
        // A block that times nothing is a header, a note or a style: skipped.
        let Some((start, end)) = lines.by_ref().find_map(cue_times) else {
            continue;
        };
        if last_end.is_some_and(|last| start - last >= PAUSE_MS) && !current.is_empty() {
            paragraphs.push(current.join(" "));
            current.clear();
        }
        last_end = Some(end);
        current.extend(lines.map(cue_text).filter(|line| !line.is_empty()));
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    paragraphs
}

/// What ends a sentence.
const TERMINATORS: [char; 4] = ['.', '!', '?', '…'];

/// What may close a sentence after its stop: quotes and brackets.
const CLOSERS: [char; 7] = ['"', '\'', '”', '’', ')', ']', '»'];

/// The sentences of a paragraph.
///
/// A sentence ends at a stop - a full stop, a question or an exclamation
/// mark, an ellipsis, with the quotes and brackets that close after it - when
/// what follows the space does not begin in lower case. A full stop after a
/// single letter (an initial) or after an abbreviation the language's rules
/// list ("Mr.") ends nothing.
#[must_use]
pub fn sentences(paragraph: &str, lexicon: Option<&Lexicon>) -> Vec<String> {
    let chars: Vec<char> = paragraph.chars().collect();
    let mut found = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < chars.len() {
        if !TERMINATORS.contains(&chars[at]) {
            at += 1;
            continue;
        }
        let stop = at;
        let mut end = at;
        while end + 1 < chars.len() && TERMINATORS.contains(&chars[end + 1]) {
            end += 1;
        }
        while end + 1 < chars.len() && CLOSERS.contains(&chars[end + 1]) {
            end += 1;
        }
        at = end + 1;
        if at >= chars.len() || !chars[at].is_whitespace() {
            continue;
        }
        let mut next = at;
        while next < chars.len() && chars[next].is_whitespace() {
            next += 1;
        }
        if next >= chars.len() || chars[next].is_lowercase() {
            continue;
        }
        if chars[stop] == '.' && end == stop && abbreviated(&chars[..stop], lexicon) {
            continue;
        }
        let sentence: String = chars[start..=end].iter().collect();
        found.push(sentence.trim().to_string());
        start = next;
        at = next;
    }
    let rest: String = chars[start.min(chars.len())..].iter().collect();
    if !rest.trim().is_empty() {
        found.push(rest.trim().to_string());
    }
    found.retain(|sentence| !lexicon::tokens(sentence).is_empty());
    found
}

/// Whether the word before a full stop makes it no end of a sentence: an
/// initial, or an abbreviation of the language.
fn abbreviated(before: &[char], lexicon: Option<&Lexicon>) -> bool {
    let word: String = before
        .iter()
        .rev()
        .take_while(|ch| ch.is_alphabetic() || **ch == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let word = word.trim_start_matches('.');
    if word.chars().count() == 1 && word.chars().all(char::is_uppercase) {
        return true;
    }
    lexicon.is_some_and(|lexicon| lexicon.is_abbreviation(&word.to_lowercase()))
}

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

/// A text as the list shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub id: i64,
    pub title: String,
    pub language: String,
    pub format: Format,
    pub added_at: DateTime<Utc>,
    /// When it was last opened; `None` for a text never opened.
    pub opened_at: Option<DateTime<Utc>>,
    /// The paragraph the learner stopped at, from 0.
    pub paragraph: i64,
    /// How many paragraphs it has.
    pub paragraphs: i64,
    /// When the learner said they had read it.
    pub finished_at: Option<DateTime<Utc>>,
    /// How much of it the learner holds, now.
    pub coverage: Coverage,
}

/// One sentence of a text, read.
#[derive(Debug, Clone, Serialize)]
pub struct Line {
    /// The sentence's id: what a word is taken from it under.
    pub sentence: i64,
    pub text: String,
    pub tokens: Vec<Token>,
}

/// A text as the reader shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Text {
    #[serde(flatten)]
    pub summary: Summary,
    /// Its paragraphs, each the sentences in it. Not `paragraphs`: the
    /// summary beside it counts them under that name.
    pub lines: Vec<Vec<Line>>,
    /// Every lemma its words can be, `en:go`: what the tokens' parts index.
    pub lemmas: Vec<String>,
    /// What the learner holds of each, by the same index.
    pub holds: Vec<Option<Hold>>,
}

/// A text the learner brings.
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

/// Why a text was not taken or changed: the client's to fix.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("load a pack first: the reader reads the language a pack teaches")]
    NoLanguage,
    #[error("the text holds no sentence to read")]
    Empty,
    #[error("there is no text {0}")]
    NotFound(i64),
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

/// Keeps a text: taken apart into sentences, each kept once among the
/// sentences of its language.
///
/// # Errors
///
/// [`Refused::NoLanguage`] before a pack is loaded, [`Refused::Empty`] for a
/// text with no sentence in it, and [`Refused::Database`] when the database
/// rejects a statement.
pub async fn add(pool: &SqlitePool, addition: &Addition, now: DateTime<Utc>) -> Result<Summary, Refused> {
    let language = pack::language_being_learnt(pool).await?.ok_or(Refused::NoLanguage)?;
    let format = addition.format.unwrap_or_else(|| Format::guess(&addition.body));
    let parsed = parse(format, &addition.body, lexicon::of(&language));
    let first = parsed.paragraphs.first().and_then(|paragraph| paragraph.first()).ok_or(Refused::Empty)?;
    let title = addition
        .title
        .as_deref()
        .map(squeeze)
        .filter(|title| !title.is_empty())
        .or_else(|| parsed.title.clone())
        .unwrap_or_else(|| title_from(first));

    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let id: i64 = sqlx::query_scalar("INSERT INTO text (language, title, format, added_at) VALUES (?, ?, ?, ?) RETURNING id")
        .bind(&language)
        .bind(&title)
        .bind(format.as_str())
        .bind(now.to_rfc3339())
        .fetch_one(&mut *tx)
        .await
        .context("failed to keep a text")?;
    let mut position = 0i64;
    for (paragraph, sentences) in parsed.paragraphs.iter().enumerate() {
        for sentence in sentences {
            let sentence_id = keep_sentence(&mut tx, &language, sentence).await?;
            sqlx::query("INSERT INTO text_sentence (text_id, position, paragraph, sentence_id) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind(position)
                .bind(i64::try_from(paragraph).unwrap_or(i64::MAX))
                .bind(sentence_id)
                .execute(&mut *tx)
                .await
                .context("failed to keep a sentence of a text")?;
            position += 1;
        }
    }
    tx.commit().await.context("failed to commit a text")?;

    let holds = holds(pool).await?;
    summary(pool, id, &holds).await?.ok_or(Refused::NotFound(id))
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

/// How much of a text the learner holds, its words read and counted.
fn count(analysis: &Analysis, holds: &HashMap<String, Hold>) -> (Vec<Option<Hold>>, Coverage) {
    let held: Vec<Option<Hold>> = analysis.lemmas.iter().map(|id| holds.get(id).copied()).collect();
    let pieces: Vec<Piece> = analysis.pieces().cloned().collect();
    let (coverage, _) = cover::cover(&pieces, &held);
    (held, coverage)
}

fn moment(text: Option<String>) -> Result<Option<DateTime<Utc>>> {
    text.map(|text| {
        DateTime::parse_from_rfc3339(&text)
            .map(|at| at.with_timezone(&Utc))
            .with_context(|| format!("{text:?} is not a moment"))
    })
    .transpose()
}

/// One text's row and the sentences in it.
async fn summary_and_lines(
    pool: &SqlitePool,
    id: i64,
    holds: &HashMap<String, Hold>,
) -> Result<Option<(Summary, Vec<(i64, i64, String)>, Analysis, Vec<Option<Hold>>)>> {
    let Some(row) = sqlx::query("SELECT id, language, title, format, added_at, opened_at, paragraph, finished_at FROM text WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("failed to read a text")?
    else {
        return Ok(None);
    };
    let lines = lines_of(pool, id).await?;
    let language: String = row.get("language");
    let analysis = analyse(&language, &lines.iter().map(|(_, _, text)| text.as_str()).collect::<Vec<_>>());
    let (held, coverage) = count(&analysis, holds);
    let paragraphs = lines.last().map_or(0, |(_, paragraph, _)| paragraph + 1);
    let summary = Summary {
        id: row.get("id"),
        title: row.get("title"),
        format: Format::parse(row.get::<String, _>("format").as_str())?,
        added_at: moment(Some(row.get("added_at")))?.context("a text without the moment it was added")?,
        opened_at: moment(row.get("opened_at"))?,
        paragraph: row.get::<i64, _>("paragraph").min((paragraphs - 1).max(0)),
        paragraphs,
        finished_at: moment(row.get("finished_at"))?,
        language,
        coverage,
    };
    Ok(Some((summary, lines, analysis, held)))
}

async fn summary(pool: &SqlitePool, id: i64, holds: &HashMap<String, Hold>) -> Result<Option<Summary>> {
    Ok(summary_and_lines(pool, id, holds).await?.map(|(summary, ..)| summary))
}

/// Every text, the one opened last first and those never opened after them,
/// newest first.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Summary>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM text ORDER BY opened_at IS NULL, opened_at DESC, added_at DESC, id DESC")
        .fetch_all(pool)
        .await
        .context("failed to list the texts")?;
    let holds = holds(pool).await?;
    let mut texts = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(summary) = summary(pool, id, &holds).await? {
            texts.push(summary);
        }
    }
    Ok(texts)
}

/// One text, every word read.
///
/// # Errors
///
/// [`Refused::NotFound`] for no such text, [`Refused::Database`] when the
/// database rejects a query.
pub async fn read(pool: &SqlitePool, id: i64) -> Result<Text, Refused> {
    let holds = holds(pool).await?;
    let (summary, lines, analysis, held) = summary_and_lines(pool, id, &holds).await?.ok_or(Refused::NotFound(id))?;
    let mut paragraphs: Vec<Vec<Line>> = Vec::new();
    for ((sentence, paragraph, text), tokens) in lines.into_iter().zip(analysis.tokens) {
        let paragraph = usize::try_from(paragraph).unwrap_or(0);
        if paragraphs.len() <= paragraph {
            paragraphs.resize_with(paragraph + 1, Vec::new);
        }
        paragraphs[paragraph].push(Line { sentence, text, tokens });
    }
    Ok(Text {
        summary,
        lines: paragraphs,
        lemmas: analysis.lemmas,
        holds: held,
    })
}

/// Where the learner is in a text, as the reader reports it.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Place {
    /// The paragraph being read, from 0.
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
pub async fn place(pool: &SqlitePool, id: i64, place: Place, now: DateTime<Utc>) -> Result<Summary, Refused> {
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
    summary(pool, id, &holds).await?.ok_or(Refused::NotFound(id))
}

/// Lets a text go. The words taken from it stay in the deck with the
/// sentences they were taken from.
///
/// # Errors
///
/// [`Refused::NotFound`] for no such text, [`Refused::Database`] when the
/// database rejects a statement.
pub async fn remove(pool: &SqlitePool, id: i64) -> Result<(), Refused> {
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
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn english() -> Option<&'static Lexicon> {
        lexicon::of("en")
    }

    #[test]
    fn a_paragraph_breaks_into_sentences_at_their_stops() {
        assert_eq!(
            sentences("She went home. Was it late? \"Yes!\" he said. It was... Then it rained.", english()),
            vec!["She went home.", "Was it late?", "\"Yes!\" he said.", "It was...", "Then it rained."]
        );
    }

    #[test]
    fn a_stop_before_lower_case_or_inside_a_word_ends_nothing() {
        assert_eq!(
            sentences("It costs 3.50 now, e.g. on Mondays.", english()),
            vec!["It costs 3.50 now, e.g. on Mondays."]
        );
    }

    #[test]
    fn an_initial_and_an_abbreviation_end_no_sentence() {
        assert_eq!(
            sentences("Mr. Brown met J. R. Smith. Dr. Lee was late.", english()),
            vec!["Mr. Brown met J. R. Smith.", "Dr. Lee was late."]
        );
        // Without the rules of the language, "Mr." is the end of a sentence.
        assert_eq!(sentences("Mr. Brown came.", None), vec!["Mr.", "Brown came."]);
    }

    #[test]
    fn a_piece_with_no_word_in_it_is_no_sentence() {
        assert_eq!(sentences("Go. 42. Now.", english()), vec!["Go.", "Now."]);
    }

    #[test]
    fn plain_text_is_paragraphs_by_blank_lines_or_by_lines() {
        let wrapped = parse(Format::Plain, "One line\nwrapped here.\n\nTwo.", english());
        assert_eq!(wrapped.paragraphs, vec![vec!["One line wrapped here.".to_string()], vec!["Two.".to_string()]]);
        let lines = parse(Format::Plain, "First line.\r\nSecond line.", english());
        assert_eq!(lines.paragraphs.len(), 2, "with no blank line anywhere, a line is a paragraph");
    }

    #[test]
    fn markdown_keeps_its_prose_and_leaves_its_marks() {
        let parsed = parse(
            Format::Markdown,
            "# The trip\n\nWe *went* to [the sea](http://x). It was **cold**.\n\n```\ncode is not prose\n```\n\n- A list item.\n\n## Later\n\n![a picture](p.png) Home `again`.",
            english(),
        );
        assert_eq!(parsed.title.as_deref(), Some("The trip"));
        assert_eq!(
            parsed.paragraphs,
            vec![
                vec!["We went to the sea.".to_string(), "It was cold.".to_string()],
                vec!["A list item.".to_string()],
                vec!["Later".to_string()],
                vec!["Home again.".to_string()],
            ]
        );
    }

    const SRT: &str = "1\n00:00:01,000 --> 00:00:02,500\n<i>Where are you</i>\ngoing?\n\n2\n00:00:02,700 --> 00:00:04,000\n- Home. [door slams]\n\n3\n00:00:09,000 --> 00:00:10,000\n♪ Later that night ♪\n";

    #[test]
    fn subtitles_run_their_cues_together_and_pause_into_paragraphs() {
        let parsed = parse(Format::Subtitles, SRT, english());
        assert_eq!(
            parsed.paragraphs,
            vec![
                vec!["Where are you going?".to_string(), "Home.".to_string()],
                vec!["Later that night".to_string()]
            ]
        );
        let vtt = "WEBVTT\n\n00:01.000 --> 00:02.000\nHello there.\n";
        assert_eq!(parse(Format::Subtitles, vtt, english()).paragraphs, vec![vec!["Hello there.".to_string()]]);
    }

    #[test]
    fn a_pasted_text_says_what_it_is() {
        assert_eq!(Format::guess(SRT), Format::Subtitles);
        assert_eq!(Format::guess("# Title\n\nText."), Format::Markdown);
        assert_eq!(Format::guess("See [this](x)."), Format::Markdown);
        assert_eq!(Format::guess("Just a #hashtag and a line.\nAnother."), Format::Plain);
    }

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

    #[tokio::test]
    async fn a_text_is_kept_as_its_sentences_and_shares_them_with_the_pack() {
        let pool = tutor().await;
        let added = add(&pool, &plain("Visit", "She went to the house.\n\nHe is a doctor. It was late."), at(0))
            .await
            .unwrap();
        assert_eq!((added.title.as_str(), added.paragraphs, added.format), ("Visit", 2, Format::Plain));

        let text = read(&pool, added.id).await.unwrap();
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
        let added = add(&pool, &plain("t", "She went to the house, Harry."), at(0)).await.unwrap();
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
        let sentence = read(&pool, added.id).await.unwrap().lines[0][0].sentence;
        crate::words::pick(&pool, "en:house", sentence, 16, at(2)).await.unwrap();

        let text = read(&pool, added.id).await.unwrap();
        assert_eq!(
            (text.summary.coverage.sewn, text.summary.coverage.deck, text.summary.coverage.unknown),
            (2, 1, 2)
        );
        assert_eq!(text.summary.coverage.yours(), 2, "a word put in the deck is not in hand yet");
        let go = text.lemmas.iter().position(|id| id == "en:go").unwrap();
        assert_eq!(text.holds[go], Some(Hold::Sewn));
        assert_eq!(
            list(&pool).await.unwrap()[0].coverage,
            text.summary.coverage,
            "the list counts as the reader does"
        );
    }

    #[tokio::test]
    async fn a_reload_of_the_pack_keeps_the_sentences_of_texts() {
        let pool = tutor().await;
        let added = add(&pool, &plain("t", "It was late."), at(0)).await.unwrap();
        crate::pack::load(&pool, &pack(2)).await.unwrap();
        assert_eq!(read(&pool, added.id).await.unwrap().lines[0][0].text, "It was late.");
    }

    #[tokio::test]
    async fn letting_a_text_go_keeps_the_sentence_a_word_was_taken_from() {
        let pool = tutor().await;
        let added = add(&pool, &plain("t", "Suddenly it rained. It was late."), at(0)).await.unwrap();
        let sentence = read(&pool, added.id).await.unwrap().lines[0][0].sentence;
        crate::words::pick(&pool, "en:suddenly", sentence, 0, at(1)).await.unwrap();

        remove(&pool, added.id).await.unwrap();
        crate::pack::load(&pool, &pack(2)).await.unwrap();
        let kept: Vec<String> = sqlx::query_scalar("SELECT text FROM sentence ORDER BY id").fetch_all(&pool).await.unwrap();
        assert!(kept.contains(&"Suddenly it rained.".to_string()), "the word's sentence stays: {kept:?}");
        assert!(!kept.contains(&"It was late.".to_string()), "a sentence nothing shows goes: {kept:?}");
        let word = crate::words::word(&pool, "en:suddenly").await.unwrap();
        assert_eq!(word.contexts[0].text, "Suddenly it rained.");
        assert_eq!(word.contexts[0].source, crate::words::Source::Text { id: None, title: None });
        assert!(matches!(read(&pool, added.id).await, Err(Refused::NotFound(_))));
    }

    #[tokio::test]
    async fn the_place_is_kept_and_finishing_is_the_learners_to_say() {
        let pool = tutor().await;
        let first = add(&pool, &plain("first", "One.\n\nTwo."), at(0)).await.unwrap();
        let second = add(&pool, &plain("second", "Three."), at(1)).await.unwrap();
        assert_eq!(list(&pool).await.unwrap()[0].id, second.id, "never opened: the newest first");

        let placed = place(
            &pool,
            first.id,
            Place {
                paragraph: Some(9),
                finished: None,
            },
            at(2),
        )
        .await
        .unwrap();
        assert_eq!(placed.paragraph, 1, "a paragraph past the end is the last");
        assert_eq!(placed.opened_at, Some(at(2)));
        assert_eq!(list(&pool).await.unwrap()[0].id, first.id, "the one opened last first");

        let finished = place(
            &pool,
            first.id,
            Place {
                paragraph: None,
                finished: Some(true),
            },
            at(3),
        )
        .await
        .unwrap();
        assert_eq!((finished.paragraph, finished.finished_at), (1, Some(at(3))));
        let again = place(
            &pool,
            first.id,
            Place {
                paragraph: Some(0),
                finished: Some(true),
            },
            at(4),
        )
        .await
        .unwrap();
        assert_eq!(again.finished_at, Some(at(3)), "finished once is finished when it was");
        let unfinished = place(
            &pool,
            first.id,
            Place {
                paragraph: None,
                finished: Some(false),
            },
            at(5),
        )
        .await
        .unwrap();
        assert_eq!(unfinished.finished_at, None);
        assert!(matches!(place(&pool, 99, Place::default(), at(6)).await, Err(Refused::NotFound(99))));
    }

    #[tokio::test]
    async fn an_empty_text_and_a_tutor_without_a_language_are_refused() {
        let pool = tutor().await;
        assert!(matches!(add(&pool, &plain("t", " 42 \n\n -- "), at(0)).await, Err(Refused::Empty)));
        let bare = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&bare).await.unwrap();
        assert!(matches!(add(&bare, &plain("t", "Hello."), at(0)).await, Err(Refused::NoLanguage)));
    }

    #[tokio::test]
    async fn a_text_without_a_title_is_named_by_what_it_carries() {
        let pool = tutor().await;
        let markdown = Addition {
            title: None,
            body: "# A day out\n\nWe went home.".to_string(),
            format: None,
        };
        assert_eq!(add(&pool, &markdown, at(0)).await.unwrap().title, "A day out");
        let pasted = Addition {
            title: Some("  ".to_string()),
            body: "We went home. Then we slept.".to_string(),
            format: None,
        };
        let pasted = add(&pool, &pasted, at(1)).await.unwrap();
        assert_eq!((pasted.title.as_str(), pasted.format), ("We went home.", Format::Plain));
    }
}
