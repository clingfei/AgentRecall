use crate::models::{AgentType, DialogueEvent, MessageRole, SessionMeta};
use crate::parser::AgentParser;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct CodexParser {
    home_dir: PathBuf,
}

impl CodexParser {
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self { home_dir }
    }

    fn codex_dir(&self) -> PathBuf {
        self.home_dir.join(".codex")
    }

    fn sessions_dir(&self) -> PathBuf {
        self.codex_dir().join("sessions")
    }

    fn index_file(&self) -> PathBuf {
        self.codex_dir().join("session_index.jsonl")
    }

    fn load_index(&self) -> HashMap<String, (String, String)> {
        let mut map = HashMap::new();
        let index_path = self.index_file();
        if !index_path.exists() {
            return map;
        }

        if let Ok(file) = File::open(index_path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
                        let thread_name = val
                            .get("thread_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("未命名会话")
                            .to_string();
                        let updated_at = val
                            .get("updated_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        map.insert(id.to_string(), (thread_name, updated_at));
                    }
                }
            }
        }
        map
    }

    fn clean_user_message(raw_text: &str) -> Option<String> {
        let text = raw_text.trim();
        if text.is_empty() {
            return None;
        }

        if text.starts_with("# AGENTS.md instructions") || text.contains("<INSTRUCTIONS>") {
            return None;
        }

        if text.starts_with("<permissions instructions>")
            || text.starts_with("<skills_instructions>")
            || text.starts_with("<collaboration_mode>")
            || text.starts_with("<environment_context>")
            || text.starts_with("<turn_aborted>")
            || text.starts_with("# Response annotations:")
        {
            return None;
        }

        if text.contains("# Context from my IDE setup:") {
            if let Some(req_idx) = text.find("## My request for Codex:") {
                let req_body = text[req_idx + "## My request for Codex:".len()..].trim();
                let sel_text = if let Some(sel_idx) = text.find("## Active selection of the file:") {
                    let after_sel = &text[sel_idx + "## Active selection of the file:".len()..];
                    let line = after_sel.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
                    if !line.trim().is_empty() && !line.starts_with('#') {
                        format!("> 📌 **选中文本**: `{}`\n\n", line.trim())
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                if !req_body.is_empty() {
                    return Some(format!("{}{}", sel_text, req_body));
                }
            }
        }

        Some(text.to_string())
    }
}

impl AgentParser for CodexParser {
    fn detect(&self) -> bool {
        self.codex_dir().exists()
    }

    fn discover_sessions(&self) -> Vec<SessionMeta> {
        let sessions_dir = self.sessions_dir();
        if !sessions_dir.exists() {
            return Vec::new();
        }

        let index_map = self.load_index();
        let mut results = Vec::new();
        let mut seen_ids = HashSet::new();

        for entry in WalkDir::new(sessions_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file() && e.path().extension().map_or(false, |ext| ext == "jsonl"))
        {
            let path = entry.path();
            let mut session_id = None;
            let mut cwd = None;
            let mtime_ms = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            if let Ok(file) = File::open(path) {
                let mut reader = BufReader::new(file);
                let mut first_line = String::new();
                if reader.read_line(&mut first_line).is_ok() && !first_line.is_empty() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&first_line) {
                        if val.get("type").and_then(|v| v.as_str()) == Some("session_meta") {
                            if let Some(payload) = val.get("payload") {
                                session_id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                                cwd = payload.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
                            }
                        }
                    }
                }
            }

            if session_id.is_none() {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if file_stem.len() >= 36 {
                        let potential_uuid = &file_stem[file_stem.len() - 36..];
                        if potential_uuid.chars().filter(|&c| c == '-').count() == 4 {
                            session_id = Some(potential_uuid.to_string());
                        }
                    }
                }
            }

            if let Some(sid) = session_id {
                if seen_ids.insert(sid.clone()) {
                    let (thread_name, updated_at) = match index_map.get(&sid) {
                        Some((name, time)) => (name.clone(), time.clone()),
                        None => (
                            format!("会话 {}", &sid[..sid.len().min(8)]),
                            chrono::DateTime::from_timestamp_millis(mtime_ms)
                                .map(|dt| dt.to_rfc3339())
                                .unwrap_or_default(),
                        ),
                    };

                    results.push(SessionMeta {
                        id: sid,
                        agent_type: AgentType::Codex,
                        thread_name,
                        updated_at,
                        file_path: path.to_string_lossy().to_string(),
                        cwd,
                        timestamp_ms: mtime_ms,
                    });
                }
            }
        }

        results.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
        results
    }

    fn parse_dialogue(&self, file_path: &Path) -> Vec<DialogueEvent> {
        let mut events = Vec::new();
        let Ok(file) = File::open(file_path) else {
            return events;
        };

        let mut seen_texts = HashSet::new();
        let reader = BufReader::new(file);

        for line in reader.lines().flatten() {
            if line.trim().is_empty() {
                continue;
            }

            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };

            let entry_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let payload = val.get("payload");

            match entry_type {
                "event_msg" => {
                    if let Some(p) = payload {
                        let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let msg = p.get("message").and_then(|v| v.as_str()).unwrap_or("").trim();

                        if ptype == "agent_message" && !msg.is_empty() {
                            if seen_texts.insert(msg.to_string()) {
                                let phase = p.get("phase").and_then(|v| v.as_str()).unwrap_or("");
                                if phase == "commentary" {
                                    events.push(DialogueEvent {
                                        role: MessageRole::Thought,
                                        text: msg.to_string(),
                                    });
                                } else {
                                    events.push(DialogueEvent {
                                        role: MessageRole::Assistant,
                                        text: msg.to_string(),
                                    });
                                }
                            }
                        } else if ptype == "user_message" && !msg.is_empty() {
                            if let Some(cleaned) = Self::clean_user_message(msg) {
                                if seen_texts.insert(cleaned.clone()) {
                                    events.push(DialogueEvent {
                                        role: MessageRole::User,
                                        text: cleaned,
                                    });
                                }
                            }
                        }
                    }
                }
                "response_item" => {
                    if let Some(p) = payload {
                        let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if ptype == "message" {
                            let role = p.get("role").and_then(|v| v.as_str()).unwrap_or("");
                            if role == "developer" {
                                continue;
                            }

                            let mut full_text = String::new();
                            if let Some(content) = p.get("content").and_then(|v| v.as_array()) {
                                for item in content {
                                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                                        full_text.push_str(t);
                                        full_text.push('\n');
                                    }
                                }
                            } else if let Some(content_str) = p.get("content").and_then(|v| v.as_str()) {
                                full_text.push_str(content_str);
                            }

                            let trimmed = full_text.trim();
                            if trimmed.is_empty() {
                                continue;
                            }

                            if role == "user" {
                                if let Some(cleaned) = Self::clean_user_message(trimmed) {
                                    if seen_texts.insert(cleaned.clone()) {
                                        events.push(DialogueEvent {
                                            role: MessageRole::User,
                                            text: cleaned,
                                        });
                                    }
                                }
                            } else if role == "assistant" {
                                if seen_texts.insert(trimmed.to_string()) {
                                    let phase = p.get("phase").and_then(|v| v.as_str()).unwrap_or("");
                                    if phase == "commentary" {
                                        events.push(DialogueEvent {
                                            role: MessageRole::Thought,
                                            text: trimmed.to_string(),
                                        });
                                    } else {
                                        events.push(DialogueEvent {
                                            role: MessageRole::Assistant,
                                            text: trimmed.to_string(),
                                        });
                                    }
                                }
                            }
                        } else if ptype == "reasoning" {
                            if let Some(summaries) = p.get("summary").and_then(|v| v.as_array()) {
                                for s in summaries {
                                    let stext = s.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
                                    if !stext.is_empty() && seen_texts.insert(stext.to_string()) {
                                        events.push(DialogueEvent {
                                            role: MessageRole::Thought,
                                            text: stext.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        events
    }
}
