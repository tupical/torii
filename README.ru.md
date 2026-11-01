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
попадает в виде типизированного `RawItem`. Библиотека строит снимки `RawItem`;
`torii.ingest_raw` сохраняет их в SQLite-хранилище сервера, `torii.list_raw`
читает. Новый элемент всегда `raw`: методов правки, ревью, маршрутизации и
журнала событий в сервере нет.

Единственный выход Intake — `RawItem`, переданный в Sensemaking. Сырьё здесь не
становится задачей: типа задачи, AI-операции и «доставки в daruma» в слое нет —
задачи материализует только план. Прежний метод `torii.parse` (свободный текст →
черновик задачи) удалён по ADR-0014 MeiSei («Intake без черновика задачи,
терминальный статус `Suppressed`») и отвечает `unknown_method`.

У `RawItemStatus` два взаимоисключающих терминальных значения: `linked` и
`suppressed`. Хост, решивший, что элемент — шум, вызывает
`RawItem::suppress(reason)`: элемент становится `suppressed`, а вызов возвращает
след `IntakeEvent::RawItemSuppressed { item_id, reason, occurred_at }`
(wire-тег `raw_item_suppressed`), который хост персистирует. Пустая причина или
уже терминальный элемент — `IntakeError::Validation`; `RawItem::ensure_routable`
не пропускает отсеянный элемент. Повторное открытие — явная запись статуса
`raw`/`needs_review`. Слой даёт только состояние и переход — порогов и скоринга
шума в нём нет, метода отсева в сервере пока нет. Крейт не зависит от daruma и
соседних слоёв и сам не делает storage I/O.

## Структура репозитория

- `src/` — библиотека `torii`: примитивы RawItem (статус, `suppress`,
  `IntakeEvent`), реэкспорт шва `AiProvider`, типы ошибок.
- `server/` — `torii-server`, тонкая независимо развёртываемая HTTP/MCP-обёртка
  над библиотекой (axum/tokio-каркас — из [`layer-kit`](../layer-kit)).
- `deploy/` — release-`build.sh` (прошивает git SHA в `/healthz`) и systemd user unit.

## Сборка и запуск

```sh
cargo run -p torii-server
# GET  /healthz   — открытая проба живости/версии
# POST /v1/mcp    — MCP-поверхность под платформенным токеном:
#                   torii.ingest_raw, torii.list_raw
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

## Документация

Канон конвейера и контракты слоёв: https://meisei.ru/docs

## Лицензия

Apache-2.0 WITH Commons-Clause — см. [LICENSE](LICENSE) и
[LICENSE.commons-clause.md](LICENSE.commons-clause.md).
