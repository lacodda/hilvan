//! The demo learner, and the learner model file it is written in.
//!
//! The demo is a fictional learner three weeks into English from Russian on
//! the shipped pack: what `hilvan load-demo` loads and what the docs are
//! shown with. `cargo test --test demo -- --ignored` makes it, by driving the
//! tutor's own code through those weeks - the inventory on the first evening,
//! a sitting most days, typed turns with the slips a learner makes, words of
//! their own, a long weekend away and a few days that went missing - and
//! writes `demo/learner.json`.
//!
//! The other tests hold that file, and every model file, to what the format
//! promises: it follows the published schema, it learns from the pack that
//! ships, every card is what its answers leave, it loads onto an empty tutor
//! and comes back out the same, and a snapshot of any day is what the tutor
//! would have counted that day.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use hilvan::breaks;
use hilvan::inventory;
use hilvan::model::{self, Model};
use hilvan::own::{self, NewWord};
use hilvan::pack::{self, OwnKind, Pack};
use hilvan::say::Template;
use hilvan::scheduling::{Rating, Scheduler, Source};
use hilvan::study::{self, Answer, Asked, Graded, Subject, Turn, Value};
use serde_json::Value as Json;
use sqlx::SqlitePool;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn shipped_pack() -> Pack {
    Pack::read(root().join("packs/en-from-ru/pack.toml")).expect("the shipped pack should read")
}

/// A tutor on a real file in a fresh directory, migrated from empty, with
/// the shipped pack loaded.
async fn tutor() -> (SqlitePool, tempfile::TempDir) {
    let data = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}?mode=rwc", data.path().join("hilvan.db").display());
    let pool = hilvan::db::connect(&url).await.expect("the database should open");
    pack::load(&pool, &shipped_pack()).await.expect("the shipped pack should load");
    (pool, data)
}

fn schema() -> jsonschema::Validator {
    let text = std::fs::read_to_string(root().join("docs/site/public/schema/learner-model-2.json")).expect("the schema should be in the docs");
    let schema: Json = serde_json::from_str(&text).expect("the schema should be JSON");
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the schema should be a schema")
}

/// What the schema says against a document, one line per complaint.
fn complaints(document: &Json) -> Vec<String> {
    schema()
        .iter_errors(document)
        .map(|error| format!("{} at {}", error, error.instance_path()))
        .collect()
}

fn demo() -> Model {
    Model::parse(model::DEMO).expect("the demo learner should read")
}

// ------------------------------------------------------------ the learner

/// A die with a fixed seed: xorshift64*, written out so the demo does not
/// change with a dependency's idea of a random stream.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    #[allow(clippy::cast_precision_loss)]
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    #[allow(clippy::cast_possible_truncation)]
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// One of four ratings, by the weights of again, hard, good and easy.
    fn rating(&mut self, weights: [f64; 4]) -> Rating {
        let mut roll = self.unit() * weights.iter().sum::<f64>();
        for (rating, weight) in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy].into_iter().zip(weights) {
            if roll < weight {
                return rating;
            }
            roll -= weight;
        }
        Rating::Easy
    }

    fn ms(&mut self, from: usize, to: usize) -> i64 {
        i64::try_from(from + self.below(to - from)).unwrap_or(i64::MAX)
    }
}

/// The first evening of the demo: a Monday, after work.
fn start() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-05T18:40:00Z").unwrap().with_timezone(&Utc)
}

/// How long the demo learner has been at it.
const DAYS: i64 = 24;

/// The long weekend away: Saturday and Sunday of the second week, said so
/// after the Friday's sitting.
const AWAY: std::ops::Range<i64> = 12..14;

/// The evenings of the third week that went missing with nothing said.
const MISSING: std::ops::Range<i64> = 17..20;

/// How likely the learner is to know a word of this rank: a few years of
/// school English, most of the first few hundred, fewer after.
#[allow(clippy::cast_precision_loss)]
fn knows(rank: u32) -> f64 {
    match rank {
        0..=300 => 0.96,
        301..=1000 => 0.96 - 0.6 * f64::from(rank - 300) / 700.0,
        _ => (0.36 - 0.25 * f64::from(rank - 1000) / 1000.0).max(0.08),
    }
}

/// Goes through the inventory from where it stands to `to`, a batch at a
/// time, the way the screen sends it.
async fn inventory_to(pool: &SqlitePool, dice: &mut Dice, to: u32, now: &mut DateTime<Utc>) {
    loop {
        let run = inventory::run(pool, inventory::BATCH).await.unwrap();
        let batch: Vec<inventory::Answer> = run
            .next
            .iter()
            .take_while(|item| item.rank <= to)
            .map(|item| inventory::Answer {
                word: item.word.clone(),
                known: dice.chance(knows(item.rank)),
            })
            .collect();
        if batch.is_empty() {
            return;
        }
        *now += Duration::seconds(i64::try_from(batch.len()).unwrap_or(0) * 2);
        inventory::answer(pool, &batch, *now).await.unwrap();
    }
}

/// A typed answer as a learner gets it wrong: a word left out, or two
/// words the wrong way round.
fn slip(dice: &mut Dice, right: &str) -> String {
    let mut words: Vec<&str> = right.split_whitespace().collect();
    if words.len() < 2 {
        return format!("{right} {right}");
    }
    if dice.chance(0.6) {
        words.remove(dice.below(words.len()));
    } else {
        let at = dice.below(words.len() - 1);
        words.swap(at, at + 1);
    }
    words.join(" ")
}

/// The four turns of a formula, asked the way the drill asks them: a worked
/// sample every third turn, a substitution otherwise.
fn turns(dice: &mut Dice, formula: &study::Formula, slip_chance: f64) -> Vec<Turn> {
    let say = formula.say.as_deref().map(|say| Template::parse(say).expect("the pack's templates read"));
    (0..4)
        .map(|turn| {
            let sample = (!formula.samples.is_empty()).then(|| &formula.samples[turn % formula.samples.len()]);
            let substitution = say.as_ref().filter(|_| turn % 3 != 0).and_then(|template| {
                let chosen: HashMap<&str, &Value> = formula
                    .slots
                    .iter()
                    .map(|slot| (slot.name.as_str(), &slot.values[dice.below(slot.values.len())]))
                    .collect();
                let text = template.render(&chosen)?;
                let values: BTreeMap<String, String> = chosen.iter().map(|(slot, value)| ((*slot).to_string(), value.target.clone())).collect();
                Some((Asked::Values(values), text))
            });
            let (asked, right) = substitution
                .or_else(|| sample.map(|sample| (Asked::Sample(sample.sentence), sample.target.clone())))
                .expect("a formula of the shipped pack has samples or slots");
            let typed = if dice.chance(slip_chance) { slip(dice, &right) } else { right };
            Turn { asked, typed }
        })
        .collect()
}

/// One sitting: the day's queue, read once and worked through - the way the
/// app runs it. What came straight back waits for tomorrow, basted.
async fn sitting(pool: &SqlitePool, dice: &mut Dice, now: &mut DateTime<Utc>) {
    {
        let today = study::today(pool, *now).await.unwrap();
        for due in today.queue {
            let duration_ms = dice.ms(2_500, 14_000);
            match due.subject {
                Subject::Formula { formula, direction } => {
                    let answer = match direction {
                        hilvan::scheduling::Direction::Produce => {
                            let turns = turns(dice, &formula, if due.is_new { 0.22 } else { 0.1 });
                            Answer::Produce {
                                turns,
                                easy: dice.chance(0.2),
                                duration_ms: Some(duration_ms),
                            }
                        }
                        hilvan::scheduling::Direction::Recognise => Answer::Recognise {
                            rating: dice.rating([0.06, 0.14, 0.62, 0.18]),
                            duration_ms: Some(duration_ms),
                        },
                    };
                    study::review(pool, &formula.id, sent(answer, *now), *now).await.unwrap();
                }
                Subject::Word { word } => {
                    let graded = Graded {
                        rating: dice.rating([0.08, 0.17, 0.6, 0.15]),
                        duration_ms: Some(duration_ms),
                    };
                    study::review_word(pool, &word.id, sent(graded, *now), *now).await.unwrap();
                }
            }
            *now += Duration::milliseconds(duration_ms + 3_000);
        }
    }
}

async fn own_word(pool: &SqlitePool, kind: OwnKind, native: &str, target: &str, now: DateTime<Utc>) {
    let word = NewWord {
        kind,
        native: native.to_string(),
        target: target.to_string(),
    };
    own::add(pool, &word, now).await.unwrap();
}

/// Three weeks of a fictional learner, as the tutor recorded them, told as
/// of the morning after the last evening.
async fn three_weeks() -> Model {
    let (pool, _data) = tutor().await;
    let mut dice = Dice(0x6869_6C76_616E_2121);

    for day in 0..DAYS {
        let mut now = start() + Duration::days(day) + Duration::minutes(dice.ms(0, 100));
        // The long weekend: the drill waits, and on the Sunday train the
        // inventory goes on - the next three hundred.
        if AWAY.contains(&day) {
            if day == AWAY.end - 1 {
                inventory_to(&pool, &mut dice, 1300, &mut now).await;
            }
            continue;
        }
        // Evenings that went missing with nothing said; on the next one the
        // tutor offers to count them as a break, and the learner does.
        if MISSING.contains(&day) {
            continue;
        }
        if day == MISSING.end {
            let today = study::today(&pool, now).await.unwrap();
            assert!(today.away_since.is_some(), "days away with something waiting should be offered as a break");
            breaks::count_away(&pool, now).await.unwrap();
        }
        match day {
            // The first evening: the commonest thousand words, then the
            // first formula.
            0 => inventory_to(&pool, &mut dice, 1000, &mut now).await,
            2 => {
                own_word(&pool, OwnKind::Verb, "готовить", "cook", now).await;
                own_word(&pool, OwnKind::Noun, "велосипед", "a bicycle", now).await;
            }
            9 => own_word(&pool, OwnKind::Verb, "водить", "drive", now).await,
            21 => own_word(&pool, OwnKind::Noun, "сад", "a garden", now).await,
            _ => {}
        }
        // Most evenings, not all of them; always the last, so the demo's
        // next day follows an evening of study, and always the Friday the
        // learner leaves on.
        if day > 0 && day < DAYS - 1 && day != AWAY.start - 1 && dice.chance(0.16) {
            continue;
        }
        sitting(&pool, &mut dice, &mut now).await;
        if day == AWAY.start - 1 {
            // Said on the way out: back on Monday morning.
            let back = start() + Duration::days(AWAY.end) + Duration::hours(-10);
            breaks::start(&pool, Some(back), now).await.unwrap();
        }
    }

    let morning = start() + Duration::days(DAYS) - Duration::hours(10);
    model::export(&pool, morning).await.unwrap()
}

/// The model as a file a person can still read and a diff can still follow:
/// the document's fields one a line, and each record of a list compact on a
/// line of its own.
fn lines(model: &Model) -> String {
    fn one<T: serde::Serialize>(name: &str, value: &T) -> String {
        format!("  {name:?}: {}", serde_json::to_string(value).unwrap())
    }
    fn list<T: serde::Serialize>(name: &str, items: &[T]) -> String {
        if items.is_empty() {
            return format!("  {name:?}: []");
        }
        let items: Vec<String> = items.iter().map(|item| format!("    {}", serde_json::to_string(item).unwrap())).collect();
        format!("  {name:?}: [\n{}\n  ]", items.join(",\n"))
    }
    // The fields in the order the model declares them, which is the order
    // the schema documents them in.
    let body = [
        one("$schema", &model.schema),
        one("format", &model.format),
        one("exported_at", &model.exported_at),
        one("as_of", &model.as_of),
        one("app", &model.app),
        list("languages", &model.languages),
        list("packs", &model.packs),
        list("cards", &model.cards),
        list("anchors", &model.anchors),
        list("own_words", &model.own_words),
        list("voices", &model.voices),
        list("inventory", &model.inventory),
        list("breaks", &model.breaks),
    ];
    format!("{{\n{}\n}}\n", body.join(",\n"))
}

#[test]
fn the_file_is_written_in_full() {
    // Every field of the model is a line of the file: one added to the model
    // and forgotten here would be a demo that does not read.
    let demo = demo();
    assert_eq!(Model::parse(&lines(&demo)).unwrap(), demo);
}

#[tokio::test]
#[ignore = "writes demo/learner.json: run when the pack or the learner model changes"]
async fn write_the_demo_learner() {
    let model = three_weeks().await;
    std::fs::write(root().join("demo/learner.json"), lines(&model)).expect("the demo should be written");
}

// ------------------------------------------------------------ the gate

#[test]
fn the_demo_learner_follows_the_schema() {
    let document: Json = serde_json::from_str(model::DEMO).unwrap();
    assert_eq!(complaints(&document), Vec::<String>::new());
}

#[test]
fn the_demo_learner_learns_from_the_pack_that_ships() {
    // A pack that renames a formula or drops a sentence leaves the demo
    // pointing at nothing: regenerate it then.
    let pack = shipped_pack();
    let demo = demo();
    assert_eq!(
        demo.packs.iter().map(|pack| (pack.id.as_str(), pack.version)).collect::<Vec<_>>(),
        vec![(pack.id.as_str(), pack.version)],
        "the demo should be made on the pack as it ships: run `cargo test --test demo -- --ignored`"
    );
    assert!(demo.cards.len() > 100, "three weeks should leave more than {} cards", demo.cards.len());

    for card in &demo.cards {
        let known = match card.kind.as_str() {
            "word" => pack.words.iter().any(|word| pack::word_id(&pack.target, &word.lemma) == card.subject) || hilvan::words::rank(&card.subject).is_some(),
            _ => pack.formulas.iter().any(|formula| formula.id == card.subject),
        };
        assert!(known, "{} {} is in neither the pack nor the lexicon", card.kind, card.subject);
    }
    let sentences: Vec<&str> = pack
        .formulas
        .iter()
        .flat_map(|formula| &formula.samples)
        .map(|sample| sample.target.as_str())
        .collect();
    for anchor in &demo.anchors {
        assert!(
            sentences.contains(&anchor.sentence.text.as_str()),
            "the anchor of {} is in no sample: {:?}",
            anchor.word,
            anchor.sentence.text
        );
    }
    let sources: Vec<Source> = demo.cards.iter().flat_map(|card| &card.reviews).map(|review| review.source).collect();
    assert!(
        sources.contains(&Source::Declared) && sources.contains(&Source::Asked),
        "the demo should both declare and answer"
    );
    assert!(
        demo.cards
            .iter()
            .flat_map(|card| &card.reviews)
            .flat_map(|review| &review.attempts)
            .any(|attempt| !attempt.correct),
        "the demo should slip now and then"
    );
    assert_eq!(demo.breaks.len(), 2, "the demo should take a break, and count one afterwards");
}

#[test]
fn every_card_is_what_its_answers_leave() {
    // A snapshot is the log played through the scheduler: it can only be
    // trusted if playing the whole log gives the card as it stands.
    for card in demo().cards {
        let replayed = Scheduler::new()
            .replay(card.reviews.iter().map(|review| (review.rating, review.at)))
            .expect("a card in the file has answers");
        let what = format!("{} {}", card.kind, card.subject);
        assert_eq!(replayed.stitch, card.state, "{what}");
        assert_eq!(replayed.due, card.due, "{what}");
        assert_eq!((replayed.reps, replayed.lapses), (card.reps, card.lapses), "{what}");
        assert_eq!(replayed.last_reviewed, card.last_reviewed, "{what}");
        assert!(
            (replayed.stability - card.stability).abs() < 1e-9,
            "{what}: {} against {}",
            replayed.stability,
            card.stability
        );
        assert!((replayed.difficulty - card.difficulty).abs() < 1e-9, "{what}");
        assert_eq!(card.introduced_at, card.reviews.first().map(|review| review.at), "{what}");
    }
}

/// The demo loaded onto a fresh tutor at `now`, and how far it was moved.
async fn loaded(now: DateTime<Utc>) -> (SqlitePool, tempfile::TempDir, Duration) {
    let (pool, data) = tutor().await;
    model::load_demo(&pool, now).await.expect("the demo should load onto an empty tutor");
    let by = Duration::days((now.date_naive() - demo().as_of.date_naive()).num_days());
    (pool, data, by)
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-12T15:00:00Z").unwrap().with_timezone(&Utc)
}

#[tokio::test]
async fn the_demo_loads_onto_an_empty_tutor_and_comes_back_out_the_same() {
    let (pool, _data, by) = loaded(now()).await;
    let out = model::export(&pool, now()).await.unwrap();
    let moved = demo().shifted(by);
    assert_eq!(out.cards, moved.cards);
    assert_eq!(out.anchors, moved.anchors);
    assert_eq!(out.own_words, moved.own_words);
    assert_eq!(out.inventory, moved.inventory);
    assert_eq!(out.voices, moved.voices);
    assert_eq!(out.packs, moved.packs);
    assert_eq!(out.breaks, moved.breaks);

    let document = serde_json::to_value(&out).unwrap();
    assert_eq!(complaints(&document), Vec::<String>::new(), "an export should follow the schema");
}

#[tokio::test]
async fn the_demo_lands_on_today() {
    // Moved by whole days, so its evenings stay evenings and its last one
    // is the evening before.
    let (pool, _data, _) = loaded(now()).await;
    let last: Option<String> = sqlx::query_scalar("SELECT max(reviewed_at) FROM review").fetch_one(&pool).await.unwrap();
    let last = DateTime::parse_from_rfc3339(&last.unwrap()).unwrap().with_timezone(&Utc);
    assert!(
        last < now() && now() - last < Duration::days(1),
        "the last answer should be within the day before, was {last}"
    );
    let today = study::today(&pool, now()).await.unwrap();
    assert!(!today.queue.is_empty(), "the demo's next day should have something waiting");
}

#[tokio::test]
async fn the_demo_will_not_go_over_a_learner() {
    let (pool, _data) = tutor().await;
    let first = inventory::run(&pool, 1).await.unwrap().next.remove(0);
    inventory::answer(&pool, &[inventory::Answer { word: first.word, known: true }], now())
        .await
        .unwrap();
    let refused = model::load_demo(&pool, now()).await.unwrap_err();
    assert!(refused.to_string().contains("already holds a learner"), "{refused:#}");
}

#[tokio::test]
async fn a_snapshot_of_now_counts_what_today_counts() {
    let (pool, _data, _) = loaded(now()).await;
    let snapshot = model::standing_at(&pool, now()).await.unwrap();
    let today = study::today(&pool, now()).await.unwrap();
    assert_eq!(snapshot.progress, today.progress);
    assert_eq!(snapshot.words.counts, today.words.counts);
    assert_eq!(snapshot.words.levels, today.words.levels);
    assert_eq!(snapshot.words.declared, today.words.declared);
}

#[tokio::test]
async fn a_snapshot_of_a_past_day_is_the_tutor_as_it_was_that_day() {
    // The model as of the twentieth evening, loaded into a tutor of its own,
    // has to count what the snapshot says the learner had that day.
    let (pool, _data, by) = loaded(now()).await;
    let then = start() + by + Duration::days(20) - Duration::hours(1);
    let snapshot = model::standing_at(&pool, then).await.unwrap();
    let past = model::as_of(&pool, then, now()).await.unwrap();
    let present = model::export(&pool, now()).await.unwrap();
    assert!(past.cards.len() < present.cards.len(), "the past should hold less than now");
    assert!(past.cards.iter().flat_map(|card| &card.reviews).all(|review| review.at <= then));

    // Each card as it stood then is what its answers by then leave - not the
    // card as it stands now with its later answers cut off.
    let mut moved_since = 0;
    for card in &past.cards {
        let replayed = Scheduler::new().replay(card.reviews.iter().map(|review| (review.rating, review.at))).unwrap();
        assert_eq!(
            (replayed.stitch, replayed.due, replayed.reps),
            (card.state, card.due, card.reps),
            "{} {}",
            card.kind,
            card.subject
        );
        let now_card = present
            .cards
            .iter()
            .find(|other| other.kind == card.kind && other.subject == card.subject)
            .unwrap();
        if now_card.due != card.due {
            moved_since += 1;
        }
    }
    assert!(moved_since > 0, "some card should have been answered since, or the comparison proves nothing");
    assert_eq!(
        complaints(&serde_json::to_value(&past).unwrap()),
        Vec::<String>::new(),
        "a snapshot should follow the schema"
    );

    let (other, _other_data) = tutor().await;
    model::import(&other, &past, then).await.unwrap();
    let today = study::today(&other, then).await.unwrap();
    assert_eq!(snapshot.progress, today.progress);
    assert_eq!(snapshot.words.levels, today.words.levels);
    assert_eq!(snapshot.words.counts, today.words.counts);
    assert!(snapshot.answers > 0);
}

#[tokio::test]
async fn an_import_replaces_the_learner_and_gives_what_it_never_met_a_new_card() {
    let (pool, _data, _) = loaded(now()).await;
    let mut smaller = model::export(&pool, now()).await.unwrap();
    smaller.cards.retain(|card| card.subject == "be-present-statement");
    smaller.anchors.clear();
    smaller.inventory.clear();
    smaller.own_words.truncate(1);
    model::import(&pool, &smaller, now()).await.unwrap();

    let after = model::export(&pool, now()).await.unwrap();
    assert_eq!(after.cards, smaller.cards, "nothing of the learner before should remain");
    assert_eq!(after.own_words.len(), 1);
    assert_eq!(after.inventory, Vec::new());
    let new: i64 = sqlx::query_scalar("SELECT count(*) FROM card WHERE state = 'new'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let pack = shipped_pack();
    let owed = i64::try_from(pack.formulas.len() * 2 + pack.words.len() - smaller.cards.len()).unwrap();
    assert_eq!(new, owed, "every formula and word the model never answered should have a new card");
}

#[tokio::test]
async fn an_imported_moment_compares_the_way_the_tutor_writes_moments() {
    // The queries compare moments as text. A card due on the second, written
    // "...:00Z", sorts after "...:00.5+00:00" by its letter, and would not be
    // due half a second after it is.
    let (pool, _data, _) = loaded(now()).await;
    let mut model = model::export(&pool, now()).await.unwrap();
    let on_the_second = DateTime::parse_from_rfc3339("2026-10-12T14:00:00Z").unwrap().with_timezone(&Utc);
    let card = model
        .cards
        .iter_mut()
        .find(|card| card.kind == "formula")
        .expect("the demo has drilled a formula");
    card.due = on_the_second;
    let id = card.subject.clone();
    model::import(&pool, &model, now()).await.unwrap();

    let today = study::today(&pool, on_the_second + Duration::milliseconds(500)).await.unwrap();
    let due: Vec<&str> = today
        .queue
        .iter()
        .filter_map(|due| match &due.subject {
            Subject::Formula { formula, direction } if *direction == hilvan::scheduling::Direction::Produce => Some(formula.id.as_str()),
            _ => None,
        })
        .collect();
    assert!(due.contains(&id.as_str()), "{id} was due half a second ago and is not in the queue: {due:?}");
}

#[tokio::test]
async fn an_anchor_in_a_sentence_no_pack_shows_is_dropped_not_failed() {
    let (pool, _data, _) = loaded(now()).await;
    let mut model = model::export(&pool, now()).await.unwrap();
    let anchors = model.anchors.len();
    model.anchors[0].sentence.text = "A sentence no pack shows.".to_string();
    let imported = model::import(&pool, &model, now()).await.unwrap();
    assert_eq!(imported.anchors_dropped, 1);
    assert_eq!(model::export(&pool, now()).await.unwrap().anchors.len(), anchors - 1);
}

/// Mutations of a good file, each something the format does not allow.
fn broken() -> Vec<(&'static str, Json)> {
    let good: Json = serde_json::from_str(model::DEMO).unwrap();
    let mut cases = Vec::new();
    let mut case = |what: &'static str, change: &dyn Fn(&mut Json)| {
        let mut document = good.clone();
        change(&mut document);
        cases.push((what, document));
    };
    case("a field the format does not have", &|d| d["streak"] = 3.into());
    case("another format", &|d| d["format"] = 1.into());
    case("a break with no start", &|d| d["breaks"][0] = serde_json::json!({ "until": null }));
    case("a card still new", &|d| d["cards"][0]["state"] = "new".into());
    case("a state FSRS calls it", &|d| d["cards"][0]["state"] = "review".into());
    case("a rating past easy", &|d| d["cards"][0]["reviews"][0]["rating"] = "perfect".into());
    case("an answer from nowhere", &|d| d["cards"][0]["reviews"][0]["source"] = "imported".into());
    case("a card without answers", &|d| d["cards"][0]["reviews"] = Json::Array(Vec::new()));
    case("a moment that is not one", &|d| d["cards"][0]["due"] = "next tuesday".into());
    case("a card of no kind", &|d| d["cards"][0]["kind"] = "sentence".into());
    case("a word in capitals", &|d| d["inventory"][0]["word"] = "en:House".into());
    case("an own word of a third kind", &|d| d["own_words"][0]["kind"] = "adjective".into());
    case("a right turn that failed", &|d| {
        let attempt = d["cards"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .flat_map(|card| card["reviews"].as_array_mut().unwrap().iter_mut())
            .flat_map(|review| review["attempts"].as_array_mut().unwrap().iter_mut())
            .find(|attempt| attempt["correct"] == true)
            .expect("the demo has a right turn");
        attempt["failure"] = "word".into();
    });
    cases
}

#[test]
fn what_the_schema_refuses_import_refuses() {
    for (what, document) in broken() {
        assert!(!complaints(&document).is_empty(), "the schema should refuse {what}");
        assert!(Model::parse(&document.to_string()).is_err(), "import should refuse {what}");
    }
}

#[test]
fn import_refuses_what_a_schema_cannot_say() {
    let mut twice = demo();
    twice.cards.push(twice.cards[0].clone());
    assert!(twice.check().is_err(), "a card listed twice");

    let mut unlisted = demo();
    unlisted.languages.retain(|language| language.native);
    assert!(unlisted.check().is_err(), "a word of a language the file does not list");

    let mut backwards = demo();
    backwards.breaks[0].until = Some(backwards.breaks[0].from - Duration::hours(1));
    assert!(backwards.check().is_err(), "a break that ends before it starts");

    let mut overlapping = demo();
    // Inside the first, where there is no answer to hold: only the overlap
    // is wrong with it.
    let first = overlapping.breaks[0].from;
    overlapping.breaks[1] = model::BreakRecord {
        from: first + Duration::hours(1),
        until: Some(first + Duration::hours(2)),
    };
    assert!(overlapping.check().is_err(), "two breaks at once");

    let mut endless = demo();
    endless.breaks[0].until = None;
    assert!(endless.check().is_err(), "a break with no end and another after it");

    let mut holding = demo();
    let answer = holding
        .cards
        .iter()
        .flat_map(|card| &card.reviews)
        .find(|review| review.source == Source::Asked)
        .unwrap()
        .at;
    holding.breaks = vec![model::BreakRecord {
        from: answer - Duration::hours(1),
        until: Some(answer + Duration::hours(1)),
    }];
    assert!(holding.check().is_err(), "a break holding an answer");
}

#[test]
fn a_model_moved_in_time_keeps_its_gaps() {
    let demo = demo();
    let moved = demo.clone().shifted(Duration::days(300));
    assert_eq!(moved.as_of - demo.as_of, Duration::days(300));
    for (before, after) in demo.cards.iter().zip(&moved.cards) {
        assert_eq!(after.due - before.due, Duration::days(300));
        for (before, after) in before.reviews.iter().zip(&after.reviews) {
            assert_eq!(after.at - before.at, Duration::days(300));
        }
    }
    assert!(
        moved
            .inventory
            .iter()
            .zip(&demo.inventory)
            .all(|(after, before)| after.at - before.at == Duration::days(300))
            && moved
                .own_words
                .iter()
                .zip(&demo.own_words)
                .all(|(after, before)| after.created_at - before.created_at == Duration::days(300))
            && moved.breaks.iter().zip(&demo.breaks).all(|(after, before)| {
                after.from - before.from == Duration::days(300) && after.until.zip(before.until).is_none_or(|(a, b)| a - b == Duration::days(300))
            })
    );
}

#[test]
fn the_demo_is_a_file_of_its_own_not_of_this_machine() {
    // Synthetic through and through: nothing of a real stand.
    let text = model::DEMO;
    for private in ["C:\\", "/home/", "/Users/", ".pi", "192.168."] {
        assert!(!text.contains(private), "the demo holds {private:?}");
    }
}

/// An answer as the demo learner's phone would send it, given the moment it
/// is taken: one learner, one answer at a time, so the moment is its identity.
fn sent<T>(answer: T, at: DateTime<Utc>) -> study::Sent<T> {
    study::Sent {
        answer_id: format!("demo-{}", at.timestamp_millis()),
        answered_at: at,
        answer,
    }
}
