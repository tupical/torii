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
//! Библиотека не публикует события: журнал приёма здесь не реализован.
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
        Ok(match String::deserialize(deserializer)?.as_str() {
            "document" => Self::Document,
            "reference" => Self::Reference,
            "binary" => Self::Binary,
            "event" => Self::Event,
            _ => Self::Text,
        })
    }
}

impl RawItemKind {
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
/// остальные варианты сохранены для чтения исторических payload.
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
}

impl RawItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RawItemStatus::Raw => "raw",
            RawItemStatus::NeedsReview => "needs_review",
            RawItemStatus::Linked => "linked",
        }
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
    fn raw_item_kind_maps_semantic_kinds_to_text() {
        for kind in ["idea", "security_plan", "implementation_plan"] {
            assert_eq!(
                serde_json::from_str::<RawItemKind>(&format!("\"{kind}\"")).unwrap(),
                RawItemKind::Text
            );
        }
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
}
