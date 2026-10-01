//! Packs: the material a learner starts from, as data rather than code.
//!
//! A pack is one TOML file of grammar formulas with worked samples and the
//! words to substitute into them - `packs/en-from-ru/pack.toml` teaches
//! English to a Russian speaker. The native language of the learner is a
//! property of the pack (ADR 0002), which is why Russian appears in this
//! product only inside a pack and never as a string in the code.
//!
//! A pack also names the words its sentences teach, with what each means in
//! the learner's own language. A word enters the deck only inside a
//! sentence: the pack marks which words a worked example carries, and the
//! loader checks each mark against the sentence - "went" is a form of "go" -
//! with the lexicon of the language (see [`crate::lexicon`]).
//!
//! Loading is idempotent: the same pack loaded twice leaves the database as
//! it was, and a pack whose contents changed replaces its own formulas and
//! words without touching what the learner has learnt about them.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::lexicon;
use crate::say::{Filling, Template};
use crate::scheduling::{Direction, Kind};

/// A pack as it is written on disk. See `packs/en-from-ru/README.md` for the
/// shape and what each field is for.
#[derive(Debug, Clone, Deserialize)]
pub struct Pack {
    /// Stable id, matching the directory the file lives in.
    pub id: String,
    /// Bumped when the contents change.
    pub version: i64,
    /// The language the prompts are written in.
    pub native: String,
    /// The language being learnt.
    pub target: String,
    /// The formulas, in no particular order: `order` decides the sequence.
    #[serde(default, rename = "formula")]
    pub formulas: Vec<Formula>,
    /// The words the formulas' sentences teach.
    #[serde(default, rename = "word")]
    pub words: Vec<Word>,
}

/// A word a pack teaches: a lemma of the language being learnt, and what it
/// means in the learner's own.
///
/// How common the word is and how it sounds are not here: they belong to the
/// language, and its lexicon says them.
#[derive(Debug, Clone, Deserialize)]
pub struct Word {
    /// As written: "doctor", "I", "Monday". One word - a phrase is a
    /// collocation, and those are a later version's.
    pub lemma: String,
    /// What it means, in the pack's native language.
    pub gloss: String,
}

/// The id a word's card keys on: the language and the lowercase lemma.
#[must_use]
pub fn word_id(language: &str, lemma: &str) -> String {
    format!("{language}:{}", lemma.to_lowercase())
}

/// Which of the three ways a shape can be said.
///
/// The three are one shape, not three: a learner who can say "I am tired" and
/// cannot ask "Are you tired?" has half a formula. Naming the form lets the
/// drill put the three on one card behind a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    Statement,
    Negation,
    Question,
}

impl Form {
    /// The word stored in the formula row.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Statement => "statement",
            Self::Negation => "negation",
            Self::Question => "question",
        }
    }

    /// Reads a form back from a formula row.
    ///
    /// # Errors
    ///
    /// Fails when the word is not one of the three.
    pub fn parse(value: &str) -> Result<Self, UnknownForm> {
        match value {
            "statement" => Ok(Self::Statement),
            "negation" => Ok(Self::Negation),
            "question" => Ok(Self::Question),
            other => Err(UnknownForm(other.to_string())),
        }
    }
}

/// A form that is not one of the three.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a form: expected statement, negation or question")]
pub struct UnknownForm(String);

/// One assembly pattern the learner builds sentences from.
#[derive(Debug, Clone, Deserialize)]
pub struct Formula {
    /// Stable across pack versions: the database keys on it.
    pub id: String,
    /// What the learner sees as the title.
    pub name: String,
    /// The assembly shown compactly, with `<placeholder>` holes.
    pub pattern: String,
    /// The sentence a substitution says, with `<slot>` and `<slot:form>`
    /// holes (see [`crate::say`]). Required when the formula has slots: the
    /// pattern is a scaffold, and a scaffold is not an answer.
    #[serde(default)]
    pub say: Option<String>,
    /// One paragraph in the pack's native language.
    pub explanation: String,
    /// The order formulas are introduced in.
    pub order: i64,
    /// The shape this formula is one form of, when it is: `be-present` holds
    /// a statement, a negation and a question. Absent for a formula that has
    /// no sisters - `let's`, `how much` - and the drill shows those without a
    /// switch.
    #[serde(default)]
    pub family: Option<String>,
    /// Which form of the family this is. Required with `family`, meaningless
    /// without it.
    #[serde(default)]
    pub form: Option<Form>,
    #[serde(default, rename = "sample")]
    pub samples: Vec<Sample>,
    #[serde(default, rename = "slot")]
    pub slots: Vec<Slot>,
}

/// A worked example: what a correct answer looks like, and the words it
/// teaches.
#[derive(Debug, Clone, Deserialize)]
pub struct Sample {
    pub native: String,
    pub target: String,
    /// Lemmas of `target` the learner meets here, each declared once in the
    /// pack's `[[word]]` list. The words the formula itself is about - the
    /// pronoun, the "am" - are the formula's, and are left out.
    #[serde(default)]
    pub words: Vec<String>,
}

/// A hole in the pattern the drill substitutes into.
#[derive(Debug, Clone, Deserialize)]
pub struct Slot {
    /// Matches a `<placeholder>` in the pattern.
    pub name: String,
    pub values: Vec<Value>,
}

/// One filling for a slot: the word in both languages, and the forms that
/// agree with it.
///
/// Forms are the keys beside `native` and `target` - `{ native = "он",
/// target = "he", be = "is" }` - because agreement belongs to the word, and a
/// table of pronouns kept somewhere else is a second truth about them.
#[derive(Debug, Clone, Deserialize)]
pub struct Value {
    pub native: String,
    pub target: String,
    #[serde(flatten)]
    pub forms: BTreeMap<String, String>,
}

impl Filling for Value {
    fn word(&self) -> &str {
        &self.target
    }

    fn form(&self, name: &str) -> Option<&str> {
        self.forms.get(name).map(String::as_str)
    }
}

impl Pack {
    /// Reads and validates a pack file.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, is not valid TOML, or does not
    /// hold a usable pack - see [`Pack::validate`].
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read the pack at {}", path.display()))?;
        let pack: Self = toml::from_str(&text).with_context(|| format!("{} is not a valid pack file", path.display()))?;
        pack.validate().with_context(|| format!("the pack at {} cannot be loaded", path.display()))?;
        Ok(pack)
    }

    /// Checks the things a pack has to get right before it reaches a learner.
    ///
    /// A broken pack is caught here rather than halfway through a load,
    /// because a half-loaded pack is a drill that skips a formula without
    /// saying so.
    ///
    /// # Errors
    ///
    /// Fails on an empty pack, a duplicate formula id, a formula with no
    /// sample, a slot with no values, a duplicate slot name within a formula,
    /// a slot whose name has no matching `<placeholder>` in the pattern, a
    /// form without a family, two formulas claiming the same form of the
    /// same family, or a word that is not where the pack says it is (see
    /// `validate_words`).
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("the pack has no id");
        }
        if self.formulas.is_empty() {
            bail!("the pack holds no formulas");
        }

        let mut seen = HashSet::new();
        for formula in &self.formulas {
            if !seen.insert(formula.id.as_str()) {
                bail!("two formulas share the id {:?}", formula.id);
            }
            if formula.samples.is_empty() {
                // Without an example the learner is asked to produce a shape
                // nobody has shown them once.
                bail!("formula {:?} has no sample", formula.id);
            }

            let mut slot_names = HashSet::new();
            for slot in &formula.slots {
                if !slot_names.insert(slot.name.as_str()) {
                    bail!("formula {:?} has two slots called {:?}", formula.id, slot.name);
                }
                if slot.values.is_empty() {
                    bail!("slot {:?} of formula {:?} has no values", slot.name, formula.id);
                }
                // A slot the pattern does not mention cannot be substituted
                // into: the drill would silently ignore it.
                let placeholder = format!("<{}>", slot.name);
                if !formula.pattern.contains(&placeholder) {
                    bail!(
                        "formula {:?} has a slot {:?}, but its pattern {:?} has no {placeholder}",
                        formula.id,
                        slot.name,
                        formula.pattern
                    );
                }
            }
            validate_say(formula)?;
        }

        self.validate_families()?;
        self.validate_words()
    }

    /// The rules that hold the words to their sentences.
    ///
    /// Every word marked in a sentence is declared, with a gloss; every
    /// declared word is in some sentence, or it could never enter the deck;
    /// and every mark is found in its sentence - the lemma or one of its
    /// forms, by the lexicon of the language. A sentence two formulas share
    /// is one sentence, so it has one translation and one set of words.
    fn validate_words(&self) -> Result<()> {
        let mut declared: HashSet<String> = HashSet::new();
        for word in &self.words {
            if word.lemma.trim().is_empty() {
                bail!("a word has no lemma");
            }
            if word.lemma.contains(char::is_whitespace) {
                bail!(
                    "word {:?} is not one word: a lemma is a single word, and phrases arrive with collocations",
                    word.lemma
                );
            }
            if word.gloss.trim().is_empty() {
                bail!("word {:?} has no gloss", word.lemma);
            }
            if !declared.insert(word.lemma.to_lowercase()) {
                bail!("two words share the lemma {:?}", word.lemma);
            }
        }

        let lexicon = lexicon::of(&self.target);
        let mut used: HashSet<String> = HashSet::new();
        let mut sentences: HashMap<&str, (&str, BTreeSet<String>)> = HashMap::new();
        for formula in &self.formulas {
            for sample in &formula.samples {
                let mut marked = BTreeSet::new();
                for lemma in &sample.words {
                    let key = lemma.to_lowercase();
                    if !declared.contains(&key) {
                        bail!(
                            "formula {:?} marks {lemma:?} in {:?}, but the pack declares no [[word]] for it: a word without a gloss",
                            formula.id,
                            sample.target
                        );
                    }
                    if lexicon::find(lexicon, &sample.target, lemma).is_none() {
                        bail!(
                            "formula {:?} marks {lemma:?} in {:?}, which holds neither it nor any form of it",
                            formula.id,
                            sample.target
                        );
                    }
                    if !marked.insert(key.clone()) {
                        bail!("formula {:?} marks {lemma:?} twice in {:?}", formula.id, sample.target);
                    }
                    used.insert(key);
                }
                match sentences.entry(sample.target.as_str()) {
                    Entry::Vacant(vacant) => {
                        vacant.insert((sample.native.as_str(), marked));
                    }
                    Entry::Occupied(occupied) => {
                        let (native, words) = occupied.get();
                        if *native != sample.native {
                            bail!(
                                "{:?} is translated two ways, {native:?} and {:?}: a sentence has one meaning",
                                sample.target,
                                sample.native
                            );
                        }
                        if *words != marked {
                            bail!("{:?} is marked with different words in two formulas", sample.target);
                        }
                    }
                }
            }
        }

        if let Some(word) = self.words.iter().find(|word| !used.contains(&word.lemma.to_lowercase())) {
            bail!(
                "word {:?} is in no sentence: a word enters the deck only inside a sentence, so this one never would",
                word.lemma
            );
        }
        Ok(())
    }

    /// The rules that hold a family of forms together.
    ///
    /// A form without a family cannot be switched to from anywhere, and two
    /// formulas claiming to be the negation of the same shape would make the
    /// switch show one of them at random. Both are pack-editing mistakes, and
    /// both are silent in the drill, which is why they are caught here.
    fn validate_families(&self) -> Result<()> {
        let mut taken: HashSet<(&str, Form)> = HashSet::new();
        for formula in &self.formulas {
            match (formula.family.as_deref(), formula.form) {
                (Some(family), Some(form)) => {
                    if family.trim().is_empty() {
                        bail!("formula {:?} has an empty family", formula.id);
                    }
                    if !taken.insert((family, form)) {
                        bail!("two formulas are the {} of family {family:?}", form.as_str());
                    }
                }
                (Some(family), None) => bail!("formula {:?} is in family {family:?} but says no form", formula.id),
                (None, Some(form)) => bail!("formula {:?} is a {} of nothing: it has no family", formula.id, form.as_str()),
                (None, None) => {}
            }
        }

        // A family of one draws a switch with nothing to switch to. It is
        // always an editing mistake - a sister renamed, or one not written
        // yet - and it is silent in the drill.
        let mut sizes: HashMap<&str, usize> = HashMap::new();
        for family in self.formulas.iter().filter_map(|formula| formula.family.as_deref()) {
            *sizes.entry(family).or_default() += 1;
        }
        if let Some((family, _)) = sizes.iter().find(|(_, count)| **count < 2) {
            bail!("family {family:?} holds one formula: a form with no sisters is a switch to nowhere");
        }
        Ok(())
    }
}

/// The rules that make `say` a sentence rather than a scaffold with gaps.
///
/// Every slot is said - a value shown in the prompt and missing from the
/// answer is a different sentence - every form asked for is on every value of
/// its slot, and every form a value carries is asked for somewhere: a form
/// nobody reads is almost always a misspelt key, and it would otherwise sit
/// there as a word nobody hears.
fn validate_say(formula: &Formula) -> Result<()> {
    let Some(say) = formula.say.as_deref() else {
        if formula.slots.is_empty() {
            return Ok(());
        }
        bail!(
            "formula {:?} has slots but no `say`: the pattern {:?} is a scaffold, and a substitution needs a sentence to answer with",
            formula.id,
            formula.pattern
        );
    };
    let template = Template::parse(say).with_context(|| format!("formula {:?} has an unreadable `say`", formula.id))?;

    let slots: HashSet<&str> = formula.slots.iter().map(|slot| slot.name.as_str()).collect();
    for name in template.slots() {
        if !slots.contains(name) {
            bail!("formula {:?} says <{name}>, but has no slot called {name:?}", formula.id);
        }
    }
    let said = template.slots();
    let forms = template.forms();
    for slot in &formula.slots {
        if !said.contains(slot.name.as_str()) {
            bail!(
                "formula {:?} never says its slot {:?}: the answer would drop a word the prompt showed",
                formula.id,
                slot.name
            );
        }
        let wanted = forms.get(slot.name.as_str()).cloned().unwrap_or_default();
        for value in &slot.values {
            for form in &wanted {
                if !value.forms.contains_key(*form) {
                    bail!(
                        "value {:?} of slot {:?} in formula {:?} has no form {form:?}, which `say` asks for",
                        value.target,
                        slot.name,
                        formula.id
                    );
                }
            }
            if let Some(unused) = value.forms.keys().find(|form| !wanted.contains(form.as_str())) {
                bail!(
                    "value {:?} of slot {:?} in formula {:?} carries a form {unused:?} that `say` never asks for",
                    value.target,
                    slot.name,
                    formula.id
                );
            }
        }
    }
    Ok(())
}

/// What a load did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loaded {
    /// Formulas the pack holds.
    pub formulas: usize,
    /// Words the pack teaches.
    pub words: usize,
    /// Cards created for formulas and words that had none, counting both
    /// directions of a formula - what is genuinely new to the learner.
    pub new_cards: usize,
    /// Whether anything was written at all.
    pub changed: bool,
}

/// Loads a pack into the database, or confirms it is already there.
///
/// Reloading the same version is a no-op. Reloading a changed version
/// replaces the pack's formulas, slots and words, and keeps every card,
/// review and anchor: what the learner has learnt belongs to the formula id
/// and the word id, not to the version of the file it arrived in. Sentences
/// are kept by what they say, so a corrected translation leaves a sentence
/// the same sentence, and a word goes on being heard in it.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub async fn load(pool: &SqlitePool, pack: &Pack) -> Result<Loaded> {
    let existing: Option<i64> = sqlx::query_scalar("SELECT version FROM pack WHERE id = ?")
        .bind(&pack.id)
        .fetch_optional(pool)
        .await
        .context("failed to look up the pack")?;

    if existing == Some(pack.version) {
        return Ok(Loaded {
            formulas: pack.formulas.len(),
            words: pack.words.len(),
            new_cards: 0,
            changed: false,
        });
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.context("failed to open a transaction")?;

    for (code, name) in [(&pack.native, "native"), (&pack.target, "target")] {
        // The name is a placeholder until a language actually needs one; the
        // code is what everything else keys on.
        sqlx::query("INSERT INTO language (code, name, is_native) VALUES (?, ?, ?) ON CONFLICT (code) DO NOTHING")
            .bind(code)
            .bind(code)
            .bind(i64::from(name == "native"))
            .execute(&mut *tx)
            .await
            .context("failed to record a language")?;
    }

    sqlx::query(
        "INSERT INTO pack (id, version, native, target, loaded_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET version = excluded.version, native = excluded.native,
             target = excluded.target, loaded_at = excluded.loaded_at",
    )
    .bind(&pack.id)
    .bind(pack.version)
    .bind(&pack.native)
    .bind(&pack.target)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .context("failed to record the pack")?;

    // The formulas and words of this pack are replaced wholesale. Slots,
    // links to sentences and marks of words go with them by cascade; cards,
    // reviews and anchors do not, because they key on the formula id and the
    // word id rather than on the row.
    for statement in ["DELETE FROM formula WHERE pack_id = ?", "DELETE FROM word WHERE pack_id = ?"] {
        sqlx::query(statement)
            .bind(&pack.id)
            .execute(&mut *tx)
            .await
            .context("failed to clear the previous version of the pack")?;
    }

    for word in &pack.words {
        sqlx::query("INSERT INTO word (id, pack_id, language, lemma, gloss) VALUES (?, ?, ?, ?, ?)")
            .bind(word_id(&pack.target, &word.lemma))
            .bind(&pack.id)
            .bind(&pack.target)
            .bind(&word.lemma)
            .bind(&word.gloss)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to insert word {}", word.lemma))?;
    }

    for formula in &pack.formulas {
        insert_formula(&mut tx, pack, formula).await?;
    }

    // A sentence no formula shows any more is gone, and an anchor in it with
    // it: the word is heard in the first sentence it is met in until it is
    // answered again. Formulas are the only thing that shows sentences
    // today; a text in the reader will be the second, and has to be counted
    // here when it comes.
    sqlx::query("DELETE FROM sentence WHERE id NOT IN (SELECT sentence_id FROM formula_sentence)")
        .execute(&mut *tx)
        .await
        .context("failed to drop the sentences no formula shows any more")?;

    // A card per formula per direction and per word, created once. `ON
    // CONFLICT DO NOTHING` is what makes a reload keep the learner's history:
    // a formula drilled for a month keeps its schedule when its wording is
    // corrected, and a word keeps its own when its gloss is.
    let subjects = pack
        .formulas
        .iter()
        .flat_map(|formula| Direction::ALL.map(|direction| (Kind::Formula(direction), formula.id.clone())))
        .chain(pack.words.iter().map(|word| (Kind::Word, word_id(&pack.target, &word.lemma))));
    let mut new_cards = 0;
    for (kind, subject) in subjects {
        let result = sqlx::query(
            "INSERT INTO card (kind, subject_id, state, stability, difficulty, due, reps, lapses)
             VALUES (?, ?, 'new', 0.0, 0.0, ?, 0, 0)
             ON CONFLICT (kind, subject_id) DO NOTHING",
        )
        .bind(kind.as_str())
        .bind(&subject)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("failed to create a card for {subject}"))?;
        new_cards += usize::try_from(result.rows_affected()).unwrap_or(0);
    }

    tx.commit().await.context("failed to commit the pack")?;
    Ok(Loaded {
        formulas: pack.formulas.len(),
        words: pack.words.len(),
        new_cards,
        changed: true,
    })
}

/// Writes one formula with its sentences, slots and slot values.
async fn insert_formula(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, pack: &Pack, formula: &Formula) -> Result<()> {
    let (pack_id, target) = (pack.id.as_str(), pack.target.as_str());
    sqlx::query(
        "INSERT INTO formula (id, pack_id, target, name, pattern, explanation, position, family, form, say)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&formula.id)
    .bind(pack_id)
    .bind(target)
    .bind(&formula.name)
    .bind(&formula.pattern)
    .bind(&formula.explanation)
    .bind(formula.order)
    .bind(formula.family.as_deref())
    .bind(formula.form.map(Form::as_str))
    .bind(formula.say.as_deref())
    .execute(&mut **tx)
    .await
    .with_context(|| format!("failed to insert formula {}", formula.id))?;

    for (position, sample) in formula.samples.iter().enumerate() {
        insert_sample(tx, pack, formula, i64::try_from(position).unwrap_or(i64::MAX), sample).await?;
    }

    for (position, slot) in formula.slots.iter().enumerate() {
        let slot_id: i64 = sqlx::query_scalar("INSERT INTO slot (formula_id, name, position) VALUES (?, ?, ?) RETURNING id")
            .bind(&formula.id)
            .bind(&slot.name)
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .fetch_one(&mut **tx)
            .await
            .with_context(|| format!("failed to insert slot {} of formula {}", slot.name, formula.id))?;

        for (position, value) in slot.values.iter().enumerate() {
            sqlx::query("INSERT INTO slot_value (slot_id, native, target, position, forms) VALUES (?, ?, ?, ?, ?)")
                .bind(slot_id)
                .bind(&value.native)
                .bind(&value.target)
                .bind(i64::try_from(position).unwrap_or(i64::MAX))
                .bind(serde_json::to_string(&value.forms).context("failed to write the forms of a value")?)
                .execute(&mut **tx)
                .await
                .with_context(|| format!("failed to insert a value of slot {}", slot.name))?;
        }
    }
    Ok(())
}

/// Writes one worked example: the sentence, kept by what it says, its
/// translation, its place among the formula's examples, and its words.
async fn insert_sample(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, pack: &Pack, formula: &Formula, position: i64, sample: &Sample) -> Result<()> {
    // `DO UPDATE` rather than `DO NOTHING`, because only an update hands back
    // the id of the row that was already there.
    let sentence: i64 = sqlx::query_scalar(
        "INSERT INTO sentence (language, text) VALUES (?, ?)
         ON CONFLICT (language, text) DO UPDATE SET text = excluded.text
         RETURNING id",
    )
    .bind(&pack.target)
    .bind(&sample.target)
    .fetch_one(&mut **tx)
    .await
    .with_context(|| format!("failed to insert the sentence {:?}", sample.target))?;

    sqlx::query(
        "INSERT INTO sentence_translation (sentence_id, language, text) VALUES (?, ?, ?)
         ON CONFLICT (sentence_id, language) DO UPDATE SET text = excluded.text",
    )
    .bind(sentence)
    .bind(&pack.native)
    .bind(&sample.native)
    .execute(&mut **tx)
    .await
    .with_context(|| format!("failed to translate the sentence {:?}", sample.target))?;

    sqlx::query("INSERT INTO formula_sentence (formula_id, sentence_id, position) VALUES (?, ?, ?)")
        .bind(&formula.id)
        .bind(sentence)
        .bind(position)
        .execute(&mut **tx)
        .await
        .with_context(|| format!("failed to attach a sentence to formula {}", formula.id))?;

    let lexicon = lexicon::of(&pack.target);
    for lemma in &sample.words {
        // The validator has found every mark already; a miss here is a pack
        // loaded without being read through `Pack::read`.
        let token = lexicon::find(lexicon, &sample.target, lemma).with_context(|| format!("{lemma:?} is not in {:?}", sample.target))?;
        // `OR IGNORE`: a sentence two formulas share is marked once, and the
        // validator has made sure the two marks agree.
        sqlx::query("INSERT OR IGNORE INTO sentence_word (sentence_id, word_id, form, start) VALUES (?, ?, ?, ?)")
            .bind(sentence)
            .bind(word_id(&pack.target, lemma))
            .bind(token.text)
            .bind(i64::try_from(token.start).unwrap_or(i64::MAX))
            .execute(&mut **tx)
            .await
            .with_context(|| format!("failed to mark {lemma:?} in {:?}", sample.target))?;
    }
    Ok(())
}

/// Cards whose formula or word no longer exists in any pack.
///
/// Not deleted automatically: a formula or a word that disappears from a
/// pack is usually an editing mistake, and the learner's history is worth
/// more than the tidiness. Reported so a later version can offer the choice.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn orphaned_cards(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT DISTINCT subject_id FROM card
         WHERE (kind IN ('formula', 'formula-recognise') AND subject_id NOT IN (SELECT id FROM formula))
            OR (kind = 'word' AND subject_id NOT IN (SELECT id FROM word))
         ORDER BY subject_id",
    )
    .fetch_all(pool)
    .await
    .context("failed to look for orphaned cards")?;
    Ok(rows.iter().map(|row| row.get::<String, _>("subject_id")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        pool
    }

    fn pack_toml(version: i64, name: &str) -> String {
        format!(
            r#"
id = "test-pack"
version = {version}
native = "ru"
target = "en"

[[formula]]
id = "be-present"
name = "{name}"
pattern = "<pronoun> + am/is/are"
say = "<pronoun> is here."
explanation = "explained"
order = 10
family = "be-present"
form = "statement"

  [[formula.sample]]
  native = "prompt"
  target = "I am at home."

  [[formula.slot]]
  name = "pronoun"
  values = [
    {{ native = "one", target = "I" }},
    {{ native = "two", target = "you" }},
  ]

[[formula]]
id = "have-got"
name = "second"
pattern = "<pronoun> + have got"
say = "<pronoun> is here."
explanation = "explained"
order = 20

  [[formula.sample]]
  native = "prompt"
  target = "I have got a car."

  [[formula.slot]]
  name = "pronoun"
  values = [{{ native = "one", target = "I" }}]
"#
        )
    }

    fn parse(toml: &str) -> Pack {
        let pack: Pack = toml::from_str(toml).expect("the fixture should parse");
        pack
    }

    #[test]
    fn the_shipped_pack_is_valid() {
        // The pack that ships with the product is material, and material
        // rots quietly. This is the gate that notices.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/en-from-ru/pack.toml");
        let pack = Pack::read(&path).expect("the shipped pack should load");
        assert_eq!(pack.native, "ru");
        assert_eq!(pack.target, "en");
        assert!(
            pack.formulas.len() >= 25,
            "the starting pack should carry the core of the language, got {} formulas",
            pack.formulas.len()
        );

        let mut positions: Vec<_> = pack.formulas.iter().map(|f| f.order).collect();
        positions.sort_unstable();
        positions.dedup();
        assert_eq!(positions.len(), pack.formulas.len(), "two formulas want the same place in the sequence");

        for formula in &pack.formulas {
            assert!(!formula.explanation.trim().is_empty(), "formula {} has no explanation", formula.id);
            assert!(
                formula.samples.len() >= 2,
                "formula {} shows only {} example(s); one is not a pattern",
                formula.id,
                formula.samples.len()
            );
        }
    }

    #[test]
    fn every_sentence_the_shipped_pack_can_say_is_a_sentence() {
        // The answer to a substitution is what the learner hears and, from
        // v0.5, what they type. A leftover scaffold - a "+", an "am/is/are" -
        // would be read aloud as nonsense and accepted as a right answer.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/en-from-ru/pack.toml");
        let pack = Pack::read(&path).expect("the shipped pack should load");
        let mut said = 0;
        for formula in pack.formulas.iter().filter(|formula| !formula.slots.is_empty()) {
            let template = Template::parse(formula.say.as_deref().unwrap()).unwrap();
            let slots: Vec<(&str, Vec<&Value>)> = formula.slots.iter().map(|slot| (slot.name.as_str(), slot.values.iter().collect())).collect();
            let sentences = crate::say::every_sentence(&template, &slots, 10_000);
            assert!(!sentences.is_empty(), "formula {} says nothing", formula.id);
            for sentence in sentences {
                assert!(
                    !sentence.contains(" + ") && !sentence.contains('/') && !sentence.contains('<'),
                    "formula {} says a scaffold: {sentence:?}",
                    formula.id
                );
                assert!(
                    sentence.ends_with(['.', '?', '!']),
                    "formula {} says an unfinished sentence: {sentence:?}",
                    formula.id
                );
                assert!(
                    sentence.starts_with(char::is_uppercase),
                    "formula {} opens in lower case: {sentence:?}",
                    formula.id
                );
                said += 1;
            }
        }
        assert!(said > 500, "the pack should be able to say hundreds of sentences, said {said}");
    }

    fn with_say(say: Option<&str>, values: &str) -> Pack {
        let say = say
            .map(|say| {
                format!(
                    "say = \"{say}\"
"
                )
            })
            .unwrap_or_default();
        parse(&format!(
            r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be"
name = "n"
pattern = "<pronoun> + am/is/are + <rest>"
{say}explanation = "e"
order = 10

  [[formula.sample]]
  native = "a"
  target = "I am here."

  [[formula.slot]]
  name = "pronoun"
  values = [{values}]

  [[formula.slot]]
  name = "rest"
  values = [{{ native = "x", target = "here" }}]
"#
        ))
    }

    const AGREEING: &str = r#"{ native = "я", target = "I", be = "am" }, { native = "он", target = "he", be = "is" }"#;

    #[test]
    fn a_well_formed_say_is_accepted() {
        with_say(Some("<pronoun> <pronoun:be> <rest>."), AGREEING)
            .validate()
            .expect("a complete template should pass");
    }

    #[test]
    fn a_formula_with_slots_and_no_say_is_rejected() {
        let error = with_say(None, AGREEING).validate().unwrap_err();
        assert!(format!("{error:#}").contains("no `say`"), "{error:#}");
    }

    #[test]
    fn a_say_naming_a_slot_that_is_not_there_is_rejected() {
        let error = with_say(Some("<pronoun> <pronoun:be> <place>."), AGREEING).validate().unwrap_err();
        assert!(format!("{error:#}").contains("no slot called \"place\""), "{error:#}");
    }

    #[test]
    fn a_say_that_drops_a_slot_is_rejected() {
        let error = with_say(Some("<pronoun> <pronoun:be> fine."), AGREEING).validate().unwrap_err();
        assert!(format!("{error:#}").contains("never says its slot \"rest\""), "{error:#}");
    }

    #[test]
    fn a_value_missing_a_form_is_rejected() {
        let values = r#"{ native = "я", target = "I", be = "am" }, { native = "он", target = "he" }"#;
        let error = with_say(Some("<pronoun> <pronoun:be> <rest>."), values).validate().unwrap_err();
        assert!(format!("{error:#}").contains("has no form \"be\""), "{error:#}");
    }

    #[test]
    fn a_form_nobody_asks_for_is_rejected() {
        // Almost always a misspelt key: `bee = "is"` would otherwise sit there
        // while the sentence went without it.
        let values = r#"{ native = "я", target = "I", be = "am" }, { native = "он", target = "he", be = "is", bee = "is" }"#;
        let error = with_say(Some("<pronoun> <pronoun:be> <rest>."), values).validate().unwrap_err();
        assert!(format!("{error:#}").contains("\"bee\" that `say` never asks for"), "{error:#}");
    }

    #[tokio::test]
    async fn say_and_forms_reach_the_database() {
        let pool = pool().await;
        load(&pool, &with_say(Some("<pronoun> <pronoun:be> <rest>."), AGREEING)).await.unwrap();
        let formula = crate::study::formula(&pool, "be").await.unwrap();
        assert_eq!(formula.say.as_deref(), Some("<pronoun> <pronoun:be> <rest>."));
        assert_eq!(formula.slots[0].values[1].forms.get("be").map(String::as_str), Some("is"));
    }

    #[test]
    fn a_pack_with_no_formulas_is_rejected() {
        let pack = parse("id = \"empty\"\nversion = 1\nnative = \"ru\"\ntarget = \"en\"\n");
        assert!(pack.validate().is_err(), "an empty pack would load as a working tutor with nothing to drill");
    }

    #[test]
    fn a_formula_with_no_sample_is_rejected() {
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "f"
name = "n"
pattern = "x"
explanation = "e"
order = 1
"#;
        let error = parse(toml).validate().unwrap_err().to_string();
        assert!(error.contains("no sample"), "{error}");
    }

    #[test]
    fn a_slot_the_pattern_never_mentions_is_rejected() {
        // The drill substitutes by placeholder; a slot the pattern does not
        // name would be loaded, stored, and never used.
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "f"
name = "n"
pattern = "<pronoun> + am"
say = "<pronoun> is here."
explanation = "e"
order = 1

  [[formula.sample]]
  native = "a"
  target = "b"

  [[formula.slot]]
  name = "verb"
  values = [{ native = "a", target = "b" }]
"#;
        let error = parse(toml).validate().unwrap_err().to_string();
        assert!(error.contains("<verb>"), "{error}");
    }

    #[test]
    fn a_form_with_no_family_is_rejected() {
        // A form nobody can switch to: the drill would never show it.
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "f"
name = "n"
pattern = "x"
explanation = "e"
order = 1
form = "negation"
  [[formula.sample]]
  native = "a"
  target = "b"
"#;
        let error = parse(toml).validate().unwrap_err().to_string();
        assert!(error.contains("no family"), "{error}");
    }

    #[test]
    fn a_family_with_no_form_is_rejected() {
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "f"
name = "n"
pattern = "x"
explanation = "e"
order = 1
family = "be-present"
  [[formula.sample]]
  native = "a"
  target = "b"
"#;
        let error = parse(toml).validate().unwrap_err().to_string();
        assert!(error.contains("no form"), "{error}");
    }

    #[test]
    fn two_formulas_claiming_the_same_form_are_rejected() {
        // The switch would show one of the two at random, and which one would
        // depend on row order.
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "one"
name = "n"
pattern = "x"
explanation = "e"
order = 1
family = "be-present"
form = "question"
  [[formula.sample]]
  native = "a"
  target = "b"

[[formula]]
id = "two"
name = "n"
pattern = "x"
explanation = "e"
order = 2
family = "be-present"
form = "question"
  [[formula.sample]]
  native = "a"
  target = "b"
"#;
        let error = parse(toml).validate().unwrap_err().to_string();
        assert!(error.contains("question"), "{error}");
    }

    #[test]
    fn a_family_of_one_is_rejected() {
        // A switch with nothing to switch to: always a sister renamed or not
        // written yet, and silent in the drill.
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "only"
name = "n"
pattern = "x"
explanation = "e"
order = 1
family = "be-present"
form = "statement"
  [[formula.sample]]
  native = "a"
  target = "b"
"#;
        let error = parse(toml).validate().unwrap_err().to_string();
        assert!(error.contains("one formula"), "{error}");
    }

    #[test]
    fn a_formula_with_no_family_at_all_is_fine() {
        // "Let's go" is nobody's negation. Most of a pack is like this at
        // first, and requiring a family would force made-up groupings.
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "lets"
name = "n"
pattern = "Let's + x"
explanation = "e"
order = 1
  [[formula.sample]]
  native = "a"
  target = "b"
"#;
        parse(toml).validate().expect("a formula without a family should load");
    }

    #[tokio::test]
    async fn a_family_and_a_form_reach_the_database() {
        let pool = pool().await;
        load(&pool, &parse(&pack_toml(1, "first"))).await.unwrap();

        let (family, form): (Option<String>, Option<String>) = sqlx::query_as("SELECT family, form FROM formula WHERE id = 'be-present'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((family.as_deref(), form.as_deref()), (Some("be-present"), Some("statement")));

        let (family, form): (Option<String>, Option<String>) = sqlx::query_as("SELECT family, form FROM formula WHERE id = 'have-got'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((family, form), (None, None), "a formula with no family should not invent one");
    }

    #[test]
    fn forms_survive_the_round_trip_through_the_database() {
        for form in [Form::Statement, Form::Negation, Form::Question] {
            assert_eq!(Form::parse(form.as_str()).unwrap(), form);
        }
        assert!(Form::parse("exclamation").is_err());
    }

    #[test]
    fn two_formulas_with_the_same_id_are_rejected() {
        let toml = r#"
id = "p"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "same"
name = "one"
pattern = "x"
explanation = "e"
order = 1
  [[formula.sample]]
  native = "a"
  target = "b"

[[formula]]
id = "same"
name = "two"
pattern = "x"
explanation = "e"
order = 2
  [[formula.sample]]
  native = "a"
  target = "b"
"#;
        let error = parse(toml).validate().unwrap_err().to_string();
        assert!(error.contains("share the id"), "{error}");
    }

    #[tokio::test]
    async fn loading_fills_the_database() {
        let pool = pool().await;
        let loaded = load(&pool, &parse(&pack_toml(1, "first"))).await.unwrap();
        assert_eq!(loaded.formulas, 2);
        assert_eq!(loaded.new_cards, 4, "each formula should get a card in each direction");
        assert!(loaded.changed);

        let formulas: i64 = sqlx::query_scalar("SELECT count(*) FROM formula").fetch_one(&pool).await.unwrap();
        let samples: i64 = sqlx::query_scalar("SELECT count(*) FROM formula_sentence").fetch_one(&pool).await.unwrap();
        let values: i64 = sqlx::query_scalar("SELECT count(*) FROM slot_value").fetch_one(&pool).await.unwrap();
        assert_eq!((formulas, samples, values), (2, 2, 3));
    }

    #[tokio::test]
    async fn loading_the_same_pack_twice_changes_nothing() {
        // The load command is run by a container on every start; a second run
        // must not duplicate a single row.
        let pool = pool().await;
        let pack = parse(&pack_toml(1, "first"));
        load(&pool, &pack).await.unwrap();

        let second = load(&pool, &pack).await.unwrap();
        assert!(!second.changed, "an unchanged pack should not be rewritten");
        assert_eq!(second.new_cards, 0);

        let formulas: i64 = sqlx::query_scalar("SELECT count(*) FROM formula").fetch_one(&pool).await.unwrap();
        let cards: i64 = sqlx::query_scalar("SELECT count(*) FROM card").fetch_one(&pool).await.unwrap();
        assert_eq!((formulas, cards), (2, 4), "a reload duplicated rows");
    }

    #[tokio::test]
    async fn a_changed_pack_keeps_what_the_learner_has_learnt() {
        // The reason cards key on the formula id: correcting a formula's
        // wording must not reset a month of drilling it.
        let pool = pool().await;
        load(&pool, &parse(&pack_toml(1, "first"))).await.unwrap();
        sqlx::query("UPDATE card SET state = 'sewn', reps = 9 WHERE kind = 'formula' AND subject_id = 'be-present'")
            .execute(&pool)
            .await
            .unwrap();

        let reloaded = load(&pool, &parse(&pack_toml(2, "corrected"))).await.unwrap();
        assert!(reloaded.changed);
        assert_eq!(reloaded.new_cards, 0, "a rewording is not a new formula");

        let name: String = sqlx::query_scalar("SELECT name FROM formula WHERE id = 'be-present'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "corrected", "the new wording should have replaced the old");

        let (state, reps): (String, i64) = sqlx::query_as("SELECT state, reps FROM card WHERE kind = 'formula' AND subject_id = 'be-present'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((state.as_str(), reps), ("sewn", 9), "the reload wiped the learner's progress");

        let samples: i64 = sqlx::query_scalar("SELECT count(*) FROM formula_sentence").fetch_one(&pool).await.unwrap();
        assert_eq!(samples, 2, "the previous version's samples were left behind");
    }

    #[tokio::test]
    async fn a_formula_dropped_from_a_pack_is_reported_not_deleted() {
        let pool = pool().await;
        load(&pool, &parse(&pack_toml(1, "first"))).await.unwrap();

        let mut shrunk = parse(&pack_toml(2, "first"));
        shrunk.formulas.retain(|formula| formula.id != "have-got");
        load(&pool, &shrunk).await.unwrap();

        assert_eq!(
            orphaned_cards(&pool).await.unwrap(),
            vec!["have-got".to_string()],
            "a formula with cards in two directions should be reported once"
        );
        let cards: i64 = sqlx::query_scalar("SELECT count(*) FROM card").fetch_one(&pool).await.unwrap();
        assert_eq!(cards, 4, "a card was deleted along with its formula");
    }

    /// A pack of two formulas whose examples teach words: "doctor" in three
    /// sentences across both, "go" by its past.
    fn worded(version: i64, doctor_gloss: &str, translation: &str) -> Pack {
        parse(&format!(
            r#"
id = "w"
version = {version}
native = "ru"
target = "en"

[[formula]]
id = "be"
name = "n"
pattern = "x"
explanation = "e"
order = 10

  [[formula.sample]]
  native = "{translation}"
  target = "He is a doctor."
  words = ["doctor"]

  [[formula.sample]]
  native = "Она пошла к врачу."
  target = "She went to the doctor."
  words = ["go", "doctor"]

[[formula]]
id = "past"
name = "n"
pattern = "y"
explanation = "e"
order = 20

  [[formula.sample]]
  native = "Врачи заняты."
  target = "The doctors are busy."
  words = ["doctor"]

[[word]]
lemma = "doctor"
gloss = "{doctor_gloss}"

[[word]]
lemma = "go"
gloss = "идти"
"#
        ))
    }

    fn with_words(words: &str, marks: &str) -> Pack {
        parse(&format!(
            r#"
id = "w"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be"
name = "n"
pattern = "x"
explanation = "e"
order = 10

  [[formula.sample]]
  native = "Он врач."
  target = "He is a doctor."
  words = [{marks}]

{words}
"#
        ))
    }

    const DOCTOR: &str = "[[word]]\nlemma = \"doctor\"\ngloss = \"врач\"\n";

    #[test]
    fn the_shipped_pack_teaches_words_its_language_knows() {
        // The typo guard: a lemma the lexicon has never heard of is almost
        // always misspelt, and it would sit in the deck with no level and no
        // transcription.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/en-from-ru/pack.toml");
        let pack = Pack::read(&path).expect("the shipped pack should load");
        let english = lexicon::of("en").unwrap();
        for word in &pack.words {
            assert!(english.entry(&word.lemma).is_some(), "{:?} is not a word the lexicon knows", word.lemma);
        }
        assert!(
            pack.words.len() >= 120,
            "the formulas' sentences should carry a vocabulary, the pack marks {} words",
            pack.words.len()
        );
    }

    #[test]
    fn a_word_marked_by_one_of_its_forms_is_accepted() {
        worded(1, "врач", "Он врач.").validate().expect("went is a form of go, doctors of doctor");
    }

    #[test]
    fn a_mark_without_a_declared_word_is_rejected() {
        let error = with_words("", "\"doctor\"").validate().unwrap_err();
        assert!(format!("{error:#}").contains("declares no [[word]]"), "{error:#}");
    }

    #[test]
    fn a_mark_the_sentence_does_not_hold_is_rejected() {
        let words = format!("{DOCTOR}[[word]]\nlemma = \"nurse\"\ngloss = \"медсестра\"\n");
        let error = with_words(&words, "\"doctor\", \"nurse\"").validate().unwrap_err();
        assert!(format!("{error:#}").contains("holds neither it nor any form of it"), "{error:#}");
    }

    #[test]
    fn a_word_in_no_sentence_is_rejected() {
        // It could never enter the deck: words come in only inside a sentence.
        let words = format!("{DOCTOR}[[word]]\nlemma = \"nurse\"\ngloss = \"медсестра\"\n");
        let error = with_words(&words, "\"doctor\"").validate().unwrap_err();
        assert!(format!("{error:#}").contains("\"nurse\" is in no sentence"), "{error:#}");
    }

    #[test]
    fn a_word_without_a_gloss_or_of_two_words_is_rejected() {
        let error = with_words("[[word]]\nlemma = \"doctor\"\ngloss = \" \"\n", "\"doctor\"")
            .validate()
            .unwrap_err();
        assert!(format!("{error:#}").contains("no gloss"), "{error:#}");
        let error = with_words("[[word]]\nlemma = \"a doctor\"\ngloss = \"врач\"\n", "\"a doctor\"")
            .validate()
            .unwrap_err();
        assert!(format!("{error:#}").contains("not one word"), "{error:#}");
        let error = with_words(&format!("{DOCTOR}[[word]]\nlemma = \"Doctor\"\ngloss = \"доктор\"\n"), "\"doctor\"")
            .validate()
            .unwrap_err();
        assert!(format!("{error:#}").contains("share the lemma"), "{error:#}");
    }

    #[test]
    fn a_sentence_translated_two_ways_is_rejected() {
        // Two formulas showing one sentence show one sentence: it cannot mean
        // two things.
        let mut pack = worded(1, "врач", "Он врач.");
        let mut copy = pack.formulas[0].samples[0].clone();
        copy.native = "Он доктор.".to_string();
        pack.formulas[1].samples.push(copy);
        let error = pack.validate().unwrap_err();
        assert!(format!("{error:#}").contains("translated two ways"), "{error:#}");
    }

    #[tokio::test]
    async fn a_word_in_three_sentences_is_one_card_with_three_contexts() {
        // Duplicates merge by what the word is, not by where it was met.
        let pool = pool().await;
        let loaded = load(&pool, &worded(1, "врач", "Он врач.")).await.unwrap();
        assert_eq!(loaded.words, 2);
        assert_eq!(loaded.new_cards, 2 * 2 + 2, "two formulas both ways, and two words");

        let cards: i64 = sqlx::query_scalar("SELECT count(*) FROM card WHERE kind = 'word' AND subject_id = 'en:doctor'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let contexts: i64 = sqlx::query_scalar("SELECT count(*) FROM sentence_word WHERE word_id = 'en:doctor'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((cards, contexts), (1, 3));

        let (form, start): (String, i64) = sqlx::query_as("SELECT form, start FROM sentence_word WHERE word_id = 'en:go'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            (form.as_str(), start),
            ("went", 4),
            "the word should be marked where it stands, as it is spelt there"
        );
    }

    #[tokio::test]
    async fn a_reload_keeps_a_sentence_and_its_anchor_when_only_the_wording_around_it_changes() {
        // A corrected gloss or translation must not move the sentence a word
        // is heard in.
        let pool = pool().await;
        load(&pool, &worded(1, "врач", "Он врач.")).await.unwrap();
        let sentence: i64 = sqlx::query_scalar("SELECT id FROM sentence WHERE text = 'He is a doctor.'")
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO anchor (word_id, sentence_id) VALUES ('en:doctor', ?)")
            .bind(sentence)
            .execute(&pool)
            .await
            .unwrap();

        load(&pool, &worded(2, "врач, доктор", "Он - врач.")).await.unwrap();
        let (anchor, translation): (i64, String) = sqlx::query_as(
            "SELECT a.sentence_id, t.text FROM anchor a JOIN sentence_translation t ON t.sentence_id = a.sentence_id WHERE a.word_id = 'en:doctor'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(anchor, sentence, "the anchor moved to another sentence");
        assert_eq!(translation, "Он - врач.", "the corrected translation should have replaced the old one");
        let gloss: String = sqlx::query_scalar("SELECT gloss FROM word WHERE id = 'en:doctor'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(gloss, "врач, доктор");
        let marks: i64 = sqlx::query_scalar("SELECT count(*) FROM sentence_word").fetch_one(&pool).await.unwrap();
        assert_eq!(marks, 4, "a reload duplicated or lost the marks of words");
    }

    #[tokio::test]
    async fn a_sentence_the_pack_drops_takes_its_anchor_with_it() {
        let pool = pool().await;
        load(&pool, &worded(1, "врач", "Он врач.")).await.unwrap();
        sqlx::query("INSERT INTO anchor (word_id, sentence_id) SELECT 'en:doctor', id FROM sentence WHERE text = 'The doctors are busy.'")
            .execute(&pool)
            .await
            .unwrap();

        let mut shrunk = worded(2, "врач", "Он врач.");
        shrunk.formulas[1].samples[0] = Sample {
            native: "Они заняты.".to_string(),
            target: "They are busy.".to_string(),
            words: Vec::new(),
        };
        load(&pool, &shrunk).await.unwrap();

        let gone: i64 = sqlx::query_scalar("SELECT count(*) FROM sentence WHERE text = 'The doctors are busy.'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let anchors: i64 = sqlx::query_scalar("SELECT count(*) FROM anchor").fetch_one(&pool).await.unwrap();
        assert_eq!((gone, anchors), (0, 0), "a sentence nobody shows, or an anchor in it, was left behind");
    }

    #[tokio::test]
    async fn a_word_dropped_from_a_pack_is_reported_not_deleted() {
        let pool = pool().await;
        load(&pool, &worded(1, "врач", "Он врач.")).await.unwrap();
        let mut shrunk = worded(2, "врач", "Он врач.");
        shrunk.words.retain(|word| word.lemma != "go");
        shrunk.formulas[0].samples[1].words.retain(|word| word != "go");
        load(&pool, &shrunk).await.unwrap();
        assert_eq!(orphaned_cards(&pool).await.unwrap(), vec!["en:go".to_string()]);
    }
}
