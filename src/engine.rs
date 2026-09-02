use crate::models::{AgentType, DialogueEvent, MessageRole, SearchResult, SessionMeta};
use crate::parser::agy::AntigravityParser;
use crate::parser::claude::ClaudeCodeParser;
use crate::parser::codex::CodexParser;
use crate::parser::opencode::OpenCodeParser;
use crate::parser::AgentParser;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct RecallEngine {
    parsers: HashMap<AgentType, Box<dyn AgentParser>>,
}

fn safe_extract_snippet(text: &str, byte_idx: usize, query_char_len: usize) -> (String, String) {
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    if char_indices.is_empty() {
        return (String::new(), String::new());
    }

    let char_pos = char_indices
        .iter()
        .position(|(b, _)| *b >= byte_idx)
        .unwrap_or(0);

    let snippet_start_char = char_pos.saturating_sub(25);
    let snippet_end_char = (char_pos + 40).min(char_indices.len());

    let anchor_start_char = char_pos.saturating_sub(10);
    let anchor_end_char = (char_pos + query_char_len + 10).min(char_indices.len());

    let snippet_start_byte = char_indices[snippet_start_char].0;
    let snippet_end_byte = if snippet_end_char < char_indices.len() {
        char_indices[snippet_end_char].0
    } else {
        text.len()
    };

    let anchor_start_byte = char_indices[anchor_start_char].0;
    let anchor_end_byte = if anchor_end_char < char_indices.len() {
        char_indices[anchor_end_char].0
    } else {
        text.len()
    };

    let snippet = text[snippet_start_byte..snippet_end_byte]
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string();

    let anchor = text[anchor_start_byte..anchor_end_byte]
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string();

    (snippet, anchor)
}

impl RecallEngine {
    pub fn new() -> Self {
        let mut parsers: HashMap<AgentType, Box<dyn AgentParser>> = HashMap::new();
        parsers.insert(AgentType::Codex, Box::new(CodexParser::new()));
        parsers.insert(AgentType::OpenCode, Box::new(OpenCodeParser::new()));
        parsers.insert(AgentType::Antigravity, Box::new(AntigravityParser::new()));
        parsers.insert(AgentType::ClaudeCode, Box::new(ClaudeCodeParser::new()));
        Self { parsers }
    }

    pub fn list_sessions(&self) -> Vec<SessionMeta> {
        let mut all_sessions: Vec<SessionMeta> = self
            .parsers
            .par_iter()
            .filter(|(_, p)| p.detect())
            .flat_map(|(_, p)| p.discover_sessions())
            .collect();

        all_sessions.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
        all_sessions
    }

    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let sessions = self.list_sessions();
        let lower_query = query.to_lowercase();
        let query_char_len = query.chars().count();

        let results: Vec<SearchResult> = sessions
            .par_iter()
            .flat_map(|session| {
                let path = Path::new(&session.file_path);
                let events = self.parse_session_file(session, path);
                let mut matches = Vec::new();

                for ev in events {
                    let text_lower = ev.text.to_lowercase();
                    for (idx, _) in text_lower.match_indices(&lower_query) {
                        let (snippet, anchor_text) =
                            safe_extract_snippet(&ev.text, idx, query_char_len);

                        matches.push(SearchResult {
                            session_id: session.id.clone(),
                            agent_type: session.agent_type,
                            thread_name: session.thread_name.clone(),
                            role: ev.role,
                            snippet,
                            anchor_text,
                            updated_at: session.updated_at.clone(),
                        });
                    }
                }
                matches
            })
            .collect();

        results
    }

    /// Render a session's dialogue events into formatted Markdown.
    /// Extracted from `get_markdown` so that callers with a `&SessionMeta`
    /// don't need to trigger another full `list_sessions()` scan.
    fn render_markdown(&self, session: &SessionMeta) -> Option<String> {
        let path = Path::new(&session.file_path);
        let events = self.parse_session_file(session, path);

        let mut md = format!("# 💬 {}\n\n", session.thread_name);
        md.push_str(&format!(
            "> **Agent**: `{}`  \n",
            session.agent_type.display_name()
        ));
        md.push_str(&format!("> **Session ID**: `{}`  \n", session.id));
        if let Some(cwd) = &session.cwd {
            md.push_str(&format!("> **工作目录 (CWD)**: `{}`  \n", cwd));
        }
        if !session.updated_at.is_empty() {
            md.push_str(&format!("> **时间**: {}  \n", session.updated_at));
        }
        md.push_str("\n---\n\n");

        for ev in events {
            match ev.role {
                MessageRole::User => {
                    md.push_str(&format!(
                        "\n## 👤 用户输入 (User)\n\n{}\n\n---\n",
                        ev.text
                    ));
                }
                MessageRole::Thought => {
                    let formatted = ev.text.replace('\n', "\n> ");
                    md.push_str(&format!(
                        "\n> 💭 **思考过程 / 行动前述**  \n> {}\n",
                        formatted
                    ));
                }
                MessageRole::Assistant => {
                    md.push_str(&format!(
                        "\n## 🤖 回答输出 (Response)\n\n{}\n\n---\n",
                        ev.text
                    ));
                }
            }
        }

        Some(md)
    }

    pub fn get_markdown(&self, session_id: &str) -> Option<String> {
        let sessions = self.list_sessions();
        let session = sessions
            .into_iter()
            .find(|s| s.id == session_id || s.id.starts_with(session_id))?;
        self.render_markdown(&session)
    }

    pub fn export_all(&self, output_dir: &Path) -> std::io::Result<usize> {
        fs::create_dir_all(output_dir)?;
        let sessions = self.list_sessions();
        let success_count = AtomicUsize::new(0);

        sessions.par_iter().for_each(|session| {
            if let Some(md) = self.render_markdown(session) {
                let safe_title: String = session
                    .thread_name
                    .chars()
                    .map(|c| if "/\\?%*:|\"<>".contains(c) { '_' } else { c })
                    .take(50)
                    .collect();
                let file_name = format!(
                    "{}_{}.md",
                    safe_title,
                    &session.id[..session.id.len().min(8)]
                );
                let target_file = output_dir.join(file_name);
                match fs::write(&target_file, md) {
                    Ok(_) => {
                        success_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("警告: 导出会话 {} 失败: {}", session.id, e);
                    }
                }
            }
        });

        Ok(success_count.load(Ordering::Relaxed))
    }

    fn parse_session_file(&self, session: &SessionMeta, path: &Path) -> Vec<DialogueEvent> {
        self.parsers
            .get(&session.agent_type)
            .map(|p| p.parse_dialogue(path))
            .unwrap_or_default()
    }
}
