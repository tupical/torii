//! `torii` — the Intake layer skeleton.
//!
//! A self-contained open-core skeleton: it defines its own primitives and a
//! provider-neutral [`AiProvider`] seam. It has **no** dependency on daruma
//! and **no** dependency on sibling `*_oss` layers. the host supplies the
//! concrete AI provider and any daruma adapters when wiring the layer into
//! its architecture — implementations live only inside the host.
//!
//! # Contract
//! - The library performs no storage I/O. The server persists raw snapshots.
//! - The only output of Intake is a [`RawItem`] handed to Sensemaking. Raw
//!   material never becomes a task here: there is no task type and no
//!   "dispatch onto daruma" path (ADR-0014; tasks are materialized only by a
//!   plan, ADR-0007).
//! - [`RawItemStatus::Suppressed`] is the terminal "screened out" state;
//!   [`RawItem::suppress`] sets it and returns an event carrying the reason.
//!   Who decides what is noise is the caller's business, not the layer's.
//! - All JSON is built with [`serde_json::json!`]; no string concatenation.
//! - Errors propagate as [`IntakeError`].

pub mod ai;
pub mod error;
pub mod raw_item;
pub mod time;

// ── Seam re-exports ─────────────────────────────────────────────────────────────
pub use ai::{wrap_untrusted, AiError, AiOutput, AiProvider, AiRequest, AiUsage, ToolCall};
pub use error::IntakeError;
pub use time::Timestamp;

// ── RawItem re-exports ──────────────────────────────────────────────────────────
pub use raw_item::{
    IntakeEvent, ItemLink, NewRawItem, RawItem, RawItemId, RawItemKind, RawItemStatus,
    RawItemSuppressed,
};
