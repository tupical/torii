//! `torii` — the Intake layer skeleton.
//!
//! A self-contained open-core skeleton: it defines its own primitives,
//! domain output types, and a provider-neutral [`AiProvider`] seam. It has
//! **no** dependency on daruma and **no** dependency on sibling `*_oss`
//! layers. the host supplies the concrete AI provider and any daruma
//! adapters when wiring the layer into its architecture — implementations
//! live only inside the host.
//!
//! # Contract
//! - The library performs no storage I/O. The server persists raw snapshots.
//! - `parse` returns a [`TaskDraft`] through the standalone MCP method;
//!   it does not create a Daruma task or advance the maturity pipeline.
//! - Raw snapshots have no update/routing API or event journal.
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
pub use raw_item::{ItemLink, NewRawItem, RawItem, RawItemId, RawItemKind, RawItemStatus};
