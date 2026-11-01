//! The shelf: open texts to read next - books of Project Gutenberg and
//! articles of the Simple English Wikipedia - each with how hard it is, and
//! what the reader picks to read next.
//!
//! The shelf is data built into the binary, like the lexicon: a catalogue
//! written by hand (`shelves/en/catalogue.toml`) and a profile of every item
//! measured from its text (`shelves/en/profiles.tsv`): how many of its
//! running words fall in each band of the frequency list. No text is
//! stored; a profile says enough to tell a level - the lexicon a reader
//! needs for 95% of it - and to estimate how much of it is the learner's
//! from what they hold of each band, before it is taken. Once taken, it is
//! a text of the learner's and is counted word by word.
//!
//! The profiles are measured by `cargo test --test shelf -- --ignored`,
//! which fetches every item through the reader's own import and counts it
//! with the lexicon; a test holds them to the lexicon they were measured
//! with.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::LazyLock;

use anyhow::{Context as _, Result, bail, ensure};
use hilvan_core::cover::{Hold, Piece};
use serde::{Deserialize, Serialize};

use crate::import::Document;
use crate::lexicon::{self, Lexicon};
use crate::reading::{Summary, analyse};

/// The upper ranks of the bands a profile counts running words in: the
/// commonest hundred lemmas, the next hundred, and so on, finer at the top
/// of the list, where most running words are.
pub const BANDS: [u32; 14] = [100, 200, 300, 500, 750, 1000, 1500, 2000, 3000, 4000, 5000, 6500, 8000, 10_000];

/// The share of running words a reader has to know to read a text on
/// their own, in percent: the bottom of the window [`next`] aims at.
pub const WINDOW_LOW: u32 = 95;

/// The top of the window: past it a text teaches little.
pub const WINDOW_HIGH: u32 = 98;

/// The levels a shelf item is placed at: the size of the lexicon it needs.
const SIZES: [u32; 6] = [1000, 2000, 3000, 5000, 8000, 10_000];

/// What a shelf item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A book, in chapters.
    Book,
    /// An article of an encyclopedia.
    Article,
}

/// One item of the catalogue, as written by hand.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub author: Option<String>,
    /// The link it is fetched by: a book's page at Gutenberg, an article's.
    pub source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalogue {
    language: String,
    item: Vec<Entry>,
}

/// How a text's running words fall in the bands of the frequency list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    /// Running words counted: names are not.
    pub words: u32,
    /// Spellings past the lexicon: never a learner's.
    pub beyond: u32,
    /// Running words in each of [`BANDS`].
    pub bands: [u32; BANDS.len()],
}

/// The band of a rank.
fn band(rank: u32) -> Option<usize> {
    BANDS.iter().position(|&bound| rank <= bound)
}

impl Profile {
    /// The profile of a text taken apart.
    ///
    /// A running word falls in the band of its rarest part, each part read
    /// as its commonest lemma - *left* as *leave* - as the reader's "I know
    /// the rest" reads it. Names are left out, as coverage leaves them.
    #[must_use]
    pub fn of(language: &str, document: &Document) -> Self {
        let sentences: Vec<&str> = document
            .chapters
            .iter()
            .flat_map(|chapter| chapter.paragraphs.iter().flatten())
            .map(String::as_str)
            .collect();
        let analysis = analyse(language, &sentences);
        let lexicon = lexicon::of(language);
        let rank = |id: &str| -> Option<u32> {
            let lemma = id.split_once(':').map_or(id, |(_, lemma)| lemma);
            lexicon.and_then(|lexicon| lexicon.entry(lemma)).map(|entry| entry.rank)
        };
        let mut profile = Self::default();
        for Piece { parts, name } in analysis.pieces() {
            if *name {
                continue;
            }
            profile.words += 1;
            let ranks: Option<Vec<u32>> = if parts.is_empty() {
                None
            } else {
                parts
                    .iter()
                    .map(|part| part.iter().filter_map(|&index| rank(&analysis.lemmas[index as usize])).min())
                    .collect()
            };
            match ranks.and_then(|ranks| ranks.into_iter().max()).and_then(band) {
                Some(at) => profile.bands[at] += 1,
                None => profile.beyond += 1,
            }
        }
        profile
    }

    /// The size of lexicon a reader needs for [`WINDOW_LOW`] percent of the
    /// text; `None` past the whole lexicon.
    #[must_use]
    pub fn level(&self) -> Option<u32> {
        if self.words == 0 {
            return None;
        }
        SIZES.into_iter().find(|&size| {
            let within: u32 = BANDS.iter().zip(self.bands).filter(|(bound, _)| **bound <= size).map(|(_, count)| count).sum();
            u64::from(within) * 100 >= u64::from(self.words) * u64::from(WINDOW_LOW)
        })
    }

    /// How much of the text is the learner's, in whole percent, from the
    /// share of each band they hold: `known[band]` in `0.0..=1.0`.
    #[must_use]
    pub fn estimate(&self, known: &[f64; BANDS.len()]) -> u32 {
        if self.words == 0 {
            return 0;
        }
        let held: f64 = self
            .bands
            .iter()
            .zip(known)
            .map(|(&count, share)| f64::from(count) * share.clamp(0.0, 1.0))
            .sum();
        whole_percent(100.0 * held / f64::from(self.words))
    }

    fn line(&self, id: &str) -> String {
        let bands: Vec<String> = self.bands.iter().map(u32::to_string).collect();
        format!("{id}\t{}\t{}\t{}", self.words, self.beyond, bands.join("\t"))
    }

    fn parse(line: &str) -> Result<(String, Self)> {
        let fields: Vec<&str> = line.split('\t').collect();
        ensure!(
            fields.len() == 3 + BANDS.len(),
            "a profile has {} fields, not {}: {line:?}",
            3 + BANDS.len(),
            fields.len()
        );
        let number = |field: &str| field.parse::<u32>().with_context(|| format!("{field:?} is not a count"));
        let mut bands = [0; BANDS.len()];
        for (slot, field) in bands.iter_mut().zip(&fields[3..]) {
            *slot = number(field)?;
        }
        let profile = Self {
            words: number(fields[1])?,
            beyond: number(fields[2])?,
            bands,
        };
        ensure!(
            profile.bands.iter().sum::<u32>() + profile.beyond == profile.words,
            "the profile of {} does not add up",
            fields[0]
        );
        Ok((fields[0].to_string(), profile))
    }
}

/// A share in percent, rounded down to a whole number from 0 to 100.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole_percent(value: f64) -> u32 {
    // Clamped to 0..=100 first: nothing to truncate, no sign to lose.
    value.floor().clamp(0.0, 100.0) as u32
}

/// The header line of a profiles file: what they were measured with.
fn header(fingerprint: &str) -> String {
    let bands: Vec<String> = BANDS.iter().map(u32::to_string).collect();
    format!("# lexicon {fingerprint} bands {}", bands.join(","))
}

/// The language and the items of a catalogue, before anything is measured.
///
/// # Errors
///
/// Fails on a malformed catalogue.
pub fn catalogue(text: &str) -> Result<(String, Vec<Entry>)> {
    let catalogue: Catalogue = toml::from_str(text).context("the shelf's catalogue is malformed")?;
    Ok((catalogue.language, catalogue.item))
}

/// The shelf of a language: its catalogue and the measured profiles.
#[derive(Debug, Clone)]
pub struct Shelf {
    pub language: String,
    pub items: Vec<(Entry, Profile)>,
}

impl Shelf {
    /// Reads a catalogue and its profiles, holding them to each other and to
    /// the lexicon of the language.
    ///
    /// # Errors
    ///
    /// Fails on a malformed file, an item listed twice, an item without a
    /// profile or a profile without an item, and profiles measured with
    /// another lexicon or other bands.
    pub fn parse(catalogue: &str, profiles: &str) -> Result<Self> {
        let catalogue: Catalogue = toml::from_str(catalogue).context("the shelf's catalogue is malformed")?;
        let fingerprint = lexicon::fingerprint(&catalogue.language).with_context(|| format!("there is no lexicon for {:?}", catalogue.language))?;
        let mut lines = profiles.lines().filter(|line| !line.trim().is_empty());
        let first = lines.next().unwrap_or_default();
        ensure!(
            first == header(&fingerprint),
            "the shelf's profiles were measured with another lexicon or other bands ({first:?}): measure them again with `cargo test --test shelf -- --ignored`"
        );
        let mut measured: HashMap<String, Profile> = HashMap::new();
        for line in lines.filter(|line| !line.starts_with('#')) {
            let (id, profile) = Profile::parse(line)?;
            ensure!(measured.insert(id.clone(), profile).is_none(), "{id} is measured twice");
        }
        let mut items = Vec::with_capacity(catalogue.item.len());
        for entry in catalogue.item {
            ensure!(
                !items.iter().any(|(listed, _): &(Entry, Profile)| listed.id == entry.id),
                "{} is listed twice",
                entry.id
            );
            let Some(profile) = measured.remove(&entry.id) else {
                bail!("{} has no profile: measure the shelf with `cargo test --test shelf -- --ignored`", entry.id)
            };
            items.push((entry, profile));
        }
        if let Some(id) = measured.keys().next() {
            bail!("{id} is measured and not in the catalogue");
        }
        Ok(Self {
            language: catalogue.language,
            items,
        })
    }

    /// The profiles file for some measured items.
    #[must_use]
    pub fn profiles(language: &str, measured: &[(String, Profile)]) -> String {
        let mut text = header(&lexicon::fingerprint(language).unwrap_or_default());
        text.push('\n');
        text.push_str("# id\twords\tbeyond\tby band\n");
        for (id, profile) in measured {
            text.push_str(&profile.line(id));
            text.push('\n');
        }
        text
    }
}

/// The English catalogue, as written.
pub const ENGLISH_CATALOGUE: &str = include_str!("../shelves/en/catalogue.toml");

static ENGLISH: LazyLock<Shelf> = LazyLock::new(|| {
    // Built in, and parsed by a test on every gate run.
    Shelf::parse(ENGLISH_CATALOGUE, include_str!("../shelves/en/profiles.tsv")).unwrap_or_else(|error| panic!("the English shelf is malformed: {error:#}"))
});

/// The shelf of a language, when hilvan has one.
#[must_use]
pub fn of(language: &str) -> Option<&'static Shelf> {
    match language {
        "en" => Some(&ENGLISH),
        _ => None,
    }
}

/// The share of each band's lemmas the learner holds: what an estimate is
/// made from.
#[must_use]
pub fn known<S: BuildHasher>(lexicon: &Lexicon, language: &str, holds: &HashMap<String, Hold, S>) -> [f64; BANDS.len()] {
    let mut lemmas = [0u32; BANDS.len()];
    let mut held = [0u32; BANDS.len()];
    for entry in lexicon.entries() {
        let Some(at) = band(entry.rank) else { continue };
        lemmas[at] += 1;
        let id = crate::pack::word_id(language, &entry.lemma);
        if matches!(holds.get(&id), Some(Hold::Sewn | Hold::Basted)) {
            held[at] += 1;
        }
    }
    let mut known = [0.0; BANDS.len()];
    for (at, share) in known.iter_mut().enumerate() {
        if lemmas[at] > 0 {
            *share = f64::from(held[at]) / f64::from(lemmas[at]);
        }
    }
    known
}

/// A shelf item as the shelf shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub author: Option<String>,
    pub source: String,
    /// Running words, names left out.
    pub words: u32,
    /// The lexicon it needs for 95%; `None` past the whole lexicon.
    pub level: Option<u32>,
    /// About how much of it is the learner's, in percent.
    pub estimate: u32,
    /// The learner's text it became, once taken.
    pub taken: Option<i64>,
}

/// The shelf for the learner: every item with its estimate, and the text it
/// became when taken.
#[must_use]
pub fn items<S: BuildHasher>(shelf: &Shelf, holds: &HashMap<String, Hold, S>, texts: &[Summary]) -> Vec<Item> {
    let Some(lexicon) = lexicon::of(&shelf.language) else {
        return Vec::new();
    };
    let known = known(lexicon, &shelf.language, holds);
    shelf
        .items
        .iter()
        .map(|(entry, profile)| Item {
            id: entry.id.clone(),
            kind: entry.kind,
            title: entry.title.clone(),
            author: entry.author.clone(),
            source: entry.source.clone(),
            words: profile.words,
            level: profile.level(),
            estimate: profile.estimate(&known),
            taken: texts
                .iter()
                .find(|text| text.source.as_deref() == Some(entry.source.as_str()))
                .map(|text| text.id),
        })
        .collect()
}

/// What to read next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Next {
    /// A text of the learner's, at the chapter they stopped in.
    Text {
        id: i64,
        title: String,
        /// The chapter, for a book.
        chapter: Option<u32>,
        chapter_title: Option<String>,
        /// How much of it is the learner's, in whole percent.
        yours: u32,
        started: bool,
    },
    /// An item of the shelf, not taken yet.
    Shelf {
        id: String,
        title: String,
        author: Option<String>,
        /// About how much of it is the learner's.
        estimate: u32,
    },
}

impl Next {
    fn share(&self) -> u32 {
        match self {
            Self::Text { yours, .. } => *yours,
            Self::Shelf { estimate, .. } => *estimate,
        }
    }

    /// The order of preference between equally fitting picks: a text being
    /// read before one not opened, a text of the learner's before the
    /// shelf.
    const fn rank(&self) -> u8 {
        match self {
            Self::Text { started: true, .. } => 0,
            Self::Text { started: false, .. } => 1,
            Self::Shelf { .. } => 2,
        }
    }
}

/// How far a share is from the window: nothing inside it, the distance
/// above it, twice the distance below - a text a little too easy reads, one
/// a little too hard is work.
#[must_use]
pub const fn distance(share: u32) -> u32 {
    // At most one of the two is more than nothing.
    2 * WINDOW_LOW.saturating_sub(share) + share.saturating_sub(WINDOW_HIGH)
}

/// What to read next: the unfinished text - at the chapter the learner is
/// in - or the shelf item not taken yet, whose share of the learner's words
/// stands nearest the window of 95 to 98 percent. Between equals, the text
/// being read, then a text of the learner's, then the one nearest the
/// middle of the window.
#[must_use]
pub fn next(texts: &[Summary], items: &[Item]) -> Option<Next> {
    let percent = |yours: u32, words: u32| {
        if words == 0 {
            None
        } else {
            Some(u32::try_from(u64::from(yours) * 100 / u64::from(words)).unwrap_or(100))
        }
    };
    let mut candidates: Vec<Next> = texts
        .iter()
        .filter(|text| text.finished_at.is_none())
        .filter_map(|text| {
            let chapter = &text.chapter;
            Some(Next::Text {
                id: text.id,
                title: text.title.clone(),
                chapter: (text.chapters > 1).then_some(chapter.position),
                chapter_title: if text.chapters > 1 { chapter.title.clone() } else { None },
                yours: percent(chapter.coverage.yours(), chapter.coverage.words)?,
                started: text.opened_at.is_some(),
            })
        })
        .collect();
    candidates.extend(items.iter().filter(|item| item.taken.is_none() && item.words > 0).map(|item| Next::Shelf {
        id: item.id.clone(),
        title: item.title.clone(),
        author: item.author.clone(),
        estimate: item.estimate,
    }));
    let middle = u32::midpoint(WINDOW_LOW, WINDOW_HIGH);
    candidates
        .into_iter()
        .min_by_key(|candidate| (distance(candidate.share()), candidate.rank(), candidate.share().abs_diff(middle)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Chapter;
    use crate::reading::ChapterSummary;
    use chrono::Utc;
    use hilvan_core::cover::Coverage;

    fn document(sentences: &[&str]) -> Document {
        Document {
            title: None,
            author: None,
            chapters: vec![Chapter {
                title: None,
                paragraphs: vec![sentences.iter().map(|sentence| (*sentence).to_string()).collect()],
            }],
        }
    }

    #[test]
    fn a_profile_counts_running_words_by_the_band_of_their_rarest_part() {
        // the (rank 1), cat, Harry (a name), went (go), don't (do + not),
        // xyzzyq (past the lexicon).
        let profile = Profile::of("en", &document(&["The cat and Harry went home.", "I don't xyzzyq."]));
        assert_eq!(profile.words, 8, "names are not counted");
        assert_eq!(profile.beyond, 1);
        assert_eq!(profile.bands.iter().sum::<u32>(), 7);
        assert!(profile.bands[0] >= 4, "the, and, I and don't are among the commonest hundred: {profile:?}");
        let cat = lexicon::of("en").unwrap().entry("cat").unwrap().rank;
        assert!(profile.bands[band(cat).unwrap()] >= 1);

        // "shan't" is "shall", in the band up to 1000, and "not", among the
        // commonest hundred: the word is as rare as its rarer part.
        let shall = band(lexicon::of("en").unwrap().entry("shall").unwrap().rank).unwrap();
        assert_ne!(shall, 0);
        let profile = Profile::of("en", &document(&["We shan't."]));
        assert_eq!((profile.bands[0], profile.bands[shall]), (1, 1), "{profile:?}");
    }

    #[test]
    fn a_level_is_the_lexicon_that_reads_ninety_five_percent() {
        let mut profile = Profile {
            words: 100,
            beyond: 1,
            ..Profile::default()
        };
        profile.bands[0] = 80;
        profile.bands[5] = 14; // up to 1000
        profile.bands[7] = 5; // up to 2000
        assert_eq!(profile.level(), Some(2000), "94 within 1000, 99 within 2000");
        profile.bands[5] = 15;
        profile.bands[7] = 4;
        assert_eq!(profile.level(), Some(1000));
        profile.bands = [0; BANDS.len()];
        profile.beyond = 100;
        assert_eq!(profile.level(), None, "past the whole lexicon");
    }

    #[test]
    fn an_estimate_weighs_each_band_by_what_the_learner_holds_of_it() {
        let mut profile = Profile {
            words: 100,
            beyond: 10,
            ..Profile::default()
        };
        profile.bands[0] = 60;
        profile.bands[1] = 30;
        let mut known = [0.0; BANDS.len()];
        known[0] = 1.0;
        known[1] = 0.5;
        assert_eq!(profile.estimate(&known), 75);
        known[1] = 2.0;
        assert_eq!(profile.estimate(&known), 90, "a share is at most all of a band");
    }

    #[test]
    fn the_english_shelf_is_whole_and_measured_with_the_lexicon_of_today() {
        let shelf = of("en").unwrap();
        assert!(shelf.items.len() >= 40, "{}", shelf.items.len());
        for (entry, profile) in &shelf.items {
            assert!(profile.words > 200, "{} is a stub of {} words", entry.id, profile.words);
            assert!(entry.source.starts_with("https://"), "{}", entry.id);
        }
        for kind in [Kind::Book, Kind::Article] {
            assert!(shelf.items.iter().any(|(entry, _)| entry.kind == kind), "{kind:?}");
        }
    }

    #[test]
    fn a_shelf_refuses_profiles_of_another_lexicon_and_items_without_one() {
        let catalogue = "language = \"en\"\n[[item]]\nid = \"a\"\nkind = \"book\"\ntitle = \"A\"\nsource = \"https://example.org/a\"\n";
        let measured = vec![(
            "a".to_string(),
            Profile {
                words: 2,
                beyond: 2,
                ..Profile::default()
            },
        )];
        let profiles = Shelf::profiles("en", &measured);
        assert_eq!(Shelf::parse(catalogue, &profiles).unwrap().items.len(), 1);
        let stale = profiles.replacen("# lexicon ", "# lexicon 00", 1);
        assert!(Shelf::parse(catalogue, &stale).is_err(), "another lexicon");
        assert!(Shelf::parse(catalogue, &Shelf::profiles("en", &[])).is_err(), "an item without a profile");
        let extra = format!("{profiles}b\t1\t1{}\n", "\t0".repeat(BANDS.len()));
        assert!(Shelf::parse(catalogue, &extra).is_err(), "a profile without an item");
        let wrong = profiles.replace("a\t2\t2", "a\t3\t2");
        assert!(Shelf::parse(catalogue, &wrong).is_err(), "a profile that does not add up");
    }

    fn summary(id: i64, yours: u32, words: u32, opened: bool, finished: bool) -> Summary {
        let coverage = Coverage {
            words,
            sewn: yours,
            unknown: words - yours,
            ..Coverage::default()
        };
        Summary {
            id,
            title: format!("text {id}"),
            author: None,
            source: None,
            language: "en".to_string(),
            format: crate::import::Format::Plain,
            added_at: Utc::now(),
            opened_at: opened.then(Utc::now),
            paragraph: 0,
            paragraphs: 1,
            finished_at: finished.then(Utc::now),
            coverage,
            chapters: 1,
            chapter: ChapterSummary {
                position: 0,
                title: None,
                paragraph: 0,
                paragraphs: 1,
                coverage,
            },
        }
    }

    fn item(id: &str, estimate: u32, taken: Option<i64>) -> Item {
        Item {
            id: id.to_string(),
            kind: Kind::Book,
            title: id.to_string(),
            author: None,
            source: format!("https://example.org/{id}"),
            words: 1000,
            level: Some(2000),
            estimate,
            taken,
        }
    }

    fn picked(next: Option<Next>) -> String {
        match next {
            Some(Next::Text { id, .. }) => format!("text {id}"),
            Some(Next::Shelf { id, .. }) => format!("shelf {id}"),
            None => "nothing".to_string(),
        }
    }

    #[test]
    fn the_next_text_is_the_one_nearest_the_window() {
        let texts = [summary(1, 90, 100, true, false), summary(2, 99, 100, false, false)];
        assert_eq!(picked(next(&texts, &[])), "text 2", "one point too easy beats five too hard");
        assert_eq!(picked(next(&texts, &[item("b", 96, None)])), "shelf b", "inside the window");
        assert_eq!(picked(next(&texts, &[item("b", 96, Some(3))])), "text 2", "a taken item is a text now");
        let window = [summary(1, 96, 100, false, false), summary(2, 97, 100, true, false)];
        assert_eq!(picked(next(&window, &[item("c", 96, None)])), "text 2", "the text being read first");
        let finished = [summary(1, 96, 100, true, true)];
        assert_eq!(picked(next(&finished, &[])), "nothing", "a text read is read");
        assert_eq!(picked(next(&[summary(1, 0, 0, true, false)], &[])), "nothing", "no words to count");
    }

    #[test]
    fn the_distance_from_the_window_counts_hard_double() {
        assert_eq!([distance(95), distance(98), distance(99), distance(94), distance(80)], [0, 0, 1, 2, 30]);
    }
}
