pub mod codex;
pub mod opencode;
pub mod agy;
pub mod claude;

use crate::models::{DialogueEvent, SessionMeta};
use std::path::Path;

pub trait AgentParser: Send + Sync {
    fn detect(&self) -> bool;
    fn discover_sessions(&self) -> Vec<SessionMeta>;
    fn parse_dialogue(&self, file_path: &Path) -> Vec<DialogueEvent>;
}
