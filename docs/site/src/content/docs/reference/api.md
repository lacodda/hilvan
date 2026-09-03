---
title: HTTP API
description: Every endpoint under /api - method, path, session requirement, request and response shapes, status codes.
---

All endpoints are under `/api` and speak JSON. A path outside `/api` that does not match a built file falls through to the tutor app; a path under `/api` that does not match a route answers `404` with `{"error":"no such endpoint"}` - see [Unknown endpoints](/hilvan/reference/health/#unknown-endpoints).

## Sessions

Two endpoints - `/api/health` and `/api/session` - are reachable with no session. Every study endpoint (`/api/today`, `/api/formulas/{id}`, `/api/formulas/{id}/review`) sits behind [`HILVAN_PASSWORD`](/hilvan/reference/configuration/#the-password): when it is set, a request with no valid session cookie gets `401`.

```json
{ "error": "sign in first" }
```

The session cookie is `hilvan_session`, `HttpOnly`, `SameSite=Lax`, and lasts 30 days from sign-in. It is not marked `Secure`, because the stand is reached over plain HTTP on a home network.

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
| `required` | boolean | Whether `HILVAN_PASSWORD` is set. |
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

Everything below requires a valid session when `HILVAN_PASSWORD` is set.

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
        ]
      },
      "stitch": "basted",
      "is_new": false,
      "due": "2026-09-03T09:00:00Z"
    }
  ],
  "reviewed_today": 1,
  "counts": { "new": 24, "basted": 3, "sewn": 2 }
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `queue` | array | Due items, then at most one new one. Empty once nothing is due and no new formula is left in the pack. |
| `queue[].formula` | object | The full formula - see `GET /api/formulas/{id}` below. |
| `queue[].stitch` | `"new"` \| `"basted"` \| `"sewn"` | Where this formula stands right now. |
| `queue[].is_new` | boolean | Whether this is the formula of the day - never answered before. |
| `queue[].due` | string (RFC 3339) | When this item became due; for a new item, the time of the request. |
| `reviewed_today` | integer | Reviews recorded since midnight UTC. |
| `counts.new` \| `counts.basted` \| `counts.sewn` | integer | How many formulas stand in each state across the whole pack. |

### `GET /api/formulas/{id}`

One formula with its samples and slots.

**Response** `200`:

```json
{
  "id": "be-present-statement",
  "name": "I am / you are",
  "pattern": "<pronoun> + am/is/are + <rest>",
  "explanation": "...",
  "samples": [
    { "native": "<prompt in the learner's own language>", "target": "I am at home." },
    { "native": "<prompt in the learner's own language>", "target": "He is a doctor." }
  ],
  "slots": [
    {
      "name": "pronoun",
      "values": [
        { "native": "<word in the learner's own language>", "target": "I" },
        { "native": "<word in the learner's own language>", "target": "you" }
      ]
    },
    {
      "name": "rest",
      "values": [{ "native": "<word in the learner's own language>", "target": "at home" }]
    }
  ]
}
```

`explanation` is in the learner's native language - the pack carries it, the server does not translate. `pattern` holds `<slot-name>` placeholders matching each entry in `slots`.

**Response** `404` when `{id}` names no formula:

```json
{ "error": "there is no formula called nonsense" }
```

### `POST /api/formulas/{id}/review`

Records an answer and reschedules the formula through FSRS.

**Request:**

```json
{ "rating": "good", "duration_ms": 4200 }
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `rating` | `"again"` \| `"hard"` \| `"good"` \| `"easy"` | yes | How well the formula came. |
| `duration_ms` | integer | no | How long the answer took, when the client measured it. Stored, not yet used to schedule. |

**Response** `200`:

```json
{ "stitch": "basted", "due": "2026-09-04T09:00:00Z", "interval_days": 1 }
```

| Field | Type | Meaning |
| --- | --- | --- |
| `stitch` | `"new"` \| `"basted"` \| `"sewn"` | The formula's state after this answer. Never `"new"` - answering is what leaves the new pile. |
| `due` | string (RFC 3339) | When the formula comes back. |
| `interval_days` | integer | Days between now and `due`. |

**Response** `404` when `{id}` names no formula:

```json
{ "error": "there is no formula called nonsense" }
```

## Errors

A server-side failure - a database error, not a client mistake - answers `500` with a message that names what failed but not the underlying detail, which is logged instead:

```json
{ "error": "today's queue could not be read" }
```
