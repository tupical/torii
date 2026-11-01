//! torii-server — thin, independently-deployed HTTP/MCP wrapper around the
//! `torii` intake lib. Its own deploy unit (own systemd service, own port).
//! Boundary-clean: no mcpbox dependency; the platform→tool auth contract and
//! the axum/tokio scaffold live in `layer_kit::{auth,serve}`.
//!
//! Routes:
//!   GET  /healthz   — open; liveness + version for the platform registry.
//!   POST /v1/mcp    — requires a valid platform token; real intake surface
//!                     (`torii.ingest_raw` builds and persists a typed RawItem
//!                     via the lib, `torii.list_raw` reads them back).
//!
//! Intake has no AI method and never turns raw text into a task: the former
//! `torii.parse` (free text → task draft) was removed by ADR-0014 and answers
//! `unknown_method` like any other unknown name.
//!
//! Env: TORII_PORT (default 8090), TORII_PLATFORM_SECRET (HMAC key; if unset,
//! /v1/mcp is closed), TORII_VERSION (defaults to the crate version),
//! TORII_DB (SQLite store path).

use axum::http::StatusCode;
use layer_kit::auth::Claims;
use layer_kit::serve::{serve, McpHandler, ServeConfig};
use layer_kit::store::Store;
use serde_json::json;
use torii::raw_item::{NewRawItem, RawItemKind};

const TOOL: &str = "torii";

/// Dispatches torii's MCP methods over the persisted RawItem store.
struct Handler {
    store: Store,
}

impl McpHandler for Handler {
    async fn dispatch(
        &self,
        claims: &Claims,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
        // One store serves every tenant: confine this call to the token's scope.
        let store = &self
            .store
            .scoped(&claims.workspace, claims.project.as_deref());
        dispatch(store, method, params).await
    }

    fn tools(&self) -> Vec<serde_json::Value> {
        tools()
    }
}

/// Tool descriptors for `tools/list` — one per method actually handled by
/// [`dispatch`].
fn tools() -> Vec<serde_json::Value> {
    vec![
        json!({
            "name": "torii_ingest_raw",
            "description": "Build a typed RawItem from raw intake (source, kind, body, optional link).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "kind": {"type": "string", "description": "Media type: text, document, reference, binary, or event. Aliases idea, note, implementation_plan, security_plan are normalized to text (reported as kind_normalized_from); any other value is rejected with invalid_params."},
                    "body": {"type": "string"},
                    "link": {"type": "string"}
                },
                "required": ["source", "kind", "body"]
            }
        }),
        json!({
            "name": "torii_list_raw",
            "description": "List persisted RawItems, newest first.",
            "inputSchema": {
                "type": "object",
                "properties": {"limit": {"type": "integer", "minimum": 1}}
            }
        }),
    ]
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();

    let store = Store::from_env(TOOL).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to open torii store");
        std::process::exit(1);
    });

    serve(
        ServeConfig {
            tool: TOOL,
            default_port: 8090,
            default_version: env!("CARGO_PKG_VERSION"),
            git_sha: option_env!("GIT_SHA").unwrap_or("dev"),
        },
        Handler { store },
    )
    .await;
}

/// Params for `torii.ingest_raw`. `kind` deserializes from the lib's
/// snake_case enum (`text`/`document`/`reference`/`binary`/`event`).
#[derive(serde::Deserialize)]
struct IngestParams {
    source: String,
    kind: String,
    body: String,
    #[serde(default)]
    link: Option<String>,
}

#[derive(serde::Deserialize)]
struct ListParams {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    100
}

fn storage_error(e: impl std::fmt::Display) -> (StatusCode, serde_json::Value) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"error": "storage_error", "detail": e.to_string()}),
    )
}

const METHODS: &[&str] = &["torii.ingest_raw", "torii.list_raw"];

/// Pure MCP dispatch over the torii intake lib — no auth, no HTTP, so it is
/// unit-testable directly. `Ok` is the method result object; `Err` is an
/// (HTTP status, error body) pair. Created RawItems are persisted before
/// success is returned.
async fn dispatch(
    store: &Store,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    if !METHODS.contains(&method) {
        return Err((
            StatusCode::BAD_REQUEST,
            json!({"error": "unknown_method", "detail": method}),
        ));
    }
    match method {
        "torii.ingest_raw" => {
            let p: IngestParams = serde_json::from_value(params).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            let Some((kind, aliased)) = RawItemKind::from_input(&p.kind) else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    json!({
                        "error": "invalid_params",
                        "detail": format!(
                            "unknown kind `{}`: expected text, document, reference, binary, event \
                             (aliases idea, note, implementation_plan, security_plan map to text)",
                            p.kind
                        )
                    }),
                ));
            };
            let mut draft = NewRawItem::new(p.source, kind, p.body);
            if let Some(target) = p.link {
                draft = draft.with_link(target);
            }
            let item = draft.build();
            store
                .put("raw_item", &item.id.to_string(), &item)
                .await
                .map_err(storage_error)?;
            let mut out = json!({ "method": "torii.ingest_raw", "raw_item": item });
            if aliased {
                out["kind_normalized_from"] = json!(p.kind);
            }
            Ok(out)
        }
        "torii.list_raw" => {
            let p: ListParams = serde_json::from_value(params).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            let items: Vec<torii::raw_item::RawItem> = store
                .list("raw_item", p.limit)
                .await
                .map_err(storage_error)?;
            Ok(json!({"method": "torii.list_raw", "raw_items": items}))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            json!({"error": "unknown_method", "detail": other}),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use torii::raw_item::RawItemStatus;

    static DB_SEQ: AtomicU64 = AtomicU64::new(1);

    fn db_path() -> String {
        std::env::temp_dir()
            .join(format!(
                "torii-server-{}-{}.db",
                std::process::id(),
                DB_SEQ.fetch_add(1, Ordering::Relaxed)
            ))
            .to_string_lossy()
            .into_owned()
    }

    async fn test_store() -> Store {
        Store::open(&db_path()).await.unwrap()
    }

    async fn dispatch(
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
        super::dispatch(&test_store().await, method, params).await
    }

    #[tokio::test]
    async fn ingest_raw_builds_typed_raw_item() {
        let out = dispatch(
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
    async fn ingest_raw_normalizes_alias_and_rejects_unknown_kind() {
        let out = dispatch(
            "torii.ingest_raw",
            json!({"source": "user://a", "kind": "idea", "body": "x"}),
        )
        .await
        .expect("alias must succeed");
        assert_eq!(out["raw_item"]["kind"], "text");
        assert_eq!(out["kind_normalized_from"], "idea");

        let out = dispatch(
            "torii.ingest_raw",
            json!({"source": "user://a", "kind": "text", "body": "x"}),
        )
        .await
        .unwrap();
        assert!(out.get("kind_normalized_from").is_none());

        let (code, body) = dispatch(
            "torii.ingest_raw",
            json!({"source": "user://a", "kind": "zzz", "body": "x"}),
        )
        .await
        .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_params");
    }

    #[tokio::test]
    async fn list_raw_and_unknown_method_rejected() {
        let out = dispatch("torii.list_raw", json!({})).await.unwrap();
        assert_eq!(out["raw_items"], json!([]));
        let (code, _) = dispatch("torii.nope", json!({})).await.unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn raw_item_persists_across_restart_and_write_errors_surface() {
        let path = db_path();
        let store = Store::open(&path).await.unwrap();
        let created = super::dispatch(
            &store,
            "torii.ingest_raw",
            json!({"source": "test", "kind": "event", "body": "persist me"}),
        )
        .await
        .unwrap();
        drop(store);
        let reopened = Store::open(&path).await.unwrap();
        let listed = super::dispatch(&reopened, "torii.list_raw", json!({}))
            .await
            .unwrap();
        assert_eq!(listed["raw_items"][0]["id"], created["raw_item"]["id"]);

        reopened.pool().close().await;
        let (code, body) = super::dispatch(
            &reopened,
            "torii.ingest_raw",
            json!({"source": "test", "kind": "text", "body": "fail"}),
        )
        .await
        .unwrap_err();
        assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"], "storage_error");
    }

    #[tokio::test]
    async fn ingest_raw_rejects_bad_params() {
        let (code, _) = dispatch("torii.ingest_raw", json!({"source": "x"}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn tools_list_names_are_all_dispatchable() {
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            let method = name.replacen('_', ".", 1);
            if let Err((_, body)) = dispatch(&method, json!({})).await {
                assert_ne!(body["error"], "unknown_method", "{method} must be real");
            }
        }
    }

    #[test]
    fn tools_catalogue_matches_methods() {
        layer_kit::test_support::assert_catalogue_matches(&tools(), METHODS);
    }

    /// ADR-0014: the free-text → task-draft path is gone — not advertised; an old
    /// client calling it gets the same honest `unknown_method` as any typo.
    #[tokio::test]
    async fn parse_is_removed_and_answers_unknown_method() {
        assert!(tools().iter().all(|t| t["name"] != "torii_parse"));
        assert!(!METHODS.contains(&"torii.parse"));
        let (code, body) = dispatch("torii.parse", json!({"input": "buy milk"}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "unknown_method");
        assert_eq!(body["detail"], "torii.parse");
    }

    /// ADR-0014: a snapshot the host suppressed and persisted stays readable
    /// through `torii.list_raw` with wire status `suppressed`.
    #[tokio::test]
    async fn suppressed_snapshot_reads_back_through_list_raw() {
        let store = test_store().await;
        let created = super::dispatch(
            &store,
            "torii.ingest_raw",
            json!({"source": "bot://ci", "kind": "event", "body": "heartbeat"}),
        )
        .await
        .unwrap();
        let mut item: torii::RawItem = serde_json::from_value(created["raw_item"].clone()).unwrap();
        let torii::IntakeEvent::RawItemSuppressed(event) = item.suppress("ci heartbeat").unwrap();
        store
            .put("raw_item", &item.id.to_string(), &item)
            .await
            .unwrap();
        let listed = super::dispatch(&store, "torii.list_raw", json!({}))
            .await
            .unwrap();
        assert_eq!(listed["raw_items"][0]["id"], created["raw_item"]["id"]);
        assert_eq!(listed["raw_items"][0]["status"], "suppressed");
        let back: torii::RawItem = serde_json::from_value(listed["raw_items"][0].clone()).unwrap();
        assert_eq!(back.status, RawItemStatus::Suppressed);
        assert!(back.ensure_routable().is_err());
        assert_eq!(event.item_id, back.id);
        assert_eq!(event.reason, "ci heartbeat");
    }

    /// daruma 01a0d3bc: one layer server serves every tenant — the platform
    /// token's workspace confines every read, so ws B never sees ws A's objects.
    #[tokio::test]
    async fn token_of_one_workspace_never_sees_another() {
        let claims = |ws: &str| Claims {
            workspace: ws.into(),
            project: None,
            tool: TOOL.into(),
            exp: i64::MAX,
        };
        let handler = Handler {
            store: test_store().await,
        };
        handler
            .dispatch(
                &claims("ws_a"),
                "torii.ingest_raw",
                json!({"source": "user://a", "kind": "text", "body": "secret of A"}),
            )
            .await
            .unwrap();
        let b = handler
            .dispatch(&claims("ws_b"), "torii.list_raw", json!({}))
            .await
            .unwrap();
        assert_eq!(b["raw_items"], json!([]));
        let a = handler
            .dispatch(&claims("ws_a"), "torii.list_raw", json!({}))
            .await
            .unwrap();
        assert_eq!(a["raw_items"][0]["body"], "secret of A");
    }
}
