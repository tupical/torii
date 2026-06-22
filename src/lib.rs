//! `intake-oss` — the Intake layer skeleton.
//!
//! A self-contained open-core skeleton: it defines its own primitives,
//! domain output types, and a provider-neutral [`AiProvider`] seam. It has
//! **no** dependency on taskagent and **no** dependency on sibling `*_oss`
//! layers. mcpbox supplies the concrete AI provider and any taskagent
//! adapters when wiring the layer into its architecture — implementations
//! live only inside mcpbox.
//!
//! # Contract
//! - The intake layer never writes to storage. `parse` returns a
//!   [`TaskDraft`]; the caller (mcpbox) dispatches it onto taskagent.
//! - All JSON is built with [`serde_json::json!`]; no string concatenation.
//! - Errors propagate as [`IntakeError`].

pub mod ai;
pub mod error;
pub mod parse;
pub mod prompts;
pub mod raw_item;
pub mod task;
pub mod time;

// ── Seam + operation re-exports ─────────────────────────────────────────────────
pub use ai::{
    create_task_tool, wrap_untrusted, AiError, AiOutput, AiProvider, AiRequest, ToolCall,
};
pub use error::IntakeError;
pub use parse::parse_task;
pub use prompts::PromptRegistry;
pub use task::{Priority, Status, TaskDraft};
pub use time::Timestamp;

// ── RawItem re-exports ──────────────────────────────────────────────────────────
pub use raw_item::{
    create_raw_item, IntakeActor, IntakeActorKind, IntakeEvent, ItemLink, NewRawItem, RawItem,
    RawItemCreated, RawItemId, RawItemKind, RawItemPatch, RawItemRouted, RawItemStatus,
    RawItemUpdated,
};
