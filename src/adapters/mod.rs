use anyhow::Result;

pub mod base;
pub mod codex;
pub mod copilot;
pub mod opencode;

pub use base::Agent;

use crate::config::AgentConfig;

pub const KNOWN_AGENTS: &[&str] = &["opencode", "codex", "copilot"];

pub fn get_adapter(name: &str, cfg: &AgentConfig) -> Result<Box<dyn Agent>> {
    match name {
        "opencode" => Ok(Box::new(opencode::OpenCodeAdapter::new(cfg))),
        "codex" => Ok(Box::new(codex::CodexAdapter::new(cfg))),
        "copilot" => Ok(Box::new(copilot::CopilotAdapter::new(cfg))),
        other => anyhow::bail!("No adapter for agent '{}'", other),
    }
}

pub async fn init_interactive() -> Result<()> {
    use std::collections::HashMap;
    use std::process::Command;

    println!("Checking available agents...");

    let mut agents = HashMap::new();

    for &agent in KNOWN_AGENTS {
        let found = Command::new("which")
            .arg(agent)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        println!("  {} {}", if found { "✓" } else { "✗" }, agent);

        if found {
            let default_model = match agent {
                "opencode" => "github-copilot/gpt-5.4-mini",
                "codex" => "o4-mini",
                "copilot" => "gpt-4o",
                _ => "unknown",
            };

            agents.insert(
                agent.to_string(),
                crate::config::AgentConfig {
                    command: agent.to_string(),
                    enabled: true,
                    default_model: default_model.to_string(),
                },
            );
        } else {
            agents.insert(
                agent.to_string(),
                crate::config::AgentConfig {
                    command: agent.to_string(),
                    enabled: false,
                    default_model: String::new(),
                },
            );
        }
    }

    let config = crate::config::RelayConfig { agents };
    config.save()?;
    println!("\nrelay.config.yaml created.");
    Ok(())
}
