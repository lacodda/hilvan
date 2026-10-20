//! The sentence a substitution turn says.
//!
//! A formula's pattern is the scaffold on the card - "<pronoun> + am/is/are +
//! <rest>" - and its `say` is the sentence the scaffold stands for:
//! "<pronoun> <pronoun:be> <rest>." A hole is either a slot, filled with the
//! value chosen for it, or a slot and a form, filled with that value's form:
//! the pronoun "he" carries be = "is", so "<pronoun:be>" agrees with whoever
//! was picked. Agreement lives in the data, next to the word it belongs to,
//! and the code knows nothing about English.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use thiserror::Error;

/// One piece of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    /// `<slot>` or `<slot:form>`.
    Hole {
        slot: String,
        form: Option<String>,
    },
}

/// A parsed `say` template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pieces: Vec<Piece>,
}

/// A template that cannot be read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("{0:?} opens a < it never closes")]
    Unclosed(String),
    #[error("{0:?} has an empty hole <>")]
    EmptyHole(String),
}

/// What a value offers a template: the word itself and its forms.
pub trait Filling {
    fn word(&self) -> &str;
    fn form(&self, name: &str) -> Option<&str>;
}

impl Template {
    /// Reads a template.
    ///
    /// # Errors
    ///
    /// Fails on a `<` without its `>` and on an empty hole.
    pub fn parse(text: &str) -> Result<Self, TemplateError> {
        let mut pieces = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('<') {
            if open > 0 {
                pieces.push(Piece::Text(rest[..open].to_string()));
            }
            let after = &rest[open + 1..];
            let close = after.find('>').ok_or_else(|| TemplateError::Unclosed(text.to_string()))?;
            let inside = after[..close].trim();
            if inside.is_empty() {
                return Err(TemplateError::EmptyHole(text.to_string()));
            }
            let (slot, form) = match inside.split_once(':') {
                Some((slot, form)) => (slot.trim().to_string(), Some(form.trim().to_string())),
                None => (inside.to_string(), None),
            };
            pieces.push(Piece::Hole { slot, form });
            rest = &after[close + 1..];
        }
        if !rest.is_empty() {
            pieces.push(Piece::Text(rest.to_string()));
        }
        Ok(Self { pieces })
    }

    /// Every slot the template mentions.
    #[must_use]
    pub fn slots(&self) -> BTreeSet<&str> {
        self.pieces
            .iter()
            .filter_map(|piece| match piece {
                Piece::Hole { slot, .. } => Some(slot.as_str()),
                Piece::Text(_) => None,
            })
            .collect()
    }

    /// Every form the template asks of a slot, by slot.
    #[must_use]
    pub fn forms(&self) -> BTreeMap<&str, BTreeSet<&str>> {
        let mut forms: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for piece in &self.pieces {
            if let Piece::Hole { slot, form: Some(form) } = piece {
                forms.entry(slot.as_str()).or_default().insert(form.as_str());
            }
        }
        forms
    }

    /// The sentence, with every hole filled from the values chosen.
    ///
    /// The first letter is raised: "he" opens a statement as "He", and "is"
    /// opens a question as "Is". Returns `None` when a slot has no value
    /// chosen or a value lacks a form the template asks for - both of which
    /// the pack validator rules out before a learner can meet them.
    #[must_use]
    pub fn render<F: Filling>(&self, chosen: &HashMap<&str, &F>) -> Option<String> {
        self.render_marked(chosen, &BTreeSet::new()).map(|marked| marked.text)
    }

    /// The sentence, with where each character came from: the formula's own
    /// text, or a word that filled a hole.
    ///
    /// What the formula itself says is its fixed text, every form it asks
    /// for, and the value of a slot it agrees with - `agreeing` names those,
    /// the slots some template of the formula asks a form of: the pronoun
    /// that picks "am" or "is" is the formula's business. The plain value of
    /// any other slot - "at home", "a doctor" - is a word. A typed answer is
    /// read through this: a miss in the formula's own text is a failure of the
    /// shape, a miss in a word is a failure of that word.
    #[must_use]
    pub fn render_marked<F: Filling>(&self, chosen: &HashMap<&str, &F>, agreeing: &BTreeSet<&str>) -> Option<Marked> {
        let mut chars: Vec<(char, Role)> = Vec::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(text) => chars.extend(text.chars().map(|c| (c, Role::Formula))),
                Piece::Hole { slot, form } => {
                    let value = chosen.get(slot.as_str())?;
                    if let Some(form) = form {
                        chars.extend(value.form(form)?.chars().map(|c| (c, Role::Formula)));
                    } else {
                        let role = if agreeing.contains(slot.as_str()) { Role::Formula } else { Role::Word };
                        chars.extend(value.word().chars().map(|c| (c, role)));
                    }
                }
            }
        }
        Some(Marked::from_chars(tidy(chars)))
    }
}

/// Where a character of a rendered sentence came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The formula's own text, a form it asked for, or the slot it agrees with.
    Formula,
    /// The plain value of a slot: a word the formula carries.
    Word,
}

/// A rendered sentence, and the role of each of its characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    pub text: String,
    /// One per character of `text`, in order.
    pub roles: Vec<Role>,
}

impl Marked {
    fn from_chars(chars: Vec<(char, Role)>) -> Self {
        let mut chars = chars;
        if let Some(&(first, role)) = chars.first() {
            // "he" opens a sentence as "He" - and a letter whose capital is
            // two letters stays two, as the app raises it.
            let capital: Vec<(char, Role)> = first.to_uppercase().map(|c| (c, role)).collect();
            chars.splice(0..1, capital);
        }
        Self {
            text: chars.iter().map(|(c, _)| *c).collect(),
            roles: chars.into_iter().map(|(_, role)| role).collect(),
        }
    }
}

/// Collapses the doubled spaces an empty form leaves behind ("I  want"), and
/// the spaces at either end.
fn tidy(chars: Vec<(char, Role)>) -> Vec<(char, Role)> {
    let mut out: Vec<(char, Role)> = Vec::with_capacity(chars.len());
    for (c, role) in chars {
        if c == ' ' && out.last().is_none_or(|(last, _)| *last == ' ') {
            continue;
        }
        out.push((c, role));
    }
    while out.last().is_some_and(|(c, _)| *c == ' ') {
        out.pop();
    }
    out
}

/// Every sentence a template can say, over the values of each slot.
///
/// Small by construction - a pack has two or three slots of five to seven
/// values - and bounded anyway, so a pack with a runaway slot cannot turn a
/// check into a hang.
#[must_use]
pub fn every_sentence<'a, F: Filling>(template: &Template, slots: &'a [(&'a str, Vec<&'a F>)], limit: usize) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut chosen: HashMap<&str, &F> = HashMap::new();
    walk(template, slots, 0, &mut chosen, &mut sentences, limit);
    sentences
}

fn walk<'a, F: Filling>(
    template: &Template,
    slots: &'a [(&'a str, Vec<&'a F>)],
    at: usize,
    chosen: &mut HashMap<&'a str, &'a F>,
    out: &mut Vec<String>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    let Some((name, values)) = slots.get(at) else {
        if let Some(sentence) = template.render(chosen) {
            out.push(sentence);
        }
        return;
    };
    for value in values {
        chosen.insert(name, value);
        walk(template, slots, at + 1, chosen, out, limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Word(&'static str, &'static [(&'static str, &'static str)]);

    impl Filling for Word {
        fn word(&self) -> &str {
            self.0
        }
        fn form(&self, name: &str) -> Option<&str> {
            self.1.iter().find(|(form, _)| *form == name).map(|(_, text)| *text)
        }
    }

    const HE: Word = Word("he", &[("be", "is"), ("s", "s")]);
    const I: Word = Word("I", &[("be", "am"), ("s", "")]);
    const HOME: Word = Word("at home", &[]);

    #[test]
    fn a_form_agrees_with_the_value_chosen() {
        let template = Template::parse("<pronoun> <pronoun:be> <rest>.").unwrap();
        let chosen = HashMap::from([("pronoun", &HE), ("rest", &HOME)]);
        assert_eq!(template.render(&chosen).as_deref(), Some("He is at home."));

        let chosen = HashMap::from([("pronoun", &I), ("rest", &HOME)]);
        assert_eq!(template.render(&chosen).as_deref(), Some("I am at home."));
    }

    #[test]
    fn a_question_opens_with_a_capital() {
        let template = Template::parse("<pronoun:be> <pronoun> <rest>?").unwrap();
        let chosen = HashMap::from([("pronoun", &HE), ("rest", &HOME)]);
        assert_eq!(template.render(&chosen).as_deref(), Some("Is he at home?"));
    }

    #[test]
    fn a_form_can_be_a_suffix_and_an_empty_one_leaves_no_gap() {
        let template = Template::parse("<pronoun> want<pronoun:s> to go.").unwrap();
        let chosen = HashMap::from([("pronoun", &HE)]);
        assert_eq!(template.render(&chosen).as_deref(), Some("He wants to go."));
        let chosen = HashMap::from([("pronoun", &I)]);
        assert_eq!(template.render(&chosen).as_deref(), Some("I want to go."));
    }

    #[test]
    fn a_missing_form_renders_nothing_rather_than_half_a_sentence() {
        let template = Template::parse("<rest:be>").unwrap();
        let chosen = HashMap::from([("rest", &HOME)]);
        assert_eq!(template.render(&chosen), None);
    }

    #[test]
    fn the_template_says_which_slots_and_forms_it_needs() {
        let template = Template::parse("<pronoun:be> <pronoun> <rest>?").unwrap();
        assert_eq!(template.slots(), BTreeSet::from(["pronoun", "rest"]));
        assert_eq!(template.forms()["pronoun"], BTreeSet::from(["be"]));
        assert!(!template.forms().contains_key("rest"));
    }

    #[test]
    fn a_broken_template_is_refused() {
        assert!(matches!(Template::parse("<pronoun am"), Err(TemplateError::Unclosed(_))));
        assert!(matches!(Template::parse("<> am"), Err(TemplateError::EmptyHole(_))));
    }

    #[test]
    fn a_rendered_sentence_knows_which_characters_are_words() {
        // The pronoun agrees, so it is the formula's; "at home" fills a plain
        // hole, so it is a word; "is" is a form, and the full stop is text.
        let template = Template::parse("<pronoun> <pronoun:be> <rest>.").unwrap();
        let chosen = HashMap::from([("pronoun", &HE), ("rest", &HOME)]);
        let marked = template.render_marked(&chosen, &BTreeSet::from(["pronoun"])).unwrap();
        assert_eq!(marked.text, "He is at home.");
        let words: String = marked
            .text
            .chars()
            .zip(&marked.roles)
            .map(|(c, role)| if *role == Role::Word { c } else { '_' })
            .collect();
        assert_eq!(words, "______at home_");

        // A slot nobody agrees with is a word, whoever it is.
        let marked = template.render_marked(&chosen, &BTreeSet::new()).unwrap();
        assert_eq!(marked.roles[0], Role::Word);
        assert_eq!(marked.roles.len(), marked.text.chars().count());
    }

    #[test]
    fn an_empty_form_leaves_no_space_at_the_edges_either() {
        const NOBODY: Word = Word("", &[("s", "")]);
        let template = Template::parse("<pronoun> want<pronoun:s> to go.").unwrap();
        let chosen = HashMap::from([("pronoun", &NOBODY)]);
        assert_eq!(template.render(&chosen).as_deref(), Some("Want to go."));
    }

    #[test]
    fn every_sentence_walks_every_combination() {
        let template = Template::parse("<pronoun> <pronoun:be> <rest>.").unwrap();
        let slots = [("pronoun", vec![&HE, &I]), ("rest", vec![&HOME])];
        let sentences = every_sentence(&template, &slots, 100);
        assert_eq!(sentences, vec!["He is at home.", "I am at home."]);
        assert_eq!(every_sentence(&template, &slots, 1).len(), 1, "the limit should hold");
    }
}
