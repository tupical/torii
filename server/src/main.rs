//! torii-server — thin, independently-deployed HTTP/MCP wrapper around the
//! `torii` intake lib. Its own deploy unit (own systemd service, own port).
//! Boundary-clean: no mcpbox dependency; the platform→tool auth contract and
//! the axum/tokio scaffold live in `layer_kit::{auth,serve}`.
//!
//! Routes:
//!   GET  /healthz   — open; liveness + version for the platform registry.
//!   POST /v1/mcp    — requires a valid platform token; real intake surface
//!                     (`torii.ingest_raw` builds a typed RawItem via the lib,
//!                     `torii.parse` runs the lib's NL→TaskDraft AI operation).
//!
//! Env: TORII_PORT (default 8090), TORII_PLATFORM_SECRET (HMAC key; if unset,
//! /v1/mcp is closed), TORII_VERSION (defaults to the crate version).
//! AI methods (`torii.parse`): OPENAI_API_KEY / OPENAI_BASE_URL /
//! OPENAI_MODEL (see `layer_kit::openai`); without a key they answer
//! `ai_not_configured`.

use axum::http::StatusCode;
use layer_kit::auth::Claims;
use layer_kit::openai::{AiConfig, OpenAiProvider};
use layer_kit::serve::{serve, McpHandler, ServeConfig};
use serde_json::json;
use torii::raw_item::{NewRawItem, RawItemKind};

const TOOL: &str = "torii";

/// Dispatches torii's MCP methods; owns the (optional) AI provider.
struct Handler {
    /// `None` when OPENAI_API_KEY is unset — AI methods then answer
    /// `ai_not_configured` instead of panicking at call time.
    ai: Option<OpenAiProvider>,
}

impl McpHandler for Handler {
    async fn dispatch(
        &self,
        _claims: &Claims,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
        dispatch(self.ai.as_ref(), method, params).await
    }

    fn tools(&self) -> Vec<serde_json::Value> {
        tools()
    }
}

/// Tool descriptors for `tools/list` — one per method actually handled by
/// [`dispatch`] (`torii.list_raw` is NOT_IMPLEMENTED, so it is omitted).
fn tools() -> Vec<serde_json::Value> {
    vec![
        json!({
            "name": "torii_ingest_raw",
            "description": "Build a typed RawItem from raw intake (source, kind, body, optional link).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "kind": {"type": "string"},
                    "body": {"type": "string"},
                    "link": {"type": "string"}
                },
                "required": ["source", "kind", "body"]
            }
        }),
        json!({
            "name": "torii_parse",
            "description": "NL→TaskDraft AI operation: parse free-form text into a task draft.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "input": {"type": "string"}
                },
                "required": ["input"]
            }
        }),
    ]
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();

    let ai = AiConfig::from_env().map(OpenAiProvider::new);
    if ai.is_none() {
        tracing::warn!("OPENAI_API_KEY unset — AI methods (torii.parse) will answer ai_not_configured");
    }

    serve(
        ServeConfig {
            tool: TOOL,
            default_port: 8090,
            default_version: env!("CARGO_PKG_VERSION"),
            git_sha: option_env!("GIT_SHA").unwrap_or("dev"),
        },
        Handler { ai },
    )
    .await;
}

/// Params for `torii.ingest_raw`. `kind` deserializes from the lib's
/// snake_case enum (`text`/`document`/`reference`/`binary`/`event`).
#[derive(serde::Deserialize)]
struct IngestParams {
    source: String,
    kind: RawItemKind,
    body: String,
    #[serde(default)]
    link: Option<String>,
}

/// Params for `torii.parse` — the lib's NL→[`TaskDraft`](torii::TaskDraft)
/// AI operation (`parse_task`).
#[derive(serde::Deserialize)]
struct ParseParams {
    /// Free-form natural-language task description.
    input: String,
}

/// Error when no AI provider is configured: an honest 503, not a panic.
fn ai_not_configured() -> (StatusCode, serde_json::Value) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        json!({"error": "ai_not_configured", "detail": "OPENAI_API_KEY not set; torii-server has no AI provider"}),
    )
}

/// Map a lib [`IntakeError`](torii::IntakeError) onto the wire: caller input
/// problems → 400, provider/upstream problems → 502.
fn ai_error(e: torii::IntakeError) -> (StatusCode, serde_json::Value) {
    match e {
        torii::IntakeError::Validation(m) => (
            StatusCode::BAD_REQUEST,
            json!({"error": "validation", "detail": m}),
        ),
        other => (
            StatusCode::BAD_GATEWAY,
            json!({"error": "ai_upstream", "detail": other.to_string()}),
        ),
    }
}

/// Pure MCP dispatch over the torii intake lib — no auth, no HTTP, so it is
/// unit-testable directly (AI methods get a fake `AiProvider` in tests).
/// `Ok` is the method result object; `Err` is an (HTTP status, error body)
/// pair. `torii` is a stateless OSS skeleton: it builds typed objects but
/// stores nothing, so read methods are unsupported.
async fn dispatch<P: torii::AiProvider>(
    ai: Option<&P>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    match method {
        "torii.ingest_raw" => {
            let p: IngestParams = serde_json::from_value(params).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            let mut draft = NewRawItem::new(p.source, p.kind, p.body);
            if let Some(target) = p.link {
                draft = draft.with_link(target);
            }
            // Real intake: a typed RawItem with its own id (provenance seed).
            Ok(json!({ "method": "torii.ingest_raw", "raw_item": draft.build() }))
        }
        "torii.parse" => {
            let p: ParseParams = serde_json::from_value(params).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            let Some(provider) = ai else {
                return Err(ai_not_configured());
            };
            // Real AI operation: NL text → a provider-neutral TaskDraft.
            let draft = torii::parse_task(provider, &p.input)
                .await
                .map_err(ai_error)?;
            Ok(json!({ "method": "torii.parse", "task_draft": draft }))
        }
        "torii.list_raw" => Err((
            StatusCode::NOT_IMPLEMENTED,
            json!({"error": "unsupported", "detail": "torii-server is stateless (OSS skeleton has no store); list_raw needs a storage adapter"}),
        )),
        other => Err((
            StatusCode::BAD_REQUEST,
            json!({"error": "unknown_method", "detail": other}),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer_kit::openai::OpenAiProvider;
    use torii::{AiError, AiOutput, AiRequest, ToolCall};

    /// Fake provider returning a fixed `create_task` call — lets dispatch
    /// tests exercise `torii.parse` without network.
    struct FakeParse {
        args: String,
    }

    impl torii::AiProvider for FakeParse {
        async fn respond(&self, _req: AiRequest) -> Result<Vec<AiOutput>, AiError> {
            Ok(vec![AiOutput::ToolCall(ToolCall {
                name: "create_task".into(),
                arguments: self.args.clone(),
            })])
        }
    }

    #[tokio::test]
    async fn ingest_raw_builds_typed_raw_item() {
        let out = dispatch(
            None::<&OpenAiProvider>,
            "torii.ingest_raw",
            json!({
                "source": "webhook://gh/push",
                "kind": "event",
                "body": "cache eviction regressed read latency",
                "link": "goal://g_1"
            }),
        )
        .await
        .expect("ingest must succeed");
        let item = &out["raw_item"];
        assert_eq!(item["kind"], "event");
        assert_eq!(item["status"], "raw");
        assert_eq!(item["source"], "webhook://gh/push");
        assert_eq!(item["link"]["target"], "goal://g_1");
        assert!(
            item["id"].as_str().is_some(),
            "RawItem must carry an id (provenance seed)"
        );
    }

    #[tokio::test]
    async fn list_raw_unsupported_and_unknown_method_rejected() {
        let (code, _) = dispatch(None::<&OpenAiProvider>, "torii.list_raw", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::NOT_IMPLEMENTED);
        let (code, _) = dispatch(None::<&OpenAiProvider>, "torii.nope", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ingest_raw_rejects_bad_params() {
        let (code, _) = dispatch(
            None::<&OpenAiProvider>,
            "torii.ingest_raw",
            json!({"source": "x"}),
        )
        .await
        .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn parse_maps_tool_call_to_task_draft() {
        let fake = FakeParse {
            args: r#"{"title":"Buy milk","description":"2%","priority":"p1","status":"todo"}"#
                .into(),
        };
        let out = dispatch(Some(&fake), "torii.parse", json!({"input": "buy milk tomorrow"}))
            .await
            .expect("parse must succeed");
        assert_eq!(out["method"], "torii.parse");
        let draft = &out["task_draft"];
        assert_eq!(draft["title"], "Buy milk");
        assert_eq!(draft["description"], "2%");
        assert_eq!(draft["priority"], "p1");
        assert_eq!(draft["status"], "todo");
    }

    #[tokio::test]
    async fn parse_without_provider_is_honest_503() {
        let (code, body) = dispatch(
            None::<&OpenAiProvider>,
            "torii.parse",
            json!({"input": "anything"}),
        )
        .await
        .unwrap_err();
        assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "ai_not_configured");
    }

    #[tokio::test]
    async fn parse_rejects_bad_params() {
        let fake = FakeParse { args: "{}".into() };
        let (code, body) = dispatch(Some(&fake), "torii.parse", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_params");
    }

    #[tokio::test]
    async fn tools_list_names_are_all_dispatchable() {
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            let method = name.replacen('_', ".", 1);
            let (_, body) = dispatch(None::<&OpenAiProvider>, &method, json!({}))
                .await
                .expect_err("empty params must not satisfy any real method");
            assert_ne!(
                body["error"], "unknown_method",
                "{method} must be a real dispatch method"
            );
        }
    }

    #[tokio::test]
    async fn parse_provider_failure_is_502() {
        struct Failing;
        impl torii::AiProvider for Failing {
            async fn respond(&self, _req: AiRequest) -> Result<Vec<AiOutput>, AiError> {
                Err(AiError::new("boom"))
            }
        }
        let (code, body) = dispatch(Some(&Failing), "torii.parse", json!({"input": "x"}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"], "ai_upstream");
    }
}
