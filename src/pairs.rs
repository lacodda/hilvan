//! Confusion pairs: two words the learner puts one for the other.
//!
//! A pair is not chosen from a list. Every typed turn keeps the words typed
//! in the place of others ([`hilvan_core::answer::Swap`]), read into lemmas -
//! "said" where "told" stood is *say* for *tell* - and two lemmas swapped
//! [`PAIR_AT`] times, either way round, get a card of their own: kind
//! `pair`, subject `en:say|tell`, the two in order. What the pair is, and
//! where it came from, the turns say; the card keeps only its schedule.
//!
//! The card asks the difference in sentences: each the learner has met, with
//! the word left out, and the two words to choose from. The sentence is its
//! own answer key - whichever of the two stands in it is right - so a choice
//! is checked here, never on the device's word.

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use hilvan_core::answer::Swap;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::lexicon;
use crate::pack::word_id;
use crate::scheduling::{Kind, Rating};
use crate::study::Languages;

/// How many times two words have to be swapped before they are a pair: once
/// is a slip, twice is a habit.
pub const PAIR_AT: usize = 2;

/// How many sentences one answer to a pair asks: enough for each word to be
/// right more than once, few enough for a sitting.
pub const PAIR_QUESTIONS: usize = 4;

/// The id of the pair two words make: the same whichever was typed for
/// which.
#[must_use]
pub fn id(a: &str, b: &str) -> String {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let second = second.split_once(':').map_or(second, |(_, lemma)| lemma);
    format!("{first}|{second}")
}

/// The two words of a pair, as word ids: `en:say|tell` is `en:say` and
/// `en:tell`. `None` for an id that is not a pair's.
#[must_use]
pub fn words_of(pair: &str) -> Option<(String, String)> {
    let (language, rest) = pair.split_once(':')?;
    let (a, b) = rest.split_once('|')?;
    (!a.is_empty() && !b.is_empty() && a != b).then(|| (format!("{language}:{a}"), format!("{language}:{b}")))
}

/// The lemmas of the words a turn typed in the place of others, as word ids
/// - the one that stood there first, the one typed second.
///
/// A swap counts when the lexicon knows both spellings and they are not one
/// lemma: "go" for "went" is a form gone wrong, "bisy" for "busy" a word
/// misspelt, and neither is two words confused. A spelling of several lemmas
/// is read as its commonest.
#[must_use]
pub fn read(language: &str, swapped: &[Swap]) -> Vec<(String, String)> {
    let Some(lexicon) = lexicon::of(language) else {
        return Vec::new();
    };
    swapped
        .iter()
        .filter_map(|swap| {
            let expected = lexicon.lemmas_of(&swap.expected);
            let typed = lexicon.lemmas_of(&swap.typed);
            if expected.iter().any(|lemma| typed.contains(lemma)) {
                return None;
            }
            Some((word_id(language, expected.first()?), word_id(language, typed.first()?)))
        })
        .collect()
}

/// Every pair the turns have swapped, with how many times.
async fn swaps<'e, E: sqlx::SqliteExecutor<'e>>(executor: E) -> Result<BTreeMap<String, usize>> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT swapped FROM attempt WHERE swapped != '[]'")
        .fetch_all(executor)
        .await
        .context("failed to read the words typed in the place of others")?;
    let mut counts = BTreeMap::new();
    for row in rows {
        let swapped: Vec<(String, String)> = serde_json::from_str(&row).with_context(|| format!("the swaps of a turn are not JSON: {row}"))?;
        for (expected, typed) in swapped {
            *counts.entry(id(&expected, &typed)).or_default() += 1;
        }
    }
    Ok(counts)
}

/// Opens a card for every pair swapped [`PAIR_AT`] times or more that has
/// none yet, inside the caller's transaction: the turn that makes a habit of
/// a slip is the one that opens it.
///
/// # Errors
///
/// Fails when the database rejects a statement or holds a turn whose swaps
/// are not JSON.
pub async fn open(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, now: DateTime<Utc>) -> Result<()> {
    for (pair, count) in swaps(&mut **tx).await? {
        if count < PAIR_AT {
            continue;
        }
        sqlx::query(
            "INSERT INTO card (kind, subject_id, state, stability, difficulty, due, reps, lapses)
             VALUES (?, ?, 'new', 0.0, 0.0, ?, 0, 0)
             ON CONFLICT (kind, subject_id) DO NOTHING",
        )
        .bind(Kind::Pair.as_str())
        .bind(&pair)
        .bind(now.to_rfc3339())
        .execute(&mut **tx)
        .await
        .with_context(|| format!("failed to open the pair {pair}"))?;
    }
    Ok(())
}

/// A pair, with everything its card needs.
#[derive(Debug, Clone, Serialize)]
pub struct Pair {
    /// `en:say|tell`.
    pub id: String,
    /// The two words, in the order of the id.
    pub words: Vec<Side>,
    /// The sentences the card asks, at most [`PAIR_QUESTIONS`], the two
    /// words taking turns being the right one while each has sentences left.
    pub questions: Vec<Question>,
    pub languages: Languages,
}

/// One word of a pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Side {
    /// `en:tell`.
    pub id: String,
    pub lemma: String,
    /// What it means, by the pack or its glossary.
    pub gloss: Option<String>,
    /// How many times it was typed where the other one stood.
    pub typed_for_the_other: usize,
}

/// A sentence the card asks, with the word left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    /// The whole sentence, as it was met.
    pub text: String,
    /// Where the word stands in it, in characters: what the card hides.
    pub start: usize,
    pub length: usize,
    /// What it means, in the learner's own language, when it is known: the
    /// prompt the turn was typed for, or the pack's translation.
    pub translation: Option<String>,
    /// The word that stands there.
    pub answer: String,
}

/// One pair, with its sentences.
///
/// # Errors
///
/// Fails when there is no such pair - no card, or an id that is not a
/// pair's - or when the database rejects a query.
pub async fn pair(pool: &SqlitePool, id: &str) -> Result<Pair> {
    let (a, b) = words_of(id).with_context(|| format!("there is no pair called {id}"))?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM card WHERE kind = ? AND subject_id = ?)")
        .bind(Kind::Pair.as_str())
        .bind(id)
        .fetch_one(pool)
        .await
        .context("failed to look for a pair")?;
    anyhow::ensure!(exists, "there is no pair called {id}");
    let language = a.split_once(':').map_or("", |(language, _)| language).to_string();
    let lexicon = lexicon::of(&language);
    let native: String = sqlx::query_scalar("SELECT native FROM pack WHERE target = ? ORDER BY loaded_at, id LIMIT 1")
        .bind(&language)
        .fetch_optional(pool)
        .await
        .context("failed to read the learner's own language")?
        .with_context(|| format!("there is no pair called {id}: no pack teaches {language}"))?;

    let counts = typed_for(pool, &a, &b).await?;
    let mut words = Vec::with_capacity(2);
    for (word, typed) in [(&a, counts.0), (&b, counts.1)] {
        let lemma = word.split_once(':').map_or(word.as_str(), |(_, lemma)| lemma);
        let lemma = lexicon
            .and_then(|lexicon| lexicon.entry(lemma))
            .map_or_else(|| lemma.to_string(), |entry| entry.lemma.clone());
        words.push(Side {
            gloss: gloss(pool, word, &native).await?,
            id: word.clone(),
            lemma,
            typed_for_the_other: typed,
        });
    }

    // Where each word stood: the sentences of the turns that swapped them
    // first - those are the mistakes - then every sentence met that holds
    // one of the two.
    let mut sentences: Vec<(String, Option<String>)> = turns_of(pool, &a, &b).await?;
    sentences.extend(met_sentences(pool, &language, &native).await?);

    let mut seen = HashSet::new();
    let mut by_word: [Vec<Question>; 2] = [Vec::new(), Vec::new()];
    for (text, translation) in sentences {
        if !seen.insert(text.clone()) {
            continue;
        }
        let found: Vec<(usize, lexicon::Found<'_>)> = words
            .iter()
            .enumerate()
            .filter_map(|(at, side)| lexicon::locate(lexicon, &text, &side.lemma).map(|found| (at, found)))
            .collect();
        // A sentence that holds both, or neither, cannot say which is right.
        let [(at, found)] = found.as_slice() else { continue };
        by_word[*at].push(Question {
            start: found.start,
            length: found.text.chars().count(),
            answer: words[*at].id.clone(),
            translation: translation.clone(),
            text: text.clone(),
        });
    }

    let mut questions = Vec::with_capacity(PAIR_QUESTIONS);
    let [mut first, mut second] = by_word.map(Vec::into_iter);
    while questions.len() < PAIR_QUESTIONS {
        let before = questions.len();
        questions.extend(first.next());
        if questions.len() < PAIR_QUESTIONS {
            questions.extend(second.next());
        }
        if questions.len() == before {
            break;
        }
    }

    Ok(Pair {
        id: id.to_string(),
        words,
        questions,
        languages: Languages { native, target: language },
    })
}

/// How many times each word of a pair was typed for the other.
async fn typed_for(pool: &SqlitePool, a: &str, b: &str) -> Result<(usize, usize)> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT swapped FROM attempt WHERE swapped != '[]'")
        .fetch_all(pool)
        .await
        .context("failed to read the words typed in the place of others")?;
    let mut counts = (0, 0);
    for row in rows {
        let swapped: Vec<(String, String)> = serde_json::from_str(&row).with_context(|| format!("the swaps of a turn are not JSON: {row}"))?;
        for (expected, typed) in swapped {
            if expected == b && typed == a {
                counts.0 += 1;
            } else if expected == a && typed == b {
                counts.1 += 1;
            }
        }
    }
    Ok(counts)
}

/// The right answers of the turns that swapped the two, newest first, each
/// with the prompt it was typed for.
async fn turns_of(pool: &SqlitePool, a: &str, b: &str) -> Result<Vec<(String, Option<String>)>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT a.expected, a.prompt, a.swapped FROM attempt a JOIN review r ON r.id = a.review_id
         WHERE a.swapped != '[]' ORDER BY r.reviewed_at DESC, a.id DESC",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the turns of a pair")?;
    let mut sentences = Vec::new();
    for (expected, prompt, swapped) in rows {
        let swapped: Vec<(String, String)> = serde_json::from_str(&swapped).with_context(|| format!("the swaps of a turn are not JSON: {swapped}"))?;
        if swapped.iter().any(|(x, y)| (x == a && y == b) || (x == b && y == a)) {
            sentences.push((expected, Some(prompt)));
        }
    }
    Ok(sentences)
}

/// Every sentence of the language the learner has met: the samples of the
/// formulas they have started, and the sentences words were taken from.
async fn met_sentences(pool: &SqlitePool, language: &str, native: &str) -> Result<Vec<(String, Option<String>)>> {
    sqlx::query_as(
        "SELECT s.text, t.text FROM sentence s
         LEFT JOIN sentence_translation t ON t.sentence_id = s.id AND t.language = ?
         WHERE s.language = ?
           AND (s.id IN (SELECT fs.sentence_id FROM formula_sentence fs
                         JOIN card c ON c.kind = 'formula' AND c.subject_id = fs.formula_id AND c.state != 'new')
                OR s.id IN (SELECT sentence_id FROM pick))
         ORDER BY s.id",
    )
    .bind(native)
    .bind(language)
    .fetch_all(pool)
    .await
    .context("failed to read the sentences met")
}

/// What a word means in the learner's own language: the pack's gloss, or
/// its glossary's.
async fn gloss(pool: &SqlitePool, word: &str, native: &str) -> Result<Option<String>> {
    let taught: Option<String> = sqlx::query_scalar("SELECT gloss FROM word WHERE id = ?")
        .bind(word)
        .fetch_optional(pool)
        .await
        .context("failed to read a word's gloss")?;
    if taught.is_some() {
        return Ok(taught);
    }
    sqlx::query_scalar(
        "SELECT g.text FROM gloss g JOIN pack p ON p.id = g.pack_id
         WHERE g.word_id = ? AND p.native = ? ORDER BY p.loaded_at, p.id LIMIT 1",
    )
    .bind(word)
    .bind(native)
    .fetch_optional(pool)
    .await
    .context("failed to read the gloss of a word")
}

/// One sentence of a pair, answered: the sentence as the card showed it,
/// and the word chosen for its gap.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub text: String,
    /// A word id: `en:tell`.
    pub chosen: String,
}

/// An answer to a pair: the choice made for each sentence it asked.
#[derive(Debug, Clone, Deserialize)]
pub struct Chosen {
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

/// A choice that cannot be checked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unchecked {
    #[error("an answer to a pair chooses for its sentences, and none came")]
    Empty,
    #[error("{0:?} is not a word of the pair")]
    NotOfThePair(String),
    #[error("{0:?} holds neither word of the pair, or both, and cannot say which is right")]
    NoKey(String),
}

/// The grade a round of choices earns: none wrong is good, one wrong hard,
/// half or more again - the rule a typed round is graded by (ADR 0007).
/// Whether each was right, in order.
///
/// # Errors
///
/// [`Unchecked`] for an empty round, a word not of the pair, and a sentence
/// that does not say which word is right.
pub fn grade(pair: &str, choices: &[Choice]) -> Result<(Rating, Vec<bool>), Unchecked> {
    let (a, b) = words_of(pair).ok_or_else(|| Unchecked::NotOfThePair(pair.to_string()))?;
    if choices.is_empty() {
        return Err(Unchecked::Empty);
    }
    let language = a.split_once(':').map_or("", |(language, _)| language);
    let lexicon = lexicon::of(language);
    let lemma = |word: &str| word.split_once(':').map_or(word, |(_, lemma)| lemma).to_string();
    let mut right = Vec::with_capacity(choices.len());
    for choice in choices {
        if choice.chosen != a && choice.chosen != b {
            return Err(Unchecked::NotOfThePair(choice.chosen.clone()));
        }
        let holds_a = lexicon::locate(lexicon, &choice.text, &lemma(&a)).is_some();
        let holds_b = lexicon::locate(lexicon, &choice.text, &lemma(&b)).is_some();
        let key = match (holds_a, holds_b) {
            (true, false) => &a,
            (false, true) => &b,
            _ => return Err(Unchecked::NoKey(choice.text.clone())),
        };
        right.push(choice.chosen == *key);
    }
    let wrong = right.iter().filter(|right| !**right).count();
    let rating = if wrong == 0 {
        Rating::Good
    } else if wrong * 2 >= right.len() {
        Rating::Again
    } else {
        Rating::Hard
    };
    Ok((rating, right))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swap(expected: &str, typed: &str) -> Swap {
        Swap {
            expected: expected.to_string(),
            typed: typed.to_string(),
        }
    }

    #[test]
    fn a_pair_is_the_same_whichever_way_round() {
        assert_eq!(id("en:tell", "en:say"), "en:say|tell");
        assert_eq!(id("en:say", "en:tell"), "en:say|tell");
        assert_eq!(words_of("en:say|tell"), Some(("en:say".to_string(), "en:tell".to_string())));
        assert_eq!(words_of("en:say"), None);
        assert_eq!(words_of("en:say|say"), None);
    }

    #[test]
    fn a_swap_of_two_lemmas_is_read_and_a_form_or_a_typo_is_not() {
        let read = read("en", &[swap("told", "said"), swap("went", "go"), swap("busy", "bisy"), swap("many", "much")]);
        assert_eq!(
            read,
            vec![("en:tell".to_string(), "en:say".to_string()), ("en:many".to_string(), "en:much".to_string())]
        );
        assert!(super::read("xx", &[swap("a", "b")]).is_empty(), "no lexicon, no lemmas");
    }

    /// Says the formula of the test pack once, its one sentence typed as
    /// given.
    async fn say(pool: &SqlitePool, typed: &str, at: DateTime<Utc>) {
        let formula = crate::study::formula(pool, "be").await.unwrap();
        let turns = vec![crate::study::Turn {
            asked: crate::study::Asked::Sample(formula.samples[0].sentence),
            typed: typed.to_string(),
        }];
        let answer = crate::study::Answer::Produce {
            turns,
            easy: false,
            duration_ms: None,
        };
        crate::study::review(pool, "be", crate::study::arriving(answer), at).await.unwrap();
    }

    #[tokio::test]
    async fn two_words_swapped_twice_open_a_pair_asked_in_their_sentences() {
        use crate::reading::tests::{at, tutor};
        use crate::study::{self, Subject};

        let pool = tutor().await;
        say(&pool, "He is a nurse.", at(0)).await;
        let pairs = |today: &study::Today| -> Vec<String> {
            today
                .queue
                .iter()
                .filter_map(|due| match &due.subject {
                    Subject::Pair { pair } => Some(pair.id.clone()),
                    _ => None,
                })
                .collect()
        };
        assert!(pairs(&study::today(&pool, at(1)).await.unwrap()).is_empty(), "once is a slip");

        say(&pool, "He is a nurse.", at(2)).await;
        let today = study::today(&pool, at(3)).await.unwrap();
        assert_eq!(pairs(&today), ["en:doctor|nurse"], "twice is a habit");

        let pair = pair(&pool, "en:doctor|nurse").await.unwrap();
        assert_eq!(
            pair.words
                .iter()
                .map(|side| (side.lemma.as_str(), side.typed_for_the_other))
                .collect::<Vec<_>>(),
            [("doctor", 0), ("nurse", 2)]
        );
        assert_eq!(pair.words[0].gloss.as_deref(), Some("врач"), "the pack's gloss");
        assert_eq!(pair.questions.len(), 1, "one sentence, met in two turns, asked once");
        let question = &pair.questions[0];
        assert_eq!(
            (question.text.as_str(), question.start, question.length, question.answer.as_str()),
            ("He is a doctor.", 8, 6, "en:doctor")
        );
        assert_eq!(question.translation.as_deref(), Some("Он врач."));

        // Sentences met in a text: one that holds only the other word is
        // asked, its turn after the first; one that holds both cannot say
        // which is right, and is not.
        let library = crate::reading::Library::default();
        let text = crate::reading::tests::plain("Ward", "The nurse is here. The nurse saw a doctor.");
        let added = crate::reading::add(&pool, &library, &text, at(2)).await.unwrap();
        let read = crate::reading::read(&pool, &library, added.id, None).await.unwrap();
        let (only, both) = (read.lines[0][0].sentence, read.lines[0][1].sentence);
        crate::words::pick(&pool, "en:nurse", only, 4, at(2)).await.unwrap();
        crate::words::pick(&pool, "en:doctor", both, 16, at(2)).await.unwrap();
        let told: Vec<(String, String)> = super::pair(&pool, "en:doctor|nurse")
            .await
            .unwrap()
            .questions
            .into_iter()
            .map(|question| (question.text, question.answer))
            .collect();
        assert_eq!(
            told,
            [
                ("He is a doctor.".to_string(), "en:doctor".to_string()),
                ("The nurse is here.".to_string(), "en:nurse".to_string())
            ]
        );

        let chosen = Chosen {
            choices: vec![Choice {
                text: question.text.clone(),
                chosen: "en:doctor".to_string(),
            }],
            duration_ms: Some(3_000),
        };
        let reviewed = study::review_pair(&pool, "en:doctor|nurse", study::arriving(chosen), at(4)).await.unwrap();
        assert_eq!(reviewed.rating, Rating::Good);
        assert!(
            pairs(&study::today(&pool, at(5)).await.unwrap()).is_empty(),
            "answered, it comes back another day"
        );
    }

    #[test]
    fn a_choice_is_checked_by_the_sentence_it_was_made_for() {
        let choice = |text: &str, chosen: &str| Choice {
            text: text.to_string(),
            chosen: chosen.to_string(),
        };
        let (rating, right) = grade(
            "en:say|tell",
            &[choice("He told me the truth.", "en:tell"), choice("She said nothing.", "en:say")],
        )
        .unwrap();
        assert_eq!((rating, right), (Rating::Good, vec![true, true]));
        let (rating, _) = grade(
            "en:say|tell",
            &[
                choice("He told me the truth.", "en:say"),
                choice("She said nothing.", "en:say"),
                choice("Tell me.", "en:tell"),
            ],
        )
        .unwrap();
        assert_eq!(rating, Rating::Hard, "one wrong of three");
        let (rating, _) = grade("en:say|tell", &[choice("He told me.", "en:say"), choice("She said it.", "en:tell")]).unwrap();
        assert_eq!(rating, Rating::Again);
        assert_eq!(grade("en:say|tell", &[]), Err(Unchecked::Empty));
        assert!(matches!(
            grade("en:say|tell", &[choice("He told me.", "en:speak")]),
            Err(Unchecked::NotOfThePair(_))
        ));
        assert!(matches!(
            grade("en:say|tell", &[choice("He said he told me.", "en:say")]),
            Err(Unchecked::NoKey(_))
        ));
    }
}
