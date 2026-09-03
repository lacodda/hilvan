<p align="center"><img src="https://raw.githubusercontent.com/lacodda/hilvan/main/assets/banner.svg" alt="hilvan" width="720"></p>

# hilvan

[![CI](https://github.com/lacodda/hilvan/actions/workflows/ci.yml/badge.svg)](https://github.com/lacodda/hilvan/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-lacodda.github.io%2Fhilvan-blue)](https://lacodda.github.io/hilvan/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/lacodda/hilvan/blob/main/LICENSE)

> A self-hosted language tutor: grammar formulas, spaced repetition and audio lessons compiled from what you already know

hilvan is a language tutor for one learner, running on a Raspberry Pi at home. Grammar formulas are drilled by substitution, spaced repetition (FSRS) decides what to review and when, sentences arrive with audio, and a reader measures how much of a real text you already know. An LLM compiles lessons from your own learner model - the formulas and words you already have - and a TTS provider voices them.

The name is the Spanish *hilván*, a basting stitch: phrases are first tacked in place loosely, then sewn for good by repetition - *hilvanar frases*, to string words into speech.

## What it will do

- **Formulas first.** Grammar drilled as substitution patterns, so production practice starts on structures you can already half-manage. Thirty of them ship as a [pack](https://github.com/lacodda/hilvan/blob/main/packs/en-from-ru/README.md) - a file, not code, which is how a second language arrives later.
- **Repetition that measures.** Spaced repetition (FSRS) over formulas and words, and a reader that scores how much of a real text you already know - coverage as the honest progress metric.
- **Material compiled for you.** An LLM assembles sentences and audio lessons from your learner model, voiced by a TTS provider - a compiler of material, not a chatbot.

First target language: English. Spanish later.


## Install

Requires Docker on the Pi. The image is built there, for the Pi's own architecture, and one container is the whole installation.

```sh
git clone https://github.com/lacodda/hilvan && cd hilvan
docker compose -f docker-compose.prod.yml up -d --build
docker compose -f docker-compose.prod.yml exec hilvan hilvan load-pack packs/en-from-ru/pack.toml
curl http://pi:8086/api/health
```

Set `HILVAN_PASSWORD` before starting, or the stand is open to anyone on the network.

Everything hilvan remembers lives in the `data` volume as one file; back it up by copying it ([ADR 0001](https://github.com/lacodda/hilvan/blob/main/docs/adr/0001-stack.md)). The full walk-through is in the docs: [lacodda.github.io/hilvan](https://lacodda.github.io/hilvan/).

## Configuration

Everything comes from the environment; a `.env` file is read first, and `.env.example` shows the shape.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `HILVAN_DATABASE_URL` | no | `sqlite://data/hilvan.db?mode=rwc` | The SQLite file holding everything the tutor remembers. |
| `HILVAN_ADDR` | no | `0.0.0.0:8086` | Socket address the HTTP server binds to. |
| `HILVAN_WEB_DIR` | no | `web/dist` | Directory holding the built app, served for every path outside `/api`. |
| `HILVAN_PASSWORD` | no | unset | The password the learner signs in with. Unset leaves the stand open to anyone who can reach it, and the server says so at startup. |
| `RUST_LOG` | no | `hilvan=info,tower_http=info` | Log filter, in `tracing-subscriber` `EnvFilter` syntax. |

## Status

**v0.1.0 Formulas** - the tutor teaches. Thirty English grammar formulas for a Russian speaker ship as a pack, spaced repetition (FSRS) decides what comes back and when, and a drill runs a sitting on a phone: the formula of the day, a prompt, your own answer, and how it went.

What is built: the learner model, formula packs as data, FSRS scheduling with three states - new, basted, sewn - one new formula a day, the drill, and a password on the door. What comes next: audio, words inside sentences, the reader, and the compiler that writes material from the learner model.

The architecture is recorded in the [ADRs](https://github.com/lacodda/hilvan/tree/main/docs/adr): [SQLite on a Pi](https://github.com/lacodda/hilvan/blob/main/docs/adr/0001-stack.md), [one model for every language](https://github.com/lacodda/hilvan/blob/main/docs/adr/0002-one-model-for-every-language.md), [FSRS](https://github.com/lacodda/hilvan/blob/main/docs/adr/0003-fsrs-scheduler.md).

## Development

Requires Rust (see `rust-version` in `Cargo.toml`) and Node LTS with pnpm.

```sh
cp .env.example .env
cargo run -- load-pack packs/en-from-ru/pack.toml   # the formulas the tutor teaches from
cargo run -- serve                        # the API on :8086; /api/health reports the database

cd web && pnpm install && pnpm dev        # the app on :5173, proxying /api to the server
cd docs/site && pnpm install && pnpm dev  # the documentation site
```

## Documentation

[lacodda.github.io/hilvan](https://lacodda.github.io/hilvan) - getting started, guides, reference, and the architecture decision records.

## License

[MIT](https://github.com/lacodda/hilvan/blob/main/LICENSE)
