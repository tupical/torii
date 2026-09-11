---
title: "Torii intake server"
audience: "developer"
intent: "reference"
owner: "MeiSei"
status: "active"
source_of_truth: "README.md"
last_verified: "2026-09-11"
---

# Torii 鳥居 — intake layer of Meisei

> **Meisei** 明晰 (“clarity”) is an open pipeline that carries raw intent through
> understanding → decision → plan → action to a finished result.

[![Meisei](https://img.shields.io/badge/meisei-明晰-1f2937.svg)](https://meisei.ru)
[![License: Apache-2.0 WITH Commons-Clause](https://img.shields.io/badge/license-Apache--2.0%20WITH%20Commons--Clause-blue.svg)](https://github.com/tupical/torii/blob/main/LICENSE)

<sub>
<b>torii</b> · satori · enma · yatagarasu · fujin · daruma
&nbsp;—&nbsp; <b>intake</b> · sensemaking · decisions · planning · actions · execution (terminal)
</sub>

## What it is

Torii is the **intake** layer of the Meisei pipeline. The library constructs
`RawItem` snapshots; `torii.ingest_raw` persists them in the server's SQLite
object store and `torii.list_raw` reads them. New items stay `raw`: there is no
edit, review, routing, or intake-event journal API. Historical status values
remain readable. `source` is caller-supplied context, not authenticated actor
evidence; the layer token carries workspace/project, not a user/agent identity.

The standalone `torii.parse` MCP method turns natural language into `TaskDraft`
using an optional AI provider. It is advertised and dispatched by this server,
with fake-provider tests covering success and errors. MCPBox's production
maturity route uses `torii.ingest_raw`, not `torii.parse`; parsing neither
creates a Daruma task nor replaces the maturity/handoff gate. The library has
no dependency on Daruma or sibling layers and performs no storage I/O itself.

## Repository layout

- `src/` — the `torii` library: RawItem primitives, `parse_task`, prompt registry,
  error types.
- `server/` — `torii-server`, a thin, independently-deployed HTTP/MCP wrapper over
  the library (the axum/tokio scaffold comes from `layer-kit`).
- `deploy/` — release `build.sh` (stamps the git SHA into `/healthz`) and a
  systemd user unit.

## Build & run

```sh
cargo run -p torii-server
# GET  /healthz   — open liveness/version probe
# POST /v1/mcp    — platform-token gated MCP surface:
#                   torii.ingest_raw, torii.parse, torii.list_raw
```

For production builds use `deploy/build.sh` so `/healthz` reports the real git SHA
instead of `"dev"`.

## Configuration (env)

| Variable | Default | Purpose |
| --- | --- | --- |
| `TORII_PORT` | `8090` | HTTP listen port |
| `TORII_PLATFORM_SECRET` | unset | HMAC key; if unset, `/v1/mcp` is closed |
| `TORII_VERSION` | crate version | Version reported by `/healthz` |
| `TORII_DB` | `./torii.db` | SQLite store path (`layer_kit::store::Store`) |
| `OPENAI_API_KEY` | unset | Optional AI provider for `torii.parse`; without a key it answers `ai_not_configured` (503) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL of the OpenAI-compatible API |
| `OPENAI_MODEL` | `gpt-4.1` | Model used by the AI operation |

## Docs

Pipeline canon and layer contracts: https://meisei.ru/docs

## License

Apache-2.0 WITH Commons-Clause — see [LICENSE](https://github.com/tupical/torii/blob/main/LICENSE) and
[LICENSE.commons-clause.md](https://github.com/tupical/torii/blob/main/LICENSE.commons-clause.md).
