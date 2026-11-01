//! Intake's slice of the AI provider seam: re-exports the shared
//! [`layer_kit::ai`] infrastructure. Intake itself has no AI operation
//! (ADR-0014 removed `parse`); the seam stays so a host can wire one in
//! without a new dependency.

pub use layer_kit::ai::wrap_untrusted;
pub use layer_kit::ai::{
    AiError, AiOutput, AiProvider, AiRequest, AiUsage, ToolCall, UNTRUSTED_CLOSE, UNTRUSTED_OPEN,
};
