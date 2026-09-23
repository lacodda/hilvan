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
| `HILVAN_PASSWORD_HASH` | no | unset | Argon2 hash of the learner's password, from `hilvan hash`. Unset leaves the stand open. |
| `HILVAN_PIPER_URL` | no | unset | The Piper service that speaks the learner's own language, e.g. `http://piper:5000`. The compose files set it; unset leaves that language silent. |
| `HILVAN_ELEVENLABS_KEY` | no | unset | ElevenLabs key with the Text to Speech permission; the language being learnt is spoken with it. Unset means Piper speaks every language. |
| `RUST_LOG` | no | `hilvan=info,tower_http=info` | Log filter, in `tracing-subscriber` `EnvFilter` syntax. |

A `.env` file in the working directory is read first, so all of these can live there during development. The file is never committed; `.env.example` shows the shape.

## The password

`HILVAN_PASSWORD_HASH` is the only thing standing between an open stand and a locked one. Unset (or blank, which counts as unset) means anyone who can reach the server can use the tutor - fine on a home network, and what a developer's machine wants. Set it, and every study endpoint asks for a session first; see [the API reference](/hilvan/reference/api/) for which endpoints that covers.

It holds a hash, never the password itself: a `.env` file travels into backups and shows up in `docker inspect`, and neither should hand anyone the password. Produce the hash with the command that ships with the server:

```sh
hilvan hash              # prompts, so the password stays out of shell history
hilvan hash 'my password'
```

It prints an Argon2id PHC string. Put it in `.env` **single-quoted** - the string is full of `$`, which both a shell and Docker Compose would otherwise expand:

```sh
HILVAN_PASSWORD_HASH='$argon2id$v=19$m=19456,t=2,p=1$...'
```

Sessions are rows in the database, not signed tokens, so they survive a restart of the server: updating the stand does not sign the learner's phone out. A session lasts 90 days and every request pushes that out, so a learner who opens the tutor at all regularly never sees the login screen again. Signing out ends the session on the server, not just in the browser.

The server warns at startup, once, when the variable is unset:

```
HILVAN_PASSWORD_HASH is not set: anyone who can reach this server can use the tutor
```

That warning is deliberate rather than a nag: an open stand should be a choice, never the result of forgetting a variable in the deployment's `.env`.

## The voice

Both engines are optional, and the server says at startup which are missing. See [Voice](/hilvan/reference/voice/) for who speaks which language.

- **Piper** is a service beside the server - the compose files run it as `piper` and point `HILVAN_PIPER_URL` at it. A URL that is not `http://` or `https://` stops the server at startup, naming the variable.
- **ElevenLabs** needs a key: in the ElevenLabs dashboard, *Profile → API Keys*, create one with the Text to Speech permission and put it in `.env`. A blank value counts as unset.

## How it is read

The server reads the environment once at startup and fails immediately if it cannot build a valid configuration:

- **`HILVAN_ADDR` malformed** - startup aborts echoing the value it could not parse.
- **`HILVAN_DATABASE_URL` not a SQLite URL** - startup aborts naming the variable.

A blank value for an optional variable means the default: a compose file that leaves `HILVAN_WEB_DIR=` empty does not make the server serve its working directory.

Failing at startup is deliberate. A server that boots with a broken configuration and only discovers it on the first request has turned a deployment error into an outage.

## The app directory

`HILVAN_WEB_DIR` is what separates development from the stand. In development Vite serves the app on its own port and proxies `/api` to the server, so the directory can stay unbuilt; asking the server for `/` then answers `404` with a line saying so. On the stand the image carries the built app at `/app/web`, and the same process serves both.
