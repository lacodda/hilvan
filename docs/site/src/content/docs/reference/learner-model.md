---
title: The learner model file
description: Everything hilvan knows about its learner, as one JSON file - what is in it, the published schema, export, import, snapshots of past days and the demo learner.
---

Everything the tutor knows about its learner fits in one JSON file: every card that has been answered with every answer behind it and the turns typed for it, the learner's own words, the voices they chose and their answers in the [inventory](/hilvan/guides/your-first-week/#what-you-already-know). It moves a learner to another stand, keeps a copy that does not need hilvan to read, and tells the model as it stood on any past day.

The material is not in it. Packs are files of their own and the lexicon is built into the binary; the model names the packs it was learnt on and keys everything on what outlives a reload of a pack - a formula's id, a word's lemma, a sentence's text.

## The schema

The format is published as a JSON Schema (draft 2020-12):

**[`https://lacodda.github.io/hilvan/schema/learner-model-1.json`](/hilvan/schema/learner-model-1.json)**

Every file names it in `$schema`, and every file hilvan writes follows it - the test suite checks each export against it. What the schema refuses, `hilvan import` refuses too; import also checks what a schema cannot say: nothing listed twice, and every language the file names is one it lists. A change that would make a reader of this format misread a file is a new format, with a schema of its own.

## What is in it

```json
{
  "$schema": "https://lacodda.github.io/hilvan/schema/learner-model-1.json",
  "format": 1,
  "exported_at": "2026-10-12T15:00:00Z",
  "as_of": "2026-10-12T15:00:00Z",
  "app": { "name": "hilvan", "version": "0.6.0" },
  "languages": [{ "code": "ru", "native": true }, { "code": "en", "native": false }],
  "packs": [{ "id": "en-from-ru", "version": 5 }],
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
  "inventory": [{ "word": "en:house", "known": true, "at": "2026-09-18T18:52:04Z" }]
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

A card's `state`, `stability`, `difficulty`, `due`, `reps` and `lapses` are what its reviews leave when played through FSRS in order; they are in the file so a reader does not have to replay to know them.

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

An import **replaces** the learner model: every card, answer, anchor, own word, voice and inventory answer goes, and the file's take their place, in one transaction. Before that, the model being replaced is written to a file beside the database - `hilvan-model-before-import-<moment>.json`, in the data volume on a stand - so an import is never the end of anything. Every formula and word of the loaded packs that the file never answered gets a new card.

What does not fit is said rather than lost: an anchor whose sentence no loaded pack shows is dropped (the word is heard in the first sentence it is met in until it is answered again), and a pack the model was learnt on that is not loaded here is named - its cards are kept and wait for it.

## Snapshots

The model as it stood at a moment is not stored anywhere: it is the review log up to that moment, played through the scheduler again. Any moment since the first answer can be asked about, by `hilvan export --at`, by `GET /api/model?at=`, and on the **Then and now** screen, which sets a day of your choosing beside today. A snapshot carries the voices chosen now - a choice keeps no date.

## The demo learner

```sh
hilvan load-pack packs/en-from-ru/pack.toml
hilvan load-demo
```

A fictional learner, three weeks into English from Russian on the shipped pack: an inventory of the commonest thousand words on the first evening and three hundred more later, a sitting most evenings, typed turns with the slips a learner makes, four words of their own. It is what the screenshots in these docs show, and a quick way to look around a new stand. It is moved in time when loaded, so its last evening of study is yesterday's.

`load-demo` refuses a tutor that already holds a learner - an answer, an own word, a chosen voice or an inventory answer: a demo is never what is meant over someone's own history. Give it a database of its own (`HILVAN_DATABASE_URL`).

The demo is not written by hand: it is three weeks of the tutor's own code, driven by a test (`cargo test --test demo -- --ignored`), and every release checks that it still follows the schema, still learns from the pack that ships, and that every card in it is what its answers leave.
