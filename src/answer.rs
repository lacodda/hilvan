//! A typed answer, checked against what the formula says.
//!
//! The learner types the sentence; the machine says whether it is right and,
//! when it is not, what went wrong. Three things can: the shape itself (the
//! "am" became "is", the "do" went missing), a word the shape carries (the
//! learner did not have "busy" for «занят»), or the order (every word there,
//! in the wrong places - the classic of a question). They are told apart
//! because they are mended apart: a slipped shape is the formula's failure,
//! and a missing word is the word's.
//!
//! Every variant the pack accepts - "I'd like" for "I would like" - is a
//! right answer, and a wrong answer is read against the variant closest to
//! it, so a learner who reached for the contraction is shown the contraction.
//! Case, punctuation and the kind of apostrophe do not matter; spelling
//! does - a misspelt word is a word that did not come.

use serde::Serialize;

use crate::say::{Marked, Role};

/// One word of a sentence, as compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// As compared: lower case, plain apostrophe, no punctuation at the edges.
    pub key: String,
    /// As it stands in the sentence.
    pub shown: String,
    /// Whether it is a word the formula carries rather than the formula's own.
    pub word: bool,
}

/// A right answer, split into what is compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub text: String,
    pub tokens: Vec<Token>,
}

impl Variant {
    /// A sentence the formula's templates rendered, with the roles they gave.
    #[must_use]
    pub fn from_marked(marked: &Marked) -> Self {
        let chars: Vec<(char, Role)> = marked.text.chars().zip(marked.roles.iter().copied()).collect();
        Self {
            text: marked.text.clone(),
            tokens: split(&chars),
        }
    }

    /// A fixed sentence, whose words are the ones the pack marked in it.
    #[must_use]
    pub fn from_sentence(text: &str, marked: &[&str]) -> Self {
        let words: Vec<String> = marked.iter().map(|form| key(form)).collect();
        let chars: Vec<(char, Role)> = text.chars().map(|c| (c, Role::Formula)).collect();
        let tokens = split(&chars)
            .into_iter()
            .map(|token| Token {
                word: words.contains(&token.key),
                ..token
            })
            .collect();
        Self {
            text: text.to_string(),
            tokens,
        }
    }
}

/// What went wrong, in the three ways the drill tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Failure {
    /// The formula's own pieces did not come.
    Formula,
    /// Every word came, in the wrong order.
    Order,
    /// The shape came; a word it carries did not.
    Word,
}

impl Failure {
    /// The word stored with the attempt.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Formula => "formula",
            Self::Order => "order",
            Self::Word => "word",
        }
    }

    /// Whether this is a failure of the shape, which the formula's card
    /// answers for. A word's failure is the word's.
    #[must_use]
    pub const fn is_the_shapes(self) -> bool {
        matches!(self, Self::Formula | Self::Order)
    }
}

/// One word of the right answer, and whether the typed answer had it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Expected {
    pub text: String,
    pub word: bool,
    pub came: bool,
}

/// What the check found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Checked {
    pub correct: bool,
    /// The answer the formula gives, as it is heard.
    pub answer: String,
    /// The right answer closest to what was typed: what it was read against.
    pub closest: String,
    /// The words of `closest`, each marked with whether it came.
    pub expected: Vec<Expected>,
    /// Words typed that the right answer does not have, as typed.
    pub extra: Vec<String>,
    /// `None` when the answer is right.
    pub failure: Option<Failure>,
    /// The words the shape carries that did not come, as they stand in
    /// `closest`: what goes to the deck of words.
    pub missed: Vec<String>,
}

/// Checks a typed answer against the right ones. The first variant is the
/// answer the formula gives; the others are accepted as well.
///
/// # Panics
///
/// Never on a formula from a loaded pack: there is always at least one
/// variant. An empty list is a caller's mistake and panics.
#[must_use]
pub fn check(variants: &[Variant], typed: &str) -> Checked {
    let answer = variants.first().expect("a formula always has an answer");
    let chars: Vec<(char, Role)> = typed.chars().map(|c| (c, Role::Formula)).collect();
    let typed = split(&chars);
    let typed_keys: Vec<&str> = typed.iter().map(|token| token.key.as_str()).collect();

    // Right if it is any right answer to the letter, once case and
    // punctuation are set aside.
    if let Some(right) = variants.iter().find(|variant| keys(variant) == typed_keys) {
        return Checked {
            correct: true,
            answer: answer.text.clone(),
            closest: right.text.clone(),
            expected: right
                .tokens
                .iter()
                .map(|token| Expected {
                    text: token.shown.clone(),
                    word: token.word,
                    came: true,
                })
                .collect(),
            extra: Vec::new(),
            failure: None,
            missed: Vec::new(),
        };
    }

    // Read against the right answer it shares the most with, the formula's
    // own on a tie.
    let (closest, pairs) = variants
        .iter()
        .map(|variant| (variant, common(&keys(variant), &typed_keys)))
        .fold(None, |best: Option<(&Variant, Vec<(usize, usize)>)>, (variant, pairs)| match best {
            Some((_, ref kept)) if kept.len() >= pairs.len() => best,
            _ => Some((variant, pairs)),
        })
        .expect("there is at least one variant");

    let came: Vec<bool> = (0..closest.tokens.len()).map(|at| pairs.iter().any(|(e, _)| *e == at)).collect();
    let used: Vec<bool> = (0..typed.len()).map(|at| pairs.iter().any(|(_, t)| *t == at)).collect();
    let missing: Vec<&Token> = closest.tokens.iter().zip(&came).filter(|(_, came)| !**came).map(|(token, _)| token).collect();

    let mut sorted_expected = keys(closest);
    let mut sorted_typed = typed_keys.clone();
    sorted_expected.sort_unstable();
    sorted_typed.sort_unstable();

    let failure = if sorted_expected == sorted_typed {
        Failure::Order
    } else if !missing.is_empty() && missing.iter().all(|token| token.word) {
        Failure::Word
    } else {
        // A piece of the shape missing - or nothing missing and something
        // added, which is a sentence the shape does not make.
        Failure::Formula
    };

    Checked {
        correct: false,
        answer: answer.text.clone(),
        closest: closest.text.clone(),
        expected: closest
            .tokens
            .iter()
            .zip(&came)
            .map(|(token, came)| Expected {
                text: token.shown.clone(),
                word: token.word,
                came: *came,
            })
            .collect(),
        extra: typed
            .iter()
            .zip(&used)
            .filter(|(_, used)| !**used)
            .map(|(token, _)| token.shown.clone())
            .collect(),
        missed: if failure == Failure::Order {
            Vec::new()
        } else {
            missing.iter().filter(|token| token.word).map(|token| token.shown.clone()).collect()
        },
        failure: Some(failure),
    }
}

fn keys(variant: &Variant) -> Vec<&str> {
    variant.tokens.iter().map(|token| token.key.as_str()).collect()
}

/// The longest run of words the two share in order, as pairs of positions
/// (expected, typed). Sentences are a dozen words; the square table is
/// nothing.
fn common(expected: &[&str], typed: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (expected.len(), typed.len());
    let mut table = vec![vec![0_usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if expected[i] == typed[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut pairs = Vec::with_capacity(table[0][0]);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if expected[i] == typed[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

/// Whether a character joins words rather than parting them: letters,
/// digits, the apostrophe of "don't" and the hyphen of "twenty-one".
fn joins(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '`' | '\u{00b4}' | '-')
}

/// A word as compared: lower case, one apostrophe, nothing at the edges.
fn key(text: &str) -> String {
    text.chars()
        .map(|c| if matches!(c, '\u{2019}' | '\u{2018}' | '`' | '\u{00b4}') { '\'' } else { c })
        .collect::<String>()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// Splits characters into words, each a word of the formula's only when all
/// of its letters came from one.
fn split(chars: &[(char, Role)]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current: Vec<(char, Role)> = Vec::new();
    let mut flush = |current: &mut Vec<(char, Role)>| {
        let shown: String = current.iter().map(|(c, _)| *c).collect();
        let key = key(&shown);
        if !key.is_empty() {
            let word = current.iter().filter(|(c, _)| c.is_alphanumeric()).all(|(_, role)| *role == Role::Word);
            tokens.push(Token {
                key,
                shown: shown.trim_matches(|c: char| !c.is_alphanumeric()).to_string(),
                word,
            });
        }
        current.clear();
    };
    for &(c, role) in chars {
        if joins(c) {
            current.push((c, role));
        } else {
            flush(&mut current);
        }
    }
    flush(&mut current);
    tokens
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::*;
    use crate::say::{Filling, Template};

    struct Value(&'static str, &'static [(&'static str, &'static str)]);

    impl Filling for Value {
        fn word(&self) -> &str {
            self.0
        }
        fn form(&self, name: &str) -> Option<&str> {
            self.1.iter().find(|(form, _)| *form == name).map(|(_, text)| *text)
        }
    }

    const SHE: Value = Value("she", &[("be", "is"), ("be-short", "she's")]);
    const BUSY: Value = Value("busy", &[]);

    /// "She is busy." and its contraction, as the formula renders them.
    fn she_is_busy() -> Vec<Variant> {
        let chosen = HashMap::from([("pronoun", &SHE), ("rest", &BUSY)]);
        let agreeing = BTreeSet::from(["pronoun"]);
        ["<pronoun> <pronoun:be> <rest>.", "<pronoun:be-short> <rest>."]
            .iter()
            .map(|say| Variant::from_marked(&Template::parse(say).unwrap().render_marked(&chosen, &agreeing).unwrap()))
            .collect()
    }

    #[test]
    fn the_answer_is_right_whatever_its_case_punctuation_or_apostrophe() {
        for typed in ["She is busy.", "she is busy", "  SHE   is busy!  ", "She’s busy", "she's busy."] {
            let checked = check(&she_is_busy(), typed);
            assert!(checked.correct, "{typed:?} should be right");
            assert_eq!(checked.failure, None);
        }
    }

    #[test]
    fn an_accepted_variant_is_shown_as_what_was_reached_for() {
        let checked = check(&she_is_busy(), "She's busy");
        assert_eq!(checked.closest, "She's busy.");
        assert_eq!(checked.answer, "She is busy.", "the formula's own answer is what is heard");
    }

    #[test]
    fn a_slip_in_the_shape_is_the_formulas() {
        let checked = check(&she_is_busy(), "She are busy");
        assert!(!checked.correct);
        assert_eq!(checked.failure, Some(Failure::Formula));
        assert_eq!(checked.extra, vec!["are"]);
        let came: Vec<_> = checked.expected.iter().map(|word| (word.text.as_str(), word.came)).collect();
        assert_eq!(came, vec![("She", true), ("is", false), ("busy", true)]);
        assert!(checked.missed.is_empty(), "no word the shape carries went missing");
    }

    #[test]
    fn a_word_that_did_not_come_is_the_words() {
        // "занят" was not there: the shape came whole.
        let checked = check(&she_is_busy(), "She is bisy.");
        assert_eq!(checked.failure, Some(Failure::Word));
        assert_eq!(checked.missed, vec!["busy"]);
        assert_eq!(checked.extra, vec!["bisy"]);
    }

    #[test]
    fn the_right_words_in_the_wrong_order_are_the_orders() {
        let checked = check(&she_is_busy(), "Is she busy?");
        assert_eq!(checked.failure, Some(Failure::Order));
        assert_eq!(checked.missed, Vec::<String>::new());
    }

    #[test]
    fn a_word_added_to_a_sentence_the_shape_does_not_make_is_the_formulas() {
        let checked = check(&she_is_busy(), "She is is busy");
        assert_eq!(checked.failure, Some(Failure::Formula));
        assert_eq!(checked.extra, vec!["is"]);
    }

    #[test]
    fn nothing_typed_is_the_shape_not_coming() {
        let checked = check(&she_is_busy(), "");
        assert_eq!(checked.failure, Some(Failure::Formula));
        assert!(checked.expected.iter().all(|word| !word.came));
    }

    #[test]
    fn a_pronoun_the_formula_agrees_with_is_the_formulas_own() {
        // "he" instead of "she": the pronoun picks the form, so it belongs to
        // the shape rather than to the words it carries.
        let checked = check(&she_is_busy(), "He is busy.");
        assert_eq!(checked.failure, Some(Failure::Formula));
    }

    #[test]
    fn a_wrong_answer_is_read_against_the_variant_it_reached_for() {
        // "She's bisy" shares two words with the contraction and one with the
        // full form: it is read against the contraction.
        let checked = check(&she_is_busy(), "She's bisy");
        assert_eq!(checked.closest, "She's busy.");
        assert_eq!(checked.failure, Some(Failure::Word));
    }

    #[test]
    fn a_fixed_sentence_takes_its_words_from_the_marks() {
        let variants = [
            Variant::from_sentence("I'm not hungry.", &["hungry"]),
            Variant::from_sentence("I am not hungry.", &["hungry"]),
        ];
        assert!(check(&variants, "I am not hungry").correct);
        assert_eq!(check(&variants, "I'm not angry").failure, Some(Failure::Word));
        assert_eq!(check(&variants, "I don't hungry").failure, Some(Failure::Formula));
    }

    #[test]
    fn punctuation_inside_a_sentence_parts_words() {
        let variants = [Variant::from_sentence("I'd like a coffee, please.", &["coffee", "please"])];
        assert!(check(&variants, "I'd like a coffee please").correct);
        assert!(check(&variants, "I'd like a coffee,please").correct);
        let checked = check(&variants, "I'd like a coffee");
        assert_eq!(checked.failure, Some(Failure::Word));
        assert_eq!(checked.missed, vec!["please"]);
    }
}
