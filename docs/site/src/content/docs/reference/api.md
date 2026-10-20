---
title: HTTP API
description: Every endpoint under /api - method, path, session requirement, request and response shapes, status codes.
---

All endpoints are under `/api` and speak JSON. A path outside `/api` that does not match a built file falls through to the tutor app; a path under `/api` that does not match a route answers `404` with `{"error":"no such endpoint"}` - see [Unknown endpoints](/hilvan/reference/health/#unknown-endpoints).

## Sessions

Two endpoints - `/api/health` and `/api/session` - are reachable with no session. Every study endpoint (`/api/today`, `/api/formulas/{id}` and its `/review`, `/api/words/{id}` and its `/review`, `/api/own-words`, `/api/inventory`, `/api/model`) sits behind [`HILVAN_PASSWORD_HASH`](/hilvan/reference/configuration/#the-password): when it is set, a request with no valid session cookie gets `401`.

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

Today's queue: formulas, then words. In each, everything due for review comes first, then the new ones the day has room for - at most one formula and ten words, fewer on a day that begins owing cards left over from earlier ones. Reviews are never crowded out by new material. On a [break](#breaks) the queue is empty.

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
      "pace": { "typical_ms": 3800, "last_ms": 9100, "answers": 6 },
      "stability": 8.06
    },
    {
      "kind": "word",
      "word": { "id": "en:home", "lemma": "home", "gloss": "...", "ipa": "hoʊm", "rank": 150, "level": 1000, "contexts": ["..."] },
      "stitch": "new",
      "is_new": true,
      "due": "2026-09-03T09:00:00Z",
      "pace": null,
      "stability": 0
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
    "waiting": 3,
    "declared": 0
  },
  "next": {
    "id": "be-present-negation",
    "name": "...",
    "waits_for": [{ "id": "be-present-statement", "name": "...", "stitch": "basted" }]
  },
  "room": { "debt": 3, "words": 7, "formulas": 1 },
  "on_break": null,
  "away_since": null,
  "last_week": { "starts_on": "2026-08-24", "slipped": 2, "names": ["I am / you are", "home"] },
  "as_of": "2026-09-03T09:00:00Z",
  "usual": { "produce": 52000, "recognise": 18000, "word": 7400 },
  "coming": ["2026-09-03T19:12:00Z", "2026-09-05T08:40:00Z"]
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
| `queue[].stability` | number | How firmly it holds: FSRS's stability, the days until recall is predicted to fall to 90%. `0` for a card never answered. The app draws it as a bar on a scale of hours to a year, never as the number. |
| `reviewed_today` | integer | Answers recorded since midnight UTC, formulas both ways and words. A word declared known in the [inventory](#inventory) is not an answer and is not counted. |
| `counts.new` \| `counts.basted` \| `counts.sewn` | integer | How many formulas stand in each state, producing side. |
| `progress.produce` \| `progress.recognise` | object | The same counts, one per direction. |
| `words.counts` | object | Every word of the loaded packs, by state. |
| `words.levels[]` | array | The [levels](/hilvan/reference/lexicon/#levels): `size` lemmas of the language, and how many of the learner's words in it are basted and sewn - the pack's and those declared known in the inventory. Cumulative. |
| `words.waiting` | integer | Words met in a sentence, not started, and not in today's queue: what later days will open. |
| `words.declared` | integer | Words held on the learner's word alone: declared known and not asked since. Counted in the levels. |
| `next` | object \| null | The first formula of the pack's sequence that has not started and is not opening today; `null` once every formula has. |
| `next.waits_for` | array | Its [parents](/hilvan/reference/packs/#parents) not yet sewn, each with its `stitch`. Empty when it waits only for a day with room for a new formula. |
| `as_of` | string (RFC 3339) | When the server read this. A copy the device keeps for a sitting out of reach says how old it is. |
| `usual.produce` \| `usual.recognise` \| `usual.word` | integer \| null | How long an answer of each kind usually takes, in milliseconds: the median of the last 30 timed answers, `null` before there are three. A sitting of a few minutes is measured with it for a card with no `pace` of its own. |
| `coming` | array of RFC 3339 | When the cards not in the queue come back over the next seven days, breaks counted, earliest first. The app counts what has come back since the read - the number on its icon - without asking again. |
| `room.debt` | integer | Cards the queue asks that had waited through a whole day they could have been answered on - come due before the start of yesterday, the breaks counted - as the day began. |
| `room.words` | integer | New words the day opens, all told: ten, one fewer for every card of debt. Words started earlier the same day use up places. |
| `room.formulas` | integer | New formulas the day opens: `1` while the debt leaves room for a word, `0` once it has taken all ten. |
| `on_break` | object \| null | The [break](#breaks) going on - `id`, `starts_at`, `ends_at` (`null` until the learner is back). While there is one, `queue` is empty and `room` is all zeros. |
| `away_since` | string (RFC 3339) \| null | The last answer, when it was three days ago or more, no break covers the time since, and something is waiting: an absence the learner can count as a break with [`POST /api/breaks/away`](#post-apibreaksaway). |
| `last_week` | object \| null | What slipped last week (`starts_on` its Monday, `slipped` how many, `names` the first three), while this week has had no answer yet; `null` when nothing slipped. See [`GET /api/drift`](#get-apidrift). |

#### What the day began owing

A card the learner could have answered on an earlier day and did not takes the place of a new one: a day that begins owing three cards opens seven new words, one that owes ten or more opens none, and the formula of the day with them. The debt is counted as the day began - a card answered since midnight is set back to how it stood then, its earlier answers played through the scheduler again - so clearing it does not open another round the same day. A card that came due less than a day before the day began is not owed: it may have come due an hour after yesterday's sitting. A word counts from when a sentence that holds it was first met, not from a due date it had before that.

#### Parents

A formula opens only once every parent named in the pack is sewn on its producing side: the new formula of the day is the first in the pack's order whose parents all are. A formula already started is never held back again.

#### Words in the queue

A word opens only once a sentence that holds it has been met - once the formula that sentence belongs to has been answered. Of the words that have, the commonest open first, at most ten a day; a word started earlier the same day uses up a place, a word declared known in the inventory does not. A word comes back for review only once a sentence holding it has been met, too: a word declared known waits, sewn, until the drill brings one.

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

#### Breaks

A break stops the schedule's clock. An interval runs only in time outside a break, so a card whose interval spans one comes back that much later - all of them, not only those that fell due during it, which would leave the weeks after a fortnight away at twice the load. Each card keeps the due date FSRS gave it; the breaks are counted when the queue is read. The memory model is not told anything: the next answer counts every day since the last one, holiday included.

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

### Checking a typed turn

There is no endpoint for it. The app checks each typed turn the moment it is typed with hilvan's own checker - the Rust the server grades with, built to WebAssembly (`crates/check`) - so the verdict comes with the tutor out of reach as it does at home. The server checks every turn again when the answer arrives, and that reading is the one the grade rests on.

A turn names what was asked and what was typed:

```json
{ "asked": { "values": { "pronoun": "they", "rest": "ready" } }, "typed": "They is redy" }
```

`asked` names the question by what it was made of: `{ "values": { slot: target } }` for a substitution - the `target` of the value put in each slot, every slot named once - or `{ "sample": <sentence id> }` for one of the formula's own sentences.

What the checker makes of it:

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

### Answers from the device

Every answer - to a formula either way, or to a word - travels from a line kept on the device, delivered in the order the answers were given whenever the tutor can be reached: a second later at home, an evening later after a day underground. Each carries two fields besides the answer itself:

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `answer_id` | string, 1 to 64 characters | yes | An identity the device minted for this answer. The same answer delivered again - the reply to the first delivery lost on the way back - is recorded once, and told what the first delivery was told. |
| `answered_at` | string (RFC 3339) | yes | When the answer was given, by the device's clock. The answer is recorded at that moment, not when it arrived; a moment still to come - a clock running ahead - is taken as the moment it arrived. |

An answer is put into the card's history at the moment it was given, and the history is played through the scheduler again, so an answer that arrives after a later one - given on a phone with no signal while a laptop answered the same card at home - takes its place between them. An answer given during a [break](#breaks) ends the break at that moment, however long after it arrives.

**Response** `422` when either field is missing or `answer_id` is empty or too long.

### `POST /api/formulas/{id}/review`

Records an answer and reschedules the formula through FSRS. How depends on the direction.

**Saying it** - the typed turns are the answer, and the server grades them:

```json
{
  "answer_id": "6f0c2b8e-3a51-4f7e-9d2c-1b8a7e4d5c10",
  "answered_at": "2026-09-03T08:41:12.400Z",
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
| `turns` | array | yes, at least one | Every turn typed, in order, in the shape [a turn](#checking-a-typed-turn) takes. Each is checked again here; nothing the device concluded decides the grade. |
| `easy` | boolean | no | The learner's word that it came without thinking. Honoured only when every turn was right. |
| `duration_ms` | integer | no | How long the answer took. Kept and reported back as `pace`; never used to schedule. |

The grade follows the slips of the shape - a `formula` or `order` failure: in half the turns or more, `again`; in at least one, `hard`; in none, `good`, or `easy` on the learner's word for a flawless round. A turn that failed only on words leaves the formula's grade alone and records `again` on the card of each missed word the pack teaches, opening it if it was new. Every turn is kept with the review.

**Understanding it** - heard, and graded by the learner:

```json
{ "answer_id": "2b9d4f7a-0c3e-4e6b-8a51-7d2f9c1e6b30", "answered_at": "2026-09-03T08:42:30.000Z", "direction": "recognise", "rating": "good", "duration_ms": 4200 }
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
  "words": ["en:ready"],
  "stability": 2.3,
  "stability_before": 3.1
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `stitch` | `"new"` \| `"basted"` \| `"sewn"` | The formula's state in this direction after this answer. Never `"new"` - answering is what leaves the new pile. |
| `rating` | string | The grade recorded: the learner's when heard, the turns' when typed. |
| `words` | array | Words sent to their own cards by this answer. Always empty when heard. |
| `due` | string (RFC 3339) | When the formula comes back in this direction, breaks counted. |
| `interval_days` | integer | Days between the answer's arrival and `due`. |
| `pace` | object \| null | How this answer compared with the usual; `null` until three timed answers. |
| `stability` \| `stability_before` | number | How firmly the card holds after the answer and held before it, in days: what the bar on the card moves between. |

An answer given during a [break](#breaks) ends it.

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
{ "answer_id": "8e1f5a2c-6d4b-4c9e-b7a3-0f2e9d8c1a57", "answered_at": "2026-09-03T08:43:05.120Z", "rating": "good", "duration_ms": 2400 }
```

**Response** `200`: the same shape as a formula's review - `stitch`, `due`, `interval_days`, `pace`, `rating`, `words`, `stability`, `stability_before`. The first answer fixes the word's anchor.

**Response** `404` when `{id}` names no word.

## Breaks

### `POST /api/breaks`

Starts a break now. **Request**: `{ "until": "2026-11-01T00:00:00Z" }`, or `{}` for a break that lasts until the learner is back. **Response** `201`: the break - `{ "id": 3, "starts_at": "...", "ends_at": null }`. `409` while another break is going on; `422` for an `until` that is not after now.

### `POST /api/breaks/away`

Counts the time since the last answer as a break that ended now - the absence `away_since` on [`GET /api/today`](#get-apitoday) offers. **Response** `201`: the break. `409` when there is no such absence: an answer within the last three days, or a break already reaching past the last answer.

### `POST /api/breaks/{id}/end`

The learner is back: ends the break now. **Response** `200`: the break, with its `ends_at`. `404` for no such break, `409` for one that is not going on. An answer given during a break ends it the same way.

### `DELETE /api/breaks/{id}`

Removes a break as if it had never been taken - what it held comes due again where FSRS put it. **Response** `204`; `404` for no such break.

## What slipped

### `GET /api/drift`

What slipped this week and last week, Monday to Sunday, UTC. A slip is an asked *again* on a card whose answer before it was not one and was given at least a day earlier: the shape that held and would not come, the word met last week and gone, the word declared known and not known after all. Learning steps within one day are not slips.

**Response** `200`:

```json
{
  "this_week": { "starts_on": "2026-10-12", "slips": [] },
  "last_week": {
    "starts_on": "2026-10-05",
    "slips": [
      {
        "kind": "word",
        "id": "en:house",
        "name": "house",
        "direction": null,
        "gloss": "...",
        "at": "2026-10-07T18:52:10Z",
        "stitch": "sewn",
        "declared": true
      }
    ]
  }
}
```

Each card is listed once a week, at its first slip. `direction` says which way round a formula slipped; `stitch` is where it stands now; `declared` is true when the answer it slipped from was the learner's declaration in the inventory.

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

## Inventory

The commonest words of the language being learnt, each asked once: do you know it? A yes declares the word known on its card - sewn at once, checked when a sentence of the drill brings it - and is not counted as an answer; a no is kept so the word is not asked again. See [What you already know](/hilvan/guides/your-first-week/#what-you-already-know).

### `GET /api/inventory`

**Response** `200`: the next fifty words to ask, commonest first, and how far the run has gone through each level.

```json
{
  "language": "en",
  "next": [{ "word": "en:african", "lemma": "African", "ipa": "ˈæfɹəkən", "rank": 1301 }],
  "levels": [
    { "size": 1000, "left": 0, "known": 758 },
    { "size": 2000, "left": 694, "known": 854 },
    { "size": 5000, "left": 3694, "known": 854 }
  ],
  "answered": 1294,
  "known": 854
}
```

`left` is how many words of the level are still to be asked; a word already being learnt in the drill is not asked. The run goes as far as the 5,000 level. `language` is `null` before a pack is loaded; `levels` is empty for a language hilvan has no lexicon for.

### `POST /api/inventory`

Answers, oldest first - a list, so a screen that lost its connection can send what it holds at once.

```json
{ "answers": [{ "word": "en:african", "known": true }, { "word": "en:mobile", "known": false }] }
```

**Response** `200`: the run as it stands after them, as `GET`. An answer to a word already answered, or already being learnt, changes nothing. **Response** `422` when a word is not a lemma of the language being learnt.

### `DELETE /api/inventory/{word}`

Takes an answer back: the word is asked again, and a yes leaves the word's card as it was before - new for a word a pack teaches, gone otherwise. Taking back an answer never given is fine.

**Response** `200`: the run as it stands after. **Response** `409` when the word has been answered in the drill since the yes - that history stands on it.

## Learner model

The whole of what the tutor knows about the learner, as one file - see [The learner model file](/hilvan/reference/learner-model/).

### `GET /api/model`

**Response** `200`: the [learner model](/hilvan/reference/learner-model/), with `Content-Disposition: attachment; filename="hilvan-model-<day>.json"`, so a link saves it as a file.

`?at=` with an RFC 3339 moment gives the model as it stood then: the answers given by then, played through the scheduler again. A moment still to come is now.

### `GET /api/model/standing`

Where the learner stood at a moment, beside where they stand now, counted the way [`GET /api/today`](#get-apitoday) counts.

`?at=` an RFC 3339 moment; now when left out.

```json
{
  "then": {
    "at": "2026-09-18T23:59:59.999Z",
    "progress": { "produce": { "new": 29, "basted": 1, "sewn": 0 }, "recognise": { "new": 30, "basted": 0, "sewn": 0 } },
    "words": { "counts": { "new": 81, "basted": 0, "sewn": 45 }, "levels": [{ "size": 1000, "basted": 0, "sewn": 756 }], "waiting": 0, "declared": 756 },
    "answers": 1
  },
  "now": { "...": "the same, now" },
  "began": "2026-09-18T19:01:40Z"
}
```

`answers` counts asked answers only. `began` is the first of them - how far back there is anything to look at - or `null`. `waiting` is not told of the past.

## Voice

`GET /api/speech`, `GET /api/listen`, `GET /api/voices` and `PUT /api/voices/{language}` sit behind the same door as the study endpoints; they are described in [Voice](/hilvan/reference/voice/).

## Errors

A server-side failure - a database error, not a client mistake - answers `500` with a message that names what failed but not the underlying detail, which is logged instead:

```json
{ "error": "today's queue could not be read" }
```
