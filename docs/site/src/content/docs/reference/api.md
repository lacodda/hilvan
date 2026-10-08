---
title: HTTP API
description: Every endpoint under /api - method, path, session requirement, request and response shapes, status codes.
---

All endpoints are under `/api` and speak JSON. A path outside `/api` that does not match a built file falls through to the tutor app; a path under `/api` that does not match a route answers `404` with `{"error":"no such endpoint"}` - see [Unknown endpoints](/hilvan/reference/health/#unknown-endpoints).

## Sessions

Two endpoints - `/api/health` and `/api/session` - are reachable with no session. Every study endpoint (`/api/today`, `/api/formulas/{id}` with its `/check` and `/review`, `/api/words/{id}` and its `/review`, `/api/own-words`) sits behind [`HILVAN_PASSWORD_HASH`](/hilvan/reference/configuration/#the-password): when it is set, a request with no valid session cookie gets `401`.

```json
{ "error": "sign in first" }
```

The session cookie is `hilvan_session`, `HttpOnly`, `SameSite=Lax`, and lasts 90 days. It is not marked `Secure`, because the stand is reached over plain HTTP on a home network. Sessions are rows in the database rather than signed tokens, so they survive a restart of the server and a sign-out ends one for good; every request refreshes the 90 days.

### `GET /api/health`

No session required.

Liveness and readiness in one place. See [Health endpoint](/hilvan/reference/health/) for the full contract.

**Response** `200` or `503`:

```json
{ "status": "ok", "version": "0.0.0" }
```

### `GET /api/session`

No session required.

Whether the door is locked, and whether this client is through it.

**Response** `200`:

```json
{ "required": true, "signed_in": false }
```

| Field | Type | Meaning |
| --- | --- | --- |
| `required` | boolean | Whether `HILVAN_PASSWORD_HASH` is set. |
| `signed_in` | boolean | Whether the request's own session cookie is valid. |

### `POST /api/session`

No session required - this is how one is obtained.

**Request:**

```json
{ "password": "correct horse battery staple" }
```

**Response** `200` on success, sets the `hilvan_session` cookie:

```json
{ "required": true, "signed_in": true }
```

When no password is configured, the endpoint answers the same shape (`required: false, signed_in: true`) without checking the body and without setting a cookie - there is nothing to sign into.

**Response** `401` on a wrong password:

```json
{ "error": "that is not the password" }
```

A wrong password and an unknown one get the same answer; no cookie is set either way.

### `DELETE /api/session`

No session required to call it, though it only does something when a session cookie is present.

Forgets the session server-side and clears the cookie.

**Response** `200`:

```json
{ "required": true, "signed_in": false }
```

## Study

Everything below requires a valid session when `HILVAN_PASSWORD_HASH` is set.

### `GET /api/today`

Today's queue: formulas, then words. In each, everything due for review comes first, then the new ones the day has room for - at most one formula and ten words. Reviews are never crowded out by new material.

**Response** `200`:

```json
{
  "queue": [
    {
      "kind": "formula",
      "formula": {
        "id": "be-present-statement",
        "name": "I am / you are",
        "pattern": "<pronoun> + am/is/are + <rest>",
        "explanation": "...",
        "samples": [{ "native": "<prompt in the learner's own language>", "target": "I am at home." }],
        "slots": [
          {
            "name": "pronoun",
            "values": [{ "native": "<word in the learner's own language>", "target": "I" }]
          }
        ],
        "family": "be-present",
        "form": "statement",
        "sisters": [
          { "id": "be-present-negation", "form": "negation", "name": "...", "pattern": "...", "stitch": "basted" }
        ]
      },
      "direction": "produce",
      "stitch": "basted",
      "is_new": false,
      "due": "2026-09-03T09:00:00Z",
      "pace": { "typical_ms": 3800, "last_ms": 9100, "answers": 6 }
    },
    {
      "kind": "word",
      "word": { "id": "en:home", "lemma": "home", "gloss": "...", "ipa": "hoʊm", "rank": 150, "level": 1000, "contexts": ["..."] },
      "stitch": "new",
      "is_new": true,
      "due": "2026-09-03T09:00:00Z",
      "pace": null
    }
  ],
  "reviewed_today": 1,
  "counts": { "new": 24, "basted": 3, "sewn": 2 },
  "progress": {
    "produce": { "new": 24, "basted": 3, "sewn": 2 },
    "recognise": { "new": 27, "basted": 2, "sewn": 0 }
  },
  "words": {
    "counts": { "new": 120, "basted": 5, "sewn": 1 },
    "levels": [
      { "size": 1000, "basted": 5, "sewn": 1 },
      { "size": 2000, "basted": 5, "sewn": 1 },
      { "size": 5000, "basted": 5, "sewn": 1 }
    ],
    "waiting": 3
  },
  "next": {
    "id": "be-present-negation",
    "name": "...",
    "waits_for": [{ "id": "be-present-statement", "name": "...", "stitch": "basted" }]
  }
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `queue` | array | Formulas due, then at most one new one; words due, then the new ones the day has room for. Empty once nothing is due and nothing new can open. |
| `queue[].kind` | `"formula"` \| `"word"` | What the item asks. A formula item carries `formula` and `direction`, a word item `word`. |
| `queue[].formula` | object | The full formula - see `GET /api/formulas/{id}` below. |
| `queue[].direction` | `"produce"` \| `"recognise"` | Which way round this item asks the formula. |
| `queue[].word` | object | The full word - see `GET /api/words/{id}` below. |
| `queue[].stitch` | `"new"` \| `"basted"` \| `"sewn"` | Where this item stands right now - a formula **in this direction**. |
| `queue[].is_new` | boolean | Whether this has never been answered - a formula in this direction, or a word. |
| `queue[].due` | string (RFC 3339) | When this item became due; for a new item, the time of the request. |
| `queue[].pace` | object \| null | How fast this card usually comes; `null` until three timed answers. |
| `reviewed_today` | integer | Answers recorded since midnight UTC, formulas both ways and words. |
| `counts.new` \| `counts.basted` \| `counts.sewn` | integer | How many formulas stand in each state, producing side. |
| `progress.produce` \| `progress.recognise` | object | The same counts, one per direction. |
| `words.counts` | object | Every word of the loaded packs, by state. |
| `words.levels[]` | array | The [levels](/hilvan/reference/lexicon/#levels): `size` lemmas of the language, and how many of the learner's words in it are basted and sewn. Cumulative. |
| `words.waiting` | integer | Words met in a sentence, not started, and not in today's queue: what later days will open. |
| `next` | object \| null | The first formula of the pack's sequence that has not started and is not opening today; `null` once every formula has. |
| `next.waits_for` | array | Its [parents](/hilvan/reference/packs/#parents) not yet sewn, each with its `stitch`. Empty when it waits only for a day with room for a new formula. |

#### Parents

A formula opens only once every parent named in the pack is sewn on its producing side: the new formula of the day is the first in the pack's order whose parents all are. A formula already started is never held back again.

#### Words in the queue

A word opens only once a sentence that holds it has been met - once the formula that sentence belongs to has been answered. Of the words that have, the commonest open first, at most ten a day; a word started earlier the same day uses up a place.

#### Directions

Producing a sentence and understanding one are different skills learnt at different speeds - recognition always runs ahead - so each formula is scheduled twice, once per direction, and the two never share a state. See [ADR 0005](https://github.com/lacodda/hilvan/blob/main/docs/adr/0005-forms-directions-and-pace.md).

A formula's recognising side opens only once the formula has been produced at least once: being asked to understand a shape nobody has taught you to build is a guess, not a review. It never counts against the day's budget of one new formula.

#### Pace

`pace` is the second dimension of knowing something: stability says whether the formula is still there, pace says whether it still costs thought.

| Field | Type | Meaning |
| --- | --- | --- |
| `typical_ms` | integer | Median of the last eight timed answers. |
| `last_ms` | integer | The most recent timed answer. |
| `answers` | integer | How many timed answers the median rests on - at least three. |

It is **reported and never scheduled on**: the same answer given in 0.9s and in 45s produces the same next due date.

### `GET /api/formulas/{id}`

One formula with its samples and slots.

**Response** `200`:

```json
{
  "id": "be-present-statement",
  "name": "I am / you are",
  "pattern": "<pronoun> + am/is/are + <rest>",
  "say": "<pronoun> <pronoun:be> <rest>.",
  "explanation": "...",
  "samples": [
    {
      "sentence": 1,
      "native": "<prompt in the learner's own language>",
      "target": "I am at home.",
      "words": [{ "word": "en:home", "lemma": "home", "form": "home", "start": 8 }],
      "accept": ["I'm at home."]
    },
    {
      "sentence": 2,
      "native": "<prompt in the learner's own language>",
      "target": "He is a doctor.",
      "words": [{ "word": "en:doctor", "lemma": "doctor", "form": "doctor", "start": 8 }],
      "accept": ["He's a doctor."]
    }
  ],
  "slots": [
    {
      "name": "pronoun",
      "takes": null,
      "values": [
        { "native": "<word in the learner's own language>", "target": "I", "forms": { "be": "am", "short": "I'm" }, "own": false },
        { "native": "<word in the learner's own language>", "target": "you", "forms": { "be": "are", "short": "you're" }, "own": false }
      ]
    },
    {
      "name": "rest",
      "takes": null,
      "values": [{ "native": "<word in the learner's own language>", "target": "at home", "forms": {}, "own": false }]
    }
  ],
  "family": "be-present",
  "form": "statement",
  "accept": ["<pronoun:short> <rest>."],
  "parents": [],
  "languages": { "native": "ru", "target": "en" },
  "sisters": [
    { "id": "be-present-negation", "form": "negation", "name": "...", "pattern": "...", "stitch": "basted" },
    { "id": "be-present-question", "form": "question", "name": "...", "pattern": "...", "stitch": "new" }
  ]
}
```

`explanation` is in the learner's native language - the pack carries it, the server does not translate. `pattern` holds `<slot-name>` placeholders matching each entry in `slots` and is the scaffold shown on the card. `say` is the sentence a substitution answers with - `<slot>` is the value's `target`, `<slot:form>` one of its `forms`, and the first letter is raised; `null` for a formula without slots. See [the pack format](/hilvan/reference/packs/#the-sentence-it-says).

`languages` names the language of each side, ISO 639-1: what the app asks [the voice](/hilvan/reference/voice/) for.

`accept` lists more templates in the syntax of `say`, each another right answer to a substitution, and a sample's `accept` lists other spellings of its sentence - see [Answers it accepts](/hilvan/reference/packs/#answers-it-accepts). `parents` are the formulas this one stands on, each with `id`, `name` and `stitch`, in the pack's order.

A slot's `takes` is `"noun"`, `"verb"` or `null`: the kind of the learner's [own words](#own-words) it takes. They stand among its `values` with `own: true`, after the pack's, and an own word spelt like a pack value is that value.

Each sample is a sentence: `sentence` is its id, the same for every formula that shows it, and `words` marks the [words](/hilvan/reference/packs/#words) it teaches - `form` as spelt in `target`, `start` the character it starts at, counted in Unicode code points.

#### Forms of a shape

| Field | Type | Meaning |
| --- | --- | --- |
| `family` | string \| null | The shape this formula is one form of, when it is one of several. |
| `form` | `"statement"` \| `"negation"` \| `"question"` \| null | Which form of that shape this is. |
| `sisters` | array | The other forms of the same shape, in the pack's own order. Empty when the formula stands alone. |
| `sisters[].stitch` | `"new"` \| `"basted"` \| `"sewn"` | Where that form stands, producing side - so the switch can show an untouched question beside a sewn statement. |

The three forms stay three formulas with three schedules, because they are learnt apart: the negation with `don't` really is a separate thing to remember. `family` is what lets the card put them behind one switch. A formula with no sisters - `let-s-verb`, `how-much-many` - carries `null` in both fields.

**Response** `404` when `{id}` names no formula:

```json
{ "error": "there is no formula called nonsense" }
```

### `POST /api/formulas/{id}/check`

Checks one typed turn against the formula's answer and the answers it accepts, without recording anything: what the drill shows the moment the learner submits.

**Request** - a turn: what was asked, and what was typed.

```json
{ "asked": { "values": { "pronoun": "they", "rest": "ready" } }, "typed": "They is redy" }
```

`asked` names the question by what it was made of: `{ "values": { slot: target } }` for a substitution - the `target` of the value put in each slot, every slot named once - or `{ "sample": <sentence id> }` for one of the formula's own sentences.

**Response** `200`:

```json
{
  "correct": false,
  "answer": "They are ready.",
  "closest": "They are ready.",
  "expected": [
    { "text": "They", "word": false, "came": true },
    { "text": "are", "word": false, "came": false },
    { "text": "ready", "word": true, "came": false }
  ],
  "extra": ["is", "redy"],
  "failure": "formula",
  "missed": ["ready"]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `correct` | boolean | Whether the typed text is one of the right answers, word for word. Case, punctuation and the kind of apostrophe are ignored; spelling is not. |
| `answer` | string | The formula's own answer, as it is heard. |
| `closest` | string | The right answer closest to what was typed - an accepted variant when that is what the learner reached for. |
| `expected[]` | array | The words of `closest`: `word` when it is a word the formula carries rather than its own, `came` when the typed answer had it. |
| `extra` | array | Words typed that `closest` does not have. |
| `failure` | `"formula"` \| `"order"` \| `"word"` \| null | What went wrong: a piece of the shape missing (or something added), every word there in the wrong order, or only carried words missing. `null` when right. |
| `missed` | array | Carried words that did not come, as they stand in `closest`. |

**Response** `404` when `{id}` names no formula; `422` when `asked` names a slot, value or sentence the formula does not have.

### `POST /api/formulas/{id}/review`

Records an answer and reschedules the formula through FSRS. How depends on the direction.

**Saying it** - the typed turns are the answer, and the server grades them:

```json
{
  "direction": "produce",
  "turns": [
    { "asked": { "sample": 1 }, "typed": "I'm at home" },
    { "asked": { "values": { "pronoun": "they", "rest": "ready" } }, "typed": "They is redy" }
  ],
  "easy": false,
  "duration_ms": 41000
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `turns` | array | yes, at least one | Every turn typed, in order, in the shape `/check` takes. Each is checked again here; nothing the client concluded decides the grade. |
| `easy` | boolean | no | The learner's word that it came without thinking. Honoured only when every turn was right. |
| `duration_ms` | integer | no | How long the answer took. Kept and reported back as `pace`; never used to schedule. |

The grade follows the slips of the shape - a `formula` or `order` failure: in half the turns or more, `again`; in at least one, `hard`; in none, `good`, or `easy` on the learner's word for a flawless round. A turn that failed only on words leaves the formula's grade alone and records `again` on the card of each missed word the pack teaches, opening it if it was new. Every turn is kept with the review.

**Understanding it** - heard, and graded by the learner:

```json
{ "direction": "recognise", "rating": "good", "duration_ms": 4200 }
```

`rating` is one of `"again"`, `"hard"`, `"good"`, `"easy"`.

Only the card in the direction named is graded: answering `"recognise"` leaves the producing schedule of the same formula exactly as it was.

**Response** `200`:

```json
{
  "stitch": "basted",
  "due": "2026-09-04T09:00:00Z",
  "interval_days": 1,
  "pace": { "typical_ms": 3800, "last_ms": 4200, "answers": 6 },
  "rating": "hard",
  "words": ["en:ready"]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `stitch` | `"new"` \| `"basted"` \| `"sewn"` | The formula's state in this direction after this answer. Never `"new"` - answering is what leaves the new pile. |
| `rating` | string | The grade recorded: the learner's when heard, the turns' when typed. |
| `words` | array | Words sent to their own cards by this answer. Always empty when heard. |
| `due` | string (RFC 3339) | When the formula comes back in this direction. |
| `interval_days` | integer | Days between now and `due`. |
| `pace` | object \| null | How this answer compared with the usual; `null` until three timed answers. |

**Response** `404` when `{id}` names no formula:

```json
{ "error": "there is no formula called nonsense" }
```

**Response** `422` when a said answer has no turns, or a turn names something the formula does not ask.

### `GET /api/words/{id}`

One word, with the sentences it has been met in. `{id}` is the language and the lowercase lemma, `en:doctor`.

**Response** `200`:

```json
{
  "id": "en:doctor",
  "lemma": "doctor",
  "gloss": "<what it means, in the learner's own language>",
  "ipa": "ˈdɑktɚ",
  "rank": 957,
  "level": 1000,
  "contexts": [
    {
      "sentence": 2,
      "text": "He is a doctor.",
      "translation": "<its meaning>",
      "form": "doctor",
      "start": 8,
      "formula": "be-present-statement",
      "anchor": true
    },
    {
      "sentence": 14,
      "text": "Is she a doctor?",
      "translation": "<its meaning>",
      "form": "doctor",
      "start": 9,
      "formula": "be-present-question",
      "anchor": false
    }
  ],
  "languages": { "native": "ru", "target": "en" }
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `lemma` | string | As written: `doctor`, `I`, `Monday`. |
| `gloss` | string | What it means, from the pack, in the learner's own language. |
| `ipa` | string \| null | How it sounds, from the [lexicon](/hilvan/reference/lexicon/#transcription). |
| `rank` | integer \| null | Its place among the lemmas of the language, 1 the commonest; `null` past the end of the lexicon. |
| `level` | `1000` \| `2000` \| `5000` \| null | The smallest level that holds it; `null` past the last. |
| `contexts` | array | The sentences it has been met in - those of formulas already answered - its anchor first. A word not met anywhere yet shows every sentence that holds it. A sentence two formulas share is one context. |
| `contexts[].anchor` | boolean | Whether this is the sentence the word is heard in. It is fixed by the word's first answer and kept. |

**Response** `404` when `{id}` names no word.

### `POST /api/words/{id}/review`

Records an answer to a word and reschedules it through FSRS. A word has one card, so there is no direction.

**Request:**

```json
{ "rating": "good", "duration_ms": 2400 }
```

**Response** `200`: the same shape as a formula's review - `stitch`, `due`, `interval_days`, `pace`, `rating`, `words`. The first answer fixes the word's anchor.

**Response** `404` when `{id}` names no word.

## Own words

Nouns and verbs from the learner's own life, put into the slots of the pack that take them. They are the learner's data, not the pack's: a reload of the pack leaves them alone.

### `GET /api/own-words`

**Response** `200`, newest first:

```json
{ "words": [{ "id": 2, "kind": "noun", "native": "<its meaning>", "target": "a bicycle", "known": true }] }
```

`known` is whether the [lexicon](/hilvan/reference/lexicon/) knows every word of `target` - a hint at a typo, not a refusal.

### `POST /api/own-words`

```json
{ "kind": "verb", "native": "<its meaning>", "target": "swim" }
```

`kind` is `"noun"` or `"verb"`. A verb is typed in its plain form; a noun as it is said after *I have* - *a car*, *some time*. Spaces are tidied; each side is at most 40 characters.

**Response** `201` with the word as listed. **Response** `422` with a sentence for the learner when a side is empty or too long, when the learner already has that word, or when no pack is loaded.

### `DELETE /api/own-words/{id}`

**Response** `204`, whether or not the word was there.

## Voice

`GET /api/speech`, `GET /api/listen`, `GET /api/voices` and `PUT /api/voices/{language}` sit behind the same door as the study endpoints; they are described in [Voice](/hilvan/reference/voice/).

## Errors

A server-side failure - a database error, not a client mistake - answers `500` with a message that names what failed but not the underlying detail, which is logged instead:

```json
{ "error": "today's queue could not be read" }
```
