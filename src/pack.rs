//! Packs: the material a learner starts from, as data rather than code.
//!
//! A pack is one TOML file of grammar formulas with worked samples and the
//! words to substitute into them - `packs/en-from-ru/pack.toml` teaches
//! English to a Russian speaker. The native language of the learner is a
//! property of the pack (ADR 0002), which is why Russian appears in this
//! product only inside a pack and never as a string in the code.
//!
//! Loading is idempotent: the same pack loaded twice leaves the database as
//! it was, and a pack whose contents changed replaces its own formulas
//! without touching what the learner has learnt about them.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::scheduling::Direction;

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

/// A worked example: what a correct answer looks like.
#[derive(Debug, Clone, Deserialize)]
pub struct Sample {
    pub native: String,
    pub target: String,
}

/// A hole in the pattern the drill substitutes into.
#[derive(Debug, Clone, Deserialize)]
pub struct Slot {
    /// Matches a `<placeholder>` in the pattern.
    pub name: String,
    pub values: Vec<Sample>,
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
    /// form without a family, or two formulas claiming the same form of the
    /// same family.
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
        }

        self.validate_families()
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

/// What a load did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loaded {
    /// Formulas the pack holds.
    pub formulas: usize,
    /// Cards created for formulas that had none, counting both directions -
    /// what is genuinely new to the learner.
    pub new_cards: usize,
    /// Whether anything was written at all.
    pub changed: bool,
}

/// Loads a pack into the database, or confirms it is already there.
///
/// Reloading the same version is a no-op. Reloading a changed version
/// replaces the pack's formulas, samples and slots, and keeps every card and
/// review: what the learner has learnt belongs to the formula id, not to the
/// version of the file it arrived in.
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

    // The formulas of this pack are replaced wholesale. Samples and slots go
    // with them by cascade; cards and reviews do not, because they key on the
    // formula id in `card.subject_id` rather than on the row.
    sqlx::query("DELETE FROM formula WHERE pack_id = ?")
        .bind(&pack.id)
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous version of the pack")?;

    for formula in &pack.formulas {
        insert_formula(&mut tx, &pack.id, &pack.target, formula).await?;
    }

    // A card per formula per direction, created once. `ON CONFLICT DO
    // NOTHING` is what makes a reload keep the learner's history: a formula
    // that has been drilled for a month keeps its schedule when its wording
    // is corrected.
    let mut new_cards = 0;
    for formula in &pack.formulas {
        for direction in Direction::ALL {
            let result = sqlx::query(
                "INSERT INTO card (kind, subject_id, state, stability, difficulty, due, reps, lapses)
                 VALUES (?, ?, 'new', 0.0, 0.0, ?, 0, 0)
                 ON CONFLICT (kind, subject_id) DO NOTHING",
            )
            .bind(direction.kind())
            .bind(&formula.id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to create a card for formula {}", formula.id))?;
            new_cards += usize::try_from(result.rows_affected()).unwrap_or(0);
        }
    }

    tx.commit().await.context("failed to commit the pack")?;
    Ok(Loaded {
        formulas: pack.formulas.len(),
        new_cards,
        changed: true,
    })
}

/// Writes one formula with its samples, slots and slot values.
async fn insert_formula(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, pack_id: &str, target: &str, formula: &Formula) -> Result<()> {
    sqlx::query(
        "INSERT INTO formula (id, pack_id, target, name, pattern, explanation, position, family, form)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(&mut **tx)
    .await
    .with_context(|| format!("failed to insert formula {}", formula.id))?;

    for (position, sample) in formula.samples.iter().enumerate() {
        sqlx::query("INSERT INTO sample (formula_id, native, target, position) VALUES (?, ?, ?, ?)")
            .bind(&formula.id)
            .bind(&sample.native)
            .bind(&sample.target)
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .execute(&mut **tx)
            .await
            .with_context(|| format!("failed to insert a sample of formula {}", formula.id))?;
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
            sqlx::query("INSERT INTO slot_value (slot_id, native, target, position) VALUES (?, ?, ?, ?)")
                .bind(slot_id)
                .bind(&value.native)
                .bind(&value.target)
                .bind(i64::try_from(position).unwrap_or(i64::MAX))
                .execute(&mut **tx)
                .await
                .with_context(|| format!("failed to insert a value of slot {}", slot.name))?;
        }
    }
    Ok(())
}

/// Cards whose formula no longer exists in any pack.
///
/// Not deleted automatically: a formula that disappears from a pack is
/// usually an editing mistake, and the learner's history is worth more than
/// the tidiness. Reported so a later version can offer the choice.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn orphaned_cards(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT DISTINCT subject_id FROM card
         WHERE kind IN ('formula', 'formula-recognise') AND subject_id NOT IN (SELECT id FROM formula)
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
        let samples: i64 = sqlx::query_scalar("SELECT count(*) FROM sample").fetch_one(&pool).await.unwrap();
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

        let samples: i64 = sqlx::query_scalar("SELECT count(*) FROM sample").fetch_one(&pool).await.unwrap();
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
}
