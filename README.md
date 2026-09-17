<p align="center"><img src="https://raw.githubusercontent.com/lacodda/hilvan/main/assets/banner.svg" alt="hilvan" width="720"></p>

# hilvan

[![CI](https://github.com/lacodda/hilvan/actions/workflows/ci.yml/badge.svg)](https://github.com/lacodda/hilvan/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-lacodda.github.io%2Fhilvan-blue)](https://lacodda.github.io/hilvan/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/lacodda/hilvan/blob/main/LICENSE)

> A self-hosted language tutor: grammar formulas, spaced repetition and audio lessons compiled from what you already know

hilvan is a language tutor for one learner, running on a Raspberry Pi at home. Grammar formulas are drilled by substitution, spaced repetition (FSRS) decides what to review and when, sentences arrive with audio, and a reader measures how much of a real text you already know. An LLM compiles lessons from your own learner model - the formulas and words you already have - and a TTS provider voices them.

The name is the Spanish *hilván*, a basting stitch: phrases are first tacked in place loosely, then sewn for good by repetition - *hilvanar frases*, to string words into speech.

## What you get

- **Formulas, not vocabulary lists.** Grammar drilled as substitution patterns, so production practice starts on structures you can already half-manage. Thirty of them ship as a [pack](https://github.com/lacodda/hilvan/blob/main/packs/en-from-ru/README.md) for English from Russian - a file, not code.
- **Spaced repetition that decides for you.** FSRS scheduling with three states - new, basted, sewn - picks one new formula a day and brings back what is due.
- **A sitting on a phone.** The formula of the day, a prompt, your own answer, and how it went.
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
| `RUST_LOG` | no | `hilvan=info,tower_http=info` | Log filter, in `tracing-subscriber` `EnvFilter` syntax. |

## Status

**v0.2.2**: the tutor teaches. Thirty English grammar formulas for a Russian speaker ship as a pack, FSRS scheduling drills each formula in both directions and groups its forms on one card, and a sitting runs end to end on a phone, behind a password. Running on a Raspberry Pi at home since v0.1.0. See the [CHANGELOG](https://github.com/lacodda/hilvan/blob/main/CHANGELOG.md) for what landed in each version, and the [ADRs](https://github.com/lacodda/hilvan/tree/main/docs/adr) for the architecture.

## Documentation

[lacodda.github.io/hilvan](https://lacodda.github.io/hilvan) - getting started, guides, reference, and the architecture decision records.

## License

[MIT](https://github.com/lacodda/hilvan/blob/main/LICENSE)
