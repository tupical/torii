//! `intake-oss` — the Intake layer extracted from the TaskAgent OSS core.
//!
//! This crate owns the AI **intake** operation — `parse` (natural language
//! → `Command::CreateTask`) — on top of the provider-neutral
//! [`taskagent_ai_infra`] infrastructure (Responses API client,
//! [`AiProvider`] abstraction, prompt rendering engine, tool schemas,
//! prompt-injection hardening).
//!
//! It is a Wave-2b sibling of `planning-oss`: the operation and its prompt
//! moved out of `taskagent/crates/ai` into this separate repository, while
//! the shared infrastructure is consumed read-only via the `vendor/oss`
//! symlink (mirroring the `mcpbox.ru` vendoring pattern).
//!
//! # Contract (inherited from the core AI layer)
//! - The intake layer **never** writes to storage. Every output is a
//!   [`taskagent_core::Command`]; the caller dispatches it.
//! - All JSON is built with [`serde_json::json!`]; no string concatenation.
//! - Errors propagate as [`taskagent_shared::CoreError`].

pub mod parse;
pub mod prompts;

// ── Re-export the infrastructure layer ─────────────────────────────────────────
//
// Preserves the operation crate's public surface (`OpenAiClient`,
// `AiConfig`, `AiProvider`, …) so callers depend on `intake_oss::*`
// without also naming `taskagent_ai_infra`.
pub use taskagent_ai_infra::{wrap_untrusted, AiConfig, AiError, AiProvider, OpenAiClient};

// The intake prompt catalogue, owned by this crate.
pub use prompts::PromptRegistry;

// ── Operation re-exports ────────────────────────────────────────────────────────

pub use parse::parse_task;
