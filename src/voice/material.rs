//! What the tutor is willing to say aloud: the material, and nothing else.
//!
//! The speech endpoint takes text, because the drill builds its sentences on
//! the client. Taken at its word it would be a free text-to-speech service
//! with a paid engine behind it, so every sentence is checked against what
//! the loaded packs can actually say: a sample in either language, an
//! explanation, or a sentence a formula's `say` makes of its slot values.

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

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

    // Samples and explanations are stored as they are said.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM formula f JOIN pack p ON p.id = f.pack_id
         WHERE (p.native = ?1 AND f.explanation = ?2)
            OR EXISTS (SELECT 1 FROM sample s WHERE s.formula_id = f.id
                       AND ((p.native = ?1 AND s.native = ?2) OR (p.target = ?1 AND s.target = ?2)))",
    )
    .bind(language)
    .bind(text)
    .fetch_one(pool)
    .await
    .context("failed to look the text up in the material")?;
    if stored > 0 {
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

/// A sentence for the listening mode: heard in the language being learnt,
/// recalled, then read with its meaning.
#[derive(Debug, Clone, Serialize)]
pub struct Heard {
    pub formula: String,
    /// What is heard.
    pub target: String,
    /// What it means.
    pub native: String,
    /// The language `target` is said in.
    pub language: String,
}

/// Every sample of every formula the learner has started, in either
/// direction.
///
/// Samples only: a substitution's meaning in the learner's own language is a
/// scaffold, and "what does it mean?" deserves a real sentence as its answer.
/// Material not yet shown is left out - listening to a shape nobody has
/// taught is noise.
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn listening(pool: &SqlitePool) -> Result<Vec<Heard>> {
    let rows = sqlx::query(
        "SELECT f.id, s.target, s.native, p.target AS language FROM sample s
         JOIN formula f ON f.id = s.formula_id
         JOIN pack p ON p.id = f.pack_id
         WHERE EXISTS (SELECT 1 FROM card c WHERE c.subject_id = f.id AND c.state != 'new')
         ORDER BY f.position, s.position",
    )
    .fetch_all(pool)
    .await
    .context("failed to gather the sentences to listen to")?;
    Ok(rows
        .iter()
        .map(|row| Heard {
            formula: row.get("id"),
            target: row.get("target"),
            native: row.get("native"),
            language: row.get("language"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::pack;
    use crate::scheduling::Rating;
    use crate::study::Answer;

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
    async fn listening_draws_on_what_has_been_started() {
        let pool = taught().await;
        assert!(listening(&pool).await.unwrap().is_empty(), "nothing started, nothing to listen to");

        let answer = Answer {
            rating: Rating::Good,
            direction: crate::scheduling::Direction::Produce,
            duration_ms: None,
        };
        study::review(&pool, "be", answer, Utc::now()).await.unwrap();
        let heard = listening(&pool).await.unwrap();
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].target, "I am at home.");
        assert_eq!(heard[0].native, "Я дома.");
        assert_eq!(heard[0].language, "en");
    }
}
