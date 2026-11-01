---
title: The learner model file
description: Everything hilvan knows about its learner, as one JSON file - what is in it, the published schema, export, import, snapshots of past days and the demo learner.
---

Everything the tutor knows about its learner fits in one JSON file: every card that has been answered with every answer behind it and the turns typed for it, the learner's own words, the voices they chose, their answers in the [inventory](/hilvan/guides/your-first-week/#what-you-already-know), the [breaks](/hilvan/guides/your-first-week/#a-break) they took, the [texts](/hilvan/guides/reading/) they read and the words they took from them. It moves a learner to another stand, keeps a copy that does not need hilvan to read, and tells the model as it stood on any past day.

The material is not in it. Packs are files of their own and the lexicon is built into the binary; the model names the packs it was learnt on and keys everything on what outlives a reload of a pack - a formula's id, a word's lemma, a sentence's text.

## The schema

The format is published as a JSON Schema (draft 2020-12):

**[`https://lacodda.github.io/hilvan/schema/learner-model-4.json`](/hilvan/schema/learner-model-4.json)**

Every file names it in `$schema`, and every file hilvan writes follows it - the test suite checks each export against it. What the schema refuses, `hilvan import` refuses too; import also checks what a schema cannot say: nothing listed twice, and every language the file names is one it lists. A change that would make a reader of this format misread a file is a new format, with a schema of its own.

Format 4 (from v0.10.0) keeps a text in `chapters` and gives it an `author` and a `source`; format 3 (from v0.9.0) added `texts` and `picks`; format 2 (from v0.7.0) added `breaks`. hilvan reads only the format it writes, so a file of an earlier format is refused; their schemas stay published at [`learner-model-3.json`](/hilvan/schema/learner-model-3.json), [`learner-model-2.json`](/hilvan/schema/learner-model-2.json) and [`learner-model-1.json`](/hilvan/schema/learner-model-1.json) for anything that reads such files.

## What is in it

```json
{
  "$schema": "https://lacodda.github.io/hilvan/schema/learner-model-4.json",
  "format": 4,
  "exported_at": "2026-10-12T15:00:00Z",
  "as_of": "2026-10-12T15:00:00Z",
  "app": { "name": "hilvan", "version": "0.10.0" },
  "languages": [{ "code": "ru", "native": true }, { "code": "en", "native": false }],
  "packs": [{ "id": "en-from-ru", "version": 6 }],
  "cards": [
    {
      "kind": "formula", "subject": "be-present-statement", "state": "sewn",
      "stability": 15.47, "difficulty": 3.22, "due": "2026-10-20T19:02:11Z",
      "reps": 4, "lapses": 0, "last_reviewed": "2026-10-05T19:02:11Z", "introduced_at": "2026-09-18T19:01:40Z",
      "reviews": [
        {
          "at": "2026-09-18T19:01:40Z", "rating": "good", "source": "asked",
          "elapsed_days": 0, "stability": 3.13, "difficulty": 5.31, "duration_ms": 6200,
          "attempts": [
            { "prompt": "<the prompt, in the learner's language>", "expected": "I am at home.", "typed": "I am at hom",
              "correct": false, "failure": "word", "missed": ["home"] }
          ]
        }
      ]
    }
  ],
  "anchors": [{ "word": "en:home", "sentence": { "language": "en", "text": "I am at home." } }],
  "own_words": [{ "language": "en", "kind": "verb", "native": "<its meaning>", "target": "cook", "created_at": "2026-09-20T19:10:00Z" }],
  "voices": [],
  "inventory": [{ "word": "en:house", "known": true, "at": "2026-09-18T18:52:04Z" }],
  "breaks": [{ "from": "2026-09-25T19:30:00Z", "until": "2026-09-29T07:00:00Z" }],
  "texts": [
    {
      "language": "en", "title": "A morning at the market", "author": null, "source": null, "format": "markdown",
      "added_at": "2026-10-20T18:41:00Z", "opened_at": "2026-10-20T18:45:55Z", "paragraph": 2, "finished_at": null,
      "chapters": [
        { "title": null, "paragraphs": [["Every Saturday my sister and I walk to the market near the river.", "It opens early."], ["Last week it rained."]] }
      ]
    },
    {
      "language": "en", "title": "Alice's Adventures in Wonderland", "author": "Lewis Carroll",
      "source": "https://www.gutenberg.org/ebooks/11", "format": "epub",
      "added_at": "2026-10-28T19:00:00Z", "opened_at": "2026-10-28T19:02:00Z", "paragraph": 40, "finished_at": null,
      "chapters": [
        { "title": "CHAPTER I. Down the Rabbit-Hole", "paragraphs": [["Alice was beginning to get very tired of sitting by her sister on the bank."]] },
        { "title": "CHAPTER II. The Pool of Tears", "paragraphs": [["“Curiouser and curiouser!” cried Alice."]] }
      ]
    }
  ],
  "picks": [
    { "word": "en:rain", "sentence": { "language": "en", "text": "Last week it rained." }, "form": "rained", "start": 13, "at": "2026-10-20T18:42:05Z" }
  ]
}
```

| Field | What it is |
| --- | --- |
| `as_of` | The moment the model is told as of: when it was written, or an earlier one for a snapshot. |
| `packs` | The packs the learner was learning from. Another version of a pack reads fine - cards key on ids. |
| `cards` | Every card answered. `kind` is `formula` (saying it), `formula-recognise` (understanding it) or `word`; a word's `subject` is `language:lemma`, lowercase. A card never answered is not listed: it is new, and a loaded pack makes those. |
| `reviews` | Every answer, oldest first. `source` is `asked` - the tutor asked and the learner answered - or `declared`, a yes in the inventory, which moves the card like an *easy* and is not counted as an answer. |
| `attempts` | The turns typed for a said formula: what was asked, the answer it was read against, what was typed and what failed - `formula`, `order` or `word`. |
| `anchors` | The sentence each word is heard in, by its text. |
| `inventory` | Every answer of the inventory, yes and no. |
| `texts` | The texts the learner reads, whole: chapters of paragraphs of sentences - a book's chapters as its source names them, one chapter for anything else - who wrote it and the link it came from where known, how it came in, where the learner stopped (the paragraph through the whole text) and whether they said they had read it. |
| `picks` | The words taken into the deck from a sentence of a text, by the sentence's text: the sentence is one the word was met in, and it outlives the text. |
| `breaks` | The breaks taken, earliest first: the stretches the schedule's clock stood still. `until` is `null` for one still going on. They do not overlap, only the last may lack an end, and none holds an asked answer - one may start at the moment of the last answer, which is how an absence is counted afterwards. |

A card's `state`, `stability`, `difficulty`, `due`, `reps` and `lapses` are what its reviews leave when played through FSRS in order; they are in the file so a reader does not have to replay to know them. `due` is FSRS's own: when a card actually comes back is that, moved by the breaks its interval spans.

## Export

```sh
hilvan export --out me.json             # the model as it stands
hilvan export --at 2026-03-01 > march.json   # as it stood at the end of 1 March, UTC
```

`--at` takes a day (its last moment, UTC) or an RFC 3339 date and time. Without `--out` the model goes to standard output.

In the app, **Then and now** on Today has the same two downloads. Over HTTP it is [`GET /api/model`](/hilvan/reference/api/#get-apimodel).

## Import

```sh
hilvan load-pack packs/en-from-ru/pack.toml   # the material first
hilvan import me.json
```

An import **replaces** the learner model: every card, answer, anchor, own word, voice, inventory answer, break, text and word taken from one goes, and the file's take their place, in one transaction. Before that, the model being replaced is written to a file beside the database - `hilvan-model-before-import-<moment>.json`, in the data volume on a stand - so an import is never the end of anything. Every formula and word of the loaded packs that the file never answered gets a new card.

What does not fit is said rather than lost: an anchor whose sentence nothing shows - no loaded pack, no text, no word taken from it - is dropped (the word is heard in the first sentence it is met in until it is answered again), and a pack the model was learnt on that is not loaded here is named - its cards are kept and wait for it.

## Snapshots

The model as it stood at a moment is not stored anywhere: it is the review log up to that moment, played through the scheduler again. Any moment since the first answer can be asked about, by `hilvan export --at`, by `GET /api/model?at=`, and on the **Then and now** screen, which sets a day of your choosing beside today. A snapshot carries the voices chosen now - a choice keeps no date - and the breaks begun by then, one that had not ended by then as still going on; the texts added by then, at the place the learner is in each now, which keeps no history either; and the words taken from them by then.

## The demo learner

```sh
hilvan load-pack packs/en-from-ru/pack.toml
hilvan load-demo
```

A fictional learner, three weeks into English from Russian on the shipped pack: an inventory of the commonest thousand words on the first evening and three hundred more later, a sitting most evenings, typed turns with the slips a learner makes, four words of their own, a long weekend away taken as a break, three missed evenings counted as one afterwards, a text read with the rest of its first paragraph swept as known and three words taken from it, and the subtitles of an episode not opened yet. It is what the screenshots in these docs show, and a quick way to look around a new stand. It is moved in time when loaded, so its last evening of study is yesterday's.

`load-demo` refuses a tutor that already holds a learner - an answer, an own word, a chosen voice, an inventory answer, a break or a text: a demo is never what is meant over someone's own history. Give it a database of its own (`HILVAN_DATABASE_URL`).

The demo is not written by hand: it is three weeks of the tutor's own code, driven by a test (`cargo test --test demo -- --ignored`), and every release checks that it still follows the schema, still learns from the pack that ships, and that every card in it is what its answers leave.
