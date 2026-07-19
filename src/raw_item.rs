//! `RawItem` — универсальный примитив приёма входящего сырья (Intake layer).
//!
//! Сущность не знает ничего о предметной области; никакой специфики
//! цифровых продуктов. Intake только принимает сырьё, обогащает его
//! минимальным контекстом и передаёт в Sensemaking.
//!
//! # Жизненный цикл
//!
//! ```text
//! raw  ──→  needs_review  ──→  linked
//!  └──────────────────────────────↑
//!         (прямая маршрутизация)
//! ```
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

/// Статус обработки [`RawItem`] в слое Intake.
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

    /// `true` если элемент уже привязан (терминальный для Intake).
    pub fn is_linked(self) -> bool {
        matches!(self, RawItemStatus::Linked)
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
    /// Установить статус `needs_review`. Нет эффекта если уже `linked`.
    pub fn flag_needs_review(&mut self) {
        if !self.status.is_linked() {
            self.status = RawItemStatus::NeedsReview;
            self.updated_at = time::now();
        }
    }

    /// Привязать к цели/контексту и перевести в `linked`.
    ///
    /// Возвращает [`IntakeEvent::RawItemRouted`] для публикации.
    pub fn route_to(&mut self, destination: impl Into<String>) -> IntakeEvent {
        let dest = destination.into();
        self.link = Some(ItemLink::new(&dest));
        self.status = RawItemStatus::Linked;
        self.updated_at = time::now();
        IntakeEvent::RawItemRouted(RawItemRouted {
            item_id: self.id,
            destination: dest,
            occurred_at: self.updated_at,
            actor: None,
        })
    }
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
    pub fn new(
        source: impl Into<String>,
        kind: RawItemKind,
        body: impl Into<String>,
    ) -> Self {
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

// ── Sparse update ─────────────────────────────────────────────────────────────

/// Разреженное обновление [`RawItem`]. `None` = не менять.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawItemPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Option<ItemLink>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<RawItemStatus>,
}

impl RawItemPatch {
    pub fn is_empty(&self) -> bool {
        self.body.is_none() && self.link.is_none() && self.status.is_none()
    }

    /// Применить патч к элементу. Возвращает событие обновления.
    pub fn apply(self, item: &mut RawItem) -> Option<IntakeEvent> {
        if self.is_empty() {
            return None;
        }
        if let Some(b) = self.body {
            item.body = b;
        }
        if let Some(l) = self.link {
            item.link = l;
        }
        if let Some(s) = self.status {
            item.status = s;
        }
        item.updated_at = time::now();
        Some(IntakeEvent::RawItemUpdated(RawItemUpdated {
            item_id: item.id,
            occurred_at: item.updated_at,
            actor: None,
        }))
    }
}

// ── События жизненного цикла ──────────────────────────────────────────────────

/// Опциональный источник действия (агент или пользователь).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IntakeActor {
    pub kind: IntakeActorKind,
    /// Непрозрачный строковый идентификатор (user-id, agent-id и т.п.).
    pub id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntakeActorKind {
    User,
    Agent,
}

/// Событие создания [`RawItem`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawItemCreated {
    pub item_id: RawItemId,
    pub occurred_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<IntakeActor>,
}

/// Событие обновления [`RawItem`] (body / link / status изменились).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawItemUpdated {
    pub item_id: RawItemId,
    pub occurred_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<IntakeActor>,
}

/// Событие маршрутизации [`RawItem`] в следующий слой (Sensemaking и т.п.).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawItemRouted {
    pub item_id: RawItemId,
    /// URI назначения (например `sensemaking://default`).
    pub destination: String,
    pub occurred_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<IntakeActor>,
}

/// Событие жизненного цикла [`RawItem`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IntakeEvent {
    RawItemCreated(RawItemCreated),
    RawItemUpdated(RawItemUpdated),
    RawItemRouted(RawItemRouted),
}

/// Создать элемент и вернуть пару (item, created-event).
pub fn create_raw_item(input: NewRawItem, actor: Option<IntakeActor>) -> (RawItem, IntakeEvent) {
    let item = input.build();
    let event = IntakeEvent::RawItemCreated(RawItemCreated {
        item_id: item.id,
        occurred_at: item.created_at,
        actor,
    });
    (item, event)
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
    fn new_raw_item_defaults_to_raw_status() {
        let item = NewRawItem::new("user://alice", RawItemKind::Text, "купить молоко").build();
        assert_eq!(item.status, RawItemStatus::Raw);
        assert_eq!(item.source, "user://alice");
        assert_eq!(item.body, "купить молоко");
        assert!(item.link.is_none());
    }

    #[test]
    fn flag_needs_review_changes_status() {
        let mut item =
            NewRawItem::new("user://bob", RawItemKind::Text, "что-то непонятное").build();
        assert_eq!(item.status, RawItemStatus::Raw);
        item.flag_needs_review();
        assert_eq!(item.status, RawItemStatus::NeedsReview);
    }

    #[test]
    fn flag_needs_review_noop_on_linked() {
        let mut item = NewRawItem::new("user://bob", RawItemKind::Text, "уже привязано").build();
        item.status = RawItemStatus::Linked;
        item.flag_needs_review();
        // остаётся linked, не деградирует
        assert_eq!(item.status, RawItemStatus::Linked);
    }

    #[test]
    fn route_to_sets_linked_and_returns_event() {
        let mut item = NewRawItem::new("webhook://gh", RawItemKind::Event, "{}").build();
        let event = item.route_to("sensemaking://default");
        assert_eq!(item.status, RawItemStatus::Linked);
        assert_eq!(
            item.link.as_ref().map(|l| l.target.as_str()),
            Some("sensemaking://default")
        );
        match event {
            IntakeEvent::RawItemRouted(r) => {
                assert_eq!(r.item_id, item.id);
                assert_eq!(r.destination, "sensemaking://default");
            }
            other => panic!("ожидалось RawItemRouted, получено: {other:?}"),
        }
    }

    #[test]
    fn create_raw_item_emits_created_event() {
        let input = NewRawItem::new("api://v1", RawItemKind::Document, r#"{"key":"val"}"#);
        let (item, event) = create_raw_item(input, None);
        match event {
            IntakeEvent::RawItemCreated(c) => assert_eq!(c.item_id, item.id),
            other => panic!("ожидалось RawItemCreated, получено: {other:?}"),
        }
    }

    #[test]
    fn raw_item_patch_apply_emits_updated_event() {
        let mut item = NewRawItem::new("user://x", RawItemKind::Text, "старый текст").build();
        let patch = RawItemPatch {
            body: Some("новый текст".to_owned()),
            ..Default::default()
        };
        let event = patch.apply(&mut item);
        assert_eq!(item.body, "новый текст");
        assert!(matches!(event, Some(IntakeEvent::RawItemUpdated(_))));
    }

    #[test]
    fn raw_item_patch_empty_returns_none() {
        let mut item = NewRawItem::new("user://x", RawItemKind::Text, "текст").build();
        let patch = RawItemPatch::default();
        let event = patch.apply(&mut item);
        assert!(event.is_none());
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
    fn intake_event_serde_tagged() {
        let event = IntakeEvent::RawItemRouted(RawItemRouted {
            item_id: RawItemId::new(),
            destination: "sensemaking://default".to_owned(),
            occurred_at: time::now(),
            actor: None,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"raw_item_routed\""), "got: {json}");
        // round-trip
        let decoded: IntakeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn raw_item_with_link_builder() {
        let item = NewRawItem::new("user://carol", RawItemKind::Reference, "https://example.com")
            .with_link("goal://g_01")
            .build();
        assert_eq!(item.link.as_ref().unwrap().target, "goal://g_01");
        assert_eq!(item.status, RawItemStatus::Raw);
    }
}
