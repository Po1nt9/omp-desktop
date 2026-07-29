//! Host tools exposed to the live voice model for agent delegation.
//! Pure definitions + argument parsing (testable without network).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Tool schema sent to the realtime session (OpenAI-compatible function tools).
pub fn tool_definitions() -> Vec<Value> {
    vec![
        function_tool(
            "list_sessions",
            "List recent OMP Runtime agent sessions for the current project (id, title, busy).",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }
            }),
        ),
        function_tool(
            "create_agent_session",
            "Create a new OMP Runtime agent session in the active project to do coding work. Prefer this for multi-step implementation tasks.",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Short session title" },
                    "prompt": {
                        "type": "string",
                        "description": "First instruction for the coding agent"
                    }
                },
                "required": ["prompt"]
            }),
        ),
        function_tool(
            "prompt_agent",
            "Send a follow-up instruction to an existing agent session (or the current live session if session_id is omitted).",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "prompt": { "type": "string" }
                },
                "required": ["prompt"]
            }),
        ),
        function_tool(
            "get_agent_status",
            "Get status of an agent session: state, last activity, whether a permission or plan is waiting.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" }
                }
            }),
        ),
        function_tool(
            "cancel_agent",
            "Cancel the in-flight turn on an agent session.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" }
                }
            }),
        ),
    ]
}

fn function_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": parameters
    })
}

/// System / session instructions for the voice model.
pub fn live_voice_instructions(project_path: Option<&str>, project_name: Option<&str>) -> String {
    let project = project_name
        .or(project_path)
        .unwrap_or("the current workspace");
    format!(
        r#"You are OMP Live Voice in the OMP Desktop coding workbench.
You speak briefly and clearly. You can listen and talk while coding agents work.

Project: {project}
{path_line}

Rules:
- You do NOT edit files yourself. For any implementation, debugging, tests, git, or multi-step work, call host tools: create_agent_session, prompt_agent, get_agent_status, cancel_agent, list_sessions.
- After starting work, keep the user updated in plain language. Offer to check status.
- Never invent tool results. Use tool returns only.
- Respect that the app shows permission prompts; if work is blocked on approval, tell the user to allow or deny in the UI.
- Prefer short spoken answers (1–3 sentences) unless the user asks for detail.
"#,
        path_line = project_path
            .map(|p| format!("Path: {p}"))
            .unwrap_or_default(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceToolName {
    ListSessions,
    CreateAgentSession,
    PromptAgent,
    GetAgentStatus,
    CancelAgent,
}

impl VoiceToolName {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "list_sessions" => Some(Self::ListSessions),
            "create_agent_session" => Some(Self::CreateAgentSession),
            "prompt_agent" => Some(Self::PromptAgent),
            "get_agent_status" => Some(Self::GetAgentStatus),
            "cancel_agent" => Some(Self::CancelAgent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CreateAgentArgs {
    pub title: Option<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PromptAgentArgs {
    pub session_id: Option<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionRefArgs {
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ListSessionsArgs {
    pub limit: Option<u32>,
}

pub fn parse_create_agent_args(raw: &str) -> Result<CreateAgentArgs, String> {
    let v: Value = serde_json::from_str(if raw.trim().is_empty() { "{}" } else { raw })
        .map_err(|e| format!("invalid create_agent_session args: {e}"))?;
    let prompt = v
        .get("prompt")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("create_agent_session requires prompt".into());
    }
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(CreateAgentArgs { title, prompt })
}

pub fn parse_prompt_agent_args(raw: &str) -> Result<PromptAgentArgs, String> {
    let v: Value = serde_json::from_str(if raw.trim().is_empty() { "{}" } else { raw })
        .map_err(|e| format!("invalid prompt_agent args: {e}"))?;
    let prompt = v
        .get("prompt")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("prompt_agent requires prompt".into());
    }
    let session_id = v
        .get("session_id")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(PromptAgentArgs {
        session_id,
        prompt,
    })
}

pub fn parse_session_ref_args(raw: &str) -> Result<SessionRefArgs, String> {
    let v: Value = serde_json::from_str(if raw.trim().is_empty() { "{}" } else { raw })
        .map_err(|e| format!("invalid session args: {e}"))?;
    let session_id = v
        .get("session_id")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(SessionRefArgs { session_id })
}

pub fn parse_list_sessions_args(raw: &str) -> Result<ListSessionsArgs, String> {
    let v: Value = serde_json::from_str(if raw.trim().is_empty() { "{}" } else { raw })
        .map_err(|e| format!("invalid list_sessions args: {e}"))?;
    let limit = v.get("limit").and_then(|x| x.as_u64()).map(|n| n as u32);
    Ok(ListSessionsArgs { limit })
}

/// Mock tool executor for tests / GROK_APP_VOICE=mock without a live agent.
pub fn mock_execute_tool(name: &str, args_json: &str) -> Result<Value, String> {
    let tool = VoiceToolName::parse(name).ok_or_else(|| format!("unknown tool: {name}"))?;
    match tool {
        VoiceToolName::ListSessions => {
            let _ = parse_list_sessions_args(args_json)?;
            Ok(json!({
                "sessions": [
                    { "id": "mock-1", "title": "Mock session", "state": "ready" }
                ]
            }))
        }
        VoiceToolName::CreateAgentSession => {
            let a = parse_create_agent_args(args_json)?;
            Ok(json!({
                "session_id": "mock-new",
                "title": a.title.unwrap_or_else(|| "Voice task".into()),
                "accepted_prompt": a.prompt,
                "state": "streaming"
            }))
        }
        VoiceToolName::PromptAgent => {
            let a = parse_prompt_agent_args(args_json)?;
            Ok(json!({
                "session_id": a.session_id.unwrap_or_else(|| "live".into()),
                "accepted_prompt": a.prompt,
                "state": "streaming"
            }))
        }
        VoiceToolName::GetAgentStatus => {
            let a = parse_session_ref_args(args_json)?;
            Ok(json!({
                "session_id": a.session_id.unwrap_or_else(|| "live".into()),
                "state": "ready",
                "summary": "Mock agent is idle and ready."
            }))
        }
        VoiceToolName::CancelAgent => {
            let a = parse_session_ref_args(args_json)?;
            Ok(json!({
                "session_id": a.session_id.unwrap_or_else(|| "live".into()),
                "cancelled": true
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_nonempty() {
        assert!(tool_definitions().len() >= 4);
    }

    #[test]
    fn parse_create_requires_prompt() {
        assert!(parse_create_agent_args("{}").is_err());
        let a = parse_create_agent_args(r#"{"prompt":"fix tests","title":"T"}"#).unwrap();
        assert_eq!(a.prompt, "fix tests");
        assert_eq!(a.title.as_deref(), Some("T"));
    }

    #[test]
    fn mock_create() {
        let v = mock_execute_tool(
            "create_agent_session",
            r#"{"prompt":"run cargo test"}"#,
        )
        .unwrap();
        assert_eq!(v["session_id"], "mock-new");
    }

    #[test]
    fn instructions_include_project() {
        let s = live_voice_instructions(Some("/tmp/app"), Some("app"));
        assert!(s.contains("app"));
        assert!(s.contains("create_agent_session"));
    }
}
