---
title: HTTP API
description: Every endpoint under /api - method, path, session requirement, request and response shapes, status codes.
---

All endpoints are under `/api` and speak JSON. A path outside `/api` that does not match a built file falls through to the tutor app; a path under `/api` that does not match a route answers `404` with `{"error":"no such endpoint"}` - see [Unknown endpoints](/hilvan/reference/health/#unknown-endpoints).

## Sessions

Two endpoints - `/api/health` and `/api/session` - are reachable with no session. Every study endpoint (`/api/today`, `/api/formulas/{id}`, `/api/formulas/{id}/review`) sits behind [`HILVAN_PASSWORD_HASH`](/hilvan/reference/configuration/#the-password): when it is set, a request with no valid session cookie gets `401`.

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

Today's queue: everything due for review, then at most one new formula if the day has room. Reviews are never crowded out by new material.

**Response** `200`:

```json
{
  "queue": [
    {
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
    }
  ],
  "reviewed_today": 1,
  "counts": { "new": 24, "basted": 3, "sewn": 2 },
  "progress": {
    "produce": { "new": 24, "basted": 3, "sewn": 2 },
    "recognise": { "new": 27, "basted": 2, "sewn": 0 }
  }
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `queue` | array | Due items, then at most one new one. Empty once nothing is due and no new formula is left in the pack. |
| `queue[].formula` | object | The full formula - see `GET /api/formulas/{id}` below. |
| `queue[].direction` | `"produce"` \| `"recognise"` | Which way round this item asks the formula. |
| `queue[].stitch` | `"new"` \| `"basted"` \| `"sewn"` | Where this formula stands **in this direction** right now. |
| `queue[].is_new` | boolean | Whether this formula has never been answered in this direction. |
| `queue[].due` | string (RFC 3339) | When this item became due; for a new item, the time of the request. |
| `queue[].pace` | object \| null | How fast this card usually comes; `null` until three timed answers. |
| `reviewed_today` | integer | Reviews recorded since midnight UTC, both directions. |
| `counts.new` \| `counts.basted` \| `counts.sewn` | integer | How many formulas stand in each state, producing side. |
| `progress.produce` \| `progress.recognise` | object | The same counts, one per direction. |

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
    { "native": "<prompt in the learner's own language>", "target": "I am at home." },
    { "native": "<prompt in the learner's own language>", "target": "He is a doctor." }
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
  "sisters": [
    { "id": "be-present-negation", "form": "negation", "name": "...", "pattern": "...", "stitch": "basted" },
    { "id": "be-present-question", "form": "question", "name": "...", "pattern": "...", "stitch": "new" }
  ]
}
```

`explanation` is in the learner's native language - the pack carries it, the server does not translate. `pattern` holds `<slot-name>` placeholders matching each entry in `slots` and is the scaffold shown on the card. `say` is the sentence a substitution answers with - `<slot>` is the value's `target`, `<slot:form>` one of its `forms`, and the first letter is raised; `null` for a formula without slots. See [the pack format](/hilvan/reference/packs/#the-sentence-it-says).

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

## Voice

`GET /api/speech`, `GET /api/listen`, `GET /api/voices` and `PUT /api/voices/{language}` sit behind the same door as the study endpoints; they are described in [Voice](/hilvan/reference/voice/).

## Errors

A server-side failure - a database error, not a client mistake - answers `500` with a message that names what failed but not the underlying detail, which is logged instead:

```json
{ "error": "today's queue could not be read" }
```
