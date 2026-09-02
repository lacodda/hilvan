# hilvan

[![CI](https://github.com/lacodda/hilvan/actions/workflows/ci.yml/badge.svg)](https://github.com/lacodda/hilvan/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-lacodda.github.io%2Fhilvan-blue)](https://lacodda.github.io/hilvan/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/lacodda/hilvan/blob/main/LICENSE)

> A self-hosted language tutor: grammar formulas, spaced repetition and audio lessons compiled from what you already know

hilvan is a language tutor for one learner, running on a Raspberry Pi at home. Grammar formulas are drilled by substitution, spaced repetition (FSRS) decides what to review and when, sentences arrive with audio, and a reader measures how much of a real text you already know. An LLM compiles lessons from your own learner model - the formulas and words you already have - and a TTS provider voices them.

The name is the Spanish *hilván*, a basting stitch: phrases are first tacked in place loosely, then sewn for good by repetition - *hilvanar frases*, to string words into speech.

## What it will do

- **Formulas first.** Grammar drilled as substitution patterns, so production practice starts on structures you can already half-manage.
- **Repetition that measures.** Spaced repetition (FSRS) over formulas and words, and a reader that scores how much of a real text you already know - coverage as the honest progress metric.
- **Material compiled for you.** An LLM assembles sentences and audio lessons from your learner model, voiced by a TTS provider - a compiler of material, not a chatbot.

First target language: English. Spanish later.


## Install

Requires Docker on the Pi. The image is built there, for the Pi's own architecture, and one container is the whole installation.

```sh
git clone https://github.com/lacodda/hilvan && cd hilvan
docker compose -f docker-compose.prod.yml up -d --build
curl http://pi:8086/api/health
```

Everything hilvan remembers lives in the `data` volume as one file; back it up by copying it ([ADR 0001](https://github.com/lacodda/hilvan/blob/main/docs/adr/0001-stack.md)). The full walk-through is in the docs: [lacodda.github.io/hilvan](https://lacodda.github.io/hilvan/).

## Configuration

Everything comes from the environment; a `.env` file is read first, and `.env.example` shows the shape.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `HILVAN_DATABASE_URL` | no | `sqlite://data/hilvan.db?mode=rwc` | The SQLite file holding everything the tutor remembers. |
| `HILVAN_ADDR` | no | `0.0.0.0:8086` | Socket address the HTTP server binds to. |
| `HILVAN_WEB_DIR` | no | `web/dist` | Directory holding the built app, served for every path outside `/api`. |
| `RUST_LOG` | no | `hilvan=info,tower_http=info` | Log filter, in `tracing-subscriber` `EnvFilter` syntax. |

## Status

**v0.0.0 skeleton** - builds, lints, ships a container. The server answers `/api/health` and serves the app; the app says hello in the line's theme. The architecture is recorded in [ADR 0001](https://github.com/lacodda/hilvan/blob/main/docs/adr/0001-stack.md). The learner model, formulas, repetition and lessons are what the next releases build. **v0.1.0 Formulas is next.**

## Development

Requires Rust (see `rust-version` in `Cargo.toml`) and Node LTS with pnpm.

```sh
cp .env.example .env
cargo run -- serve                        # the API on :8086; /api/health reports the database

cd web && pnpm install && pnpm dev        # the app on :5173, proxying /api to the server
cd docs/site && pnpm install && pnpm dev  # the documentation site
```

## Documentation

[lacodda.github.io/hilvan](https://lacodda.github.io/hilvan) - getting started, guides, reference, and the architecture decision records.

## License

[MIT](https://github.com/lacodda/hilvan/blob/main/LICENSE)
