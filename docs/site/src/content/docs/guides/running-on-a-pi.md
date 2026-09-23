---
title: Running on a Raspberry Pi
description: The stand - one container built on the Pi, the database in a volume, port 8086.
---

The stand is a Raspberry Pi 4 running Docker. The image is built on the Pi itself, which keeps the architecture honest: no cross-compilation and no chance of an x86 binary that only fails there.

## What goes on the Pi

Two things, and only the first is part of this repository:

- **The source tree**, staged from a committed state by `tools/stage-deploy.sh` and uploaded. `git archive` is the filter, so a file that was never committed cannot reach the stand.
- **A `.env` file** next to the compose file, never committed:

```sh
HILVAN_PORT=8086   # the stand's address
```

## Bringing it up

```sh
docker compose -f docker-compose.prod.yml up -d --build
```

The first build on a Pi takes a while - a Rust release build and a Node build. Later builds reuse the dependency layers and are much shorter.

```sh
curl http://pi:8086/api/health
```

The container's own healthcheck calls the same endpoint, so `docker compose ps` says whether the server has actually reached its database.

## Where the state is

The database is one file in the `data` volume. A backup is a copy of it:

```sh
docker compose -f docker-compose.prod.yml cp server:/data/hilvan.db ./hilvan-backup.db
```

Copying while the server runs is safe: the database is in WAL mode. Restoring is copying the file back and restarting the container.

## The voice

The compose files run a second container beside the server: `piper`, the local voice that speaks the learner's own language. Its image carries its voices, so it needs no network at start, and it is published at the tutor's own version - pinning `HILVAN_VERSION` pins both. On a Pi 4 it answers a sentence in one to two seconds once a voice is loaded, and takes about 150 MB plus 130 MB for each voice in use.

Its port is published as `8088` (`HILVAN_PIPER_PORT` moves it) so other services on the Pi can speak with the same voice; hilvan itself reaches it by name inside the compose network.

For the language being learnt, add an ElevenLabs key to `.env` and bring the stand up again:

```sh
HILVAN_ELEVENLABS_KEY=sk_...
```

Without it, Piper speaks that language too, and [health](/hilvan/reference/health/) says `"elevenlabs": "off"`. Recording your own voice in the drill needs the page to be opened over HTTPS - a browser only lends the microphone to a secure page.
