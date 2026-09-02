mod engine;
mod models;
mod parser;

use clap::{Parser, Subcommand};
use colored::*;
use engine::RecallEngine;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "agentrecall")]
#[command(about = "Unified search, history recall, and export across AI coding agents (Codex, OpenCode, AGY, Claude Code)", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all agent sessions
    List {
        /// Output results in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Search across all agent conversations, thoughts, and outputs
    Search {
        /// Search query keyword
        query: String,

        /// Output results in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Get full rendered conversation markdown
    Get {
        /// Session ID
        session_id: String,

        /// Output raw JSON instead of Markdown
        #[arg(long)]
        json: bool,
    },
    /// Export all sessions to Markdown files for native IDE search
    Export {
        /// Target export directory
        #[arg(short, long, default_value = ".agentrecall-history")]
        output: PathBuf,

        /// Output results in JSON format
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let engine = RecallEngine::new();

    match cli.command {
        Commands::List { json } => {
            let sessions = engine.list_sessions();
            if json {
                println!("{}", serde_json::to_string(&sessions).unwrap());
            } else {
                println!("\n{} 找到 {} 个会话:\n", "AgentRecall".bold().cyan(), sessions.len());
                for s in &sessions {
                    println!(
                        "  {} {} {} [{}]",
                        "•".blue(),
                        s.thread_name.bold(),
                        format!("({})", &s.id[..s.id.len().min(8)]).dimmed(),
                        s.agent_type.display_name().yellow()
                    );
                    if !s.updated_at.is_empty() {
                        println!("    {}", s.updated_at.dimmed());
                    }
                }
                println!();
            }
        }
        Commands::Search { query, json } => {
            let matches = engine.search(&query);
            if json {
                println!("{}", serde_json::to_string(&matches).unwrap());
            } else {
                println!(
                    "\n🔍 搜索 \"{}\" 命中 {} 处:\n{}",
                    query.yellow().bold(),
                    matches.len(),
                    "=".repeat(60).dimmed()
                );
                for m in &matches {
                    let role_tag = match m.role {
                        models::MessageRole::User => "[用户输入]".green(),
                        models::MessageRole::Thought => "[思考过程]".magenta(),
                        models::MessageRole::Assistant => "[回答输出]".cyan(),
                    };
                    println!(
                        "\n📌 {} {} ({})",
                        m.thread_name.bold(),
                        format!("[{}]", m.agent_type.display_name()).yellow(),
                        &m.session_id[..m.session_id.len().min(8)].dimmed()
                    );
                    println!("   {} ...{}...", role_tag, m.snippet);
                }
                println!("\n{}\n", "=".repeat(60).dimmed());
            }
        }
        Commands::Get { session_id, json } => {
            if let Some(md) = engine.get_markdown(&session_id) {
                if json {
                    let obj = serde_json::json!({
                        "session_id": session_id,
                        "markdown": md
                    });
                    println!("{}", serde_json::to_string(&obj).unwrap());
                } else {
                    println!("{}", md);
                }
            } else {
                eprintln!("错误: 未找到会话 ID {}", session_id);
                std::process::exit(1);
            }
        }
        Commands::Export { output, json } => {
            match engine.export_all(&output) {
                Ok(count) => {
                    if json {
                        let res = serde_json::json!({
                            "success": true,
                            "count": count,
                            "output_dir": output.to_string_lossy()
                        });
                        println!("{}", serde_json::to_string(&res).unwrap());
                    } else {
                        println!(
                            "{} 成功导出 {} 个会话至 {}",
                            "✓".green(),
                            count.to_string().bold(),
                            output.display().to_string().cyan()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("导出失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
