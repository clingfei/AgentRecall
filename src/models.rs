use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    Codex,
    OpenCode,
    Antigravity,
    ClaudeCode,
}

impl AgentType {
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentType::Codex => "OpenAI Codex",
            AgentType::OpenCode => "OpenCode",
            AgentType::Antigravity => "Antigravity (AGY)",
            AgentType::ClaudeCode => "Claude Code",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub agent_type: AgentType,
    pub thread_name: String,
    pub updated_at: String,
    pub file_path: String,
    pub cwd: Option<String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Thought,
    Assistant,
}

impl MessageRole {
    pub fn label(&self) -> &'static str {
        match self {
            MessageRole::User => "用户输入",
            MessageRole::Thought => "思考过程",
            MessageRole::Assistant => "回答输出",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueEvent {
    pub role: MessageRole,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub session_id: String,
    pub agent_type: AgentType,
    pub thread_name: String,
    pub role: MessageRole,
    pub snippet: String,
    pub anchor_text: String,
    pub updated_at: String,
}
