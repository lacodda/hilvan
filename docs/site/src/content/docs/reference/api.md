---
title: HTTP API
description: Every endpoint under /api - method, path, session requirement, request and response shapes, status codes.
---

All endpoints are under `/api` and speak JSON. A path outside `/api` that does not match a built file falls through to the tutor app; a path under `/api` that does not match a route answers `404` with `{"error":"no such endpoint"}` - see [Unknown endpoints](/hilvan/reference/health/#unknown-endpoints).

## Sessions

Two endpoints - `/api/health` and `/api/session` - are reachable with no session. Every study endpoint (`/api/today`, `/api/formulas/{id}`, `/api/words/{id}` and their `/review`) sits behind [`HILVAN_PASSWORD_HASH`](/hilvan/reference/configuration/#the-password): when it is set, a request with no valid session cookie gets `401`.

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
      "words": [{ "word": "en:home", "lemma": "home", "form": "home", "start": 8 }]
    },
    {
      "sentence": 2,
      "native": "<prompt in the learner's own language>",
      "target": "He is a doctor.",
      "words": [{ "word": "en:doctor", "lemma": "doctor", "form": "doctor", "start": 8 }]
    }
  ],
  "slots": [
    {
      "name": "pronoun",
      "values": [
        { "native": "<word in the learner's own language>", "target": "I", "forms": { "be": "am" } },
        { "native": "<word in the learner's own language>", "target": "you", "forms": { "be": "are" } }
      ]
    },
    {
      "name": "rest",
      "values": [{ "native": "<word in the learner's own language>", "target": "at home", "forms": {} }]
    }
  ],
  "family": "be-present",
  "form": "statement",
  "languages": { "native": "ru", "target": "en" },
  "sisters": [
    { "id": "be-present-negation", "form": "negation", "name": "...", "pattern": "...", "stitch": "basted" },
    { "id": "be-present-question", "form": "question", "name": "...", "pattern": "...", "stitch": "new" }
  ]
}
```

`explanation` is in the learner's native language - the pack carries it, the server does not translate. `pattern` holds `<slot-name>` placeholders matching each entry in `slots` and is the scaffold shown on the card. `say` is the sentence a substitution answers with - `<slot>` is the value's `target`, `<slot:form>` one of its `forms`, and the first letter is raised; `null` for a formula without slots. See [the pack format](/hilvan/reference/packs/#the-sentence-it-says).

`languages` names the language of each side, ISO 639-1: what the app asks [the voice](/hilvan/reference/voice/) for.

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

### `POST /api/formulas/{id}/review`

Records an answer and reschedules the formula through FSRS.

**Request:**

```json
{ "rating": "good", "direction": "produce", "duration_ms": 4200 }
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `rating` | `"again"` \| `"hard"` \| `"good"` \| `"easy"` | yes | How well the formula came. |
| `direction` | `"produce"` \| `"recognise"` | no | Which direction was answered. Defaults to `"produce"`. |
| `duration_ms` | integer | no | How long the answer took, when the client measured it. Kept, and reported back as `pace`; never used to schedule. |

Only the card in the direction named is graded: answering `"recognise"` leaves the producing schedule of the same formula exactly as it was.

**Response** `200`:

```json
{
  "stitch": "basted",
  "due": "2026-09-04T09:00:00Z",
  "interval_days": 1,
  "pace": { "typical_ms": 3800, "last_ms": 4200, "answers": 6 }
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `stitch` | `"new"` \| `"basted"` \| `"sewn"` | The formula's state in this direction after this answer. Never `"new"` - answering is what leaves the new pile. |
| `due` | string (RFC 3339) | When the formula comes back in this direction. |
| `interval_days` | integer | Days between now and `due`. |
| `pace` | object \| null | How this answer compared with the usual; `null` until three timed answers. |

**Response** `404` when `{id}` names no formula:

```json
{ "error": "there is no formula called nonsense" }
```

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

**Response** `200`: the same shape as a formula's review - `stitch`, `due`, `interval_days`, `pace`. The first answer fixes the word's anchor.

**Response** `404` when `{id}` names no word.

## Voice

`GET /api/speech`, `GET /api/listen`, `GET /api/voices` and `PUT /api/voices/{language}` sit behind the same door as the study endpoints; they are described in [Voice](/hilvan/reference/voice/).

## Errors

A server-side failure - a database error, not a client mistake - answers `500` with a message that names what failed but not the underlying detail, which is logged instead:

```json
{ "error": "today's queue could not be read" }
```
