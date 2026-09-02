use crate::models::{DialogueEvent, SessionMeta};
use crate::parser::AgentParser;
use std::path::Path;

pub struct OpenCodeParser;
impl OpenCodeParser {
    pub fn new() -> Self {
        Self
    }
}
impl AgentParser for OpenCodeParser {
    fn detect(&self) -> bool {
        false
    }
    fn discover_sessions(&self) -> Vec<SessionMeta> {
        Vec::new()
    }
    fn parse_dialogue(&self, _file_path: &Path) -> Vec<DialogueEvent> {
        Vec::new()
    }
}
