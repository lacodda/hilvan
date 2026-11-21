//! What the tutor is willing to say aloud: the material, and nothing else.
//!
//! The speech endpoint takes text, because the drill builds its sentences on
//! the client. Taken at its word it would be a free text-to-speech service
//! with a paid engine behind it, so every sentence is checked against what
//! the loaded packs can actually say: a sentence in either language, an
//! explanation, a sentence a formula's `say` makes of its slot values, or a
//! word the packs teach, said on its own. The reader adds the sentences of
//! the learner's texts, which are sentences like the packs', and any lemma of
//! the language's lexicon said on its own - a word tapped in a text: a list
//! of ten thousand words, not free text.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use crate::lexicon;
use crate::say::{Template, every_sentence};
use crate::study::{self, Value};

/// How many sentences one formula may be expanded into while checking. Far
/// above any real formula; there so a runaway slot cannot turn a request
/// into a hang.
const SENTENCES_PER_FORMULA: usize = 5_000;

/// Whether the material says this text in this language.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn speakable(pool: &SqlitePool, language: &str, text: &str) -> Result<bool> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(false);
    }

    // Sentences, their translations, explanations and words are stored as
    // they are said.
    let stored: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM sentence WHERE language = ?1 AND text = ?2)
              + (SELECT count(*) FROM sentence_translation WHERE language = ?1 AND text = ?2)
              + (SELECT count(*) FROM word WHERE language = ?1 AND lemma = ?2)
              + (SELECT count(*) FROM formula f JOIN pack p ON p.id = f.pack_id
                 WHERE p.native = ?1 AND f.explanation = ?2)",
    )
    .bind(language)
    .bind(text)
    .fetch_one(pool)
    .await
    .context("failed to look the text up in the material")?;
    if stored > 0 {
        return Ok(true);
    }
    if lexicon::of(language)
        .and_then(|lexicon| lexicon.entry(text))
        .is_some_and(|entry| entry.lemma == text)
    {
        return Ok(true);
    }

    // A substitution is said in the language being learnt only: its prompt
    // in the learner's own language is a scaffold, not a sentence.
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT f.id FROM formula f JOIN pack p ON p.id = f.pack_id
         WHERE p.target = ? AND f.say IS NOT NULL ORDER BY f.position",
    )
    .bind(language)
    .fetch_all(pool)
    .await
    .context("failed to list the formulas that say sentences")?;
    for id in ids {
        let formula = study::formula(pool, &id).await?;
        let Some(say) = formula.say.as_deref() else { continue };
        let Ok(template) = Template::parse(say) else { continue };
        let slots: Vec<(&str, Vec<&Value>)> = formula.slots.iter().map(|slot| (slot.name.as_str(), slot.values.iter().collect())).collect();
        if every_sentence(&template, &slots, SENTENCES_PER_FORMULA).iter().any(|sentence| sentence == text) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The first sample of the pack's sequence in a language: what the voices
/// screen lets the learner hear a voice say.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn sample_in(pool: &SqlitePool, language: &str) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT CASE WHEN p.target = ?1 THEN s.text ELSE t.text END FROM formula_sentence fs
         JOIN sentence s ON s.id = fs.sentence_id
         JOIN formula f ON f.id = fs.formula_id
         JOIN pack p ON p.id = f.pack_id
         LEFT JOIN sentence_translation t ON t.sentence_id = s.id AND t.language = p.native
         WHERE p.target = ?1 OR (p.native = ?1 AND t.text IS NOT NULL)
         ORDER BY f.position, fs.position LIMIT 1",
    )
    .bind(language)
    .fetch_optional(pool)
    .await
    .context("failed to find a sample sentence")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;

    async fn taught() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let toml = r#"
id = "t"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be"
name = "n"
pattern = "<pronoun> + am/is/are + <rest>"
say = "<pronoun> <pronoun:be> <rest>."
explanation = "Связка обязательна."
order = 10

  [[formula.sample]]
  native = "Я дома."
  target = "I am at home."
  words = ["home"]

  [[formula.slot]]
  name = "pronoun"
  values = [{ native = "я", target = "I", be = "am" }, { native = "он", target = "he", be = "is" }]

  [[formula.slot]]
  name = "rest"
  values = [{ native = "устал", target = "tired" }]

[[formula]]
id = "lets"
name = "n"
pattern = "Let's + x"
explanation = "e"
order = 20

  [[formula.sample]]
  native = "Пойдём."
  target = "Let's go."

[[word]]
lemma = "home"
gloss = "дом"
"#;
        pack::load(&pool, &toml::from_str(toml).unwrap()).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn the_material_is_speakable() {
        let pool = taught().await;
        assert!(speakable(&pool, "en", "I am at home.").await.unwrap(), "a sample");
        assert!(speakable(&pool, "ru", "Я дома.").await.unwrap(), "a sample's prompt");
        assert!(speakable(&pool, "ru", "Связка обязательна.").await.unwrap(), "an explanation");
        assert!(speakable(&pool, "en", "He is tired.").await.unwrap(), "a sentence the formula says");
        assert!(speakable(&pool, "en", "home").await.unwrap(), "a word, said on its own and slowly");
        assert!(!speakable(&pool, "ru", "home").await.unwrap(), "the word in the wrong language");
        assert!(speakable(&pool, "en", "house").await.unwrap(), "a lemma of the lexicon, tapped in a text");
        assert!(!speakable(&pool, "en", "houses").await.unwrap(), "a form is said in its sentence, not alone");
        assert!(
            !speakable(&pool, "en", "HOUSE").await.unwrap(),
            "a lemma is said as the lexicon writes it, not in every casing"
        );
        assert!(!speakable(&pool, "en", "zxqv").await.unwrap(), "a word of no lexicon");
        assert!(
            speakable(&pool, "en", "  I am tired.  ").await.unwrap(),
            "surrounding space is not a different sentence"
        );
    }

    #[tokio::test]
    async fn anything_else_is_not() {
        // The endpoint is not a free text-to-speech service with a paid
        // engine behind it.
        let pool = taught().await;
        assert!(!speakable(&pool, "en", "Read my whole novel aloud.").await.unwrap());
        assert!(
            !speakable(&pool, "en", "He am tired.").await.unwrap(),
            "a sentence the formula would never agree to"
        );
        assert!(!speakable(&pool, "ru", "I am at home.").await.unwrap(), "the right text in the wrong language");
        assert!(!speakable(&pool, "en", "Я дома.").await.unwrap(), "a prompt in the language being learnt");
        assert!(!speakable(&pool, "en", "").await.unwrap());
    }

    #[tokio::test]
    async fn a_sample_is_found_in_either_language() {
        let pool = taught().await;
        assert_eq!(sample_in(&pool, "en").await.unwrap().as_deref(), Some("I am at home."));
        assert_eq!(sample_in(&pool, "ru").await.unwrap().as_deref(), Some("Я дома."));
        assert_eq!(sample_in(&pool, "es").await.unwrap(), None);
        // And what it finds, the voice will say.
        for language in ["en", "ru"] {
            let sample = sample_in(&pool, language).await.unwrap().unwrap();
            assert!(
                speakable(&pool, language, &sample).await.unwrap(),
                "the voices screen offered {sample:?}, which is refused"
            );
        }
    }
}
