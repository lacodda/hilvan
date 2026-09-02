---
title: Configuration
description: Environment variables the server reads, their defaults, and what happens when they are wrong.
---

hilvan is configured entirely through the environment. There is no configuration file to drift from the deployment.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `HILVAN_DATABASE_URL` | no | `sqlite://data/hilvan.db?mode=rwc` | The SQLite file holding everything the tutor remembers. `mode=rwc` creates it; the server creates the directory. |
| `HILVAN_ADDR` | no | `0.0.0.0:8086` | Socket address the HTTP server binds to. |
| `HILVAN_WEB_DIR` | no | `web/dist` | Directory holding the built app, served for every path outside `/api`. |
| `RUST_LOG` | no | `hilvan=info,tower_http=info` | Log filter, in `tracing-subscriber` `EnvFilter` syntax. |

A `.env` file in the working directory is read first, so all of these can live there during development. The file is never committed; `.env.example` shows the shape.

## How it is read

The server reads the environment once at startup and fails immediately if it cannot build a valid configuration:

- **`HILVAN_ADDR` malformed** - startup aborts echoing the value it could not parse.
- **`HILVAN_DATABASE_URL` not a SQLite URL** - startup aborts naming the variable.

A blank value for an optional variable means the default: a compose file that leaves `HILVAN_WEB_DIR=` empty does not make the server serve its working directory.

Failing at startup is deliberate. A server that boots with a broken configuration and only discovers it on the first request has turned a deployment error into an outage.

## The app directory

`HILVAN_WEB_DIR` is what separates development from the stand. In development Vite serves the app on its own port and proxies `/api` to the server, so the directory can stay unbuilt; asking the server for `/` then answers `404` with a line saying so. On the stand the image carries the built app at `/app/web`, and the same process serves both.
