---
title: Health endpoint
description: The contract of GET /api/health - status codes, body shape, and what "degraded" means.
---

`GET /api/health` answers liveness and readiness in one place: the process responds, and a database round-trip says whether it can actually do its job.

## Request

```
GET /api/health
```

No authentication, no parameters.

## Response

| Condition | Status | `status` |
| --- | --- | --- |
| Database reachable | `200` | `"ok"` |
| Database unreachable | `503` | `"degraded"` |

```json
{
  "status": "ok",
  "version": "0.0.0"
}
```

`version` is the server's own package version, which makes the endpoint the authoritative answer to "what is actually deployed here".

## Why degraded is not down

A process that answers `503` with a parseable body is telling you something a connection refusal cannot: it started, it read its configuration, it bound its port, and the thing it cannot reach is its storage. The container's healthcheck calls this endpoint, so `docker compose ps` distinguishes "the server is gone" from "the server is fine and the volume is not".

## Unknown endpoints

Anything else under `/api` answers `404` with `{"error":"no such endpoint"}`. The API never falls through to the app: a misspelled path has to look like a mistake, not like a page.
