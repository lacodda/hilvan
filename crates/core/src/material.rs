//! A formula's material as the drill reads it: its sentences, and the values
//! its slots are filled from.
//!
//! The server sends these inside a formula and the app sends them back to
//! the checker built from this crate, so they read and write the same JSON.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::say::Filling;

/// A worked example of a formula: a sentence, and what it means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    /// The sentence's id: the same sentence shown by another formula, or
    /// heard as a word's anchor, is the same id.
    pub sentence: i64,
    /// The prompt, in the learner's native language.
    pub native: String,
    /// The answer, in the language being learnt.
    pub target: String,
    /// The words the sentence teaches, where they stand in `target`.
    pub words: Vec<Mark>,
    /// Other spellings of `target` just as right: "I am not hungry." for
    /// "I'm not hungry.".
    pub accept: Vec<String>,
}

/// A word marked in a sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    /// The word's id, `en:doctor`.
    pub word: String,
    pub lemma: String,
    /// How it is spelt here: "went" for go.
    pub form: String,
    /// The character it starts at.
    pub start: i64,
}

/// A hole in the pattern, with what can go in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub name: String,
    /// Which kind of the learner's own words the slot takes, when it takes
    /// any: they stand among its values, marked `own`.
    pub takes: Option<OwnKind>,
    pub values: Vec<Value>,
}

/// One filling for a slot, with the forms that agree with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Value {
    pub native: String,
    pub target: String,
    /// What `say` picks from for agreement: `be` = "is" on "he".
    pub forms: BTreeMap<String, String>,
    /// Whether this is one of the learner's own words rather than the pack's.
    pub own: bool,
}

impl Filling for Value {
    fn word(&self) -> &str {
        &self.target
    }

    fn form(&self, name: &str) -> Option<&str> {
        self.forms.get(name).map(String::as_str)
    }
}

/// What kind of word of the learner's own a slot takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnKind {
    Noun,
    Verb,
}

impl OwnKind {
    /// The word stored in the slot and own-word rows.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Noun => "noun",
            Self::Verb => "verb",
        }
    }

    /// Reads a kind back from a row.
    ///
    /// # Errors
    ///
    /// Fails when the word is neither kind.
    pub fn parse(value: &str) -> Result<Self, UnknownKind> {
        match value {
            "noun" => Ok(Self::Noun),
            "verb" => Ok(Self::Verb),
            other => Err(UnknownKind(other.to_string())),
        }
    }
}

/// A kind of own word that is neither a noun nor a verb.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a kind of word: expected noun or verb")]
pub struct UnknownKind(String);
