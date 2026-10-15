//! What the learner does today.
//!
//! Two questions, and the whole of the product is the honest answer to them:
//! what is waiting to be reviewed, and what new thing is worth starting. The
//! scheduling itself lives in [`crate::scheduling`]; this module is the part
//! that knows about formulas, days and the size of a sitting, and
//! [`crate::words`] the part that knows about words.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::answer::{self, Checked, Failure, Variant};
use crate::breaks::{self, Taken};
use crate::drift::{self, Note};
use crate::lexicon;
use crate::own;
use crate::pack::{Form, OwnKind};
use crate::say::{Filling, Template};
use crate::scheduling::{self, Break, Card, Direction, Kind, Rating, Scheduled, Scheduler, Source, Stitch};
use crate::words::{self, Standing, Word};

/// How many formulas start on any one day.
///
/// One (the plan's "formula of the day"): a formula is a shape you assemble
/// until it stops needing thought, and two new ones in a day means neither
/// gets that. This is a product decision, not a tuning knob.
pub const NEW_FORMULAS_PER_DAY: usize = 1;

/// How many words start on any one day.
///
/// Ten: a formula of the day brings five or six worked examples and a word
/// or two in each, so ten is enough that the words of today's formula are
/// rarely held back, and few enough that a pack with a long list cannot
/// bury a sitting in vocabulary. A product decision, like the formula's one;
/// the queue opens the commonest first.
///
/// The most a day opens, not what it always opens: what is left over from
/// earlier days takes its places first (see [`Room`]).
pub const NEW_WORDS_PER_DAY: usize = 10;

/// How long a card may wait after it comes due before it is owed.
///
/// A day: a card that came due an hour after yesterday's sitting could not
/// have been answered in it, and the learner who studies every evening must
/// never be in debt for the clock drifting past their sitting. One that has
/// waited through a whole day the learner could have answered it on is.
const GRACE: Duration = Duration::days(1);

/// A formula with everything the drill needs to run it.
#[derive(Debug, Clone, Serialize)]
pub struct Formula {
    pub id: String,
    pub name: String,
    pub pattern: String,
    /// The sentence a substitution says, with `<slot>` and `<slot:form>`
    /// holes (see [`crate::say`]). `None` for a formula without slots.
    pub say: Option<String>,
    /// In the learner's native language: the pack carries it, the code does not.
    pub explanation: String,
    pub samples: Vec<Sample>,
    pub slots: Vec<Slot>,
    /// The shape this is one form of, when it is one of several.
    pub family: Option<String>,
    pub form: Option<Form>,
    /// The other forms of the same shape, in statement-negation-question
    /// order: what the switch on the card offers. Empty when the formula
    /// stands alone.
    pub sisters: Vec<Sister>,
    /// Other templates as right as `say`: "<pronoun>'d like <thing>." A
    /// typed answer matching any of them is right.
    pub accept: Vec<String>,
    /// The formulas this one stands on, in the pack's order, with where
    /// each stands: the card names them, and a formula opens only once all
    /// of them are sewn.
    pub parents: Vec<Parent>,
    /// Which language each side is in: what the drill asks the voice for.
    pub languages: Languages,
}

/// A formula another stands on, as its child shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Parent {
    pub id: String,
    pub name: String,
    /// Where it stands, producing side.
    pub stitch: Stitch,
}

/// The two languages of a formula, ISO 639-1: `native` for prompts and
/// explanations, `target` for what is being learnt.
#[derive(Debug, Clone, Serialize)]
pub struct Languages {
    pub native: String,
    pub target: String,
}

/// One other form of the same shape, as the switch shows it.
///
/// Enough to draw the switch and to move to that form without a second round
/// trip; the drill fetches the full formula when the learner actually
/// switches.
#[derive(Debug, Clone, Serialize)]
pub struct Sister {
    pub id: String,
    pub form: Form,
    pub name: String,
    pub pattern: String,
    /// Where that form stands - so the switch can show that the question is
    /// still new while the statement is sewn.
    pub stitch: Stitch,
}

/// A worked example of a formula: a sentence, and what it means.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    /// The sentence's id: the same sentence shown by another formula, or
    /// heard as a word's anchor, is the same id.
    pub sentence: i64,
    /// The prompt, in the learner's native language.
    pub native: String,
    /// The answer, in the language being learnt.
    pub target: String,
    /// The words the sentence teaches, where they stand in `target`.
    pub words: Vec<Mark>,
    /// Other spellings of `target` just as right: "I am not hungry." for
    /// "I'm not hungry.".
    pub accept: Vec<String>,
}

/// A word marked in a sentence.
#[derive(Debug, Clone, Serialize)]
pub struct Mark {
    /// The word's id, `en:doctor`.
    pub word: String,
    pub lemma: String,
    /// How it is spelt here: "went" for go.
    pub form: String,
    /// The character it starts at.
    pub start: i64,
}

/// A hole in the pattern, with what can go in it.
#[derive(Debug, Clone, Serialize)]
pub struct Slot {
    pub name: String,
    /// Which kind of the learner's own words the slot takes, when it takes
    /// any: they stand among its values, marked `own`.
    pub takes: Option<OwnKind>,
    pub values: Vec<Value>,
}

/// One filling for a slot, with the forms that agree with it.
#[derive(Debug, Clone, Serialize)]
pub struct Value {
    pub native: String,
    pub target: String,
    /// What `say` picks from for agreement: `be` = "is" on "he".
    pub forms: BTreeMap<String, String>,
    /// Whether this is one of the learner's own words rather than the pack's.
    pub own: bool,
}

impl Filling for Value {
    fn word(&self) -> &str {
        &self.target
    }

    fn form(&self, name: &str) -> Option<&str> {
        self.forms.get(name).map(String::as_str)
    }
}

/// What one item of the queue is about.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Subject {
    /// A formula, asked one way round.
    Formula {
        formula: Formula,
        /// Which way round this turn asks it.
        direction: Direction,
    },
    /// A word, asked in its sentence.
    Word { word: Word },
}

/// One item in today's queue: a formula in a direction, or a word, and the
/// state it is in.
#[derive(Debug, Clone, Serialize)]
pub struct Due {
    #[serde(flatten)]
    pub subject: Subject,
    pub stitch: Stitch,
    /// True when this is being seen for the first time - a formula in this
    /// direction, or a word.
    pub is_new: bool,
    pub due: DateTime<Utc>,
    /// How fast this card usually comes, when it has come often enough to
    /// say. Shown on the card; the scheduler does not read it.
    pub pace: Option<Pace>,
    /// How firmly it holds: FSRS's stability, the days until recall is
    /// predicted to fall to 90%. 0 for a card never answered. The card draws
    /// it as a bar, not as the number.
    pub stability: f64,
}

/// How quickly a card is answered, as the second dimension of knowing it.
///
/// Stability says whether the formula is still there; pace says whether it
/// still costs thought. A formula answered right after eight seconds of
/// assembling it is not the same as one that arrives - and FSRS cannot see
/// the difference, because both are "good".
///
/// Reported, not scheduled on (owner's decision, 2026-09-03): a home-made
/// correction on top of FSRS would move every interval with no way to tell
/// what moved them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pace {
    /// The median of the last few answers, in milliseconds.
    pub typical_ms: i64,
    /// The most recent answer, in milliseconds.
    pub last_ms: i64,
    /// How many timed answers the median rests on.
    pub answers: i64,
}

/// How many timed answers a pace needs before it is shown.
///
/// Three: one is noise, two cannot have a middle, and a number the learner
/// cannot trust is worse than no number.
const PACE_MIN_ANSWERS: usize = 3;

/// How many recent answers the typical pace is taken over.
const PACE_WINDOW: i64 = 8;

/// How many formulas, or words, stand in each state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub new: i64,
    pub basted: i64,
    pub sewn: i64,
}

impl Counts {
    /// Counts one more card in the state it stands in.
    pub const fn add(&mut self, stitch: Stitch) {
        match stitch {
            Stitch::New => self.new += 1,
            Stitch::Basted => self.basted += 1,
            Stitch::Sewn => self.sewn += 1,
        }
    }
}

/// The same counts told twice, once per direction.
///
/// Recognition runs ahead of production - it always does - and the two
/// columns side by side are the honest picture of where a learner stands.
/// One averaged number would hide exactly the gap worth seeing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    /// Saying it: native prompt, target answer.
    pub produce: Counts,
    /// Understanding it: target prompt, native answer.
    pub recognise: Counts,
}

/// The answer to "what am I doing today".
#[derive(Debug, Clone, Serialize)]
pub struct Today {
    /// Formulas waiting to come back and at most one new one, then words
    /// waiting to come back and the new ones the day has room for.
    pub queue: Vec<Due>,
    /// Answers given since midnight UTC, formulas and words.
    pub reviewed_today: i64,
    /// Formulas by state, producing side.
    pub counts: Counts,
    /// The same standing, split by direction.
    pub progress: Progress,
    /// Where the learner stands with words.
    pub words: Standing,
    /// The next formula the pack's sequence would open, when it is not in
    /// today's queue, and the parents it is waiting on. `None` once every
    /// formula is started.
    pub next: Option<Next>,
    /// What the day has room for that is new, and what took the rest.
    pub room: Room,
    /// The break the learner is on. While there is one the queue is empty:
    /// the schedule waits.
    pub on_break: Option<Taken>,
    /// The last answer, when the learner has been away since it for
    /// [`breaks::AWAY_DAYS`] or more, no break covers the time, and
    /// something is waiting: an absence the learner can count as a break.
    pub away_since: Option<DateTime<Utc>>,
    /// What slipped last week, while this week has had no answer yet: the
    /// note the first sitting of a week opens on.
    pub last_week: Option<Note>,
}

/// What a day has room for that is new.
///
/// A card left over from an earlier day takes the place of a new one: a
/// learner who missed two evenings comes back to the cards they missed, not
/// to those and ten new words on top - the slide every course product
/// starts down, where the reviews outgrow the sitting and the learner stops
/// coming. Counted as the day began, so a day's room is the day's: clearing
/// what was owed does not open another round, and "done for today" stays
/// done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Room {
    /// Cards that had waited through a whole day they could have been
    /// answered on, as the day began.
    pub debt: i64,
    /// New words the day opens, all told: [`NEW_WORDS_PER_DAY`], one fewer
    /// for every card of debt.
    pub words: i64,
    /// New formulas the day opens: the formula of the day while the debt
    /// leaves room for a word, none once it has taken all ten - a new shape
    /// brings its sentences' words, and there would be no room for them.
    pub formulas: i64,
}

/// The formula next in line, and what holds it.
#[derive(Debug, Clone, Serialize)]
pub struct Next {
    pub id: String,
    pub name: String,
    /// Its parents not yet sewn. Empty when it waits only for a day with
    /// room for a new formula.
    pub waits_for: Vec<Parent>,
}

/// What the learner sends back after answering a formula.
///
/// Saying it is typed and checked: the turns are the answer, and the
/// machine grades them. Understanding it is heard and graded by the learner,
/// because what the learner understood is not something they can type.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "direction", rename_all = "lowercase")]
pub enum Answer {
    Produce {
        /// Every turn the learner typed, in order.
        turns: Vec<Turn>,
        /// The learner's word that it came without thinking. Honoured only
        /// when every turn was right: the one thing the machine cannot see.
        #[serde(default)]
        easy: bool,
        /// How long the answer took, when the client measured it. Shown to
        /// the learner, never fed to the scheduler.
        #[serde(default)]
        duration_ms: Option<i64>,
    },
    Recognise {
        rating: Rating,
        #[serde(default)]
        duration_ms: Option<i64>,
    },
}

/// One typed turn: what was asked, and what the learner typed.
#[derive(Debug, Clone, Deserialize)]
pub struct Turn {
    pub asked: Asked,
    pub typed: String,
}

/// Which question a turn asked: one of the formula's own sentences, or a
/// substitution, named by the value it put in each slot.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Asked {
    /// The id of a sentence the formula shows.
    Sample(i64),
    /// Slot name to the `target` of the value chosen for it.
    Values(BTreeMap<String, String>),
}

/// Why an answer could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum AnswerError {
    #[error("there is no {0}")]
    NotFound(String),
    #[error("{0}")]
    Unanswerable(String),
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

/// What the learner sends back after answering a word: how it went, and
/// how long it took. A word has one card, so there is no direction to say.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Graded {
    pub rating: Rating,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

/// What an answer changed.
#[derive(Debug, Clone, Serialize)]
pub struct Reviewed {
    pub stitch: Stitch,
    pub due: DateTime<Utc>,
    /// Days until it comes back.
    pub interval_days: i64,
    /// How this answer compared with the usual pace of this card, when there
    /// is a usual pace to compare with.
    pub pace: Option<Pace>,
    /// The grade the answer was given - by the learner when heard, by the
    /// typed turns when said.
    pub rating: Rating,
    /// The words whose failure was sent to their own cards: a typed answer
    /// that got the shape right and a word wrong.
    pub words: Vec<String>,
    /// How firmly it holds now, and how firmly it held before the answer:
    /// FSRS's stability, in days. The card shows the bar move.
    pub stability: f64,
    pub stability_before: f64,
}

/// Today's queue: formulas, then words; in each, everything due and then
/// the new ones the day has room for.
///
/// Reviews come first and are never crowded out by new material - a day where
/// the learner starts something new while forgetting yesterday's is the
/// failure mode every course product has.
///
/// Both directions of a formula queue independently: understanding a shape
/// and producing it come back on their own schedules, and the recognising
/// card of a formula the learner has never produced is not offered - you
/// cannot be asked to understand what you were never shown. A word opens
/// the same way: only once a sentence that holds it has been met.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn today(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Today> {
    let counts = counts(pool).await?;
    let progress = progress(pool).await?;
    let taken = breaks::all(pool).await?;
    let stops = breaks::of(&taken);
    let on_break = breaks::in_force(&taken, now);

    // On a break the schedule waits: nothing comes back, nothing opens.
    let (room, queue) = if on_break.is_some() {
        (Room::default(), Vec::new())
    } else {
        let room = room(pool, now, &stops).await?;
        let mut queue = formulas_today(pool, now, &stops, room).await?;
        queue.extend(words_today(pool, now, &stops, room).await?);
        (room, queue)
    };

    // Waiting means waiting for a later day: the new words already in
    // today's queue are not waiting, they are next.
    let mut words = words::standing(pool).await?;
    let opened = queue.iter().filter(|due| due.is_new && matches!(due.subject, Subject::Word { .. })).count();
    words.waiting -= i64::try_from(opened).unwrap_or(0);

    // Counting time away as a break moves what came due in it; with
    // nothing waiting there is nothing to move, and nothing to offer.
    let away_since = if queue.iter().any(|due| !due.is_new) {
        breaks::away(pool, &taken, now).await?
    } else {
        None
    };

    Ok(Today {
        next: next_in_line(pool, &queue).await?,
        queue,
        reviewed_today: reviewed_since(pool, start_of_day(now)).await?,
        counts,
        progress,
        words,
        room,
        last_week: if on_break.is_some() { None } else { drift::note(pool, now).await? },
        on_break,
        away_since,
    })
}

/// What the day has room for that is new, from what it began owing.
async fn room(pool: &SqlitePool, now: DateTime<Utc>, breaks: &[Break]) -> Result<Room> {
    let debt = debt(pool, now, breaks).await?;
    let words = (i64::try_from(NEW_WORDS_PER_DAY).unwrap_or(i64::MAX) - debt).max(0);
    Ok(Room {
        debt,
        words,
        formulas: if words > 0 {
            i64::try_from(NEW_FORMULAS_PER_DAY).unwrap_or(i64::MAX)
        } else {
            0
        },
    })
}

/// How many cards the day began owing: cards the queue would ask that had
/// come due - the breaks counted - before the start of yesterday, and so
/// had waited through a whole day they could have been answered on.
///
/// As the day began: a card answered since midnight is taken as it stood
/// then, its answers before midnight played through the scheduler again. A
/// word counts from when it could first be asked - when a sentence that
/// holds it was met - and not from a due date it had long before that, as a
/// word declared known has.
async fn debt(pool: &SqlitePool, now: DateTime<Utc>, breaks: &[Break]) -> Result<i64> {
    let midnight = start_of_day(now);
    let line = midnight - GRACE;

    let rows = sqlx::query(
        "SELECT c.id, c.kind, c.due, c.last_reviewed, NULL AS met_at FROM card c
         WHERE c.kind IN ('formula', 'formula-recognise') AND c.state != 'new'
         UNION ALL
         SELECT c.id, c.kind, c.due, c.last_reviewed,
                (SELECT min(fc.introduced_at) FROM sentence_word sw
                 JOIN formula_sentence fs ON fs.sentence_id = sw.sentence_id
                 JOIN card fc ON fc.kind = 'formula' AND fc.subject_id = fs.formula_id AND fc.state != 'new'
                 WHERE sw.word_id = w.id) AS met_at
         FROM card c JOIN word w ON w.id = c.subject_id
         WHERE c.kind = 'word' AND c.state != 'new'",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the cards the queue can ask")?;

    // The answers before midnight of the cards answered since, to set them
    // back to the start of the day.
    let mut before: HashMap<i64, Vec<(Rating, DateTime<Utc>)>> = HashMap::new();
    for (card, rating, at) in sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT r.card_id, r.rating, r.reviewed_at FROM review r JOIN card c ON c.id = r.card_id
         WHERE c.last_reviewed >= ? AND r.reviewed_at < ?
         ORDER BY r.card_id, r.reviewed_at, r.id",
    )
    .bind(midnight.to_rfc3339())
    .bind(midnight.to_rfc3339())
    .fetch_all(pool)
    .await
    .context("failed to read the answers before today")?
    {
        before.entry(card).or_default().push((Rating::from_i64(rating)?, parse_stamp(&at)?));
    }

    let scheduler = Scheduler::new();
    let mut owed = 0;
    for row in rows {
        // A word whose sentence has not been met cannot be asked yet.
        let met_at = row.get::<Option<String>, _>("met_at").map(|at| parse_stamp(&at)).transpose()?;
        if row.get::<&str, _>("kind") == Kind::Word.as_str() && met_at.is_none() {
            continue;
        }
        let last: Option<String> = row.get("last_reviewed");
        let answered_today = last.as_deref().map(parse_stamp).transpose()?.is_some_and(|at| at >= midnight);
        let back = if answered_today {
            // New at midnight is not owed; otherwise the card as it was then.
            let Some(card) = before.remove(&row.get::<i64, _>("id")).and_then(|answers| scheduler.replay(answers)) else {
                continue;
            };
            card.back_at(breaks)
        } else {
            back_at(&row.get::<String, _>("due"), last, breaks)?
        };
        let Some(back) = back else { continue };
        let askable = met_at.map_or(back, |met| back.max(met));
        if askable < line {
            owed += 1;
        }
    }
    Ok(owed)
}

/// When a card comes back, from its row as the database keeps it: its FSRS
/// due and its last answer, with the breaks counted.
///
/// # Errors
///
/// Fails on a moment that is not one.
pub(crate) fn back_at(due: &str, last_reviewed: Option<String>, breaks: &[Break]) -> Result<Option<DateTime<Utc>>> {
    let due = parse_stamp(due)?;
    Ok(match last_reviewed {
        Some(last) => scheduling::comes_back(parse_stamp(&last)?, due, breaks),
        None => Some(due),
    })
}

/// The first formula of the pack's sequence that has not started and is
/// not opening today, with the parents it waits on.
async fn next_in_line(pool: &SqlitePool, queue: &[Due]) -> Result<Option<Next>> {
    let opening: BTreeSet<&str> = queue
        .iter()
        .filter_map(|due| match &due.subject {
            Subject::Formula { formula, .. } if due.is_new => Some(formula.id.as_str()),
            _ => None,
        })
        .collect();
    let rows = sqlx::query(
        "SELECT f.id, f.name FROM card c JOIN formula f ON f.id = c.subject_id
         WHERE c.kind = 'formula' AND c.state = 'new'
         ORDER BY f.position, f.id",
    )
    .fetch_all(pool)
    .await
    .context("failed to look for the next formula")?;
    let Some(row) = rows.iter().find(|row| !opening.contains(row.get::<&str, _>("id"))) else {
        return Ok(None);
    };
    let id: String = row.get("id");
    let waits_for = parents(pool, &id).await?.into_iter().filter(|parent| parent.stitch != Stitch::Sewn).collect();
    Ok(Some(Next {
        name: row.get("name"),
        id,
        waits_for,
    }))
}

/// The formulas of today's queue.
async fn formulas_today(pool: &SqlitePool, now: DateTime<Utc>, breaks: &[Break], room: Room) -> Result<Vec<Due>> {
    let mut queue = Vec::new();

    // FSRS's due is never later than the moment a card comes back, so the
    // query narrows to the candidates and the breaks decide among them.
    let due_rows = sqlx::query(
        "SELECT c.kind, f.id, c.due, c.last_reviewed FROM card c
         JOIN formula f ON f.id = c.subject_id
         WHERE c.kind IN ('formula', 'formula-recognise') AND c.state != 'new' AND c.due <= ?
         ORDER BY c.due, f.position, c.kind",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await
    .context("failed to read the review queue")?;

    for row in due_rows {
        if back_at(&row.get::<String, _>("due"), row.get("last_reviewed"), breaks)?.is_none_or(|back| back > now) {
            continue;
        }
        let id: String = row.get("id");
        let direction = Direction::parse(row.get::<String, _>("kind").as_str())?;
        queue.push(due(pool, &id, direction, false, now).await?);
    }

    // The recognising side of a formula already being produced: it opens as
    // soon as the formula is introduced, and never before, so it costs no
    // slot in the day's budget of new material.
    for id in unopened_recognitions(pool).await? {
        queue.push(due(pool, &id, Direction::Recognise, true, now).await?);
    }

    let started_today = introduced_since(pool, Kind::Formula(Direction::Produce), start_of_day(now)).await?;
    if started_today < room.formulas {
        // The next formula is the first one in the pack's own sequence that
        // has never been answered and whose parents are all sewn: the pack
        // decides the order, the tree decides what is ready, not the clock.
        if let Some(id) = next_new(pool).await? {
            queue.push(due(pool, &id, Direction::Produce, true, now).await?);
        }
    }
    Ok(queue)
}

/// The words of today's queue: those due, then as many met words as the
/// day's room for new ones holds.
async fn words_today(pool: &SqlitePool, now: DateTime<Utc>, breaks: &[Break], room: Room) -> Result<Vec<Due>> {
    let mut queue = Vec::new();
    for id in words::due(pool, now, breaks).await? {
        queue.push(word_due(pool, &id, false, now).await?);
    }
    let started_today = introduced_since(pool, Kind::Word, start_of_day(now)).await?;
    let left = usize::try_from((room.words - started_today).max(0)).unwrap_or(0);
    for id in words::waiting(pool).await?.into_iter().take(left) {
        queue.push(word_due(pool, &id, true, now).await?);
    }
    Ok(queue)
}

/// One queue entry, with the formula, its sisters and its pace.
async fn due(pool: &SqlitePool, id: &str, direction: Direction, is_new: bool, now: DateTime<Utc>) -> Result<Due> {
    let kind = Kind::Formula(direction);
    let card = card(pool, kind, id).await?;
    Ok(Due {
        subject: Subject::Formula {
            formula: formula(pool, id).await?,
            direction,
        },
        stitch: card.as_ref().map_or(Stitch::New, |card| card.stitch),
        stability: card.map_or(0.0, |card| card.stability),
        is_new,
        due: now,
        pace: pace(pool, kind, id).await?,
    })
}

/// One queue entry for a word, with its contexts and its pace.
async fn word_due(pool: &SqlitePool, id: &str, is_new: bool, now: DateTime<Utc>) -> Result<Due> {
    let card = card(pool, Kind::Word, id).await?;
    Ok(Due {
        subject: Subject::Word {
            word: words::word(pool, id).await?,
        },
        stitch: card.as_ref().map_or(Stitch::New, |card| card.stitch),
        stability: card.map_or(0.0, |card| card.stability),
        is_new,
        due: now,
        pace: pace(pool, Kind::Word, id).await?,
    })
}

/// Records an answer and reschedules the formula in the direction answered.
///
/// Said, the answer is its typed turns: each is checked again here - the
/// server is the one truth about what was right - kept whole, and turned
/// into a grade. A turn that got the shape right and a word wrong sends the
/// word to its own card instead of failing the formula.
///
/// # Errors
///
/// `NotFound` when the formula has no card in that direction - it is in no
/// loaded pack - `Unanswerable` for a said answer with no turns or a turn
/// that names something the formula does not ask, and `Database` when the
/// database rejects a statement.
pub async fn review(pool: &SqlitePool, formula_id: &str, answer: Answer, now: DateTime<Utc>) -> Result<Reviewed, AnswerError> {
    let not_found = || AnswerError::NotFound(format!("formula called {formula_id}"));
    match answer {
        Answer::Recognise { rating, duration_ms } => record(pool, Kind::Formula(Direction::Recognise), formula_id, rating, duration_ms, now, &[])
            .await?
            .ok_or_else(not_found),
        Answer::Produce { turns, easy, duration_ms } => {
            if turns.is_empty() {
                return Err(AnswerError::Unanswerable("a said answer is its typed turns, and none came".to_string()));
            }
            let formula = formula(pool, formula_id).await.map_err(|_| not_found())?;
            let judged = turns
                .iter()
                .map(|turn| judge(&formula, turn).map(|(prompt, checked)| (prompt, checked, turn.typed.clone())))
                .collect::<Result<Vec<_>, _>>()?;

            let failures: Vec<Option<Failure>> = judged.iter().map(|(_, checked, _)| checked.failure).collect();
            let rating = rate(&failures, easy);
            let attempts: Vec<Attempt> = judged
                .iter()
                .map(|(prompt, checked, typed)| Attempt {
                    prompt: prompt.clone(),
                    expected: checked.closest.clone(),
                    typed: typed.clone(),
                    failure: checked.failure,
                    missed: checked.missed.clone(),
                })
                .collect();
            // The failure goes to the deck that owns it: a word the shape
            // carried and the learner did not have comes back on its own
            // card, and the formula is not marked down for it.
            let missed: Vec<&str> = judged
                .iter()
                .filter(|(_, checked, _)| checked.failure == Some(Failure::Word))
                .flat_map(|(_, checked, _)| checked.missed.iter().map(String::as_str))
                .collect();
            let words = cards_for(pool, &formula.languages.target, &missed).await?;

            // One transaction: a formula graded with its word failures lost,
            // or the other way round, would be a sitting half remembered.
            let kind = Kind::Formula(Direction::Produce);
            let mut tx = pool.begin().await.context("failed to open a transaction")?;
            let (before, next) = apply(&mut tx, kind, formula_id, Given::asked(rating, duration_ms, &attempts), now)
                .await?
                .ok_or_else(not_found)?;
            for word in &words {
                apply(&mut tx, Kind::Word, word, Given::asked(Rating::Again, None, &[]), now).await?;
            }
            tx.commit().await.context("failed to commit the review")?;

            for word in &words {
                // A word opened by its failure is heard in the first sentence
                // it stands in, like one opened by its own answer.
                words::anchor(pool, word).await?;
            }
            let mut reviewed = reviewed(pool, kind, formula_id, rating, (&before, &next), now).await?;
            reviewed.words = words;
            Ok(reviewed)
        }
    }
}

/// Checks one typed turn without recording it: what the drill shows the
/// moment the learner submits. The same check runs again when the formula
/// is graded, so nothing the client holds decides a grade.
///
/// # Errors
///
/// `NotFound` when there is no such formula, `Unanswerable` when the turn
/// names something the formula does not ask, `Database` when a query fails.
pub async fn check(pool: &SqlitePool, formula_id: &str, turn: &Turn) -> Result<Checked, AnswerError> {
    let formula = formula(pool, formula_id)
        .await
        .map_err(|_| AnswerError::NotFound(format!("formula called {formula_id}")))?;
    judge(&formula, turn).map(|(_, checked)| checked)
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

/// One turn, read against the right answers the formula gives for it: the
/// prompt it showed, in the learner's own language, and the verdict.
fn judge(formula: &Formula, turn: &Turn) -> Result<(String, Checked), AnswerError> {
    let unanswerable = |why: String| AnswerError::Unanswerable(format!("formula {} {why}", formula.id));
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
            let say = formula.say.as_deref().ok_or_else(|| unanswerable("has nothing to substitute".to_string()))?;
            if let Some(name) = values.keys().find(|name| !formula.slots.iter().any(|slot| &slot.name == *name)) {
                return Err(unanswerable(format!("has no slot called {name:?}")));
            }
            let mut chosen: HashMap<&str, &Value> = HashMap::new();
            let mut prompt = formula.pattern.clone();
            for slot in &formula.slots {
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

/// The words of the deck that missed spellings are: "went" is go's. Only
/// words with a card, in the language of the formula.
async fn cards_for(pool: &SqlitePool, language: &str, missed: &[&str]) -> Result<Vec<String>> {
    if missed.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT w.id, w.lemma FROM word w JOIN card c ON c.kind = 'word' AND c.subject_id = w.id
         WHERE w.language = ? ORDER BY w.id",
    )
    .bind(language)
    .fetch_all(pool)
    .await
    .context("failed to read the words of the deck")?;
    let lexicon = lexicon::of(language);
    let mut found: Vec<String> = Vec::new();
    for spelling in missed {
        let spelling = spelling.to_lowercase();
        for row in &rows {
            let lemma: String = row.get("lemma");
            let is_it = lemma.to_lowercase() == spelling || lexicon.is_some_and(|lexicon| lexicon.is_spelling_of(&spelling, &lemma));
            let id: String = row.get("id");
            if is_it && !found.contains(&id) {
                found.push(id);
            }
        }
    }
    Ok(found)
}

/// Records an answer to a word and reschedules it.
///
/// The first answer fixes the word's anchor: the sentence it was shown in is
/// the one it is heard in from then on.
///
/// # Errors
///
/// Fails when there is no such word in any loaded pack, or when the
/// database rejects a statement.
pub async fn review_word(pool: &SqlitePool, word_id: &str, graded: Graded, now: DateTime<Utc>) -> Result<Reviewed> {
    let reviewed = record(pool, Kind::Word, word_id, graded.rating, graded.duration_ms, now, &[])
        .await?
        .with_context(|| format!("there is no word called {word_id}"))?;
    words::anchor(pool, word_id).await?;
    Ok(reviewed)
}

/// Records that the learner knows a word without being asked: an easy,
/// declared, on the word's card - made first when the word has none, as a
/// word of the language that no pack teaches has not. Inside the caller's
/// transaction; the card as the declaration leaves it.
///
/// # Errors
///
/// Fails when the database rejects a statement.
pub(crate) async fn declare(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, word_id: &str, now: DateTime<Utc>) -> Result<Card> {
    sqlx::query(
        "INSERT INTO card (kind, subject_id, state, stability, difficulty, due, reps, lapses)
         VALUES ('word', ?, 'new', 0.0, 0.0, ?, 0, 0)
         ON CONFLICT (kind, subject_id) DO NOTHING",
    )
    .bind(word_id)
    .bind(now.to_rfc3339())
    .execute(&mut **tx)
    .await
    .context("failed to make a card for a declared word")?;
    let given = Given {
        rating: Rating::Easy,
        source: Source::Declared,
        duration_ms: None,
        attempts: &[],
    };
    apply(tx, Kind::Word, word_id, given, now)
        .await?
        .map(|(_, next)| next)
        .with_context(|| format!("the card of {word_id} vanished while it was declared"))
}

/// Sets a card back to what the answers it still has leave: the history
/// played through the scheduler again, after some of it was taken back.
/// With no answers left the card is new again - or gone, when it schedules
/// a word no pack teaches, which only a declaration had made it for.
///
/// # Errors
///
/// Fails when the database rejects a statement or holds a rating the
/// scheduler does not know.
pub(crate) async fn rebuild(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, kind: Kind, subject: &str, now: DateTime<Utc>) -> Result<()> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT r.rating, r.reviewed_at FROM review r JOIN card c ON c.id = r.card_id
         WHERE c.kind = ? AND c.subject_id = ? ORDER BY r.reviewed_at, r.id",
    )
    .bind(kind.as_str())
    .bind(subject)
    .fetch_all(&mut **tx)
    .await
    .context("failed to read the answers of a card")?;
    let answers = rows
        .iter()
        .map(|(rating, at)| Ok((Rating::from_i64(*rating)?, parse_stamp(at)?)))
        .collect::<Result<Vec<_>>>()?;
    let first = answers.first().map(|(_, at)| *at);

    let Some(card) = Scheduler::new().replay(answers) else {
        let taught: bool = match kind {
            Kind::Word => sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM word WHERE id = ?)"),
            Kind::Formula(_) => sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM formula WHERE id = ?)"),
        }
        .bind(subject)
        .fetch_one(&mut **tx)
        .await
        .context("failed to ask whether a pack teaches a card's subject")?;
        let query = if taught {
            sqlx::query(
                "UPDATE card SET state = 'new', stability = 0.0, difficulty = 0.0, due = ?, reps = 0, lapses = 0,
                     last_reviewed = NULL, introduced_at = NULL
                 WHERE kind = ? AND subject_id = ?",
            )
            .bind(now.to_rfc3339())
        } else {
            sqlx::query("DELETE FROM card WHERE kind = ? AND subject_id = ?")
        };
        query
            .bind(kind.as_str())
            .bind(subject)
            .execute(&mut **tx)
            .await
            .context("failed to set a card back to new")?;
        return Ok(());
    };

    sqlx::query(
        "UPDATE card SET state = ?, stability = ?, difficulty = ?, due = ?, reps = ?, lapses = ?,
             last_reviewed = ?, introduced_at = ?
         WHERE kind = ? AND subject_id = ?",
    )
    .bind(card.stitch.as_str())
    .bind(card.stability)
    .bind(card.difficulty)
    .bind(card.due.to_rfc3339())
    .bind(card.reps)
    .bind(card.lapses)
    .bind(card.last_reviewed.map(|at| at.to_rfc3339()))
    .bind(first.map(|at| at.to_rfc3339()))
    .bind(kind.as_str())
    .bind(subject)
    .execute(&mut **tx)
    .await
    .context("failed to save a rebuilt card")?;
    Ok(())
}

/// One answer, as it is applied to a card.
struct Given<'a> {
    rating: Rating,
    source: Source,
    /// How long it took, when the client measured it.
    duration_ms: Option<i64>,
    /// The turns typed for it; none for an answer that was not typed.
    attempts: &'a [Attempt],
}

impl<'a> Given<'a> {
    const fn asked(rating: Rating, duration_ms: Option<i64>, attempts: &'a [Attempt]) -> Self {
        Self {
            rating,
            source: Source::Asked,
            duration_ms,
            attempts,
        }
    }
}

/// A typed turn, as it is kept with the review it belongs to.
struct Attempt {
    prompt: String,
    expected: String,
    typed: String,
    failure: Option<Failure>,
    missed: Vec<String>,
}

/// Applies an answer to one card, on its own: the new schedule and the
/// review, committed together. `None` when there is no such card.
async fn record(
    pool: &SqlitePool,
    kind: Kind,
    subject: &str,
    rating: Rating,
    duration_ms: Option<i64>,
    now: DateTime<Utc>,
    attempts: &[Attempt],
) -> Result<Option<Reviewed>> {
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let Some((before, next)) = apply(&mut tx, kind, subject, Given::asked(rating, duration_ms, attempts), now).await? else {
        return Ok(None);
    };
    tx.commit().await.context("failed to commit the review")?;
    Ok(Some(reviewed(pool, kind, subject, rating, (&before, &next), now).await?))
}

/// What an answer did to a card, as the app is told: the card before it
/// and after it.
///
/// The due is FSRS's own: an asked answer ends any break going on, and a
/// break cannot start later than now, so nothing stands between the answer
/// and the day it comes back.
async fn reviewed(pool: &SqlitePool, kind: Kind, subject: &str, rating: Rating, (before, next): (&Card, &Card), now: DateTime<Utc>) -> Result<Reviewed> {
    Ok(Reviewed {
        stitch: next.stitch,
        due: next.due,
        interval_days: (next.due - now).num_days(),
        pace: pace(pool, kind, subject).await?,
        rating,
        words: Vec::new(),
        stability: next.stability,
        stability_before: before.stability,
    })
}

/// Applies an answer to a card inside a transaction: the new schedule, and
/// the review kept whole with the turns typed for it. The card before the
/// answer and as it now stands, or `None` when there is no such card.
///
/// An asked answer ends any break going on: the learner who answers is
/// back, and a break never holds an answer.
async fn apply(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, kind: Kind, subject: &str, given: Given<'_>, now: DateTime<Utc>) -> Result<Option<(Card, Card)>> {
    let Given {
        rating,
        source,
        duration_ms,
        attempts,
    } = given;
    let Some(card) = card(&mut **tx, kind, subject).await? else {
        return Ok(None);
    };
    if source == Source::Asked {
        breaks::end_by_answer(tx, now).await?;
    }

    let Scheduled { card: next, elapsed_days } = Scheduler::new().review(&card, rating, now);

    sqlx::query(
        "UPDATE card SET state = ?, stability = ?, difficulty = ?, due = ?, reps = ?, lapses = ?,
             last_reviewed = ?, introduced_at = COALESCE(introduced_at, ?)
         WHERE kind = ? AND subject_id = ?",
    )
    .bind(next.stitch.as_str())
    .bind(next.stability)
    .bind(next.difficulty)
    .bind(next.due.to_rfc3339())
    .bind(next.reps)
    .bind(next.lapses)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(kind.as_str())
    .bind(subject)
    .execute(&mut **tx)
    .await
    .context("failed to save the card")?;

    // The history is kept in full so the schedule can be recomputed when the
    // memory model changes (ADR 0003).
    let review: i64 = sqlx::query_scalar(
        "INSERT INTO review (card_id, rating, reviewed_at, elapsed_days, stability, difficulty, duration_ms, source)
         SELECT id, ?, ?, ?, ?, ?, ?, ? FROM card WHERE kind = ? AND subject_id = ?
         RETURNING id",
    )
    .bind(rating.as_i64())
    .bind(now.to_rfc3339())
    .bind(elapsed_days)
    .bind(next.stability)
    .bind(next.difficulty)
    .bind(duration_ms)
    .bind(source.as_str())
    .bind(kind.as_str())
    .bind(subject)
    .fetch_one(&mut **tx)
    .await
    .context("failed to record the review")?;

    // The turns typed for it, kept as long as the review: the real
    // mistakes, which later versions pair into confusions and explain.
    for (position, attempt) in attempts.iter().enumerate() {
        sqlx::query(
            "INSERT INTO attempt (review_id, position, prompt, expected, typed, correct, failure, missed)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(review)
        .bind(i64::try_from(position).unwrap_or(i64::MAX))
        .bind(&attempt.prompt)
        .bind(&attempt.expected)
        .bind(&attempt.typed)
        .bind(i64::from(attempt.failure.is_none()))
        .bind(attempt.failure.map(Failure::as_str))
        .bind(serde_json::to_string(&attempt.missed).context("failed to write the missed words of a turn")?)
        .execute(&mut **tx)
        .await
        .context("failed to record a typed turn")?;
    }
    Ok(Some((card, next)))
}

/// One formula with its samples and slots.
///
/// # Errors
///
/// Fails when there is no such formula, or when the database rejects a query.
pub async fn formula(pool: &SqlitePool, id: &str) -> Result<Formula> {
    let row = sqlx::query(
        "SELECT f.id, f.name, f.pattern, f.say, f.accept, f.explanation, f.family, f.form, p.native, p.target
         FROM formula f JOIN pack p ON p.id = f.pack_id WHERE f.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("failed to read a formula")?
    .with_context(|| format!("there is no formula called {id}"))?;

    let native: String = row.get("native");
    let target: String = row.get("target");
    let samples = samples(pool, id, &native).await?;

    let slot_rows = sqlx::query("SELECT id, name, takes FROM slot WHERE formula_id = ? ORDER BY position")
        .bind(id)
        .fetch_all(pool)
        .await
        .context("failed to read the slots of a formula")?;

    let mut slots = Vec::with_capacity(slot_rows.len());
    for slot in slot_rows {
        let slot_id: i64 = slot.get("id");
        let mut values = sqlx::query("SELECT native, target, forms FROM slot_value WHERE slot_id = ? ORDER BY position")
            .bind(slot_id)
            .fetch_all(pool)
            .await
            .context("failed to read the values of a slot")?
            .iter()
            .map(value)
            .collect::<Result<Vec<_>>>()?;
        let takes = slot.get::<Option<String>, _>("takes").map(|kind| OwnKind::parse(&kind)).transpose()?;
        if let Some(kind) = takes {
            // The learner's own words after the pack's; one that says what a
            // pack value already says is that value, not a second one.
            for word in own::of_kind(pool, &target, kind).await? {
                if values.iter().any(|value| value.target == word.target) {
                    continue;
                }
                values.push(Value {
                    native: word.native,
                    target: word.target,
                    forms: BTreeMap::new(),
                    own: true,
                });
            }
        }
        slots.push(Slot {
            name: slot.get("name"),
            takes,
            values,
        });
    }

    let family: Option<String> = row.get("family");
    let form = row.get::<Option<String>, _>("form").map(|word| Form::parse(&word)).transpose()?;
    let accept: String = row.get("accept");

    Ok(Formula {
        id: row.get("id"),
        name: row.get("name"),
        pattern: row.get("pattern"),
        say: row.get("say"),
        accept: serde_json::from_str(&accept).with_context(|| format!("the answers formula {id} accepts are not JSON: {accept}"))?,
        parents: parents(pool, id).await?,
        languages: Languages {
            native: row.get("native"),
            target: row.get("target"),
        },
        explanation: row.get("explanation"),
        sisters: match family.as_deref() {
            Some(family) => sisters(pool, family, id).await?,
            None => Vec::new(),
        },
        family,
        form,
        samples,
        slots,
    })
}

/// The formulas one stands on, in the pack's order, with where each stands.
async fn parents(pool: &SqlitePool, id: &str) -> Result<Vec<Parent>> {
    let rows = sqlx::query(
        "SELECT f.id, f.name, c.state FROM formula_parent fp
         JOIN formula f ON f.id = fp.parent_id
         LEFT JOIN card c ON c.kind = 'formula' AND c.subject_id = f.id
         WHERE fp.formula_id = ?
         ORDER BY f.position, f.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .context("failed to read the parents of a formula")?;
    rows.iter()
        .map(|row| {
            Ok(Parent {
                id: row.get("id"),
                name: row.get("name"),
                stitch: row.get::<Option<String>, _>("state").map_or(Ok(Stitch::New), |state| Stitch::parse(&state))?,
            })
        })
        .collect()
}

/// The other forms of the same shape, in the order the switch shows them.
///
/// Statement, negation, question - the order they are learnt in and the order
/// they are asked in, taken from the pack's own sequence rather than from the
/// enum, so a pack that introduces the question first is shown that way.
async fn sisters(pool: &SqlitePool, family: &str, without: &str) -> Result<Vec<Sister>> {
    let rows = sqlx::query(
        "SELECT f.id, f.form, f.name, f.pattern, c.state FROM formula f
         LEFT JOIN card c ON c.subject_id = f.id AND c.kind = 'formula'
         WHERE f.family = ? AND f.id != ?
         ORDER BY f.position, f.id",
    )
    .bind(family)
    .bind(without)
    .fetch_all(pool)
    .await
    .context("failed to read the other forms of a formula")?;

    let mut sisters = Vec::with_capacity(rows.len());
    for row in rows {
        // A row in a family without a form is rejected by the pack validator,
        // so this can only be a database edited by hand.
        let Some(word) = row.get::<Option<String>, _>("form") else { continue };
        sisters.push(Sister {
            id: row.get("id"),
            form: Form::parse(&word)?,
            name: row.get("name"),
            pattern: row.get("pattern"),
            stitch: row.get::<Option<String>, _>("state").map_or(Ok(Stitch::New), |state| Stitch::parse(&state))?,
        });
    }
    Ok(sisters)
}

/// How many formulas stand in each state, counting the producing side.
///
/// Producing is the side the product is about, and the one number a learner
/// quotes about themselves. The split by direction is [`progress`].
///
/// # Errors
///
/// Fails when the database rejects the query.
pub async fn counts(pool: &SqlitePool) -> Result<Counts> {
    counts_for(pool, Kind::Formula(Direction::Produce)).await
}

/// The same standing told once per direction.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn progress(pool: &SqlitePool) -> Result<Progress> {
    Ok(Progress {
        produce: counts_for(pool, Kind::Formula(Direction::Produce)).await?,
        recognise: counts_for(pool, Kind::Formula(Direction::Recognise)).await?,
    })
}

async fn counts_for(pool: &SqlitePool, kind: Kind) -> Result<Counts> {
    let rows = sqlx::query("SELECT state, count(*) AS n FROM card WHERE kind = ? GROUP BY state")
        .bind(kind.as_str())
        .fetch_all(pool)
        .await
        .context("failed to count the formulas")?;

    let mut counts = Counts::default();
    for row in rows {
        let n: i64 = row.get("n");
        match Stitch::parse(row.get::<String, _>("state").as_str()) {
            Ok(Stitch::New) => counts.new = n,
            Ok(Stitch::Basted) => counts.basted = n,
            Ok(Stitch::Sewn) => counts.sewn = n,
            Err(error) => {
                // A state nobody wrote on purpose: counted nowhere rather
                // than counted wrong, and said out loud.
                tracing::warn!(%error, "a card holds a state the product does not know");
            }
        }
    }
    Ok(counts)
}

fn value(row: &SqliteRow) -> Result<Value> {
    let forms: String = row.get("forms");
    Ok(Value {
        native: row.get("native"),
        target: row.get("target"),
        forms: serde_json::from_str(&forms).with_context(|| format!("the forms of a slot value are not JSON: {forms}"))?,
        own: false,
    })
}

/// The worked examples of a formula, with the words marked in them.
async fn samples(pool: &SqlitePool, formula_id: &str, native: &str) -> Result<Vec<Sample>> {
    let rows = sqlx::query(
        "SELECT s.id, s.text, s.accept, COALESCE(t.text, '') AS translation FROM formula_sentence fs
         JOIN sentence s ON s.id = fs.sentence_id
         LEFT JOIN sentence_translation t ON t.sentence_id = s.id AND t.language = ?
         WHERE fs.formula_id = ?
         ORDER BY fs.position",
    )
    .bind(native)
    .bind(formula_id)
    .fetch_all(pool)
    .await
    .context("failed to read the samples of a formula")?;

    let marks = sqlx::query(
        "SELECT sw.sentence_id, sw.word_id, w.lemma, sw.form, sw.start FROM sentence_word sw
         JOIN word w ON w.id = sw.word_id
         WHERE sw.sentence_id IN (SELECT sentence_id FROM formula_sentence WHERE formula_id = ?)
         ORDER BY sw.sentence_id, sw.start",
    )
    .bind(formula_id)
    .fetch_all(pool)
    .await
    .context("failed to read the words of a formula's samples")?;
    let mut by_sentence: BTreeMap<i64, Vec<Mark>> = BTreeMap::new();
    for row in marks {
        by_sentence.entry(row.get("sentence_id")).or_default().push(Mark {
            word: row.get("word_id"),
            lemma: row.get("lemma"),
            form: row.get("form"),
            start: row.get("start"),
        });
    }

    rows.iter()
        .map(|row| {
            let sentence: i64 = row.get("id");
            let accept: String = row.get("accept");
            Ok(Sample {
                sentence,
                native: row.get("translation"),
                target: row.get("text"),
                words: by_sentence.get(&sentence).cloned().unwrap_or_default(),
                accept: serde_json::from_str(&accept).with_context(|| format!("the spellings sentence {sentence} accepts are not JSON: {accept}"))?,
            })
        })
        .collect()
}

/// Midnight UTC of the day `now` falls in.
///
/// UTC rather than a local timezone: a single learner in one place, and a day
/// boundary that does not move is worth more than one that lands at midnight
/// exactly. When the day starts mattering - streaks, in v0.21.0 - it becomes
/// a setting.
fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive().and_hms_opt(0, 0, 0).map_or(now, |naive| naive.and_utc())
}

async fn card<'e, E: sqlx::SqliteExecutor<'e>>(executor: E, kind: Kind, subject: &str) -> Result<Option<Card>> {
    let row = sqlx::query(
        "SELECT state, stability, difficulty, due, reps, lapses, last_reviewed
         FROM card WHERE kind = ? AND subject_id = ?",
    )
    .bind(kind.as_str())
    .bind(subject)
    .fetch_optional(executor)
    .await
    .context("failed to read a card")?;

    let Some(row) = row else { return Ok(None) };
    Ok(Some(Card {
        stitch: Stitch::parse(row.get::<String, _>("state").as_str())?,
        stability: row.get("stability"),
        difficulty: row.get("difficulty"),
        due: timestamp(&row, "due")?.context("a card has no due date")?,
        reps: row.get("reps"),
        lapses: row.get("lapses"),
        last_reviewed: timestamp(&row, "last_reviewed")?,
    }))
}

fn timestamp(row: &SqliteRow, column: &str) -> Result<Option<DateTime<Utc>>> {
    let raw: Option<String> = row.get(column);
    raw.map(|text| parse_stamp(&text).with_context(|| format!("{column} is not a timestamp")))
        .transpose()
}

/// A moment as the database keeps it: RFC 3339, written by `to_rfc3339`.
///
/// # Errors
///
/// Fails on text that is not RFC 3339.
pub(crate) fn parse_stamp(text: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .map(|at| at.with_timezone(&Utc))
        .with_context(|| format!("{text:?} is not a timestamp"))
}

/// The first formula of the pack's sequence never answered whose parents
/// are all sewn. A parent with no card at all - a pack edited by hand - is
/// not sewn either: `NULL` is spelt out, because `state != 'sewn'` alone
/// would let it through.
async fn next_new(pool: &SqlitePool) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT f.id FROM card c
         JOIN formula f ON f.id = c.subject_id
         WHERE c.kind = 'formula' AND c.state = 'new'
           AND NOT EXISTS (
               SELECT 1 FROM formula_parent fp
               LEFT JOIN card pc ON pc.kind = 'formula' AND pc.subject_id = fp.parent_id
               WHERE fp.formula_id = f.id AND (pc.state IS NULL OR pc.state != 'sewn'))
         ORDER BY f.position, f.id
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .context("failed to look for the next new formula")
}

/// How many cards of a kind were started since `since`.
///
/// Formulas are counted on the producing side only: the day's budget of one
/// new formula is about new shapes, and the recognising card of a shape
/// already being drilled is not a new shape.
async fn introduced_since(pool: &SqlitePool, kind: Kind, since: DateTime<Utc>) -> Result<i64> {
    // A card the learner declared known was not started by the tutor: the
    // day's room is for material the drill opens, and an inventory of a
    // thousand words in one evening must not close it for the day.
    sqlx::query_scalar(
        "SELECT count(*) FROM card c WHERE c.kind = ? AND c.introduced_at >= ?
           AND NOT EXISTS (SELECT 1 FROM review r WHERE r.card_id = c.id AND r.source = 'declared')",
    )
    .bind(kind.as_str())
    .bind(since.to_rfc3339())
    .fetch_one(pool)
    .await
    .context("failed to count what was started today")
}

async fn reviewed_since(pool: &SqlitePool, since: DateTime<Utc>) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM review WHERE reviewed_at >= ? AND source = 'asked'")
        .bind(since.to_rfc3339())
        .fetch_one(pool)
        .await
        .context("failed to count today's reviews")
}

/// Formulas being produced whose recognising side has never been asked.
///
/// A shape is shown in the drill before it is asked backwards: understanding
/// a sentence you have never been taught to build is a guess, not a review.
async fn unopened_recognitions(pool: &SqlitePool) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT p.subject_id FROM card p
         JOIN card r ON r.subject_id = p.subject_id AND r.kind = 'formula-recognise'
         JOIN formula f ON f.id = p.subject_id
         WHERE p.kind = 'formula' AND p.state != 'new' AND r.state = 'new'
         ORDER BY f.position, f.id",
    )
    .fetch_all(pool)
    .await
    .context("failed to look for formulas ready to be asked backwards")
}

/// How fast this card usually comes.
///
/// The median of the last few timed answers rather than the mean: one
/// interrupted turn - a phone put down mid-drill - would drag a mean for
/// weeks, and the number is there to be trusted at a glance.
///
/// `None` until there are enough answers to have a middle worth showing.
async fn pace(pool: &SqlitePool, kind: Kind, subject: &str) -> Result<Option<Pace>> {
    let mut recent: Vec<i64> = sqlx::query_scalar(
        "SELECT r.duration_ms FROM review r
         JOIN card c ON c.id = r.card_id
         WHERE c.kind = ? AND c.subject_id = ? AND r.duration_ms IS NOT NULL
         ORDER BY r.reviewed_at DESC, r.id DESC
         LIMIT ?",
    )
    .bind(kind.as_str())
    .bind(subject)
    .bind(PACE_WINDOW)
    .fetch_all(pool)
    .await
    .context("failed to read how long a card usually takes")?;

    if recent.len() < PACE_MIN_ANSWERS {
        return Ok(None);
    }

    let last_ms = recent[0];
    let answers = i64::try_from(recent.len()).unwrap_or(i64::MAX);
    recent.sort_unstable();
    let typical_ms = recent[recent.len() / 2];

    Ok(Some(Pace { typical_ms, last_ms, answers }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;

    impl Due {
        /// The formula this item asks, or an empty id for a word: most of
        /// these tests are about formulas, and say so by asking for one.
        fn formula_id(&self) -> &String {
            static NONE: String = String::new();
            match &self.subject {
                Subject::Formula { formula, .. } => &formula.id,
                Subject::Word { .. } => &NONE,
            }
        }

        fn direction(&self) -> Option<Direction> {
            match &self.subject {
                Subject::Formula { direction, .. } => Some(*direction),
                Subject::Word { .. } => None,
            }
        }

        fn word_id(&self) -> Option<&str> {
            match &self.subject {
                Subject::Word { word } => Some(&word.id),
                Subject::Formula { .. } => None,
            }
        }
    }

    /// The first three fixture formulas are the three forms of one shape, so
    /// the switch has something to switch between; the rest stand alone.
    const FORMS: [&str; 3] = ["statement", "negation", "question"];

    fn fixture(formulas: usize) -> pack::Pack {
        use std::fmt::Write as _;

        let mut toml = String::from(
            "id = \"t\"
version = 1
native = \"ru\"
target = \"en\"
",
        );
        for index in 0..formulas {
            let family = FORMS
                .get(index)
                .map(|form| {
                    format!(
                        "family = \"shape\"
form = \"{form}\"
"
                    )
                })
                .unwrap_or_default();
            let _ = write!(
                toml,
                "
[[formula]]
id = \"f{index}\"
name = \"n{index}\"
pattern = \"<pronoun> + x\"
say = \"<pronoun> x.\"
                 explanation = \"e\"
order = {}
{family}

  [[formula.sample]]
  native = \"a\"
  target = \"b\"

                   [[formula.slot]]
  name = \"pronoun\"
  values = [{{ native = \"one\", target = \"I\" }}]
",
                (index + 1) * 10
            );
        }
        toml::from_str(&toml).expect("the fixture should parse")
    }

    async fn loaded(formulas: usize) -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("an in-memory database");
        sqlx::migrate!().run(&pool).await.expect("migrations should apply");
        pack::load(&pool, &fixture(formulas)).await.expect("the fixture should load");
        pool
    }

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z").unwrap().with_timezone(&Utc) + chrono::Duration::days(day)
    }

    /// A grade put straight onto a card, in the producing direction: most of
    /// these tests are about the schedule, not about typing, and the typed
    /// turns that earn a grade have tests of their own below.
    #[derive(Debug, Clone, Copy)]
    struct Said {
        rating: Rating,
        direction: Direction,
        duration_ms: Option<i64>,
    }

    const fn said(rating: Rating) -> Said {
        Said {
            rating,
            direction: Direction::Produce,
            duration_ms: None,
        }
    }

    /// Grades a formula as `said` says. Shadows the public `review`, which
    /// takes typed turns.
    async fn review(pool: &SqlitePool, id: &str, said: Said, now: DateTime<Utc>) -> Result<Reviewed> {
        record(pool, Kind::Formula(said.direction), id, said.rating, said.duration_ms, now, &[])
            .await?
            .with_context(|| format!("there is no formula called {id}"))
    }

    impl Said {
        /// The same answer, with how long it took.
        const fn taking(self, duration_ms: i64) -> Self {
            Self {
                duration_ms: Some(duration_ms),
                ..self
            }
        }

        /// The same answer, asked the other way round.
        const fn backwards(self) -> Self {
            Self {
                direction: Direction::Recognise,
                ..self
            }
        }
    }

    #[tokio::test]
    async fn a_fresh_learner_gets_exactly_one_formula() {
        // The whole promise of "one new formula a day": a first sitting shows
        // one shape, not a syllabus.
        let pool = loaded(5).await;
        let today = today(&pool, at(0)).await.unwrap();

        assert_eq!(today.queue.len(), 1);
        assert!(today.queue[0].is_new);
        assert_eq!(today.queue[0].formula_id(), "f0", "the pack's own order should decide what comes first");
        assert_eq!(today.counts, Counts { new: 5, basted: 0, sewn: 0 });
    }

    #[tokio::test]
    async fn the_second_formula_of_the_day_is_not_offered() {
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let today = today(&pool, at(0)).await.unwrap();
        assert!(
            today.queue.iter().all(|due| !(due.is_new && due.direction() == Some(Direction::Produce))),
            "a second new formula was offered on the same day: {:?}",
            today.queue.iter().map(Due::formula_id).collect::<Vec<_>>()
        );
        assert_eq!(today.reviewed_today, 1);
    }

    #[tokio::test]
    async fn tomorrow_brings_the_next_formula() {
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let tomorrow = today(&pool, at(1)).await.unwrap();
        let new: Vec<_> = tomorrow
            .queue
            .iter()
            .filter(|due| due.is_new && due.direction() == Some(Direction::Produce))
            .map(|due| due.formula_id().as_str())
            .collect();
        assert_eq!(new, vec!["f1"], "a new day should open exactly the next formula in the pack");
    }

    #[tokio::test]
    async fn a_formula_answered_comes_back_and_one_forgotten_comes_back_sooner() {
        let pool = loaded(3).await;
        let good = review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let again = review(&pool, "f1", said(Rating::Again), at(0)).await.unwrap();

        assert!(again.due <= good.due, "a formula that would not come should return sooner than one that did");
        assert_eq!(again.stitch, Stitch::Basted);
    }

    #[tokio::test]
    async fn a_review_is_kept_in_full() {
        // The history is the asset: the schedule can be recomputed from it,
        // and nothing else can.
        let pool = loaded(2).await;
        review(&pool, "f0", said(Rating::Hard).taking(4200), at(0)).await.unwrap();

        let (rating, duration, stability): (i64, Option<i64>, f64) = sqlx::query_as("SELECT rating, duration_ms, stability FROM review")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((rating, duration), (2, Some(4200)));
        assert!(stability > 0.0, "the review should record the memory state it produced");
    }

    #[tokio::test]
    async fn a_due_formula_is_queued_before_anything_new() {
        // Reviews must never be crowded out by new material.
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Again), at(0)).await.unwrap();

        let later = today(&pool, at(2)).await.unwrap();
        assert_eq!(later.queue.first().map(|due| due.formula_id().as_str()), Some("f0"));
        assert!(!later.queue[0].is_new);
    }

    #[tokio::test]
    async fn a_formula_not_yet_due_stays_out_of_the_queue() {
        let pool = loaded(3).await;
        let reviewed = review(&pool, "f0", said(Rating::Easy), at(0)).await.unwrap();
        assert!(reviewed.interval_days >= 1, "an easy answer should push a formula past today");

        let same_day = today(&pool, at(0)).await.unwrap();
        assert!(
            same_day
                .queue
                .iter()
                .all(|due| !(due.formula_id() == "f0" && due.direction() == Some(Direction::Produce))),
            "an answered formula came back the same day"
        );
    }

    #[tokio::test]
    async fn answering_something_that_is_not_a_formula_fails() {
        let pool = loaded(1).await;
        let error = review(&pool, "not-a-formula", said(Rating::Good), at(0)).await.unwrap_err();
        assert!(error.to_string().contains("not-a-formula"), "{error:#}");
    }

    #[tokio::test]
    async fn a_formula_carries_its_samples_and_slots() {
        let pool = loaded(1).await;
        let formula = formula(&pool, "f0").await.unwrap();
        assert_eq!(formula.samples.len(), 1);
        assert_eq!(formula.slots.len(), 1);
        assert_eq!(formula.slots[0].name, "pronoun");
        assert_eq!(formula.slots[0].values[0].target, "I");
    }

    #[tokio::test]
    async fn a_formula_carries_the_other_forms_of_its_shape() {
        // What the switch on the card is drawn from.
        let pool = loaded(5).await;
        let formula = formula(&pool, "f0").await.unwrap();

        assert_eq!(formula.family.as_deref(), Some("shape"));
        assert_eq!(formula.form, Some(Form::Statement));
        let forms: Vec<_> = formula.sisters.iter().map(|sister| sister.form).collect();
        assert_eq!(forms, vec![Form::Negation, Form::Question], "the switch should offer the other two forms");
        assert!(
            formula.sisters.iter().all(|sister| sister.id != "f0"),
            "a formula should not be offered as its own sister"
        );
    }

    #[tokio::test]
    async fn a_formula_that_stands_alone_offers_no_switch() {
        let pool = loaded(5).await;
        let formula = formula(&pool, "f4").await.unwrap();
        assert_eq!(formula.family, None);
        assert!(formula.sisters.is_empty(), "a formula with no family should offer nothing to switch to");
    }

    #[tokio::test]
    async fn the_switch_shows_where_each_form_stands() {
        // The point of putting the stitch on the switch: a learner who has
        // sewn the statement and never touched the question can see it.
        let pool = loaded(3).await;
        review(&pool, "f1", said(Rating::Good), at(0)).await.unwrap();

        let formula = formula(&pool, "f0").await.unwrap();
        let negation = formula.sisters.iter().find(|sister| sister.form == Form::Negation).unwrap();
        let question = formula.sisters.iter().find(|sister| sister.form == Form::Question).unwrap();
        assert_ne!(negation.stitch, Stitch::New, "the form that was answered should not read as new");
        assert_eq!(question.stitch, Stitch::New, "the form never answered should read as new");
    }

    #[tokio::test]
    async fn the_two_directions_of_a_formula_are_scheduled_apart() {
        // The whole of the reverse drill: understanding runs ahead of
        // producing, and the two must not be averaged into one number.
        let pool = loaded(3).await;
        let produced = review(&pool, "f0", said(Rating::Again), at(0)).await.unwrap();
        let recognised = review(&pool, "f0", said(Rating::Easy).backwards(), at(0)).await.unwrap();

        assert!(
            recognised.due > produced.due,
            "an easy recognition and a failed production should not land on the same day"
        );

        let progress = progress(&pool).await.unwrap();
        assert_eq!(progress.produce.new, 2, "answering one direction should not move the other count");
        assert_eq!(progress.recognise.new, 2);
        assert_eq!(progress.produce.basted + progress.produce.sewn, 1);
        assert_eq!(progress.recognise.basted + progress.recognise.sewn, 1);
    }

    #[tokio::test]
    async fn answering_one_direction_leaves_the_other_untouched() {
        let pool = loaded(2).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let (state, reps): (String, i64) = sqlx::query_as("SELECT state, reps FROM card WHERE kind = 'formula-recognise' AND subject_id = 'f0'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((state.as_str(), reps), ("new", 0), "producing an answer graded the recognising card too");
    }

    #[tokio::test]
    async fn a_formula_is_asked_backwards_only_after_it_has_been_shown() {
        // Asking someone to understand a shape nobody has taught them to
        // build is a guess, not a review.
        let pool = loaded(4).await;
        let first = today(&pool, at(0)).await.unwrap();
        assert!(
            first.queue.iter().all(|due| due.direction() == Some(Direction::Produce)),
            "a fresh learner was asked to recognise something never shown"
        );

        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let after = today(&pool, at(0)).await.unwrap();
        let backwards: Vec<_> = after
            .queue
            .iter()
            .filter(|due| due.direction() == Some(Direction::Recognise))
            .map(|due| due.formula_id().as_str())
            .collect();
        assert_eq!(backwards, vec!["f0"], "the formula just shown should open its recognising side");
    }

    #[tokio::test]
    async fn the_reverse_side_costs_no_slot_in_the_day_budget() {
        // One new formula a day is about new shapes. If the recognising card
        // counted against it, the learner would meet a new shape every other
        // day instead of every day.
        let pool = loaded(5).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        review(&pool, "f0", said(Rating::Good).backwards(), at(0)).await.unwrap();

        // The budget counts new shapes, and a shape is the producing side.
        // Asserted on the count itself: with a budget of one, a count of two
        // and a count of one behave identically in the queue, so the queue
        // cannot show the difference and this is the only place it is
        // visible.
        assert_eq!(
            introduced_since(&pool, Kind::Formula(Direction::Produce), start_of_day(at(0))).await.unwrap(),
            1,
            "the reverse card was counted as a new shape"
        );

        let same_day = today(&pool, at(0)).await.unwrap();
        assert_eq!(
            same_day
                .queue
                .iter()
                .filter(|due| due.is_new && due.direction() == Some(Direction::Produce))
                .count(),
            0,
            "the formula of the day was already answered, so no second one is due"
        );

        let tomorrow = today(&pool, at(1)).await.unwrap();
        let fresh: Vec<_> = tomorrow
            .queue
            .iter()
            .filter(|due| due.is_new && due.direction() == Some(Direction::Produce))
            .map(|due| due.formula_id().as_str())
            .collect();
        assert_eq!(fresh, vec!["f1"], "the reverse drill of yesterday ate the new formula of today");

        // And the day after: the reverse card answered on day 1 must not
        // count against the shape opening on day 2 either.
        review(&pool, "f1", said(Rating::Good), at(1)).await.unwrap();
        review(&pool, "f1", said(Rating::Good).backwards(), at(1)).await.unwrap();
        let later = today(&pool, at(2)).await.unwrap();
        let fresh: Vec<_> = later
            .queue
            .iter()
            .filter(|due| due.is_new && due.direction() == Some(Direction::Produce))
            .map(|due| due.formula_id().as_str())
            .collect();
        assert_eq!(fresh, vec!["f2"], "a reverse card counted against the budget of new shapes");
    }

    #[tokio::test]
    async fn pace_appears_once_there_is_a_middle_worth_showing() {
        let pool = loaded(2).await;
        let mut day = 0;
        for took in [4000, 30_000, 5000] {
            let reviewed = review(&pool, "f0", said(Rating::Good).taking(took), at(day)).await.unwrap();
            day = reviewed.due.signed_duration_since(at(0)).num_days();
            // Only the last of the three has enough history behind it.
            if took == 5000 {
                let pace = reviewed.pace.expect("three timed answers should be enough for a pace");
                assert_eq!(pace.answers, 3);
                assert_eq!(pace.last_ms, 5000);
                assert_eq!(
                    pace.typical_ms, 5000,
                    "one interrupted turn should not drag the usual pace: the median of 4000, 5000 and 30000 is 5000"
                );
            } else {
                assert!(reviewed.pace.is_none(), "a pace was reported from {took}ms and too little history");
            }
        }
    }

    #[tokio::test]
    async fn an_untimed_answer_never_invents_a_pace() {
        let pool = loaded(2).await;
        let mut day = 0;
        for _ in 0..4 {
            let reviewed = review(&pool, "f0", said(Rating::Good), at(day)).await.unwrap();
            day = reviewed.due.signed_duration_since(at(0)).num_days();
            assert!(reviewed.pace.is_none(), "a pace was reported for answers nobody timed");
        }
    }

    #[tokio::test]
    async fn pace_does_not_touch_the_schedule() {
        // The decision of the owner (2026-09-03): speed is shown, not
        // scheduled on. A home-made correction over FSRS would move every
        // interval with no way to tell what moved it.
        let quick = loaded(2).await;
        let slow = loaded(2).await;

        let fast = review(&quick, "f0", said(Rating::Good).taking(900), at(0)).await.unwrap();
        let crawl = review(&slow, "f0", said(Rating::Good).taking(45_000), at(0)).await.unwrap();
        assert_eq!(fast.due, crawl.due, "how long the answer took changed when the formula comes back");
        assert_eq!(fast.stitch, crawl.stitch);
    }

    #[tokio::test]
    async fn a_queued_card_carries_its_pace() {
        let pool = loaded(2).await;
        let mut day = 0;
        for took in [3000, 3500, 4000] {
            let reviewed = review(&pool, "f0", said(Rating::Good).taking(took), at(day)).await.unwrap();
            day = reviewed.due.signed_duration_since(at(0)).num_days();
        }

        let due = today(&pool, at(day))
            .await
            .unwrap()
            .queue
            .into_iter()
            .find(|due| due.formula_id() == "f0" && due.direction() == Some(Direction::Produce))
            .expect("the formula should be waiting on the day it is due");
        assert_eq!(due.pace.expect("a drilled card should carry its pace").typical_ms, 3500);
    }

    #[tokio::test]
    async fn the_counts_follow_the_learner() {
        let pool = loaded(4).await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let counts = counts(&pool).await.unwrap();
        assert_eq!(counts.new, 3, "the formula answered should have left the new pile");
        assert_eq!(counts.basted + counts.sewn, 1);
    }

    /// Two formulas whose examples teach four words: "doctor" in a sentence
    /// of each, "umbrella" past the 5k level.
    const WORDED: &str = r#"
id = "w"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "f0"
name = "n"
pattern = "x"
explanation = "e"
order = 10

  [[formula.sample]]
  native = "Он врач."
  target = "He is a doctor."
  words = ["doctor"]

  [[formula.sample]]
  native = "Я дома."
  target = "I am at home."
  words = ["home"]

[[formula]]
id = "f1"
name = "n"
pattern = "y"
explanation = "e"
order = 20

  [[formula.sample]]
  native = "Мой друг врач."
  target = "My friend is a doctor."
  words = ["friend", "doctor"]

  [[formula.sample]]
  native = "Где мой зонт?"
  target = "Where is my umbrella?"
  words = ["umbrella"]

[[word]]
lemma = "doctor"
gloss = "врач"

[[word]]
lemma = "home"
gloss = "дом"

[[word]]
lemma = "friend"
gloss = "друг"

[[word]]
lemma = "umbrella"
gloss = "зонт"
"#;

    async fn worded() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let pack: pack::Pack = toml::from_str(WORDED).unwrap();
        pack.validate().unwrap();
        pack::load(&pool, &pack).await.unwrap();
        pool
    }

    fn words_in(today: &Today) -> Vec<&str> {
        today.queue.iter().filter_map(Due::word_id).collect()
    }

    const fn graded(rating: Rating) -> Graded {
        Graded { rating, duration_ms: None }
    }

    #[tokio::test]
    async fn a_word_waits_until_a_sentence_that_holds_it_has_been_met() {
        // The whole of "a word enters the deck only inside a sentence".
        let pool = worded().await;
        assert!(
            words_in(&today(&pool, at(0)).await.unwrap()).is_empty(),
            "a word was offered before any sentence was met"
        );

        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let after = today(&pool, at(0)).await.unwrap();
        let mut offered = words_in(&after);
        offered.sort_unstable();
        assert_eq!(
            offered,
            vec!["en:doctor", "en:home"],
            "the words of the formula just drilled should open, and only those"
        );
        assert_eq!(after.words.waiting, 0, "both words are in today's queue: nothing waits for a later day");
    }

    #[tokio::test]
    async fn words_come_after_the_formulas_and_the_commonest_first() {
        let pool = worded().await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let queue = today(&pool, at(0)).await.unwrap().queue;
        let first_word = queue.iter().position(|due| due.word_id().is_some()).unwrap();
        assert!(queue[..first_word].iter().all(|due| due.direction().is_some()));
        assert!(queue[first_word..].iter().all(|due| due.word_id().is_some()), "a formula came after a word");
        // home is far commoner than doctor.
        assert_eq!(queue[first_word].word_id(), Some("en:home"));
    }

    #[tokio::test]
    async fn a_word_met_in_two_sentences_shows_both_and_only_those_met() {
        let pool = worded().await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let doctor = words::word(&pool, "en:doctor").await.unwrap();
        assert_eq!(doctor.contexts.len(), 1, "the sentence of a formula not yet drilled was shown");
        assert_eq!(doctor.contexts[0].text, "He is a doctor.");
        assert!(doctor.contexts[0].anchor);
        assert_eq!((doctor.contexts[0].form.as_str(), doctor.contexts[0].start), ("doctor", 8));
        assert_eq!(doctor.level, Some(1000));
        assert!(doctor.ipa.is_some(), "the lexicon says how doctor sounds");

        review(&pool, "f1", said(Rating::Good), at(1)).await.unwrap();
        let doctor = words::word(&pool, "en:doctor").await.unwrap();
        let texts: Vec<_> = doctor.contexts.iter().map(|context| context.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["He is a doctor.", "My friend is a doctor."],
            "one word, two contexts, the first met first"
        );
    }

    #[tokio::test]
    async fn a_sentence_two_formulas_share_is_one_context() {
        // The same example in two formulas is one sentence: the word is met
        // in it once, however many formulas show it.
        let shared = WORDED.replace(
            "  native = \"Где мой зонт?\"\n  target = \"Where is my umbrella?\"\n  words = [\"umbrella\"]",
            "  native = \"Он врач.\"\n  target = \"He is a doctor.\"\n  words = [\"doctor\"]",
        );
        let shared = shared.replace("[[word]]\nlemma = \"umbrella\"\ngloss = \"зонт\"\n", "");
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let pack: pack::Pack = toml::from_str(&shared).unwrap();
        pack.validate().unwrap();
        pack::load(&pool, &pack).await.unwrap();
        review(&pool, "f1", said(Rating::Good), at(0)).await.unwrap();

        let doctor = words::word(&pool, "en:doctor").await.unwrap();
        let texts: Vec<_> = doctor.contexts.iter().map(|context| context.text.as_str()).collect();
        assert_eq!(texts, vec!["He is a doctor.", "My friend is a doctor."]);
        assert_eq!(doctor.contexts[0].formula, "f0", "a shared sentence stands where it is first shown");
    }

    #[tokio::test]
    async fn the_first_answer_fixes_the_sentence_a_word_is_heard_in() {
        let pool = worded().await;
        review(&pool, "f1", said(Rating::Good), at(0)).await.unwrap();
        review_word(&pool, "en:doctor", graded(Rating::Good), at(0)).await.unwrap();

        // Drilling the earlier formula later brings a sentence that comes
        // first in the pack; the word is still heard where it was learnt.
        review(&pool, "f0", said(Rating::Good), at(1)).await.unwrap();
        let doctor = words::word(&pool, "en:doctor").await.unwrap();
        assert_eq!(doctor.contexts.len(), 2);
        assert_eq!(doctor.contexts[0].text, "My friend is a doctor.", "the anchor moved");
        assert!(doctor.contexts[0].anchor && !doctor.contexts[1].anchor);
    }

    #[tokio::test]
    async fn an_answered_word_comes_back_on_its_own_schedule() {
        let pool = worded().await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let reviewed = review_word(&pool, "en:home", graded(Rating::Again), at(0)).await.unwrap();
        assert_eq!(reviewed.stitch, Stitch::Basted);

        let later = today(&pool, reviewed.due).await.unwrap();
        let due = later
            .queue
            .iter()
            .find(|due| due.word_id() == Some("en:home"))
            .expect("the word should be back");
        assert!(!due.is_new);
        assert!(review_word(&pool, "en:nothing", graded(Rating::Good), at(0)).await.is_err());
    }

    #[tokio::test]
    async fn the_day_opens_no_more_than_its_budget_of_words() {
        use std::fmt::Write as _;
        let nouns = [
            "cat", "dog", "house", "car", "book", "tree", "water", "city", "door", "table", "chair", "window",
        ];
        let mut toml = String::from(
            "id = \"many\"\nversion = 1\nnative = \"ru\"\ntarget = \"en\"\n\n[[formula]]\nid = \"f0\"\nname = \"n\"\npattern = \"x\"\nexplanation = \"e\"\norder = 10\n",
        );
        for noun in nouns {
            let _ = write!(
                toml,
                "\n  [[formula.sample]]\n  native = \"{noun}\"\n  target = \"I see the {noun}.\"\n  words = [\"{noun}\"]\n"
            );
        }
        for noun in nouns {
            let _ = write!(toml, "\n[[word]]\nlemma = \"{noun}\"\ngloss = \"x\"\n");
        }
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pack::load(&pool, &toml::from_str(&toml).unwrap()).await.unwrap();
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();

        let offered = words_in(&today(&pool, at(0)).await.unwrap())
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(offered.len(), NEW_WORDS_PER_DAY);
        let english = crate::lexicon::of("en").unwrap();
        let ranks: Vec<u32> = offered.iter().map(|id| english.entry(&id[3..]).unwrap().rank).collect();
        assert!(ranks.windows(2).all(|pair| pair[0] < pair[1]), "the commonest should open first: {offered:?}");

        // Each word started today takes one place of today's budget.
        for id in offered.iter().take(4) {
            review_word(&pool, id, graded(Rating::Easy), at(0)).await.unwrap();
        }
        let after = today(&pool, at(0)).await.unwrap();
        let fresh = after.queue.iter().filter(|due| due.is_new && due.word_id().is_some()).count();
        assert_eq!(fresh, NEW_WORDS_PER_DAY - 4);
        // Twelve met, four started, six in today's queue: two wait for tomorrow.
        assert_eq!(after.words.waiting, 2);
    }

    /// `be` stands alone; `want` stands on `be`; `ask` stands alone, later in
    /// the sequence; `like` stands on `want`. Every formula substitutes a
    /// pronoun and a rest, so a turn can be typed against it.
    const TREE: &str = r#"
id = "tree"
version = 1
native = "ru"
target = "en"

[[formula]]
id = "be"
name = "to be"
pattern = "<pronoun> + am/is + <rest>"
say = "<pronoun> <pronoun:be> <rest>."
accept = ["<pronoun:short> <rest>."]
explanation = "e"
order = 10
  [[formula.sample]]
  native = "Она занята."
  target = "She is busy."
  words = ["busy"]
  accept = ["She's busy."]
  [[formula.slot]]
  name = "pronoun"
  values = [
    { native = "я", target = "I", be = "am", short = "I'm" },
    { native = "она", target = "she", be = "is", short = "she's" },
  ]
  [[formula.slot]]
  name = "rest"
  takes = "noun"
  values = [{ native = "занята", target = "busy" }, { native = "дома", target = "at home" }]

[[formula]]
id = "want"
name = "want"
pattern = "<pronoun> + want + <rest>"
say = "<pronoun> want <rest>."
explanation = "e"
order = 20
parents = ["be"]
  [[formula.sample]]
  native = "Я хочу чай."
  target = "I want tea."
  [[formula.slot]]
  name = "pronoun"
  values = [{ native = "я", target = "I" }]
  [[formula.slot]]
  name = "rest"
  values = [{ native = "чай", target = "tea" }]

[[formula]]
id = "ask"
name = "ask"
pattern = "Can + <pronoun> + go?"
say = "Can <pronoun> go?"
explanation = "e"
order = 30
  [[formula.sample]]
  native = "Можно мне?"
  target = "Can I go?"
  [[formula.slot]]
  name = "pronoun"
  values = [{ native = "я", target = "I" }]

[[formula]]
id = "like"
name = "would like"
pattern = "<pronoun> + would like + <rest>"
say = "<pronoun> would like <rest>."
explanation = "e"
order = 40
parents = ["want"]
  [[formula.sample]]
  native = "Я бы хотел чай."
  target = "I would like tea."
  [[formula.slot]]
  name = "pronoun"
  values = [{ native = "я", target = "I" }]
  [[formula.slot]]
  name = "rest"
  values = [{ native = "чай", target = "tea" }]

[[word]]
lemma = "busy"
gloss = "занятой"
"#;

    async fn tree() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let pack: pack::Pack = toml::from_str(TREE).unwrap();
        pack.validate().unwrap();
        pack::load(&pool, &pack).await.unwrap();
        pool
    }

    fn new_formulas(today: &Today) -> Vec<&str> {
        today
            .queue
            .iter()
            .filter(|due| due.is_new && due.direction() == Some(Direction::Produce))
            .map(|due| due.formula_id().as_str())
            .collect()
    }

    /// Sews a formula: two good answers a day apart, the way a learner
    /// coming back would.
    async fn sew(pool: &SqlitePool, id: &str, day: i64) {
        review(pool, id, said(Rating::Good), at(day)).await.unwrap();
        let sewn = review(pool, id, said(Rating::Good), at(day) + chrono::Duration::hours(1)).await.unwrap();
        assert_eq!(sewn.stitch, Stitch::Sewn, "the fixture assumes two good answers sew a formula");
    }

    #[tokio::test]
    async fn a_formula_waits_until_its_parents_are_sewn() {
        // The whole of idea 1: grammar goes up the tree.
        let pool = tree().await;
        review(&pool, "be", said(Rating::Good), at(0)).await.unwrap();

        // Day 1: "be" is only basted, so "want" is held and the next formula
        // that stands on nothing opens instead.
        let day = today(&pool, at(1)).await.unwrap();
        assert_eq!(new_formulas(&day), vec!["ask"], "a formula opened on a loose parent, or nothing opened at all");
        let next = day.next.expect("the formula held back should be named");
        assert_eq!(next.id, "want");
        assert_eq!(next.waits_for.iter().map(|parent| parent.id.as_str()).collect::<Vec<_>>(), vec!["be"]);

        // Once the parent is sewn, the child is next in line again.
        sew(&pool, "be", 1).await;
        review(&pool, "ask", said(Rating::Good), at(1)).await.unwrap();
        let day = today(&pool, at(2)).await.unwrap();
        assert_eq!(new_formulas(&day), vec!["want"]);
        let next = day.next.expect("would like is still to come");
        assert_eq!(next.id, "like");
        assert!(!next.waits_for.is_empty(), "would like stands on want, which has only just opened");
    }

    #[tokio::test]
    async fn a_parent_with_no_card_holds_its_child_shut() {
        // A database edited by hand: the parent's card is gone. NULL is not
        // "sewn", and a comparison that forgot it would open the child.
        let pool = tree().await;
        sqlx::query("DELETE FROM card WHERE subject_id = 'be'").execute(&pool).await.unwrap();
        let day = today(&pool, at(0)).await.unwrap();
        assert!(!new_formulas(&day).contains(&"want"), "a formula opened on a parent nobody has");
    }

    #[tokio::test]
    async fn a_started_formula_is_not_shut_again_when_its_parent_slips() {
        let pool = tree().await;
        sew(&pool, "be", 0).await;
        review(&pool, "want", said(Rating::Good), at(1)).await.unwrap();
        review(&pool, "be", said(Rating::Again), at(2)).await.unwrap();
        let later = today(&pool, at(30)).await.unwrap();
        assert!(
            later.queue.iter().any(|due| due.formula_id() == "want" && !due.is_new),
            "a formula already being drilled vanished because its parent came loose"
        );
    }

    #[tokio::test]
    async fn the_card_names_the_parents_it_stands_on() {
        let pool = tree().await;
        let want = formula(&pool, "want").await.unwrap();
        let parents: Vec<_> = want.parents.iter().map(|parent| (parent.id.as_str(), parent.stitch)).collect();
        assert_eq!(parents, vec![("be", Stitch::New)]);
        assert!(formula(&pool, "be").await.unwrap().parents.is_empty());
    }

    fn typed(asked: Asked, text: &str) -> Turn {
        Turn {
            asked,
            typed: text.to_string(),
        }
    }

    fn values(pairs: &[(&str, &str)]) -> Asked {
        Asked::Values(pairs.iter().map(|(slot, value)| ((*slot).to_string(), (*value).to_string())).collect())
    }

    fn produced(turns: Vec<Turn>, easy: bool) -> Answer {
        Answer::Produce {
            turns,
            easy,
            duration_ms: Some(5000),
        }
    }

    #[tokio::test]
    async fn a_said_answer_is_graded_by_what_the_turns_got_wrong() {
        let pool = tree().await;
        let she = || values(&[("pronoun", "she"), ("rest", "at home")]);
        let right = || typed(she(), "She's at home");
        let slip = || typed(she(), "She are at home");

        let flawless = super::review(&pool, "be", produced(vec![right(), right()], true), at(0)).await.unwrap();
        assert_eq!(flawless.rating, Rating::Easy, "flawless turns and the learner's word earn easy");

        let pool = tree().await;
        let one = super::review(&pool, "be", produced(vec![right(), slip(), right(), right()], true), at(0))
            .await
            .unwrap();
        assert_eq!(one.rating, Rating::Hard, "one slip in four is effort, and the learner's easy is not taken");

        let pool = tree().await;
        let half = super::review(&pool, "be", produced(vec![right(), slip(), slip(), right()], false), at(0))
            .await
            .unwrap();
        assert_eq!(half.rating, Rating::Again, "half the turns slipping means the shape is not there");
    }

    #[test]
    fn the_grade_follows_the_slips_of_the_shape_only() {
        use crate::answer::Failure::{Formula as Shape, Order, Word};
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

    #[tokio::test]
    async fn a_word_that_did_not_come_goes_to_its_own_card() {
        // "в колоду уходит именно сбой": the shape came, the word did not -
        // the formula keeps its grade and the word comes back on its own.
        let pool = tree().await;
        let sentence = formula(&pool, "be").await.unwrap().samples[0].sentence;
        let reviewed = super::review(&pool, "be", produced(vec![typed(Asked::Sample(sentence), "She is bisy.")], false), at(0))
            .await
            .unwrap();
        assert_eq!(reviewed.rating, Rating::Good, "the formula was marked down for a word");
        assert_eq!(reviewed.words, vec!["en:busy"]);

        let (state, reps): (String, i64) = sqlx::query_as("SELECT state, reps FROM card WHERE kind = 'word' AND subject_id = 'en:busy'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((state.as_str(), reps), ("basted", 1), "the missed word should be back in the deck");
        let rating: i64 =
            sqlx::query_scalar("SELECT r.rating FROM review r JOIN card c ON c.id = r.card_id WHERE c.kind = 'word' AND c.subject_id = 'en:busy'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rating, Rating::Again.as_i64());
    }

    #[tokio::test]
    async fn a_slip_of_the_shape_sends_no_word_to_the_deck() {
        let pool = tree().await;
        let sentence = formula(&pool, "be").await.unwrap().samples[0].sentence;
        let reviewed = super::review(&pool, "be", produced(vec![typed(Asked::Sample(sentence), "")], false), at(0))
            .await
            .unwrap();
        assert_eq!(reviewed.rating, Rating::Again);
        assert!(reviewed.words.is_empty(), "nothing typed says nothing about the words");
        let reps: i64 = sqlx::query_scalar("SELECT reps FROM card WHERE kind = 'word' AND subject_id = 'en:busy'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(reps, 0);
    }

    #[tokio::test]
    async fn every_turn_is_kept_with_its_review() {
        let pool = tree().await;
        let turns = vec![
            typed(values(&[("pronoun", "I"), ("rest", "busy")]), "I'm busy"),
            typed(values(&[("pronoun", "she"), ("rest", "busy")]), "Busy she is"),
        ];
        super::review(&pool, "be", produced(turns, false), at(0)).await.unwrap();
        let kept: Vec<(String, String, String, i64, Option<String>)> =
            sqlx::query_as("SELECT prompt, expected, typed, correct, failure FROM attempt ORDER BY position")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            kept[0],
            ("я + am/is + занята".to_string(), "I'm busy.".to_string(), "I'm busy".to_string(), 1, None)
        );
        assert_eq!(kept[1].3, 0);
        assert_eq!(kept[1].4.as_deref(), Some("order"));
    }

    #[tokio::test]
    async fn a_turn_naming_what_the_formula_does_not_ask_is_refused() {
        let pool = tree().await;
        for asked in [
            values(&[("pronoun", "thou"), ("rest", "busy")]),
            values(&[("pronoun", "I")]),
            values(&[("pronoun", "I"), ("rest", "busy"), ("place", "here")]),
            Asked::Sample(9999),
        ] {
            let error = check(&pool, "be", &typed(asked, "x")).await.unwrap_err();
            assert!(matches!(error, AnswerError::Unanswerable(_)), "{error:#}");
        }
        assert!(matches!(
            check(&pool, "nonsense", &typed(Asked::Sample(1), "x")).await.unwrap_err(),
            AnswerError::NotFound(_)
        ));
        let none = super::review(&pool, "be", produced(Vec::new(), false), at(0)).await.unwrap_err();
        assert!(matches!(none, AnswerError::Unanswerable(_)));
        let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review").fetch_one(&pool).await.unwrap();
        assert_eq!(reviews, 0, "a refused answer was recorded");
    }

    #[tokio::test]
    async fn own_words_stand_in_the_slots_that_take_them_and_answer_like_any_value() {
        let pool = tree().await;
        let car = own::NewWord {
            kind: OwnKind::Noun,
            native: "в машине".to_string(),
            target: "in the car".to_string(),
        };
        own::add(&pool, &car, at(0)).await.unwrap();
        // One that says what a pack value says is that value.
        let busy = own::NewWord {
            target: "busy".to_string(),
            ..car.clone()
        };
        own::add(&pool, &busy, at(0)).await.unwrap();
        let verb = own::NewWord {
            kind: OwnKind::Verb,
            ..car.clone()
        };
        own::add(&pool, &verb, at(0)).await.unwrap();

        let be = formula(&pool, "be").await.unwrap();
        let rest: Vec<(&str, bool)> = be.slots[1].values.iter().map(|value| (value.target.as_str(), value.own)).collect();
        assert_eq!(rest, vec![("busy", false), ("at home", false), ("in the car", true)]);
        assert!(be.slots[0].values.iter().all(|value| !value.own), "a slot that takes nothing took an own word");

        let checked = check(&pool, "be", &typed(values(&[("pronoun", "she"), ("rest", "in the car")]), "She's in the car."))
            .await
            .unwrap();
        assert!(checked.correct);
        let checked = check(&pool, "be", &typed(values(&[("pronoun", "she"), ("rest", "in the car")]), "She is in the bus."))
            .await
            .unwrap();
        assert_eq!(checked.failure, Some(Failure::Word), "an own word missed is a word missed");
    }

    #[tokio::test]
    async fn the_levels_count_the_words_in_hand_by_how_common_they_are() {
        let pool = worded().await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        review(&pool, "f1", said(Rating::Good), at(0)).await.unwrap();
        for id in ["en:home", "en:doctor", "en:umbrella"] {
            review_word(&pool, id, graded(Rating::Good), at(0)).await.unwrap();
        }
        let standing = words::standing(&pool).await.unwrap();
        assert_eq!(standing.counts.new, 1, "friend was not answered");
        let held: Vec<(u32, i64)> = standing.levels.iter().map(|level| (level.size, level.basted + level.sewn)).collect();
        // home and doctor are in the first thousand; umbrella is past 5k and
        // counted in no level.
        assert_eq!(held, vec![(1000, 2), (2000, 2), (5000, 2)]);
    }

    // ------------------------------------------------ the schedule lives

    /// Twelve common nouns met in one formula, and a second formula with no
    /// words: room enough to see the day's ten new words shrink, and a
    /// formula of the day to hold back.
    async fn crowded() -> SqlitePool {
        use std::fmt::Write as _;
        let nouns = [
            "cat", "dog", "house", "car", "book", "tree", "water", "city", "door", "table", "chair", "window",
        ];
        let mut toml = String::from(
            "id = \"many\"\nversion = 1\nnative = \"ru\"\ntarget = \"en\"\n\n[[formula]]\nid = \"f0\"\nname = \"n\"\npattern = \"x\"\nexplanation = \"e\"\norder = 10\n",
        );
        for noun in nouns {
            let _ = write!(
                toml,
                "\n  [[formula.sample]]\n  native = \"{noun}\"\n  target = \"I see the {noun}.\"\n  words = [\"{noun}\"]\n"
            );
        }
        toml.push_str("\n[[formula]]\nid = \"f1\"\nname = \"m\"\npattern = \"y\"\nexplanation = \"e\"\norder = 20\n\n  [[formula.sample]]\n  native = \"a\"\n  target = \"b\"\n");
        for noun in nouns {
            let _ = write!(toml, "\n[[word]]\nlemma = \"{noun}\"\ngloss = \"x\"\n");
        }
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pack::load(&pool, &toml::from_str(&toml).unwrap()).await.unwrap();
        pool
    }

    /// The first evening of `crowded`: the formula drilled and `words` of the
    /// words it opened answered, all at `at(0)`.
    async fn first_evening(pool: &SqlitePool, words: usize) -> Vec<String> {
        review(pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let opened: Vec<String> = words_in(&today(pool, at(0)).await.unwrap()).into_iter().map(str::to_string).collect();
        for id in opened.iter().take(words) {
            review_word(pool, id, graded(Rating::Good), at(0)).await.unwrap();
        }
        opened
    }

    fn new_words_in(today: &Today) -> usize {
        today.queue.iter().filter(|due| due.is_new && due.word_id().is_some()).count()
    }

    #[tokio::test]
    async fn what_the_clock_drifts_past_a_sitting_is_not_owed() {
        // A card that came due ten minutes after yesterday's sitting could
        // not have been answered in it: the next day owes nothing.
        let pool = crowded().await;
        first_evening(&pool, 4).await;
        let next_day = today(&pool, at(1)).await.unwrap();
        assert_eq!(
            next_day.room,
            Room {
                debt: 0,
                words: 10,
                formulas: 1
            }
        );
    }

    #[tokio::test]
    async fn a_card_left_from_an_earlier_day_takes_a_new_words_place() {
        let pool = crowded().await;
        first_evening(&pool, 4).await;

        // A day missed: the formula and four words waited through it.
        let back = today(&pool, at(2)).await.unwrap();
        assert_eq!(
            back.room,
            Room {
                debt: 5,
                words: 5,
                formulas: 1
            }
        );
        assert_eq!(new_words_in(&back), 5, "five places were taken by what was owed");
        assert_eq!(new_formulas(&back), vec!["f1"], "room for a word is room for the formula of the day");

        // Paying the debt does not open another round: the day's room is
        // what the day began with.
        for due in back.queue.iter().filter(|due| !due.is_new) {
            match &due.subject {
                Subject::Word { word } => review_word(&pool, &word.id, graded(Rating::Good), at(2)).await.map(|_| ()),
                Subject::Formula { formula, direction } => {
                    let said = Said {
                        direction: *direction,
                        ..said(Rating::Good)
                    };
                    review(&pool, &formula.id, said, at(2)).await.map(|_| ())
                }
            }
            .unwrap();
        }
        let later = today(&pool, at(2) + Duration::hours(2)).await.unwrap();
        assert_eq!(later.room.debt, 5, "the day's debt is counted as the day began");
        assert_eq!(new_words_in(&later), 5);
    }

    #[tokio::test]
    async fn a_debt_of_ten_leaves_no_room_for_anything_new() {
        let pool = crowded().await;
        first_evening(&pool, 10).await;
        let back = today(&pool, at(2)).await.unwrap();
        assert_eq!(
            back.room,
            Room {
                debt: 11,
                words: 0,
                formulas: 0
            }
        );
        assert_eq!(new_words_in(&back), 0);
        assert!(new_formulas(&back).is_empty(), "a new formula was opened on a day that owed eleven cards");
        assert!(back.queue.iter().filter(|due| !due.is_new).count() >= 11, "what is owed is still asked");
    }

    #[tokio::test]
    async fn a_word_is_owed_from_when_its_sentence_was_met() {
        // Declared known long ago, its schedule ran out before any sentence
        // brought it: met today, it is today's word, not an old debt.
        let pool = worded().await;
        let known = |word: &str| crate::inventory::Answer {
            word: word.to_string(),
            known: true,
        };
        crate::inventory::answer(&pool, &[known("en:doctor")], at(0)).await.unwrap();
        review(&pool, "f0", said(Rating::Good), at(40)).await.unwrap();

        let met = today(&pool, at(40) + Duration::hours(1)).await.unwrap();
        assert!(
            met.queue.iter().any(|due| due.word_id() == Some("en:doctor")),
            "the declared word should be asked once met"
        );
        assert_eq!(met.room.debt, 0);
        // Left unanswered through the next day, it is owed like any other.
        assert!(today(&pool, at(42)).await.unwrap().room.debt >= 1);
    }

    #[tokio::test]
    async fn a_break_holds_the_queue_and_moves_what_it_spans_by_its_length() {
        let pool = worded().await;
        let first = review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let interval = first.due - at(0);
        let leaving = at(0) + interval / 2;
        crate::breaks::start(&pool, Some(at(3)), leaving).await.unwrap();

        let away = today(&pool, at(1)).await.unwrap();
        assert!(away.on_break.is_some());
        assert!(
            away.queue.is_empty(),
            "the schedule should wait while the learner is away: {:?}",
            away.queue.len()
        );
        assert_eq!(away.room, Room::default(), "nothing opens on a break");

        // Back: the formula had half its interval left when the break began.
        let comes_back = at(3) + (interval - interval / 2);
        let just_before = today(&pool, comes_back - Duration::seconds(1)).await.unwrap();
        assert!(
            !just_before
                .queue
                .iter()
                .any(|due| due.formula_id() == "f0" && due.direction() == Some(Direction::Produce) && !due.is_new),
            "f0 came back before its interval ran out"
        );
        let at_time = today(&pool, comes_back).await.unwrap();
        assert!(at_time.on_break.is_none());
        assert!(
            at_time
                .queue
                .iter()
                .any(|due| due.formula_id() == "f0" && due.direction() == Some(Direction::Produce)),
            "f0 should be back once the rest of its interval has run"
        );
        assert_eq!(at_time.room.debt, 0, "what a break moved is not owed");
    }

    #[tokio::test]
    async fn a_word_waits_out_a_break_too() {
        let pool = worded().await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        let answered = review_word(&pool, "en:home", graded(Rating::Good), at(0)).await.unwrap();
        let interval = answered.due - at(0);
        crate::breaks::start(&pool, Some(at(2)), at(0) + interval / 2).await.unwrap();

        let comes_back = at(2) + (interval - interval / 2);
        let home = |today: &Today| today.queue.iter().any(|due| due.word_id() == Some("en:home"));
        assert!(
            !home(&today(&pool, comes_back - Duration::seconds(1)).await.unwrap()),
            "home came back before its interval ran out"
        );
        assert!(
            home(&today(&pool, comes_back).await.unwrap()),
            "home should be back once the rest of its interval has run"
        );
    }

    #[tokio::test]
    async fn an_answer_given_on_a_break_ends_it() {
        let pool = worded().await;
        review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        crate::breaks::start(&pool, None, at(1)).await.unwrap();
        review(&pool, "f0", said(Rating::Good).backwards(), at(2)).await.unwrap();
        let taken = crate::breaks::all(&pool).await.unwrap();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].ends_at, Some(at(2)), "answering is being back");
    }

    #[tokio::test]
    async fn an_absence_counted_as_a_break_owes_nothing() {
        let pool = crowded().await;
        first_evening(&pool, 4).await;

        let back = today(&pool, at(5)).await.unwrap();
        assert_eq!(back.room.debt, 5);
        assert_eq!(back.away_since, Some(at(0)), "five days away with cards waiting should be offered as a break");

        crate::breaks::count_away(&pool, at(5)).await.unwrap();
        let counted = today(&pool, at(5)).await.unwrap();
        assert_eq!(
            counted.room,
            Room {
                debt: 0,
                words: 10,
                formulas: 1
            }
        );
        assert_eq!(counted.away_since, None, "an absence already counted is not offered again");
        assert!(
            counted.queue.iter().all(|due| due.is_new),
            "what came due while away should come back after the break, not on the day back"
        );
    }

    #[tokio::test]
    async fn a_short_absence_is_not_offered_as_a_break() {
        let pool = crowded().await;
        first_evening(&pool, 4).await;
        // Two days: a weekend. What came due over it is owed.
        assert_eq!(today(&pool, at(2)).await.unwrap().away_since, None);
        assert!(crate::breaks::count_away(&pool, at(2)).await.is_err());
    }

    #[tokio::test]
    async fn the_card_says_how_firmly_it_holds_and_the_answer_how_far_it_moved() {
        let pool = worded().await;
        let fresh = today(&pool, at(0)).await.unwrap();
        assert_eq!(fresh.queue[0].stability, 0.0, "a card never answered holds nothing yet");

        let first = review(&pool, "f0", said(Rating::Good), at(0)).await.unwrap();
        assert_eq!(first.stability_before, 0.0);
        assert!(first.stability > 0.0);

        let queued = today(&pool, first.due).await.unwrap();
        let again = queued
            .queue
            .iter()
            .find(|due| due.formula_id() == "f0" && due.direction() == Some(Direction::Produce))
            .unwrap();
        assert!((again.stability - first.stability).abs() < 1e-9, "the queue should carry the card's stability");
        let second = review(&pool, "f0", said(Rating::Easy), first.due).await.unwrap();
        assert!((second.stability_before - first.stability).abs() < 1e-9);
        assert!(second.stability > second.stability_before, "an easy answer should make it hold longer");
    }
}
