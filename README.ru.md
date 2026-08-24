# Torii 鳥居 — intake-слой Meisei

> **Meisei** 明晰 («ясность») — открытый конвейер, который проводит сырой замысел
> через понимание → решение → план → действие к готовому результату.

[![Meisei](https://img.shields.io/badge/meisei-明晰-1f2937.svg)](https://meisei.ru)
[![License: Apache-2.0 WITH Commons-Clause](https://img.shields.io/badge/license-Apache--2.0%20WITH%20Commons--Clause-blue.svg)](LICENSE)

<sub>
<b>torii</b> · satori · enma · yatagarasu · fujin · daruma
&nbsp;—&nbsp; <b>intake</b> · осмысление · решения · планирование · действия · исполнение (терминальный слой)
</sub>

## Что это

Torii — **intake**-слой конвейера MeiSei: единственная точка входа, куда сырьё
попадает в виде типизированного [`RawItem`]. AI-операция (`parse`) превращает
естественно-языковой ввод в структурированный `TaskDraft` через
провайдер-нейтральный шов `AiProvider`. Intake-слой **никогда не пишет в
хранилище** — результат парсинга возвращается вызывающему (host), который
доставляет его на слой исполнения (daruma). Крейт не зависит от daruma и
соседних слоёв; адаптеры живут только внутри host.

## Структура репозитория

- `src/` — библиотека `torii`: примитивы RawItem, `parse_task`, реестр промптов,
  типы ошибок.
- `server/` — `torii-server`, тонкая независимо развёртываемая HTTP/MCP-обёртка
  над библиотекой (axum/tokio-каркас — из [`layer-kit`](../layer-kit)).
- `deploy/` — release-`build.sh` (прошивает git SHA в `/healthz`) и systemd user unit.

## Сборка и запуск

```sh
cargo run -p torii-server
# GET  /healthz   — открытая проба живости/версии
# POST /v1/mcp    — MCP-поверхность под платформенным токеном:
#                   torii.ingest_raw, torii.parse, torii.list_raw
```

Для продовых сборок используйте `deploy/build.sh`, чтобы `/healthz` отдавал
реальный git SHA, а не `"dev"`.

## Конфигурация (env)

| Переменная | По умолчанию | Назначение |
| --- | --- | --- |
| `TORII_PORT` | `8090` | HTTP-порт |
| `TORII_PLATFORM_SECRET` | не задан | HMAC-ключ; если не задан, `/v1/mcp` закрыт |
| `TORII_VERSION` | версия крейта | Версия, отдаваемая `/healthz` |
| `TORII_DB` | `./torii.db` | Путь к SQLite-хранилищу (`layer_kit::store::Store`) |
| `OPENAI_API_KEY` | не задан | Опциональный AI-провайдер для `torii.parse`; без ключа — ответ `ai_not_configured` (503) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Базовый URL OpenAI-совместимого API |
| `OPENAI_MODEL` | `gpt-4.1` | Модель, используемая AI-операцией |

## Документация

Канон конвейера и контракты слоёв: https://meisei.ru/docs

## Лицензия

Apache-2.0 WITH Commons-Clause — см. [LICENSE](LICENSE) и
[LICENSE.commons-clause.md](LICENSE.commons-clause.md).
