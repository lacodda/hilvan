//! A typed turn, read against the formula that asked it, and the grade a
//! round of turns earns.
//!
//! The phone runs this the moment a turn is typed, to say whether it was
//! right; the server runs it again over every turn when the answer arrives,
//! and that second reading is the one the grade rests on. One function, so
//! the two cannot disagree.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::answer::{self, Checked, Failure, Variant};
use crate::material::{Sample, Slot, Value};
use crate::rating::Rating;
use crate::say::Template;

/// One typed turn: what was asked, and what the learner typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub asked: Asked,
    pub typed: String,
}

/// Which question a turn asked: one of the formula's own sentences, or a
/// substitution, named by the value it put in each slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Asked {
    /// The id of a sentence the formula shows.
    Sample(i64),
    /// Slot name to the `target` of the value chosen for it.
    Values(BTreeMap<String, String>),
}

/// What a turn is read against: the parts of a formula a check needs,
/// borrowed from wherever the formula is held.
#[derive(Debug, Clone, Copy)]
pub struct Asking<'a> {
    pub id: &'a str,
    pub pattern: &'a str,
    pub say: Option<&'a str>,
    pub accept: &'a [String],
    pub samples: &'a [Sample],
    pub slots: &'a [Slot],
}

/// A formula as the app holds it - the JSON the server sends - read for its
/// check. Everything else the formula carries is left where it is.
#[derive(Debug, Clone, Deserialize)]
pub struct Formula {
    pub id: String,
    pub pattern: String,
    pub say: Option<String>,
    #[serde(default)]
    pub accept: Vec<String>,
    pub samples: Vec<Sample>,
    pub slots: Vec<Slot>,
}

impl Formula {
    /// The formula, as a check reads it.
    #[must_use]
    pub fn asking(&self) -> Asking<'_> {
        Asking {
            id: &self.id,
            pattern: &self.pattern,
            say: self.say.as_deref(),
            accept: &self.accept,
            samples: &self.samples,
            slots: &self.slots,
        }
    }
}

/// A turn that names something the formula does not ask: a client out of
/// step with the pack.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("formula {formula} {why}")]
pub struct Unanswerable {
    pub formula: String,
    pub why: String,
}

/// One turn, read against the right answers the formula gives for it: the
/// prompt it showed, in the learner's own language, and the verdict.
///
/// # Errors
///
/// `Unanswerable` when the turn asks a sentence the formula does not show, a
/// slot it does not have, a value a slot does not hold, or a substitution of
/// a formula that has nothing to substitute.
pub fn judge(formula: &Asking<'_>, turn: &Turn) -> Result<(String, Checked), Unanswerable> {
    let unanswerable = |why: String| Unanswerable {
        formula: formula.id.to_string(),
        why,
    };
    match &turn.asked {
        Asked::Sample(sentence) => {
            let sample = formula
                .samples
                .iter()
                .find(|sample| sample.sentence == *sentence)
                .ok_or_else(|| unanswerable(format!("shows no sentence {sentence}")))?;
            let marks: Vec<&str> = sample.words.iter().map(|mark| mark.form.as_str()).collect();
            let variants: Vec<Variant> = std::iter::once(&sample.target)
                .chain(&sample.accept)
                .map(|text| Variant::from_sentence(text, &marks))
                .collect();
            Ok((sample.native.clone(), answer::check(&variants, &turn.typed)))
        }
        Asked::Values(values) => {
            let say = formula.say.ok_or_else(|| unanswerable("has nothing to substitute".to_string()))?;
            if let Some(name) = values.keys().find(|name| !formula.slots.iter().any(|slot| &slot.name == *name)) {
                return Err(unanswerable(format!("has no slot called {name:?}")));
            }
            let mut chosen: HashMap<&str, &Value> = HashMap::new();
            let mut prompt = formula.pattern.to_string();
            for slot in formula.slots {
                let target = values
                    .get(&slot.name)
                    .ok_or_else(|| unanswerable(format!("was asked with nothing in {:?}", slot.name)))?;
                let value = slot
                    .values
                    .iter()
                    .find(|value| &value.target == target)
                    .ok_or_else(|| unanswerable(format!("has no {target:?} in {:?}", slot.name)))?;
                chosen.insert(slot.name.as_str(), value);
                prompt = prompt.replace(&format!("<{}>", slot.name), &value.native);
            }
            let templates = std::iter::once(say)
                .chain(formula.accept.iter().map(String::as_str))
                .map(Template::parse)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| unanswerable(format!("has a template that cannot be read: {error}")))?;
            // The slots the formula agrees with are its own, whichever of its
            // templates asks the form.
            let agreeing: BTreeSet<&str> = templates.iter().flat_map(|template| template.forms().into_keys()).collect();
            let variants = templates
                .iter()
                .map(|template| template.render_marked(&chosen, &agreeing).map(|marked| Variant::from_marked(&marked)))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| unanswerable("has a value without a form its templates ask for".to_string()))?;
            Ok((prompt, answer::check(&variants, &turn.typed)))
        }
    }
}

/// The grade a said answer earns, from what went wrong in its turns.
///
/// A slip of the shape - its own words or their order - in half the turns
/// or more means it is not there yet: again. One slip is effort: hard. No
/// slip is good, and a word that did not come is the word's failure, not
/// the formula's. Easy only on the learner's word and only for turns with
/// nothing wrong at all: whether it cost thought is the one thing typing
/// does not show, and the time it took is shown, never graded on (ADR 0005).
#[must_use]
pub fn rate(failures: &[Option<Failure>], easy: bool) -> Rating {
    let slips = failures.iter().filter(|failure| failure.is_some_and(Failure::is_the_shapes)).count();
    if slips > 0 && slips * 2 >= failures.len() {
        Rating::Again
    } else if slips > 0 {
        Rating::Hard
    } else if easy && failures.iter().all(Option::is_none) {
        Rating::Easy
    } else {
        Rating::Good
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::Failure::{Formula as Shape, Order, Word};
    use crate::material::Mark;

    /// "Я дома" and "<pronoun> + am/is/are + <rest>", as the server sends
    /// them: the JSON the app hands the checker.
    fn at_home() -> Formula {
        serde_json::from_value(serde_json::json!({
            "id": "be-at",
            "name": "Where someone is",
            "pattern": "<pronoun> + am/is/are + <rest>",
            "say": "<pronoun> <pronoun:be> <rest>.",
            "accept": ["<pronoun:be-short> <rest>."],
            "explanation": "unused by the check",
            "samples": [{
                "sentence": 7,
                "native": "Я дома.",
                "target": "I'm at home.",
                "words": [{ "word": "en:home", "lemma": "home", "form": "home", "start": 8 }],
                "accept": ["I am at home."]
            }],
            "slots": [
                { "name": "pronoun", "takes": null, "values": [
                    { "native": "он", "target": "he", "forms": { "be": "is", "be-short": "he's" }, "own": false }
                ]},
                { "name": "rest", "takes": "noun", "values": [
                    { "native": "на работе", "target": "at work", "forms": {}, "own": false }
                ]}
            ],
            "sisters": [],
            "parents": []
        }))
        .expect("the formula JSON the server sends")
    }

    fn values(pairs: &[(&str, &str)]) -> Asked {
        Asked::Values(pairs.iter().map(|(slot, value)| ((*slot).to_string(), (*value).to_string())).collect())
    }

    #[test]
    fn a_sample_is_read_against_its_sentence_and_its_variants() {
        let formula = at_home();
        let turn = Turn {
            asked: Asked::Sample(7),
            typed: "I am at home".to_string(),
        };
        let (prompt, checked) = judge(&formula.asking(), &turn).unwrap();
        assert_eq!(prompt, "Я дома.");
        assert!(checked.correct, "an accepted spelling is right");
    }

    #[test]
    fn a_substitution_says_its_prompt_and_reads_the_contraction_too() {
        let formula = at_home();
        let turn = Turn {
            asked: values(&[("pronoun", "he"), ("rest", "at work")]),
            typed: "He's at work.".to_string(),
        };
        let (prompt, checked) = judge(&formula.asking(), &turn).unwrap();
        assert_eq!(prompt, "он + am/is/are + на работе");
        assert!(checked.correct);
        assert_eq!(checked.answer, "He is at work.");
    }

    #[test]
    fn a_word_the_shape_carries_is_the_words_and_the_pronoun_is_the_shapes() {
        let formula = at_home();
        let asked = values(&[("pronoun", "he"), ("rest", "at work")]);
        let word = judge(
            &formula.asking(),
            &Turn {
                asked: asked.clone(),
                typed: "He is at wrk".to_string(),
            },
        )
        .unwrap()
        .1;
        assert_eq!(word.failure, Some(Word));
        let shape = judge(
            &formula.asking(),
            &Turn {
                asked,
                typed: "She is at work".to_string(),
            },
        )
        .unwrap()
        .1;
        assert_eq!(shape.failure, Some(Shape), "the pronoun the formula agrees with is its own");
    }

    #[test]
    fn a_turn_the_formula_does_not_ask_is_refused_in_words() {
        let formula = at_home();
        let asking = formula.asking();
        let refused = |asked: Asked| {
            judge(
                &asking,
                &Turn {
                    asked,
                    typed: "anything".to_string(),
                },
            )
            .unwrap_err()
            .to_string()
        };
        assert_eq!(refused(Asked::Sample(8)), "formula be-at shows no sentence 8");
        assert!(refused(values(&[("pronoun", "he")])).contains("nothing in \"rest\""));
        assert!(refused(values(&[("pronoun", "he"), ("rest", "at work"), ("when", "now")])).contains("no slot called \"when\""));
        assert!(refused(values(&[("pronoun", "we"), ("rest", "at work")])).contains("has no \"we\""));
    }

    #[test]
    fn a_formula_without_say_has_nothing_to_substitute() {
        let mut formula = at_home();
        formula.say = None;
        let turn = Turn {
            asked: values(&[("pronoun", "he"), ("rest", "at work")]),
            typed: "He is at work".to_string(),
        };
        assert!(judge(&formula.asking(), &turn).unwrap_err().why.contains("nothing to substitute"));
    }

    #[test]
    fn the_grade_follows_the_slips_of_the_shape_only() {
        assert_eq!(rate(&[None, None], false), Rating::Good);
        assert_eq!(rate(&[None, None], true), Rating::Easy);
        assert_eq!(
            rate(&[None, Some(Word)], true),
            Rating::Good,
            "a word's failure is not the formula's, and is not easy either"
        );
        assert_eq!(rate(&[None, None, None, Some(Order)], false), Rating::Hard);
        assert_eq!(rate(&[None, None, Some(Shape), Some(Order)], false), Rating::Again);
        assert_eq!(rate(&[Some(Shape)], true), Rating::Again);
    }

    #[test]
    fn the_material_reads_back_what_it_writes() {
        let sample = Sample {
            sentence: 1,
            native: "Я дома.".to_string(),
            target: "I'm at home.".to_string(),
            words: vec![Mark {
                word: "en:home".to_string(),
                lemma: "home".to_string(),
                form: "home".to_string(),
                start: 8,
            }],
            accept: vec![],
        };
        let json = serde_json::to_string(&sample).unwrap();
        assert_eq!(serde_json::from_str::<Sample>(&json).unwrap(), sample);
    }
}
