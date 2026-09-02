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
