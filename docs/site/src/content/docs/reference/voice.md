---
title: Voice
description: Who speaks each language, the speech, listening and voices endpoints, the audio cache and the ElevenLabs budget.
---

Every sentence of the material can be heard. Two engines share the work, and the learner never has to know which one is talking:

| Language | Engine | Why |
| --- | --- | --- |
| The one being learnt (`target` of the pack) | **ElevenLabs**, `eleven_flash_v2_5` | A native speaker is what pronunciation is learnt from, and the paid budget goes on nothing else. |
| The learner's own (`native` of the pack) | **Piper**, a service running beside the server | The teacher, the translations, the explanations: free, local, a second or two per sentence on a Pi 4. |

Without an ElevenLabs key the language being learnt is spoken by a Piper voice that speaks it - the voices screen and [health](/hilvan/reference/health/) both say so, rather than it happening quietly. ElevenLabs is never offered for the learner's own language.

## What can be spoken

The speech endpoint takes text, because the drill assembles its sentences on the client. It is not a free text-to-speech service: every request is checked against what the loaded packs can actually say before any engine hears it -

- a sample, in either language;
- a formula's explanation, in the learner's own language;
- a sentence a formula's [`say`](/hilvan/reference/packs/#the-sentence-it-says) makes of its slot values, in the language being learnt.

Anything else is refused with `422`.

## `GET /api/speech`

One sentence, spoken. A GET so the app can hand the URL straight to an `<audio>` element.

| Parameter | Required | Meaning |
| --- | --- | --- |
| `language` | yes | ISO 639-1 code of the language the text is in. |
| `text` | yes | The sentence. Surrounding whitespace is ignored. |
| `tempo` | no | `normal` (default) or `slow`. The app asks for `slow` while a formula is new. |

**Response** `200` with the sound: `audio/mpeg` from ElevenLabs, `audio/wav` from Piper. The `ETag` is the cache key and `Cache-Control` is `private, no-cache`: the browser keeps the sound and asks each time, an unchanged one costs an empty `304`, and a voice changed on the voices screen is a different key, so the old voice is never served.

| Status | When |
| --- | --- |
| `422` | The text is not something the material says. |
| `404` | No voice speaks the language - no engine configured, or none answering. |
| `502` | The engine failed: ElevenLabs out of credits, Piper stuck. The body names the cause. |

## `GET /api/listen`

The sentences of the listening mode: every sample of every formula the learner has started, in either direction.

```json
{
  "sentences": [
    { "formula": "be-present-statement", "target": "I am at home.", "native": "<its meaning>", "language": "en" }
  ]
}
```

Samples only: "what does it mean?" deserves a real sentence as its answer, and a substitution's meaning in the learner's own language is a scaffold. Listening is practice and is not graded - there is no listening card in the schedule.

## `GET /api/voices`

What the voices screen shows: each language with the voice it is spoken in and the voices it may be.

```json
{
  "languages": [
    {
      "code": "en",
      "role": "target",
      "spoken": { "voice": { "engine": "elevenlabs", "id": "...", "name": "Rachel", "languages": [] }, "chosen": false },
      "options": [ { "engine": "elevenlabs", "id": "...", "name": "Rachel", "languages": [] } ],
      "sample": "I am at home."
    },
    {
      "code": "ru",
      "role": "native",
      "spoken": { "voice": { "engine": "piper", "id": "ru_RU-irina-medium", "name": "Irina", "languages": ["ru"] }, "chosen": true },
      "options": [ { "engine": "piper", "id": "ru_RU-denis-medium", "name": "Denis", "languages": ["ru"] } ],
      "sample": "<the first sample, in the learner's own language>"
    }
  ],
  "piper": "ok",
  "elevenlabs": "off",
  "budget": null
}
```

`chosen` is `false` when the learner has not picked a voice and the language speaks with the first one offered for its role. A chosen voice that is not offered right now - a lapsed key - is kept, and the default speaks until it is back. `piper` and `elevenlabs` are `ok`, `unreachable` or `off` (not configured). `sample` is the first sample of the pack in that language - what the screen lets the learner hear a voice say. `budget` is described [below](#the-elevenlabs-budget).

## `PUT /api/voices/{language}`

Chooses the voice a language is spoken in.

```json
{ "engine": "piper", "voice": "ru_RU-dmitri-medium" }
```

Answers the voice with `200`, or `404` when that voice is not offered for the language - which includes any ElevenLabs voice for the learner's own language.

## The audio cache

Every sound is made once and kept in the database, keyed on a hash of the engine, voice, model, settings, language and text: a slower tempo or another voice is a different entry, never the wrong sound. The cache lives in the same SQLite file as everything else, so a backup of the tutor includes the sounds that took credits to make.

Health reports what the cache holds. Sounds nobody has asked for in a while - material that left the pack, a voice the learner moved away from - are dropped by hand:

```sh
hilvan prune-audio                   # unused for 90 days
hilvan prune-audio --unused-days 30
```

Explicit rather than on a timer: a paid sound dropped is a sound paid for again.

## The ElevenLabs budget

```json
{
  "remaining": 20000,
  "limit": 30000,
  "resets_at": "2026-10-01T00:00:00Z",
  "per_day": 100.0,
  "runs_out_at": "2027-04-19T00:00:00Z",
  "lasts": true
}
```

`remaining` and `resets_at` come from the account; `per_day` is what the cache says was spent over the last two weeks, and `runs_out_at` is when `remaining` ends at that rate (`null` when nothing is being spent). `lasts` answers the one question worth asking before a lesson: will this period's credits hold until they start over? The account is asked at most every ten minutes.
