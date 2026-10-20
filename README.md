<p align="center"><img src="https://raw.githubusercontent.com/lacodda/hilvan/main/assets/banner.svg" alt="hilvan" width="720"></p>

<p align="center">
  <a href="https://github.com/lacodda/hilvan/actions"><img src="https://img.shields.io/github/actions/workflow/status/lacodda/hilvan/ci.yml?style=flat-square" alt="CI"></a>
  <a href="https://github.com/lacodda/hilvan/blob/main/LICENSE"><img src="https://img.shields.io/github/license/lacodda/hilvan?style=flat-square" alt="License"></a>
</p>

> A self-hosted language tutor: grammar formulas, spaced repetition and audio lessons compiled from what you already know

hilvan is a language tutor for one learner, running on a Raspberry Pi at home. Grammar formulas are drilled by substitution, spaced repetition (FSRS) decides what to review and when, sentences arrive with audio, and a reader measures how much of a real text you already know. An LLM compiles lessons from your own learner model - the formulas and words you already have - and a TTS provider voices them.

The name is the Spanish *hilván*, a basting stitch: phrases are first tacked in place loosely, then sewn for good by repetition - *hilvanar frases*, to string words into speech.

<p align="center">
  <img src="https://raw.githubusercontent.com/lacodda/hilvan/main/docs/site/public/screenshots/today.png" alt="Today: what is due, formulas by state, words against the levels" width="200">
  <img src="https://raw.githubusercontent.com/lacodda/hilvan/main/docs/site/public/screenshots/drill.png" alt="A typed turn: the shape came, a word did not" width="200">
  <img src="https://raw.githubusercontent.com/lacodda/hilvan/main/docs/site/public/screenshots/inventory.png" alt="What you already know: one of the commonest words at a time" width="200">
  <img src="https://raw.githubusercontent.com/lacodda/hilvan/main/docs/site/public/screenshots/then-and-now.png" alt="Then and now: a day three weeks ago beside today" width="200">
</p>

## What you get

- **Formulas, not vocabulary lists.** Grammar drilled as substitution patterns, so production practice starts on structures you can already half-manage. Thirty of them ship as a [pack](https://github.com/lacodda/hilvan/blob/main/packs/en-from-ru/README.md) for English from Russian - a file, not code - and each opens only once the formulas it stands on are sewn.
- **Typed, checked, mended by cause.** You type the sentence; the tutor checks it, tells a slipped shape from a missed word or the wrong order, grades the formula itself, and sends a missed word back to its own card. Nouns and verbs of your own life go into the drill beside the pack's.
- **Words from the sentences you meet.** Each worked example marks the words it teaches; a word opens once you have met its sentence, is asked in that sentence by ear, and counts toward the commonest 1,000, 2,000 and 5,000 words of English.
- **Every sentence heard.** The language you learn is spoken by a native ElevenLabs voice, your own by Piper running beside the tutor - slowly while a formula is new, at speaking speed once it holds. Each sound is made once and kept, and the paid budget shows how long it will last.
- **A start from what you know.** On the first evening the commonest words go by one at a time; the ones you know start sewn and are checked when a sentence brings them, so the levels begin where you are, not at zero.
- **Your learner model, yours.** Every answer, own word and inventory answer downloads as one JSON file with a [published schema](https://lacodda.github.io/hilvan/reference/learner-model/), imports on another stand, and shows any past day beside today.
- **Spaced repetition that decides for you.** FSRS scheduling with three states - new, basted, sewn - picks one new formula a day and brings back what is due.
- **A sitting wherever you are.** On a phone in one hand, in the five or ten minutes you have, with the tutor out of reach: each typed turn is checked on the device by the tutor's own code, and the answers arrive, in order, when it can be reached.
- **A password on the door.** The server refuses to run open and says so in its first log line if you skip it.

## Install

Requires Docker on the Pi. The image is built there, for the Pi's own architecture, and one container is the whole installation.

```sh
curl -o docker-compose.yml https://raw.githubusercontent.com/lacodda/hilvan/main/docker-compose.install.yml
docker run --rm -it ghcr.io/lacodda/hilvan:latest hilvan hash    # prints the hash of a password it asks for
printf "HILVAN_PASSWORD_HASH='%s'
" '<paste the hash>' > .env && chmod 600 .env

docker compose up -d
docker compose exec server hilvan load-pack packs/en-from-ru/pack.toml
curl http://pi:8086/api/health
```

Nothing is built: the image comes from the registry, so the machine needs Docker and nothing else. Building from source is `docker-compose.prod.yml`.

The hash goes into `.env` **single-quoted** - a PHC string is full of `$`, which compose would otherwise expand. Leave `HILVAN_PASSWORD_HASH` out and the tutor is open to anyone who can reach it; the server says so in its first log line.

Everything hilvan remembers lives in the `data` volume as one file; back it up by copying it ([ADR 0001](https://github.com/lacodda/hilvan/blob/main/docs/adr/0001-stack.md)). The full walk-through is in the docs: [lacodda.github.io/hilvan](https://lacodda.github.io/hilvan/).

## Configuration

Everything comes from the environment; a `.env` file is read first, and `.env.example` shows the shape.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `HILVAN_DATABASE_URL` | no | `sqlite://data/hilvan.db?mode=rwc` | The SQLite file holding everything the tutor remembers. |
| `HILVAN_ADDR` | no | `0.0.0.0:8086` | Socket address the HTTP server binds to. |
| `HILVAN_WEB_DIR` | no | `web/dist` | Directory holding the built app, served for every path outside `/api`. |
| `HILVAN_PASSWORD_HASH` | no | unset | Argon2 hash of the learner's password, from `hilvan hash`. Unset leaves the stand open to anyone who can reach it, and the server says so at startup. |
| `HILVAN_PIPER_URL` | no | unset | The Piper voice service for your own language; the compose files run it and set this. |
| `HILVAN_ELEVENLABS_KEY` | no | unset | ElevenLabs key the language being learnt is spoken with. Unset means Piper speaks it too. |
| `RUST_LOG` | no | `hilvan=info,tower_http=info` | Log filter, in `tracing-subscriber` `EnvFilter` syntax. |

## Status

**v0.8.0**, early, built for one learner: thirty formulas for a Russian speaker, a schedule that stops for a break, and sittings that run out of the tutor's reach. See the [CHANGELOG](https://github.com/lacodda/hilvan/blob/main/CHANGELOG.md) and the [ADRs](https://github.com/lacodda/hilvan/tree/main/docs/adr).

## Documentation

[lacodda.github.io/hilvan](https://lacodda.github.io/hilvan) - getting started, guides, reference, and the architecture decision records.

## License

MIT (c) [Kirill Lakhtachev](https://lacodda.com)

The English lexicon in [`lexicons/en`](https://github.com/lacodda/hilvan/tree/main/lexicons/en) is data under CC BY-SA 4.0, with its sources credited there.
