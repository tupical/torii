//! `RawItem` — универсальный примитив приёма входящего сырья (Intake layer).
//!
//! Сущность не знает ничего о предметной области; никакой специфики
//! цифровых продуктов. Intake только принимает сырьё, обогащает его
//! минимальным контекстом и передаёт в Sensemaking.
//!
//! # Достижимый контракт
//!
//! MCP принимает и перечисляет сохранённые снимки. Новый элемент всегда `raw`;
//! методов изменения, модерации и маршрутизации в сервере нет. Исторические
//! статусы читаются для совместимости, но не означают работающий lifecycle.
//!
//! Терминальных статусов два, и они взаимоисключающи: `linked` (передано
//! дальше) и `suppressed` (отсеяно, ADR-0014):
//!
//! ```text
//! raw ──▶ needs_review ──▶ linked
//!   └──────────┴─────────▶ suppressed   (RawItem::suppress, с причиной)
//! ```
//!
//! Отсев выставляет вызывающая сторона (человек, агент, хост) — ядро не
//! решает, что является шумом, и не содержит порогов или скоринга.
//! [`RawItem::suppress`] возвращает событие-след [`RawItemSuppressed`] с
//! обязательной причиной: причина живёт в событии, а не в `RawItem`, и
//! персистирует его хост. Отсеянный элемент маршрутизировать нельзя —
//! [`RawItem::ensure_routable`] отвечает ошибкой. Повторное открытие — явная
//! запись статуса (`item.status = RawItemStatus::Raw`), отдельного API нет.
//! Журнала приёма в библиотеке нет; актор не аутентифицирован и в событие не
//! пишется.
//!
//! # Пример
//!
//! ```rust
//! use torii::raw_item::{NewRawItem, RawItemKind, RawItemStatus};
//!
//! let item = NewRawItem::new("user://alice", RawItemKind::Text, "купить молоко")
//!     .build();
//! assert_eq!(item.status, RawItemStatus::Raw);
//! ```

use crate::error::IntakeError;
use crate::time::{self, Timestamp};
use serde::{Deserialize, Serialize};

// ── ID ────────────────────────────────────────────────────────────────────────

layer_kit::newtype_id! {
    /// Strongly-typed UUIDv7 identifier for a [`RawItem`].
    pub struct RawItemId("ri");
}

// ── Тип входящего сырья ───────────────────────────────────────────────────────

/// Тип входящего материала. Intake не знает предметных деталей — только
/// структурную форму поступающего сырья.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RawItemKind {
    /// Свободный текст (идея, заметка, диалог).
    Text,
    /// Структурированный документ (JSON, TOML, YAML и т.п.).
    Document,
    /// Ссылка на внешний ресурс (URL, путь к файлу).
    Reference,
    /// Двоичные данные (изображение, аудио, прочее).
    Binary,
    /// Событие из внешней системы (webhook-payload и т.п.).
    Event,
}

// keep arms in sync with RawItemKind variants
impl<'de> Deserialize<'de> for RawItemKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_input(&s).map(|(k, _)| k).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown kind `{s}`: expected text, document, reference, binary, event \
                 (aliases idea, note, implementation_plan, security_plan map to text)"
            ))
        })
    }
}

impl RawItemKind {
    /// Разбор входного `kind`. Второй элемент — `true`, если значение было алиасом
    /// (семантический вид → `Text`), чтобы вызывающий сообщил о нормализации.
    /// Неизвестное значение → `None` (раньше молча превращалось в `Text`).
    pub fn from_input(s: &str) -> Option<(Self, bool)> {
        Some(match s {
            "text" => (Self::Text, false),
            "document" => (Self::Document, false),
            "reference" => (Self::Reference, false),
            "binary" => (Self::Binary, false),
            "event" => (Self::Event, false),
            "idea" | "note" | "implementation_plan" | "security_plan" => (Self::Text, true),
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RawItemKind::Text => "text",
            RawItemKind::Document => "document",
            RawItemKind::Reference => "reference",
            RawItemKind::Binary => "binary",
            RawItemKind::Event => "event",
        }
    }
}

// ── Статус ────────────────────────────────────────────────────────────────────

/// Сериализованный статус [`RawItem`]. Сервер создаёт только `Raw`;
/// `NeedsReview`/`Linked` сохранены для чтения исторических payload,
/// `Suppressed` выставляет хост через [`RawItem::suppress`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawItemStatus {
    /// Только что принято, ещё не обработано.
    #[default]
    Raw,
    /// Требует ручной проверки перед маршрутизацией.
    NeedsReview,
    /// Привязано к цели/контексту и передано в Sensemaking.
    Linked,
    /// Отсеяно до Sensemaking (шум, дубль, бот …); терминально, причина — в
    /// событии [`RawItemSuppressed`].
    Suppressed,
}

impl RawItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RawItemStatus::Raw => "raw",
            RawItemStatus::NeedsReview => "needs_review",
            RawItemStatus::Linked => "linked",
            RawItemStatus::Suppressed => "suppressed",
        }
    }

    /// `true` для конечных статусов: `Linked` и `Suppressed`.
    pub fn is_terminal(self) -> bool {
        matches!(self, RawItemStatus::Linked | RawItemStatus::Suppressed)
    }
}

// ── Привязка к цели / контексту ───────────────────────────────────────────────

/// Опциональная привязка RawItem к цели или контексту.
///
/// Хранит строку-URI назначения (например `goal://g_<uuid>` или
/// `context://project/<slug>`). Намеренно непрозрачна — Intake не
/// интерпретирует семантику URI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ItemLink {
    /// Непрозрачный URI цели или контекста.
    pub target: String,
}

impl ItemLink {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
        }
    }
}

// ── Основная сущность ─────────────────────────────────────────────────────────

/// Сырой элемент, принятый слоем Intake.
///
/// Не привязан к предметной области. Содержит только:
/// - откуда пришёл (`source`),
/// - в какой форме (`kind`),
/// - что именно (`body`),
/// - к чему привязан (`link`),
/// - текущий статус обработки (`status`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawItem {
    pub id: RawItemId,
    /// URI источника (например `user://alice`, `webhook://gh/push`).
    pub source: String,
    /// Структурная форма сырья.
    pub kind: RawItemKind,
    /// Содержимое: текст, сериализованный документ, URL и т.д.
    pub body: String,
    /// Опциональная привязка к цели или контексту.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ItemLink>,
    /// Текущий статус в слое Intake.
    pub status: RawItemStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl RawItem {
    /// Отсеять элемент: статус `Suppressed`, `updated_at` = сейчас, событие-след
    /// с причиной.
    ///
    /// Тихого отсева нет: пустая причина — `IntakeError::Validation`. Элемент в
    /// терминальном статусе (`Linked` уже передан дальше, `Suppressed` уже
    /// отсеян) не меняется и тоже даёт `Validation`.
    pub fn suppress(&mut self, reason: impl Into<String>) -> Result<IntakeEvent, IntakeError> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(IntakeError::validation(
                "suppress requires a non-empty reason",
            ));
        }
        if self.status.is_terminal() {
            return Err(IntakeError::validation(format!(
                "raw item {} is already {}; only a non-terminal item can be suppressed",
                self.id,
                self.status.as_str()
            )));
        }
        let now = time::now();
        self.status = RawItemStatus::Suppressed;
        self.updated_at = now;
        Ok(IntakeEvent::RawItemSuppressed(RawItemSuppressed {
            item_id: self.id,
            reason,
            occurred_at: now,
        }))
    }

    /// Проверка перед передачей в Sensemaking: отсеянный элемент
    /// маршрутизировать нельзя (`IntakeError::Validation`), остальные статусы
    /// проходят.
    pub fn ensure_routable(&self) -> Result<(), IntakeError> {
        if self.status == RawItemStatus::Suppressed {
            return Err(IntakeError::validation(format!(
                "raw item {} is suppressed and cannot be routed; reopen it first",
                self.id
            )));
        }
        Ok(())
    }
}

// ── События ───────────────────────────────────────────────────────────────────

/// Событие-след отсева [`RawItem`]. Хост персистирует его так же, как
/// персистирует сам элемент.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawItemSuppressed {
    pub item_id: RawItemId,
    /// Почему элемент отсеян. Свободный текст, всегда непустой.
    pub reason: String,
    pub occurred_at: Timestamp,
}

/// Событие слоя Intake. Wire-форма — tagged по `type`
/// (`"raw_item_suppressed"`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IntakeEvent {
    RawItemSuppressed(RawItemSuppressed),
}

// ── Входные данные для создания ───────────────────────────────────────────────

/// Входные данные для создания нового [`RawItem`].
pub struct NewRawItem {
    pub id: Option<RawItemId>,
    pub source: String,
    pub kind: RawItemKind,
    pub body: String,
    pub link: Option<ItemLink>,
}

impl NewRawItem {
    pub fn new(source: impl Into<String>, kind: RawItemKind, body: impl Into<String>) -> Self {
        Self {
            id: None,
            source: source.into(),
            kind,
            body: body.into(),
            link: None,
        }
    }

    /// Указать опциональную привязку к цели/контексту.
    pub fn with_link(mut self, target: impl Into<String>) -> Self {
        self.link = Some(ItemLink::new(target));
        self
    }

    /// Построить [`RawItem`] со статусом `raw`.
    pub fn build(self) -> RawItem {
        let now = time::now();
        RawItem {
            id: self.id.unwrap_or_default(),
            source: self.source,
            kind: self.kind,
            body: self.body,
            link: self.link,
            status: RawItemStatus::Raw,
            created_at: now,
            updated_at: now,
        }
    }
}

// ── Тесты ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_item_status_as_str_roundtrip() {
        let pairs = [
            (RawItemStatus::Raw, "raw"),
            (RawItemStatus::NeedsReview, "needs_review"),
            (RawItemStatus::Linked, "linked"),
            (RawItemStatus::Suppressed, "suppressed"),
        ];
        for (status, expected) in pairs {
            assert_eq!(status.as_str(), expected);
        }
    }

    #[test]
    fn raw_item_status_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&RawItemStatus::NeedsReview).unwrap(),
            "\"needs_review\""
        );
        assert_eq!(
            serde_json::to_string(&RawItemStatus::Raw).unwrap(),
            "\"raw\""
        );
        assert_eq!(
            serde_json::to_string(&RawItemStatus::Linked).unwrap(),
            "\"linked\""
        );
    }

    #[test]
    fn raw_item_kind_maps_semantic_aliases_to_text() {
        for kind in ["idea", "note", "security_plan", "implementation_plan"] {
            assert_eq!(
                serde_json::from_str::<RawItemKind>(&format!("\"{kind}\"")).unwrap(),
                RawItemKind::Text
            );
            assert_eq!(
                RawItemKind::from_input(kind),
                Some((RawItemKind::Text, true))
            );
        }
        assert_eq!(
            RawItemKind::from_input("text"),
            Some((RawItemKind::Text, false))
        );
    }

    #[test]
    fn raw_item_kind_rejects_unknown() {
        assert!(RawItemKind::from_input("zzz").is_none());
        assert!(serde_json::from_str::<RawItemKind>("\"zzz\"").is_err());
    }

    #[test]
    fn raw_item_kind_preserves_media_kinds() {
        for (kind, expected) in [
            ("text", RawItemKind::Text),
            ("document", RawItemKind::Document),
            ("reference", RawItemKind::Reference),
            ("binary", RawItemKind::Binary),
            ("event", RawItemKind::Event),
        ] {
            assert_eq!(
                serde_json::from_str::<RawItemKind>(&format!("\"{kind}\"")).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn raw_item_kind_roundtrips_all_variants() {
        for kind in [
            RawItemKind::Text,
            RawItemKind::Document,
            RawItemKind::Reference,
            RawItemKind::Binary,
            RawItemKind::Event,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(serde_json::from_str::<RawItemKind>(&json).unwrap(), kind);
        }
    }

    #[test]
    fn new_raw_item_defaults_to_raw_status() {
        let item = NewRawItem::new("user://alice", RawItemKind::Text, "купить молоко").build();
        assert_eq!(item.status, RawItemStatus::Raw);
        assert_eq!(item.source, "user://alice");
        assert_eq!(item.body, "купить молоко");
        assert!(item.link.is_none());
    }

    #[test]
    fn raw_item_id_display_has_prefix() {
        let id = RawItemId::new();
        assert!(id.to_string().starts_with("ri_"), "got: {id}");
    }

    #[test]
    fn raw_item_id_roundtrip() {
        let id = RawItemId::new();
        let parsed: RawItemId = id.to_string().parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn raw_item_with_link_builder() {
        let item = NewRawItem::new(
            "user://carol",
            RawItemKind::Reference,
            "https://example.com",
        )
        .with_link("goal://g_01")
        .build();
        assert_eq!(item.link.as_ref().unwrap().target, "goal://g_01");
        assert_eq!(item.status, RawItemStatus::Raw);
    }
    fn item() -> RawItem {
        NewRawItem::new("webhook://gh", RawItemKind::Event, "{}").build()
    }

    #[test]
    fn terminal_statuses_are_linked_and_suppressed() {
        assert!(!RawItemStatus::Raw.is_terminal());
        assert!(!RawItemStatus::NeedsReview.is_terminal());
        assert!(RawItemStatus::Linked.is_terminal());
        assert!(RawItemStatus::Suppressed.is_terminal());
    }

    #[test]
    fn suppress_sets_status_and_returns_event_with_reason() {
        for from in [RawItemStatus::Raw, RawItemStatus::NeedsReview] {
            let mut item = item();
            item.status = from;
            let before = item.updated_at;
            let event = item.suppress("bot noise").unwrap();
            assert_eq!(item.status, RawItemStatus::Suppressed);
            assert!(item.updated_at >= before);
            let IntakeEvent::RawItemSuppressed(e) = event;
            assert_eq!(e.item_id, item.id);
            assert_eq!(e.reason, "bot noise");
            assert_eq!(e.occurred_at, item.updated_at);
        }
    }

    #[test]
    fn suppress_requires_reason() {
        let mut item = item();
        for reason in ["", "   "] {
            assert!(matches!(
                item.suppress(reason),
                Err(IntakeError::Validation(_))
            ));
        }
        assert_eq!(item.status, RawItemStatus::Raw);
    }

    #[test]
    fn suppress_refuses_terminal_items() {
        for from in [RawItemStatus::Linked, RawItemStatus::Suppressed] {
            let mut item = item();
            item.status = from;
            let at = item.updated_at;
            assert!(matches!(
                item.suppress("dup"),
                Err(IntakeError::Validation(_))
            ));
            assert_eq!(item.status, from, "terminal status must not change");
            assert_eq!(item.updated_at, at);
        }
    }

    #[test]
    fn suppressed_item_is_not_routable_until_reopened() {
        let mut item = item();
        assert!(item.ensure_routable().is_ok());
        item.suppress("noise").unwrap();
        assert!(matches!(
            item.ensure_routable(),
            Err(IntakeError::Validation(_))
        ));
        // Повторное открытие — явная запись статуса.
        item.status = RawItemStatus::Raw;
        assert!(item.ensure_routable().is_ok());
        assert!(item.suppress("noise again").is_ok());
    }

    #[test]
    fn suppressed_status_and_event_serde_roundtrip() {
        assert_eq!(
            serde_json::to_string(&RawItemStatus::Suppressed).unwrap(),
            "\"suppressed\""
        );
        assert_eq!(
            serde_json::from_str::<RawItemStatus>("\"suppressed\"").unwrap(),
            RawItemStatus::Suppressed
        );
        let mut item = item();
        let event = item.suppress("duplicate of ri_x").unwrap();
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "raw_item_suppressed");
        assert_eq!(json["item_id"], serde_json::to_value(item.id).unwrap());
        assert_eq!(json["reason"], "duplicate of ri_x");
        assert_eq!(serde_json::from_value::<IntakeEvent>(json).unwrap(), event);

        let item_json = serde_json::to_value(&item).unwrap();
        assert_eq!(item_json["status"], "suppressed");
        assert_eq!(serde_json::from_value::<RawItem>(item_json).unwrap(), item);
    }
}
