//! Intake's slice of the AI provider seam: re-exports the shared
//! [`layer_kit::ai`] infrastructure and adds the one domain-specific tool
//! schema `parse` needs.

use serde_json::{json, Value};

pub use layer_kit::ai::{
    AiError, AiOutput, AiProvider, AiRequest, ToolCall, UNTRUSTED_CLOSE, UNTRUSTED_OPEN,
};
pub use layer_kit::ai::wrap_untrusted;

/// JSON schema for the `create_task` function tool used by `parse`.
pub fn create_task_tool() -> Value {
    json!({
        "type": "function",
        "name": "create_task",
        "description": "Create a new task from a natural language description.",
        "parameters": {
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Short, imperative title for the task (≤120 chars)."
                },
                "description": {
                    "type": "string",
                    "description": "Optional detailed description or acceptance criteria."
                },
                "priority": {
                    "type": "string",
                    "enum": ["p0", "p1", "p2", "p3"],
                    "description": "Priority: p0=urgent, p1=high, p2=medium (default), p3=low."
                },
                "status": {
                    "type": "string",
                    "enum": ["inbox", "todo", "in_progress", "done"],
                    "description": "Initial status. Defaults to inbox when omitted."
                }
            },
            "required": ["title"]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_task_tool_shape() {
        let t = create_task_tool();
        assert_eq!(t["name"], "create_task");
        assert_eq!(t["parameters"]["required"][0], "title");
    }
}
