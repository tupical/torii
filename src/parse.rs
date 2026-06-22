//! Natural language → [`TaskDraft`] parser (intake `parse` operation).

use serde::Serialize;
use serde_json::Value;

use crate::ai::{create_task_tool, wrap_untrusted, AiOutput, AiProvider, AiRequest};
use crate::error::IntakeError;
use crate::prompts::PromptRegistry;
use crate::task::{Priority, Status, TaskDraft};

#[derive(Serialize)]
struct ParseCtx<'a> {
    input: &'a str,
}

/// Parse a natural-language task description into a [`TaskDraft`].
///
/// Renders the `parse` prompt, calls `provider` with the `create_task`
/// function tool, and maps the returned arguments onto [`TaskDraft`]. The
/// concrete model client is supplied by the caller via [`AiProvider`].
pub async fn parse_task<P: AiProvider>(
    provider: &P,
    input: &str,
) -> Result<TaskDraft, IntakeError> {
    let input = wrap_untrusted("task description to parse", input);
    let prompt = PromptRegistry::load("parse", "default", &ParseCtx { input: &input })?;

    let req = AiRequest {
        input: Value::String(prompt),
        tools: vec![create_task_tool()],
        tool_choice: Some("required".into()),
    };

    let outputs = provider.respond(req).await?;

    let tc = outputs
        .into_iter()
        .find_map(|o| match o {
            AiOutput::ToolCall(tc) if tc.name == "create_task" => Some(tc),
            _ => None,
        })
        .ok_or_else(|| IntakeError::ai("parse_task: model returned no create_task call"))?;

    let args: Value =
        serde_json::from_str(&tc.arguments).map_err(|e| IntakeError::serde(e.to_string()))?;

    let title = args["title"]
        .as_str()
        .ok_or_else(|| IntakeError::validation("create_task: missing required field 'title'"))?
        .to_owned();

    let mut draft = TaskDraft::new(title);

    if let Some(desc) = args["description"].as_str() {
        draft.description = Some(desc.to_owned());
    }
    if let Some(p) = args["priority"].as_str() {
        draft.priority = parse_priority(p);
    }
    if let Some(s) = args["status"].as_str() {
        draft.status = parse_status(s);
    }

    Ok(draft)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_priority(s: &str) -> Option<Priority> {
    match s {
        "p0" => Some(Priority::P0),
        "p1" => Some(Priority::P1),
        "p2" => Some(Priority::P2),
        "p3" => Some(Priority::P3),
        _ => None,
    }
}

fn parse_status(s: &str) -> Option<Status> {
    match s {
        "inbox" => Some(Status::Inbox),
        "todo" => Some(Status::Todo),
        "in_progress" => Some(Status::InProgress),
        "in_review" => Some(Status::InReview),
        "done" => Some(Status::Done),
        "cancelled" => Some(Status::Cancelled),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{AiError, ToolCall};

    /// Minimal provider that returns a fixed `create_task` call — lets us
    /// exercise the whole parse→map path without a real model.
    struct FakeProvider {
        args: String,
    }

    impl AiProvider for FakeProvider {
        async fn respond(&self, _req: AiRequest) -> Result<Vec<AiOutput>, AiError> {
            Ok(vec![AiOutput::ToolCall(ToolCall {
                name: "create_task".into(),
                arguments: self.args.clone(),
            })])
        }
    }

    #[tokio::test]
    async fn parse_task_maps_tool_call_to_draft() {
        let fake = FakeProvider {
            args: r#"{"title":"Buy milk","description":"2%","priority":"p1","status":"todo"}"#
                .into(),
        };
        let draft = parse_task(&fake, "buy milk tomorrow").await.unwrap();
        assert_eq!(draft.title, "Buy milk");
        assert_eq!(draft.description.as_deref(), Some("2%"));
        assert_eq!(draft.priority, Some(Priority::P1));
        assert_eq!(draft.status, Some(Status::Todo));
    }

    #[tokio::test]
    async fn parse_task_missing_title_is_validation_error() {
        let fake = FakeProvider {
            args: r#"{"priority":"p1"}"#.into(),
        };
        let err = parse_task(&fake, "x").await.unwrap_err();
        assert!(matches!(err, IntakeError::Validation(_)));
    }

    #[test]
    fn parse_priority_roundtrip() {
        assert!(matches!(parse_priority("p0"), Some(Priority::P0)));
        assert!(matches!(parse_priority("p1"), Some(Priority::P1)));
        assert!(matches!(parse_priority("p2"), Some(Priority::P2)));
        assert!(matches!(parse_priority("p3"), Some(Priority::P3)));
        assert!(parse_priority("unknown").is_none());
    }

    #[test]
    fn parse_status_roundtrip() {
        assert!(matches!(parse_status("inbox"), Some(Status::Inbox)));
        assert!(matches!(parse_status("todo"), Some(Status::Todo)));
        assert!(matches!(parse_status("in_progress"), Some(Status::InProgress)));
        assert!(matches!(parse_status("in_review"), Some(Status::InReview)));
        assert!(matches!(parse_status("done"), Some(Status::Done)));
        assert!(matches!(parse_status("cancelled"), Some(Status::Cancelled)));
        assert!(parse_status("other").is_none());
    }

    #[test]
    fn status_as_str_matches_parse_input() {
        // Lock the wire-format symmetry: as_str produces strings parse_status accepts.
        for &s in &[
            Status::Inbox,
            Status::Todo,
            Status::InProgress,
            Status::InReview,
            Status::Done,
            Status::Cancelled,
        ] {
            assert_eq!(parse_status(s.as_str()), Some(s), "{s:?} round-trip");
        }
    }
}
