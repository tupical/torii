//! torii-server — thin, independently-deployed HTTP/MCP wrapper around the
//! `torii` intake lib. Its own deploy unit (own systemd service, own port).
//! Boundary-clean: no mcpbox dependency; the platform→tool auth contract is a
//! configured shared key (see `auth`).
//!
//! Routes:
//!   GET  /healthz   — open; liveness + version for the platform registry.
//!   POST /v1/mcp    — requires a valid platform token; real intake surface
//!                     (`torii.ingest_raw` builds a typed RawItem via the lib).
//!
//! Env: TORII_PORT (default 8090), TORII_PLATFORM_SECRET (HMAC key; if unset,
//! /v1/mcp is closed), TORII_VERSION (defaults to the crate version).

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
    let state = Arc::new(AppState {
        version,
        platform_secret,
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
    match dispatch(&req.method, req.params) {
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

/// Pure MCP dispatch over the torii intake lib — no auth, no HTTP, so it is
/// unit-testable directly. `Ok` is the method result object; `Err` is an
/// (HTTP status, error body) pair. `torii` is a stateless OSS skeleton: it
/// builds typed objects but stores nothing, so read methods are unsupported.
fn dispatch(
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

    #[test]
    fn ingest_raw_builds_typed_raw_item() {
        let out = dispatch(
            "torii.ingest_raw",
            json!({
                "source": "webhook://gh/push",
                "kind": "event",
                "body": "cache eviction regressed read latency",
                "link": "goal://g_1"
            }),
        )
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

    #[test]
    fn list_raw_unsupported_and_unknown_method_rejected() {
        let (code, _) = dispatch("torii.list_raw", json!({})).unwrap_err();
        assert_eq!(code, StatusCode::NOT_IMPLEMENTED);
        let (code, _) = dispatch("torii.nope", json!({})).unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn ingest_raw_rejects_bad_params() {
        let (code, _) = dispatch("torii.ingest_raw", json!({"source": "x"})).unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }
}
