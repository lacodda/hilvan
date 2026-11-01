//! How much of a text is the learner's own.
//!
//! A text is read as running words, and each running word is one or more
//! lemmas: *went* is *go*, *don't* is *do* and *not*, and *left* is either
//! *leave* or *left* - the lexicon cannot tell which without understanding
//! the sentence. Coverage is the share of running words the learner holds:
//! a word is held when every part of it is, and a part is held when any of
//! the lemmas it can be is.
//!
//! The server counts a whole library with this; the reader on the phone
//! counts the text it shows again after every tap, offline or not. Both run
//! the same arithmetic, so the number on the list is the number on the page.

use serde::{Deserialize, Serialize};

/// Where the learner stands with one lemma, as its card says.
///
/// Ordered from the least held to the most: a part that can be two lemmas
/// takes the better of the two, a word of two parts the worse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hold {
    /// In the deck and not started: met in a sentence, or put there from a
    /// text, and waiting for its first answer.
    Deck,
    /// Being learnt: answered, and still coming back within days.
    Basted,
    /// Held: the schedule trusts it for weeks - including a word declared
    /// known.
    Sewn,
}

/// One running word of a text, as the lexicon read it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Piece {
    /// The words it is made of - one for most, two for *don't* - each the
    /// lemmas it can be, as indices into the text's list of lemmas. Empty
    /// for a spelling the lexicon has never met.
    pub parts: Vec<Vec<u32>>,
    /// Whether it is a name: *London*, *Harry*. Names are not words of the
    /// language and are not counted.
    #[serde(default)]
    pub name: bool,
}

/// How one running word stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Standing {
    /// A name: left out of the count.
    Name,
    /// A spelling past the lexicon: counted, and never the learner's.
    Beyond,
    /// A word of the lexicon the learner has no card for, or none started.
    Unknown,
    Deck,
    Basted,
    Sewn,
}

impl From<Hold> for Standing {
    fn from(hold: Hold) -> Self {
        match hold {
            Hold::Deck => Self::Deck,
            Hold::Basted => Self::Basted,
            Hold::Sewn => Self::Sewn,
        }
    }
}

/// How a text stands, counted in running words.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    /// Running words counted: every word but names.
    pub words: u32,
    /// Held for weeks.
    pub sewn: u32,
    /// Being learnt.
    pub basted: u32,
    /// In the deck, not started.
    pub deck: u32,
    /// Words of the lexicon not in hand.
    pub unknown: u32,
    /// Spellings the lexicon does not know.
    pub beyond: u32,
    /// Names, not counted in `words`.
    pub names: u32,
}

impl Coverage {
    /// Running words in hand: held or being learnt. What "how much of it is
    /// yours" counts.
    #[must_use]
    pub const fn yours(&self) -> u32 {
        self.sewn + self.basted
    }

    /// Adds one running word.
    pub const fn add(&mut self, standing: Standing) {
        if matches!(standing, Standing::Name) {
            self.names += 1;
            return;
        }
        self.words += 1;
        match standing {
            Standing::Name => {}
            Standing::Beyond => self.beyond += 1,
            Standing::Unknown => self.unknown += 1,
            Standing::Deck => self.deck += 1,
            Standing::Basted => self.basted += 1,
            Standing::Sewn => self.sewn += 1,
        }
    }

    /// Adds another count to this one: a chapter to a book, a text to a
    /// shelf.
    pub const fn merge(&mut self, other: &Self) {
        self.words += other.words;
        self.sewn += other.sewn;
        self.basted += other.basted;
        self.deck += other.deck;
        self.unknown += other.unknown;
        self.beyond += other.beyond;
        self.names += other.names;
    }
}

/// How one running word stands, given what the learner holds of each lemma
/// of the text (`holds`, by the same indices as the piece's parts).
///
/// An index past the end of `holds` is a lemma the learner holds nothing of.
#[must_use]
pub fn standing(piece: &Piece, holds: &[Option<Hold>]) -> Standing {
    if piece.name {
        return Standing::Name;
    }
    if piece.parts.is_empty() || piece.parts.iter().any(Vec::is_empty) {
        return Standing::Beyond;
    }
    let held = |part: &Vec<u32>| {
        part.iter()
            .filter_map(|&index| holds.get(usize::try_from(index).ok()?).copied().flatten())
            .max()
    };
    // The best reading of each part, the worst part of the word: "don't" is
    // as much the learner's as the less held of "do" and "not".
    let mut worst: Option<Hold> = Some(Hold::Sewn);
    for part in &piece.parts {
        worst = worst.min(held(part));
    }
    worst.map_or(Standing::Unknown, Standing::from)
}

/// Every running word of a text and how it stands, and the count of them.
#[must_use]
pub fn cover(pieces: &[Piece], holds: &[Option<Hold>]) -> (Coverage, Vec<Standing>) {
    let mut coverage = Coverage::default();
    let standings = pieces
        .iter()
        .map(|piece| {
            let standing = standing(piece, holds);
            coverage.add(standing);
            standing
        })
        .collect();
    (coverage, standings)
}

/// The count of a text kept as its distinct running words and how often
/// each stands in it: a novel of a hundred thousand words is a few thousand
/// pieces. `tally` holds `(piece, times)`, the piece an index into `pieces`;
/// an index past the end counts nothing.
///
/// The same arithmetic as [`cover`] - each piece stands as [`standing`]
/// says - weighed by how often it is met.
#[must_use]
pub fn weigh(pieces: &[Piece], tally: &[(u32, u32)], holds: &[Option<Hold>]) -> Coverage {
    let mut coverage = Coverage::default();
    for &(index, times) in tally {
        let Some(piece) = usize::try_from(index).ok().and_then(|index| pieces.get(index)) else {
            continue;
        };
        let mut one = Coverage::default();
        one.add(standing(piece, holds));
        coverage.merge(&one.times(times));
    }
    coverage
}

impl Coverage {
    /// This count, `times` over.
    #[must_use]
    pub const fn times(&self, times: u32) -> Self {
        Self {
            words: self.words.saturating_mul(times),
            sewn: self.sewn.saturating_mul(times),
            basted: self.basted.saturating_mul(times),
            deck: self.deck.saturating_mul(times),
            unknown: self.unknown.saturating_mul(times),
            beyond: self.beyond.saturating_mul(times),
            names: self.names.saturating_mul(times),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(parts: &[&[u32]]) -> Piece {
        Piece {
            parts: parts.iter().map(|part| part.to_vec()).collect(),
            name: false,
        }
    }

    #[test]
    fn a_word_stands_as_its_lemma_does() {
        let holds = [Some(Hold::Sewn), Some(Hold::Basted), Some(Hold::Deck), None];
        assert_eq!(standing(&piece(&[&[0]]), &holds), Standing::Sewn);
        assert_eq!(standing(&piece(&[&[1]]), &holds), Standing::Basted);
        assert_eq!(standing(&piece(&[&[2]]), &holds), Standing::Deck);
        assert_eq!(standing(&piece(&[&[3]]), &holds), Standing::Unknown);
        assert_eq!(standing(&piece(&[&[9]]), &holds), Standing::Unknown, "a lemma never listed is not held");
    }

    #[test]
    fn a_spelling_of_two_lemmas_is_as_held_as_the_better_one() {
        // "left": the learner who knows "leave" reads it.
        let holds = [None, Some(Hold::Sewn)];
        assert_eq!(standing(&piece(&[&[0, 1]]), &holds), Standing::Sewn);
        assert_eq!(standing(&piece(&[&[1, 0]]), &holds), Standing::Sewn, "whichever comes first");
        // Both held, one better than the other: the better one is how it reads.
        let both = [Some(Hold::Deck), Some(Hold::Basted)];
        assert_eq!(standing(&piece(&[&[0, 1]]), &both), Standing::Basted);
    }

    #[test]
    fn a_word_of_two_parts_is_as_held_as_the_weaker_one() {
        // "don't" for a learner who has "do" and not yet "not".
        let holds = [Some(Hold::Sewn), Some(Hold::Basted), None];
        assert_eq!(standing(&piece(&[&[0], &[1]]), &holds), Standing::Basted);
        assert_eq!(standing(&piece(&[&[0], &[2]]), &holds), Standing::Unknown);
    }

    #[test]
    fn a_spelling_past_the_lexicon_counts_and_is_never_held() {
        let holds = [Some(Hold::Sewn)];
        assert_eq!(standing(&piece(&[]), &holds), Standing::Beyond);
        assert_eq!(standing(&piece(&[&[0], &[]]), &holds), Standing::Beyond, "a part nobody can read");
    }

    #[test]
    fn names_are_left_out_of_the_count() {
        let pieces = [Piece { parts: vec![], name: true }, piece(&[&[0]]), piece(&[&[1]]), piece(&[])];
        let (coverage, standings) = cover(&pieces, &[Some(Hold::Sewn), Some(Hold::Deck)]);
        assert_eq!(standings, vec![Standing::Name, Standing::Sewn, Standing::Deck, Standing::Beyond]);
        assert_eq!(
            coverage,
            Coverage {
                words: 3,
                sewn: 1,
                basted: 0,
                deck: 1,
                unknown: 0,
                beyond: 1,
                names: 1
            }
        );
        assert_eq!(coverage.yours(), 1, "a word in the deck is not in hand yet");
    }

    #[test]
    fn a_tally_of_distinct_pieces_counts_as_the_running_words_do() {
        let pieces = [
            piece(&[&[0]]),
            piece(&[&[1]]),
            Piece { parts: vec![], name: true },
            piece(&[]),
            piece(&[&[0], &[2]]),
        ];
        let holds = [Some(Hold::Sewn), Some(Hold::Deck), Some(Hold::Basted)];
        // The text the tally stands for, word by word.
        let running: Vec<Piece> = [0, 0, 0, 1, 2, 3, 3, 4, 1, 0].iter().map(|&at: &usize| pieces[at].clone()).collect();
        let (whole, _) = cover(&running, &holds);
        let tally = [(0, 4), (1, 2), (2, 1), (3, 2), (4, 1)];
        assert_eq!(weigh(&pieces, &tally, &holds), whole);
        assert_eq!(
            weigh(&pieces, &[(0, 4), (9, 7)], &holds),
            Coverage {
                words: 4,
                sewn: 4,
                ..Coverage::default()
            },
            "a piece nobody listed counts nothing"
        );
    }

    #[test]
    fn counts_add_up() {
        let (mut one, _) = cover(&[piece(&[&[0]]), piece(&[&[1]])], &[Some(Hold::Basted)]);
        let (two, _) = cover(&[piece(&[&[0]])], &[Some(Hold::Sewn)]);
        one.merge(&two);
        assert_eq!((one.words, one.yours(), one.unknown), (3, 2, 1));
    }
}
