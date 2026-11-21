---
title: Audio lessons
description: How a lesson is made from the queue, what it sounds like, the lesson endpoints, answers by ear, and what a lesson costs.
---

An audio lesson is the day's queue said aloud, the way Pimsleur's courses are: a question in the learner's own language, a pause to answer aloud in, the answer in a native voice, room to say it after them, and the phrase once more. Every phrase comes back inside the lesson at growing intervals. The learner takes the file on a walk and says how it went afterwards; what they heard moves the schedule like any answer. For how it is used, see [your first week](/hilvan/guides/your-first-week/#by-ear).

## What a lesson is made of

The phrases come from [`GET /api/today`](/hilvan/reference/api/#get-apitoday)'s queue in its own order - what is due, then what is new - and, for a lesson longer than the queue, from the cards that come back over the next seven days, the soonest first. On a break there is nothing to hear.

| Card | Heard as | Asks |
| --- | --- | --- |
| A formula, saying it | one of its worked examples, another each day | the meaning, then the phrase |
| A formula, understanding it | one of its worked examples | the phrase, then the meaning |
| A word | its anchor sentence with the sentence's translation; a word from a text, which has none, alone with its gloss | the phrase, then the meaning |
| A pair | not at all: choosing a word for a gap is done with the eyes | - |

A word with neither a translated sentence nor a gloss stays on its card. A lesson is in one pair of languages - the one the queue starts with.

## What it sounds like

One asking of a phrase:

1. the question - the meaning in the teacher's voice, or the phrase in the native one;
2. a pause to answer in: three seconds plus the length of the phrase, never less than four or more than six;
3. the answer;
4. room to say the phrase after the native voice: its length plus a second, from one and a half to six;
5. the phrase once more, in the native voice.

A phrase the learner has never met is **heard** before it is asked: a new formula's explanation, the meaning, the phrase slowly with room to repeat it, the phrase at speed with room again. It is then asked about 20 seconds, 2 minutes and 8 minutes on. A phrase already met is asked at once and again 5 minutes on. Between phrases there is a second and a half of silence.

A phrase comes into the lesson only if all of it - every time it will be heard - still fits the length; one that does not is left to the cards, and a later, shorter one may take its room. A recall that falls due is asked as soon as the phrase before it ends; when no new phrase is left to come in, the recall due soonest is asked early rather than leaving silence. The plan is one pure function in the core crate (`hilvan_core::lesson`), held by tests that a mutation of the intervals, the pauses or the length breaks.

## How it is made

1. The phrases are planned on what the [audio cache](/hilvan/reference/voice/#the-audio-cache) already knows of their lines and a guess for the rest.
2. The lines the plan needs are priced: a line the cache does not have, in a language ElevenLabs speaks, costs what the plan charges for its characters. A lesson that costs more than the [budget](/hilvan/reference/voice/#the-elevenlabs-budget) has left is not started, and says so with both numbers; one that costs anything at all is not started while the account cannot be asked.
3. Each line is said through the cache - the learner's own language by Piper, the language being learnt by its voice - and decoded by ffmpeg to 16-bit mono at 24 kHz, cut to whole milliseconds.
4. The plan is made again on the real lengths, from the phrases the first one chose.
5. The silences are laid between the lines sample by sample, and ffmpeg encodes the whole once: MP3, 48 kbit/s mono, with an ID3 chapter for every time a phrase is heard or asked, titled by the phrase.

The file is kept in the database, like every sound the tutor makes; the three latest lessons are kept. A lesson that was being made when the server stopped is marked failed at the next start. ffmpeg is the one on the path, or `HILVAN_FFMPEG` ([configuration](/hilvan/reference/configuration/)); the image ships Debian's.

## `GET /api/lessons`

What the lesson screen opens on.

```json
{
  "minutes": 25,
  "lengths": [15, 25, 40],
  "lessons": [
    {
      "id": 3,
      "asked_at": "2026-11-21T18:02:11Z",
      "minutes": 25,
      "state": "ready",
      "error": null,
      "done": 48,
      "total": 48,
      "built_at": "2026-11-21T18:03:40Z",
      "length_ms": 1476200,
      "credits": 412,
      "phrases": [
        {
          "card": { "kind": "formula", "id": "be-present-statement", "direction": "produce" },
          "native": "<the meaning, in the learner's language>",
          "target": "I am at home.",
          "asks": "produce",
          "fresh": false
        }
      ],
      "chapters": [
        { "phrase": 0, "turn": "asked", "start_ms": 0, "end_ms": 11800 }
      ]
    }
  ]
}
```

`minutes` is the length the learner last asked for, 25 until they have. `lessons` are the lessons kept, the latest first. A lesson's `state` is `building`, `ready` or `failed`; while it is `building`, `done` of `total` lines have been said; a failed one carries `error` in words the learner can act on. `credits` is what its new lines cost. `phrases` are in the order first heard, and each chapter names its phrase by its place there; `turn` is `heard` for the first hearing of a fresh phrase and `asked` for every asking. Both lists are empty until the lesson is ready.

## `POST /api/lessons`

Asks for a lesson, and keeps its length as the one the learner likes.

```json
{ "minutes": 25 }
```

**Response** `202` with the lesson as it starts, `state: "building"`; the app follows it with `GET /api/lessons/{id}`. A stand makes one lesson at a time: asking while one is being made answers with that one. **Response** `422` for a length other than 15, 25 or 40.

## `GET /api/lessons/{id}`

One lesson, in the shape above. `404` when there is no such lesson.

## `GET /api/lessons/{id}/audio`

The lesson's file, `audio/mpeg`. It never changes once made: `Cache-Control` is `private, max-age=31536000, immutable`. The app downloads it whole and plays it from the device. `404` while the lesson is not ready, or once it has been let go.

## `POST /api/heard`

A phrase heard in a lesson, graded by the learner afterwards - held or slipped. It travels the device's line like every answer ([answers from the device](/hilvan/reference/api/#answers-from-the-device)), so a lesson graded in the park arrives at home.

```json
{
  "answer_id": "9b2e4f10-7c3a-4d55-a1e8-2f6b0c9d8e71",
  "answered_at": "2026-11-22T08:14:05.000Z",
  "card": { "kind": "formula", "id": "be-present-statement", "direction": "produce" },
  "rating": "good"
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `card` | object | yes | The card the phrase answers: `{ "kind": "formula", "id", "direction" }` or `{ "kind": "word", "id" }`, as the lesson's phrase names it. |
| `rating` | string | yes | `again`, `hard`, `good` or `easy`. The app sends `good` for held and `again` for slipped. |

`answered_at` is when the phrase was heard, by the device's clock. The answer moves the card through FSRS as a graded card does, with no time and nothing typed, and the review is marked `heard` in the [learner model](/hilvan/reference/learner-model/). A word heard for the first time is anchored in the sentence the lesson said it in.

**Response** `200`: the same shape as a formula's review. **Response** `404` when the card is not there.
