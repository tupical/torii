//! Intake prompt catalogue.
//!
//! The prompt *rendering engine* and the shared [`SharedRegistry`] live in
//! `taskagent-ai-infra`. This module only declares the catalogue of
//! intake-operation prompts — one `prompts/*.toml` per operation (parse) —
//! because those prompts are operational, not infrastructure.
//!
//! All known prompts are baked into the binary via `include_str!`; the
//! first [`PromptRegistry::load`] call parses them.
//!
//! ```ignore
//! use serde::Serialize;
//! use intake_oss::prompts::PromptRegistry;
//!
//! #[derive(Serialize)]
//! struct ParseCtx<'a> { input: &'a str }
//!
//! let s = PromptRegistry::load("parse", "default", &ParseCtx { input: "buy milk" })?;
//! ```

use once_cell::sync::Lazy;
use serde::Serialize;
use taskagent_ai_infra::prompts::PromptRegistry as SharedRegistry;
use taskagent_shared::CoreError;

static PROMPTS: Lazy<SharedRegistry> =
    Lazy::new(|| SharedRegistry::new(&[("parse", include_str!("../prompts/parse.toml"))]));

/// Process-wide catalogue of intake prompts. All sources are baked into
/// the binary via `include_str!`; the first `load` call parses them.
pub struct PromptRegistry;

impl PromptRegistry {
    /// Render `name` / `variant` against `params`. See
    /// [`SharedRegistry::load`] for error semantics.
    pub fn load<P: Serialize>(name: &str, variant: &str, params: &P) -> Result<String, CoreError> {
        PROMPTS.load(name, variant, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_prompt_loads() {
        for (name, _file) in PROMPTS.iter() {
            assert!(!name.is_empty());
        }
        assert!(!PROMPTS.is_empty(), "no prompts loaded");
    }

    #[test]
    fn parse_default_substitutes_input() {
        #[derive(Serialize)]
        struct Ctx<'a> {
            input: &'a str,
        }
        let s = PromptRegistry::load("parse", "default", &Ctx { input: "buy milk" }).unwrap();
        assert!(s.contains("buy milk"), "{s}");
        assert!(s.contains("create_task"), "{s}");
    }
}
