//! torii-server — thin, independently-deployed HTTP/MCP wrapper around the
//! `torii` intake lib. Its own deploy unit (own systemd service, own port).
//! Boundary-clean: no mcpbox dependency; the platform→tool auth contract is a
//! configured shared key (see `auth`).
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
//! OPENAI_MODEL (see `ai`); without a key they answer `ai_not_configured`.

mod ai;
mod auth;

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use torii::raw_item::{NewRawItem, RawItemKind};

const TOOL: &str = "torii";

struct AppState {
    version: String,
    platform_secret: Option<Vec<u8>>,
    /// Concrete AI provider; `None` when OPENAI_API_KEY is unset — AI methods
    /// then answer `ai_not_configured` instead of panicking at call time.
    ai: Option<ai::OpenAiProvider>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();

    let version =
        std::env::var("TORII_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let platform_secret = std::env::var("TORII_PLATFORM_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .map(String::into_bytes);
    if platform_secret.is_none() {
        tracing::warn!("TORII_PLATFORM_SECRET unset — /v1/mcp will reject all requests");
    }
    let ai = ai::AiConfig::from_env().map(ai::OpenAiProvider::new);
    if ai.is_none() {
        tracing::warn!("OPENAI_API_KEY unset — AI methods (torii.parse) will answer ai_not_configured");
    }
    let state = Arc::new(AppState {
        version,
        platform_secret,
        ai,
    });

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/mcp", post(mcp))
        .with_state(state);

    let port = std::env::var("TORII_PORT").unwrap_or_else(|_| "8090".to_string());
    // localhost-bound: only the co-located platform reaches it (C3 hardening).
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr}: {e}"));
    tracing::info!(%addr, tool = TOOL, "torii-server listening");
    axum::serve(listener, app).await.expect("server error");
}

async fn healthz(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "service": TOOL,
        "status": "ok",
        "version": s.version,
        "git_sha": option_env!("GIT_SHA").unwrap_or("dev"),
    }))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn mcp(State(s): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(secret) = &s.platform_secret else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"auth_disabled"}))).into_response();
    };
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let Some(claims) = token.and_then(|t| auth::verify(secret, TOOL, now_secs(), t)) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"invalid_platform_token"})),
        )
            .into_response();
    };

    // Auth passed — dispatch the MCP method against the torii intake lib.
    let req: McpRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "bad_request", "detail": e.to_string()})),
            )
                .into_response();
        }
    };
    match dispatch(s.ai.as_ref(), &req.method, req.params).await {
        Ok(mut result) => {
            // Stamp the token-scoped call context onto the response envelope.
            result["tool"] = json!(TOOL);
            result["version"] = json!(s.version);
            result["workspace"] = json!(claims.workspace);
            result["project"] = json!(claims.project);
            Json(result).into_response()
        }
        Err((code, payload)) => (code, Json(payload)).into_response(),
    }
}

/// One MCP call: `{ "method": "torii.ingest_raw", "params": { ... } }`.
#[derive(serde::Deserialize)]
struct McpRequest {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
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
            None::<&ai::OpenAiProvider>,
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
        let (code, _) = dispatch(None::<&ai::OpenAiProvider>, "torii.list_raw", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::NOT_IMPLEMENTED);
        let (code, _) = dispatch(None::<&ai::OpenAiProvider>, "torii.nope", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ingest_raw_rejects_bad_params() {
        let (code, _) = dispatch(
            None::<&ai::OpenAiProvider>,
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
            None::<&ai::OpenAiProvider>,
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
