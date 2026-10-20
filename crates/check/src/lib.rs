//! The core, as the tutor app calls it from WebAssembly.
//!
//! JSON in, JSON out, through two buffers in the module's own memory: the app
//! asks for an input buffer of the length it needs, writes the request into
//! it, calls the function, and reads the reply from the output buffer. No
//! bindings generator - the whole interface is four exports, and a tool that
//! has to match a library version on every machine that builds the app costs
//! more than the twenty lines of JavaScript it saves (`web/src/core.ts`).
//!
//! A request the app could not have meant - JSON that does not parse, a turn
//! the formula does not ask - is answered with `{"error": ...}`, never a
//! trap: a checker that brings the page down mid-sitting is worse than one
//! that says it could not check.

use std::cell::RefCell;

use hilvan_core::answer::{Checked, Failure};
use hilvan_core::check::{self, Formula, Turn};
use hilvan_core::rating::Rating;
use serde::{Deserialize, Serialize};

/// What `check` is asked: a formula as the server sent it, and one turn.
#[derive(Deserialize)]
struct CheckRequest {
    formula: Formula,
    turn: Turn,
}

/// What `rate` is asked: what went wrong in each turn, and whether the
/// learner said a flawless round came without thinking.
#[derive(Deserialize)]
struct RateRequest {
    failures: Vec<Option<Failure>>,
    easy: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Reply {
    Checked(Checked),
    Rating(Rating),
    Error(String),
}

/// Reads a request, runs it, and writes the reply.
fn answer<T: for<'de> Deserialize<'de>>(request: &[u8], run: impl FnOnce(T) -> Reply) -> Vec<u8> {
    let reply = match serde_json::from_slice::<T>(request) {
        Ok(request) => run(request),
        Err(error) => Reply::Error(format!("the request could not be read: {error}")),
    };
    // A reply of strings, booleans and lists: there is nothing in it that
    // could fail to serialise.
    serde_json::to_vec(&reply).unwrap_or_else(|_| br#"{"error":"the reply could not be written"}"#.to_vec())
}

/// One typed turn, checked against the formula that asked it.
#[must_use]
pub fn check_json(request: &[u8]) -> Vec<u8> {
    answer(request, |request: CheckRequest| match check::judge(&request.formula.asking(), &request.turn) {
        Ok((_, checked)) => Reply::Checked(checked),
        Err(refused) => Reply::Error(refused.to_string()),
    })
}

/// The grade a round of turns earns.
#[must_use]
pub fn rate_json(request: &[u8]) -> Vec<u8> {
    answer(request, |request: RateRequest| Reply::Rating(check::rate(&request.failures, request.easy)))
}

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Runs a request held in the input buffer, leaving the reply in the output
/// buffer; the reply's length.
fn run(handle: fn(&[u8]) -> Vec<u8>) -> usize {
    let reply = INPUT.with_borrow(|input| handle(input));
    let length = reply.len();
    OUTPUT.set(reply);
    length
}

/// The exports. Creating a pointer is safe; the app is the one that reads
/// and writes through it, inside the module's memory.
#[allow(unsafe_code)]
mod exports {
    use super::{INPUT, OUTPUT, check_json, rate_json, run};

    /// Makes the input buffer `length` bytes long; where to write it.
    #[unsafe(no_mangle)]
    pub extern "C" fn input(length: usize) -> *mut u8 {
        INPUT.with_borrow_mut(|input| {
            input.clear();
            input.resize(length, 0);
            input.as_mut_ptr()
        })
    }

    /// Checks the turn in the input buffer; the length of the reply.
    #[unsafe(no_mangle)]
    pub extern "C" fn check() -> usize {
        run(check_json)
    }

    /// Grades the round in the input buffer; the length of the reply.
    #[unsafe(no_mangle)]
    pub extern "C" fn rate() -> usize {
        run(rate_json)
    }

    /// Where the last reply starts.
    #[unsafe(no_mangle)]
    pub extern "C" fn output() -> *const u8 {
        OUTPUT.with_borrow(Vec::as_ptr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("the reply is JSON")
    }

    const FORMULA: &str = r#"{
        "id": "be-at", "name": "Where someone is", "pattern": "<pronoun> + am/is/are + <rest>",
        "say": "<pronoun> <pronoun:be> <rest>.", "accept": [], "explanation": "",
        "samples": [{ "sentence": 7, "native": "Я дома.", "target": "I'm at home.",
                      "words": [{ "word": "en:home", "lemma": "home", "form": "home", "start": 8 }], "accept": [] }],
        "slots": [{ "name": "pronoun", "takes": null, "values": [{ "native": "он", "target": "he", "forms": { "be": "is" }, "own": false }] },
                  { "name": "rest", "takes": null, "values": [{ "native": "на работе", "target": "at work", "forms": {}, "own": false }] }]
    }"#;

    #[test]
    fn a_turn_is_checked_as_the_server_would() {
        let request =
            format!(r#"{{ "formula": {FORMULA}, "turn": {{ "asked": {{ "values": {{ "pronoun": "he", "rest": "at work" }} }}, "typed": "He is at wrk" }} }}"#);
        let reply = reply(&check_json(request.as_bytes()));
        assert_eq!(reply["checked"]["correct"], false);
        assert_eq!(reply["checked"]["failure"], "word");
        assert_eq!(reply["checked"]["missed"][0], "work");
    }

    #[test]
    fn a_turn_the_formula_does_not_ask_is_an_error_not_a_trap() {
        let request = format!(r#"{{ "formula": {FORMULA}, "turn": {{ "asked": {{ "sample": 8 }}, "typed": "x" }} }}"#);
        assert_eq!(reply(&check_json(request.as_bytes()))["error"], "formula be-at shows no sentence 8");
        assert!(
            reply(&check_json(b"not json"))["error"]
                .as_str()
                .unwrap()
                .starts_with("the request could not be read")
        );
    }

    #[test]
    fn a_round_is_graded_as_the_server_would() {
        assert_eq!(reply(&rate_json(br#"{ "failures": [null, "order"], "easy": false }"#))["rating"], "again");
        assert_eq!(reply(&rate_json(br#"{ "failures": [null, null], "easy": true }"#))["rating"], "easy");
    }

    #[test]
    fn the_buffers_carry_a_request_in_and_a_reply_out() {
        let request = br#"{ "failures": [null], "easy": false }"#;
        let address = exports::input(request.len());
        assert!(!address.is_null());
        INPUT.with_borrow_mut(|input| input.copy_from_slice(request));
        let length = exports::rate();
        let written = OUTPUT.with_borrow(Clone::clone);
        assert_eq!(written.len(), length);
        assert_eq!(reply(&written)["rating"], "good");
        assert_eq!(exports::output(), OUTPUT.with_borrow(Vec::as_ptr));
    }
}
