---
title: "Torii intake server"
audience: "developer"
intent: "reference"
owner: "MeiSei"
status: "active"
source_of_truth: "README.md"
last_verified: "2026-11-01"
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
object store and `torii.list_raw` reads them. New items stay `raw`: the server
has no edit, review, routing, or intake-event journal API. `source` is
caller-supplied context, not authenticated actor evidence; the layer token
carries workspace/project, not a user/agent identity.

The only output of Intake is a `RawItem` handed to Sensemaking. Raw material
never becomes a task here: there is no task type, no AI operation, and no
"dispatch onto Daruma" path — tasks are materialized only by a plan. The former
`torii.parse` method (free text → task draft) was removed by Meisei ADR-0014
("Intake without a task draft; terminal `Suppressed` status") and now answers
`unknown_method`.

`RawItemStatus` has two mutually exclusive terminal values: `linked` and
`suppressed`. A host that decides an item is noise calls
`RawItem::suppress(reason)`: the item becomes `suppressed` and the call returns
an `IntakeEvent::RawItemSuppressed { item_id, reason, occurred_at }` trace
(wire tag `raw_item_suppressed`) for the host to persist. An empty reason or an
already terminal item is refused with `IntakeError::Validation`;
`RawItem::ensure_routable` refuses a suppressed item. Reopening is an explicit
status write back to `raw`/`needs_review`. The layer only provides the state and
the transition — it holds no threshold or scoring of what counts as noise, and
the server does not expose a suppress method yet. The library has no dependency
on Daruma or sibling layers and performs no storage I/O itself.

## Repository layout

- `src/` — the `torii` library: RawItem primitives (status, `suppress`,
  `IntakeEvent`), the re-exported `AiProvider` seam, error types.
- `server/` — `torii-server`, a thin, independently-deployed HTTP/MCP wrapper over
  the library (the axum/tokio scaffold comes from `layer-kit`).
- `deploy/` — release `build.sh` (stamps the git SHA into `/healthz`) and a
  systemd user unit.

## Build & run

```sh
cargo run -p torii-server
# GET  /healthz   — open liveness/version probe
# POST /v1/mcp    — platform-token gated MCP surface:
#                   torii.ingest_raw, torii.list_raw
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

## Docs

Pipeline canon and layer contracts: https://meisei.ru/docs

## License

Apache-2.0 WITH Commons-Clause — see [LICENSE](https://github.com/tupical/torii/blob/main/LICENSE) and
[LICENSE.commons-clause.md](https://github.com/tupical/torii/blob/main/LICENSE.commons-clause.md).
