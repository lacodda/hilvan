//! An audio lesson, made the way Pimsleur's courses are made.
//!
//! A lesson is heard, not read: on a walk, hands free, offline. Each of its
//! phrases is asked the same way every time - the question, a pause long
//! enough to answer aloud, the answer, room to say the phrase after it, and
//! the phrase once more in the native voice - and each comes back inside the
//! lesson at growing intervals, so a phrase met at the start is asked again
//! before it fades (Pimsleur's graduated interval recall). A phrase the
//! learner has never met is heard first - its meaning, the phrase slowly,
//! the phrase at speed - and is asked more often than one already met.
//!
//! This is the plan only: which lines are said, by whom, in what order, with
//! how much silence between them, and where each phrase starts. How long a
//! line lasts comes from its sound, which the caller has; the server turns
//! the plan into one file. Nothing here touches a database, a clock or a
//! voice.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// The shortest pause left for an answer, in milliseconds.
pub const PAUSE_MIN_MS: i64 = 4_000;
/// The longest pause left for an answer.
pub const PAUSE_MAX_MS: i64 = 6_000;
/// What a pause holds on top of the phrase itself: the moment it takes to
/// find the words before saying them.
const PAUSE_BASE_MS: i64 = 3_000;

/// The shortest room left to say a phrase after the native voice.
pub const ECHO_MIN_MS: i64 = 1_500;
/// The longest room left to say a phrase after the native voice.
pub const ECHO_MAX_MS: i64 = 6_000;
/// What the room to say a phrase holds on top of the phrase itself.
const ECHO_BASE_MS: i64 = 1_000;

/// The silence between two lines that simply follow each other.
pub const BREATH_MS: i64 = 700;
/// The silence between one phrase and the next.
pub const GAP_MS: i64 = 1_500;

/// When a phrase heard for the first time is asked: this long after it was
/// last heard, each time - about twenty seconds, two minutes, eight.
pub const FRESH_RECALLS_MS: [i64; 3] = [20_000, 120_000, 480_000];
/// When a phrase already met is asked again after it was first asked: once,
/// five minutes on. The schedule brought it back because it was due; the
/// second asking is the lesson's own.
pub const MET_RECALLS_MS: [i64; 1] = [300_000];

/// Who says a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Voice {
    /// The learner's own language: a meaning, an explanation.
    Teacher,
    /// The language being learnt, said by a native speaker.
    Speaker,
}

/// One line the lesson says.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Line {
    pub voice: Voice,
    pub text: String,
    /// Said slowly, so every sound is heard: a phrase heard for the first
    /// time.
    pub slow: bool,
}

impl Line {
    fn teacher(text: &str) -> Self {
        Self {
            voice: Voice::Teacher,
            text: text.to_string(),
            slow: false,
        }
    }

    fn speaker(text: &str, slow: bool) -> Self {
        Self {
            voice: Voice::Speaker,
            text: text.to_string(),
            slow,
        }
    }
}

/// Which side of a phrase the lesson asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Asks {
    /// The meaning is heard and the phrase said: what Pimsleur asks.
    Produce,
    /// The phrase is heard and its meaning recalled.
    Recognise,
}

/// A phrase the lesson may ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// What it means, in the learner's own language.
    pub native: String,
    /// The phrase, in the language being learnt.
    pub target: String,
    pub asks: Asks,
    /// Never answered yet: heard before it is asked, and asked more often.
    pub fresh: bool,
    /// What the teacher says before a fresh phrase is first heard: a new
    /// formula's explanation.
    pub lead: Option<String>,
}

impl Item {
    /// Every line this phrase can be heard in, each once: what has to be
    /// said before it can be in a lesson.
    #[must_use]
    pub fn lines(&self) -> Vec<Line> {
        let mut lines = vec![Line::teacher(&self.native), Line::speaker(&self.target, false)];
        if self.fresh {
            lines.push(Line::speaker(&self.target, true));
            if let Some(lead) = &self.lead {
                lines.push(Line::teacher(lead));
            }
        }
        lines
    }

    /// The first hearing of a fresh phrase: its lead, its meaning, the
    /// phrase slowly with room to say it, and at speed with room again.
    fn heard(&self, length: &impl Fn(&Line) -> Option<i64>) -> Option<Vec<Part>> {
        let normal = Line::speaker(&self.target, false);
        let slow = Line::speaker(&self.target, true);
        let normal_ms = length(&normal)?;
        let slow_ms = length(&slow)?;
        let mut parts = Vec::new();
        if let Some(lead) = &self.lead {
            parts.push(Part::say(Line::teacher(lead)));
            parts.push(Part::Silence { ms: BREATH_MS });
        }
        parts.push(Part::say(Line::teacher(&self.native)));
        parts.push(Part::Silence { ms: BREATH_MS });
        parts.push(Part::say(slow));
        parts.push(Part::Silence { ms: echo_for(slow_ms) });
        parts.push(Part::say(normal));
        parts.push(Part::Silence { ms: echo_for(normal_ms) });
        Some(parts)
    }

    /// One asking: the question, a pause to answer in, the answer, room to
    /// say the phrase, and the phrase once more in the native voice.
    fn asked(&self, length: &impl Fn(&Line) -> Option<i64>) -> Option<Vec<Part>> {
        let target = Line::speaker(&self.target, false);
        let native = Line::teacher(&self.native);
        let target_ms = length(&target)?;
        let (question, answer) = match self.asks {
            Asks::Produce => (native, target.clone()),
            Asks::Recognise => (target.clone(), native),
        };
        Some(vec![
            Part::say(question),
            Part::Silence { ms: pause_for(target_ms) },
            Part::say(answer),
            Part::Silence { ms: echo_for(target_ms) },
            Part::say(target),
        ])
    }

    /// When this phrase is asked again, after each time it is heard.
    const fn recalls(&self) -> &'static [i64] {
        if self.fresh { &FRESH_RECALLS_MS } else { &MET_RECALLS_MS }
    }
}

/// The pause left to answer in, for a phrase this long: four to six seconds.
#[must_use]
pub const fn pause_for(phrase_ms: i64) -> i64 {
    clamp(PAUSE_BASE_MS + phrase_ms, PAUSE_MIN_MS, PAUSE_MAX_MS)
}

/// The room left to say a phrase this long after the native voice.
#[must_use]
pub const fn echo_for(phrase_ms: i64) -> i64 {
    clamp(ECHO_BASE_MS + phrase_ms, ECHO_MIN_MS, ECHO_MAX_MS)
}

const fn clamp(value: i64, min: i64, max: i64) -> i64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// One piece of the lesson's sound, in the order it is heard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    Say { line: Line },
    Silence { ms: i64 },
}

impl Part {
    const fn say(line: Line) -> Self {
        Self::Say { line }
    }
}

/// What a stretch of the lesson does with its phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Turn {
    /// A fresh phrase, heard for the first time and not yet asked.
    Heard,
    /// The phrase asked.
    Asked,
}

/// Where one phrase is heard or asked: a chapter of the lesson's file, and
/// what the player skips by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    /// The phrase, by its place among the items offered.
    pub item: usize,
    pub turn: Turn,
    pub start_ms: i64,
    /// When its last line ends; the silence before the next phrase is not
    /// its.
    pub end_ms: i64,
}

/// A lesson, planned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Everything heard, in order.
    pub parts: Vec<Part>,
    /// Every time a phrase is heard or asked, in order.
    pub chapters: Vec<Chapter>,
    /// The phrases the lesson holds, by their place among those offered,
    /// in the order they are first heard.
    pub items: Vec<usize>,
    /// How long it lasts.
    pub length_ms: i64,
}

/// One phrase with every stretch it will be heard in, costed.
struct Costed {
    item: usize,
    /// Its stretches in order, each with how long it lasts, the gap after
    /// it included.
    turns: Vec<(Turn, Vec<Part>, i64)>,
    recalls: &'static [i64],
}

impl Costed {
    fn of(index: usize, item: &Item, length: &impl Fn(&Line) -> Option<i64>) -> Option<Self> {
        let asked = item.asked(length)?;
        let mut turns = Vec::new();
        if item.fresh {
            turns.push((Turn::Heard, item.heard(length)?));
        }
        let asks = if item.fresh { item.recalls().len() } else { 1 + item.recalls().len() };
        turns.extend(std::iter::repeat_n((Turn::Asked, asked), asks));
        let turns = turns
            .into_iter()
            .map(|(turn, parts)| {
                let ms = duration(&parts, length)? + GAP_MS;
                Some((turn, parts, ms))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            item: index,
            turns,
            recalls: item.recalls(),
        })
    }

    fn total(&self) -> i64 {
        self.turns.iter().map(|(_, _, ms)| ms).sum()
    }
}

/// How long some parts last.
fn duration(parts: &[Part], length: &impl Fn(&Line) -> Option<i64>) -> Option<i64> {
    parts.iter().try_fold(0, |sum, part| {
        Some(
            sum + match part {
                Part::Say { line } => length(line)?,
                Part::Silence { ms } => *ms,
            },
        )
    })
}

/// The next time a phrase in the lesson is asked.
struct Waiting {
    /// When it is due, by the lesson's clock.
    at: i64,
    /// Which of the phrases in the lesson, by place in `placed`.
    phrase: usize,
}

/// Plans a lesson of at most `length_ms` from the phrases offered, in the
/// order offered: the schedule's order, with what is due first.
///
/// A phrase comes in when the whole of it - every time it will be heard -
/// still fits; one that does not is left for the cards, and a later, shorter
/// one may take its room. A phrase due again is asked as soon as it is due;
/// while none is, the next phrase comes in; when none is left to come in,
/// the phrase due soonest is asked early rather than leaving silence. A
/// phrase whose lines have no sound - `length` says `None` - cannot be said
/// and is left out.
#[must_use]
pub fn plan(items: &[Item], length_ms: i64, length: impl Fn(&Line) -> Option<i64>) -> Plan {
    let mut offered: VecDeque<Costed> = items.iter().enumerate().filter_map(|(index, item)| Costed::of(index, item, &length)).collect();
    let mut placed: Vec<(Costed, usize)> = Vec::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut committed = 0;
    let mut plan = Plan::default();

    loop {
        waiting.sort_by_key(|wait| (wait.at, wait.phrase));
        let now = plan.length_ms;
        let next = if waiting.first().is_some_and(|wait| wait.at <= now) {
            Some(waiting.remove(0).phrase)
        } else if let Some(costed) = take_fitting(&mut offered, length_ms - committed) {
            committed += costed.total();
            plan.items.push(costed.item);
            placed.push((costed, 0));
            Some(placed.len() - 1)
        } else if waiting.is_empty() {
            None
        } else {
            Some(waiting.remove(0).phrase)
        };
        let Some(phrase) = next else { break };

        let (costed, done) = &mut placed[phrase];
        let (turn, parts, ms) = &costed.turns[*done];
        let start_ms = plan.length_ms;
        plan.parts.extend(parts.iter().cloned());
        plan.parts.push(Part::Silence { ms: GAP_MS });
        plan.length_ms += ms;
        plan.chapters.push(Chapter {
            item: costed.item,
            turn: *turn,
            start_ms,
            end_ms: plan.length_ms - GAP_MS,
        });
        *done += 1;
        // Each stretch but the last is followed by a recall: a fresh
        // phrase's first asking comes the first interval after it was
        // heard, a phrase already met is asked again the first interval
        // after it was first asked.
        if *done < costed.turns.len() {
            let interval = costed.recalls.get(*done - 1).copied().unwrap_or(0);
            waiting.push(Waiting {
                at: plan.length_ms + interval,
                phrase,
            });
        }
    }

    // The last phrase is followed by nothing.
    if let Some(Part::Silence { ms }) = plan.parts.last()
        && *ms == GAP_MS
    {
        plan.parts.pop();
        plan.length_ms -= GAP_MS;
    }
    plan
}

/// The first phrase still offered that fits in `room`. Those before it are
/// dropped: the room only shrinks, so what does not fit now never will.
fn take_fitting(offered: &mut VecDeque<Costed>, room: i64) -> Option<Costed> {
    while let Some(costed) = offered.pop_front() {
        if costed.total() <= room {
            return Some(costed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line lasts 60 ms a character, and twice that slowly.
    #[allow(clippy::unnecessary_wraps)]
    fn measured(line: &Line) -> Option<i64> {
        let chars = i64::try_from(line.text.chars().count()).unwrap();
        Some(chars * if line.slow { 120 } else { 60 })
    }

    fn item(target: &str, fresh: bool) -> Item {
        Item {
            native: format!("<{target}>"),
            target: target.to_string(),
            asks: Asks::Produce,
            fresh,
            lead: None,
        }
    }

    fn said(parts: &[Part]) -> Vec<(Voice, &str, bool)> {
        parts
            .iter()
            .filter_map(|part| match part {
                Part::Say { line } => Some((line.voice, line.text.as_str(), line.slow)),
                Part::Silence { .. } => None,
            })
            .collect()
    }

    fn silences(parts: &[Part]) -> Vec<i64> {
        parts
            .iter()
            .filter_map(|part| match part {
                Part::Silence { ms } => Some(*ms),
                Part::Say { .. } => None,
            })
            .collect()
    }

    /// The parts of one chapter, by where it starts in the plan.
    fn parts_of(plan: &Plan, chapter: &Chapter) -> Vec<Part> {
        let mut at = 0;
        let mut out = Vec::new();
        for part in &plan.parts {
            let ms = match part {
                Part::Say { line } => measured(line).unwrap(),
                Part::Silence { ms } => *ms,
            };
            if at >= chapter.start_ms && at + ms <= chapter.end_ms {
                out.push(part.clone());
            }
            at += ms;
        }
        out
    }

    #[test]
    fn a_pause_leaves_four_to_six_seconds_to_answer() {
        assert_eq!(pause_for(200), PAUSE_MIN_MS, "a short phrase still gets four seconds");
        assert_eq!(pause_for(2_000), 5_000, "the pause grows with the phrase");
        assert_eq!(pause_for(9_000), PAUSE_MAX_MS, "and stops at six");
        assert_eq!(echo_for(100), ECHO_MIN_MS);
        assert_eq!(echo_for(2_000), 3_000);
        assert_eq!(echo_for(20_000), ECHO_MAX_MS);
    }

    #[test]
    fn a_phrase_is_asked_question_pause_answer_room_and_the_phrase_again() {
        let produce = item("I am at home.", false);
        let plan = plan(std::slice::from_ref(&produce), 600_000, measured);
        let first = plan.chapters[0];
        assert_eq!(first.turn, Turn::Asked, "a phrase already met is asked at once");
        let parts = parts_of(&plan, &first);
        assert_eq!(
            said(&parts),
            [
                (Voice::Teacher, "<I am at home.>", false),
                (Voice::Speaker, "I am at home.", false),
                (Voice::Speaker, "I am at home.", false),
            ],
            "the meaning, then the phrase, then the phrase again"
        );
        let target = measured(&Line::speaker("I am at home.", false)).unwrap();
        assert_eq!(silences(&parts), [pause_for(target), echo_for(target)]);

        let recognise = Item {
            asks: Asks::Recognise,
            ..produce
        };
        let plan = plan_of(&[recognise]);
        assert_eq!(
            said(&parts_of(&plan, &plan.chapters[0])),
            [
                (Voice::Speaker, "I am at home.", false),
                (Voice::Teacher, "<I am at home.>", false),
                (Voice::Speaker, "I am at home.", false),
            ],
            "understanding it: the phrase, its meaning, the phrase again"
        );
    }

    fn plan_of(items: &[Item]) -> Plan {
        plan(items, 3_600_000, measured)
    }

    #[test]
    fn a_fresh_phrase_is_heard_before_it_is_asked_and_asked_more_often() {
        let mut fresh = item("Let's go.", true);
        fresh.lead = Some("Let's - это предложение.".to_string());
        let plan = plan_of(&[fresh, item("I am at home.", false)]);

        let heard: Vec<&Chapter> = plan.chapters.iter().filter(|chapter| chapter.item == 0).collect();
        assert_eq!(heard[0].turn, Turn::Heard, "a fresh phrase starts by being heard");
        assert_eq!(
            said(&parts_of(&plan, heard[0])),
            [
                (Voice::Teacher, "Let's - это предложение.", false),
                (Voice::Teacher, "<Let's go.>", false),
                (Voice::Speaker, "Let's go.", true),
                (Voice::Speaker, "Let's go.", false),
            ],
            "the lead, the meaning, the phrase slowly, the phrase at speed"
        );
        assert_eq!(heard.len(), 1 + FRESH_RECALLS_MS.len());
        assert!(heard[1..].iter().all(|chapter| chapter.turn == Turn::Asked));

        let met: Vec<&Chapter> = plan.chapters.iter().filter(|chapter| chapter.item == 1).collect();
        assert_eq!(met.len(), 1 + MET_RECALLS_MS.len(), "a phrase already met is asked twice");
        assert!(met.iter().all(|chapter| chapter.turn == Turn::Asked));
    }

    /// Enough phrases already met to keep a lesson going between recalls.
    fn many(count: usize) -> Vec<Item> {
        (0..count).map(|n| item(&format!("Sentence number {n}."), false)).collect()
    }

    #[test]
    fn a_phrase_comes_back_at_growing_intervals_that_are_kept_while_there_is_material() {
        let mut items = vec![item("Let's go.", true)];
        items.extend(many(80));
        let plan = plan(&items, 1_500_000, measured);
        let fresh: Vec<&Chapter> = plan.chapters.iter().filter(|chapter| chapter.item == 0).collect();
        assert_eq!(fresh.len(), 4);
        let gaps: Vec<i64> = fresh.windows(2).map(|pair| pair[1].start_ms - (pair[0].end_ms + GAP_MS)).collect();
        for (gap, wanted) in gaps.iter().zip(FRESH_RECALLS_MS) {
            assert!(*gap >= wanted, "asked {gap} ms after the last time, before the {wanted} ms it is due");
            // Due while another phrase is being asked, it waits for that one
            // to end and no longer.
            assert!(
                *gap < wanted + 60_000,
                "asked {gap} ms after the last time, long after the {wanted} ms it was due"
            );
        }
        assert!(gaps.windows(2).all(|pair| pair[1] > pair[0]), "the intervals grow: {gaps:?}");

        // A phrase already met is asked again five minutes on.
        let met: Vec<&Chapter> = plan.chapters.iter().filter(|chapter| chapter.item == 1).collect();
        let gap = met[1].start_ms - (met[0].end_ms + GAP_MS);
        assert!(gap >= MET_RECALLS_MS[0] && gap < MET_RECALLS_MS[0] + 60_000, "asked again {gap} ms on");
    }

    #[test]
    fn the_lesson_keeps_to_its_length_and_fills_it() {
        let items = many(200);
        for minutes in [15, 25, 40] {
            let length = minutes * 60_000;
            let plan = plan(&items, length, measured);
            assert!(plan.length_ms <= length, "{minutes} minutes ran to {} ms", plan.length_ms);
            // Full but for less than one phrase's worth of room.
            assert!(plan.length_ms > length - 60_000, "{minutes} minutes held only {} ms", plan.length_ms);
            assert!(plan.items.len() < items.len(), "the rest is left for the cards");
        }
    }

    #[test]
    fn the_parts_add_up_to_the_length_and_the_chapters_follow_each_other() {
        let mut items = vec![item("Let's go.", true)];
        items.extend(many(30));
        let plan = plan(&items, 900_000, measured);
        let sum: i64 = plan
            .parts
            .iter()
            .map(|part| match part {
                Part::Say { line } => measured(line).unwrap(),
                Part::Silence { ms } => *ms,
            })
            .sum();
        assert_eq!(sum, plan.length_ms);
        assert_eq!(plan.chapters[0].start_ms, 0);
        assert_eq!(plan.chapters.last().unwrap().end_ms, plan.length_ms, "nothing trails the last phrase");
        for pair in plan.chapters.windows(2) {
            assert_eq!(pair[1].start_ms, pair[0].end_ms + GAP_MS, "a phrase follows the last after a gap");
        }
        assert!(!matches!(plan.parts.last(), Some(Part::Silence { .. })));
    }

    #[test]
    fn what_is_due_comes_first_and_the_order_offered_is_kept() {
        let items = many(10);
        let plan = plan_of(&items);
        assert_eq!(plan.items, (0..10).collect::<Vec<_>>());
        let first_heard: Vec<usize> = plan.chapters.iter().fold(Vec::new(), |mut seen, chapter| {
            if !seen.contains(&chapter.item) {
                seen.push(chapter.item);
            }
            seen
        });
        assert_eq!(first_heard, plan.items, "first heard in the order offered");
    }

    #[test]
    fn a_phrase_too_long_for_the_room_left_gives_way_to_a_shorter_one() {
        let long = item(&"a very long sentence indeed ".repeat(4), false);
        let short = item("Hi.", false);
        let room = Costed::of(1, &short, &measured).unwrap().total();
        let plan = plan(&[long, short], room, measured);
        assert_eq!(plan.items, [1], "the long phrase did not fit; the short one did");
    }

    #[test]
    fn a_phrase_that_cannot_be_said_is_left_out() {
        let silent = item("Nobody can say this.", false);
        let plan = plan(&[silent, item("Hi.", false)], 600_000, |line| {
            (!line.text.contains("Nobody")).then(|| measured(line).unwrap())
        });
        assert_eq!(plan.items, [1]);
    }

    #[test]
    fn a_phrase_is_not_asked_twice_running_while_others_are_still_coming_in() {
        let mut items = vec![item("Let's go.", true)];
        items.extend(many(6));
        let plan = plan_of(&items);
        let last = *plan.items.last().unwrap();
        let last_in = plan.chapters.iter().position(|chapter| chapter.item == last).unwrap();
        for (at, pair) in plan.chapters.windows(2).enumerate() {
            assert!(
                pair[0].item != pair[1].item || at >= last_in,
                "phrase {} asked twice running at {} ms, with phrases still to come in",
                pair[0].item,
                pair[1].start_ms
            );
        }
    }

    #[test]
    fn a_phrase_says_only_the_lines_it_names() {
        let mut fresh = item("Let's go.", true);
        fresh.lead = Some("Let's - это предложение.".to_string());
        let met = item("I am at home.", false);
        for item in [fresh, met] {
            let lines = item.lines();
            let plan = plan_of(std::slice::from_ref(&item));
            for part in &plan.parts {
                if let Part::Say { line } = part {
                    assert!(lines.contains(line), "{line:?} is said and was not named");
                }
            }
            let said: std::collections::BTreeSet<&Line> = plan
                .parts
                .iter()
                .filter_map(|part| match part {
                    Part::Say { line } => Some(line),
                    Part::Silence { .. } => None,
                })
                .collect();
            assert_eq!(said.len(), lines.len(), "a named line is never said: {lines:?}");
        }
    }

    #[test]
    fn nothing_offered_is_a_lesson_of_nothing() {
        let plan = plan_of(&[]);
        assert_eq!(plan, Plan::default());
    }
}
