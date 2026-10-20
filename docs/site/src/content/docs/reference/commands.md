---
title: Commands
description: Every command the hilvan binary has - the server, and the handful of jobs run beside it.
---

One binary does everything: it serves the tutor, and it runs the few jobs that are done by hand - hashing the password, loading a pack, moving the learner model in and out. Every command reads the same [environment](/hilvan/reference/configuration/), so the one that writes to the database writes to the database the server uses.

In a container the commands run inside it, next to the server:

```sh
docker compose exec -T server hilvan load-pack packs/en-from-ru/pack.toml
```

## `hilvan serve`

Runs the HTTP server: the API under `/api` and the tutor app everywhere else. It is what the container runs, and what `hilvan` with no command does too.

```sh
hilvan serve
```

On start it applies any pending migrations to the database, then binds `HILVAN_ADDR` (`0.0.0.0:8086` by default). A stand with no `HILVAN_PASSWORD_HASH` is open to anyone who can reach it, and the server says so in its log. The log goes to standard error; `RUST_LOG` filters it.

## `hilvan hash`

Hashes a password for `HILVAN_PASSWORD_HASH`, with Argon2id. Prompted for when it is not given, so it does not end up in the shell's history.

```sh
hilvan hash
```

The hash goes in the environment of the server, in single quotes in a `.env` file: it is full of `$`, which Docker Compose would otherwise read as variables.

## `hilvan load-pack <path>`

Loads a formula pack, or confirms it is already there. An unchanged pack is a no-op, and a changed one replaces its formulas without touching what the learner has learnt about them: the cards are kept by the formula's id. See [Packs](/hilvan/reference/packs/) for the format.

```sh
hilvan load-pack packs/en-from-ru/pack.toml
```

A migration updates the shape of the database, not the material in it: after an upgrade that changed the pack, load it again.

## `hilvan export`

Writes the learner model - everything the tutor knows about the learner - as one file, in the format the [learner model](/hilvan/reference/learner-model/) page publishes a schema for.

```sh
hilvan export --out hilvan-model.json
hilvan export --at 2026-03-01 --out march.json
```

| Option | Meaning |
| --- | --- |
| `-o`, `--out <file>` | Where to write it; standard output when left out. |
| `--at <moment>` | The model as it stood then instead of as it stands: a day (`YYYY-MM-DD`, its end, UTC) or an RFC 3339 date and time. |

## `hilvan import <file>`

Replaces the learner model with one `hilvan export` wrote. The model it replaces is written next to the database first, so an import is never the end of anything. The packs stay: load the ones the model was learnt on before importing it.

```sh
hilvan import hilvan-model.json
```

## `hilvan load-demo`

Loads a fictional learner, three weeks into English, onto a tutor that holds no learner: for a look around, for the screenshots in these docs, for a check of a new stand. It refuses a tutor that holds a learner already. Load the pack first.

```sh
hilvan load-demo
```

## `hilvan prune-audio`

Drops sounds nobody has asked for in a while from the cache. Explicit rather than automatic: a paid sound dropped is a sound paid for again, and that is the learner's call, not a timer's. See [Voice](/hilvan/reference/voice/).

```sh
hilvan prune-audio --unused-days 90
```

| Option | Meaning |
| --- | --- |
| `--unused-days <days>` | Sounds unused for this many days go. 90 by default. |
