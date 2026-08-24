# Torii 鳥居 — intake layer of Meisei

> **Meisei** 明晰 (“clarity”) is an open pipeline that carries raw intent through
> understanding → decision → plan → action to a finished result.

[![Meisei](https://img.shields.io/badge/meisei-明晰-1f2937.svg)](https://meisei.ru)
[![License: Apache-2.0 WITH Commons-Clause](https://img.shields.io/badge/license-Apache--2.0%20WITH%20Commons--Clause-blue.svg)](LICENSE)

<sub>
<b>torii</b> · satori · enma · yatagarasu · fujin · daruma
&nbsp;—&nbsp; <b>intake</b> · sensemaking · decisions · planning · actions · execution (terminal)
</sub>

## What it is

Torii is the **intake** layer of the Meisei pipeline: the single entry point where
raw material enters as a typed [`RawItem`]. Its AI operation (`parse`) turns natural
language input into a structured `TaskDraft` through a provider-neutral
`AiProvider` seam. The intake layer **never writes to storage** — the parse result
is returned to the caller (the host), which dispatches it onto the execution layer
(daruma). The crate has no dependency on daruma or sibling layers; adapters live
only inside the host.

## Repository layout

- `src/` — the `torii` library: RawItem primitives, `parse_task`, prompt registry,
  error types.
- `server/` — `torii-server`, a thin, independently-deployed HTTP/MCP wrapper over
  the library (the axum/tokio scaffold comes from [`layer-kit`](../layer-kit)).
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

Apache-2.0 WITH Commons-Clause — see [LICENSE](LICENSE) and
[LICENSE.commons-clause.md](LICENSE.commons-clause.md).
