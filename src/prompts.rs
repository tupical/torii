//! Intake prompt catalogue.
//!
//! The prompt *rendering engine* lives in `taskagent-ai-infra`
//! ([`PromptFile`] / [`render_variant`]). This module owns the catalogue
//! of intake-operation prompts — one `prompts/*.toml` per operation
//! (parse) — because those prompts are operational, not infrastructure.
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

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::Serialize;
use taskagent_ai_infra::prompts::{render_variant, PromptFile};
use taskagent_shared::CoreError;

/// Process-wide catalogue of intake prompts. All sources are baked into
/// the binary via `include_str!`; the first `load` call parses them.
pub struct PromptRegistry;

static PROMPTS: Lazy<HashMap<&'static str, PromptFile>> = Lazy::new(|| {
    let raw: &[(&str, &str)] = &[("parse", include_str!("../prompts/parse.toml"))];
    let mut out = HashMap::with_capacity(raw.len());
    for (name, body) in raw {
        let parsed = PromptFile::parse(name, body)
            .unwrap_or_else(|e| panic!("prompts/{name}.toml: {e}"));
        out.insert(*name, parsed);
    }
    out
});

impl PromptRegistry {
    /// Render `name` / `variant` against `params`. `params` must implement
    /// `Serialize`; tinytemplate accesses fields via dot-notation.
    ///
    /// `CoreError::Validation` is returned for an unknown prompt or
    /// variant. `CoreError::Ai` wraps tinytemplate render failures (e.g.
    /// a referenced variable that the params struct doesn't expose).
    pub fn load<P: Serialize>(name: &str, variant: &str, params: &P) -> Result<String, CoreError> {
        let prompt = PROMPTS
            .get(name)
            .ok_or_else(|| CoreError::validation(format!("unknown prompt: {name}")))?;
        render_variant(name, variant, prompt, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Empty {}

    #[test]
    fn every_bundled_prompt_loads() {
        for (name, _file) in PROMPTS.iter() {
            assert!(!name.is_empty());
        }
        assert!(!PROMPTS.is_empty(), "no prompts loaded");
    }

    #[test]
    fn unknown_prompt_returns_validation_error() {
        let err = PromptRegistry::load("does_not_exist", "default", &Empty {}).unwrap_err();
        assert_eq!(err.code(), "validation");
    }

    #[test]
    fn unknown_variant_returns_validation_error() {
        let err = PromptRegistry::load("parse", "no_such_variant", &Empty {}).unwrap_err();
        assert_eq!(err.code(), "validation");
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
