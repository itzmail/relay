use anyhow::Result;

pub mod base;
pub mod codex;
pub mod copilot;
pub mod opencode;
pub mod pi;

pub use base::Agent;

use crate::config::AgentConfig;

pub const KNOWN_AGENTS: &[&str] = &["opencode", "codex", "copilot", "pi"];

pub fn get_adapter(name: &str, cfg: &AgentConfig) -> Result<Box<dyn Agent>> {
    match name {
        "opencode" => Ok(Box::new(opencode::OpenCodeAdapter::new(cfg))),
        "codex" => Ok(Box::new(codex::CodexAdapter::new(cfg))),
        "copilot" => Ok(Box::new(copilot::CopilotAdapter::new(cfg))),
        "pi" => Ok(Box::new(pi::PiAdapter::new(cfg))),
        other => anyhow::bail!("No adapter for agent '{}'", other),
    }
}

pub async fn init_interactive() -> Result<()> {
    use std::collections::HashMap;
    use std::io::{self, BufRead, Write};
    use std::process::Command;

    let stdin = io::stdin();
    let mut agents = HashMap::new();

    println!("Checking available agents...");

    let available: Vec<&str> = KNOWN_AGENTS
        .iter()
        .filter(|&&agent| {
            let found = Command::new("which")
                .arg(agent)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            println!("  {} {}", if found { "✓" } else { "✗" }, agent);
            found
        })
        .copied()
        .collect();

    if available.is_empty() {
        println!("\nNo agents found in PATH. Install opencode, codex, copilot, or pi first.");
        return Ok(());
    }

    println!();

    for &agent in &available {
        print!("Enable {} ? [Y/n] ", agent);
        io::stdout().flush()?;

        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let enabled = !line.trim().eq_ignore_ascii_case("n");

        if !enabled {
            agents.insert(
                agent.to_string(),
                crate::config::AgentConfig {
                    command: agent.to_string(),
                    enabled: false,
                    default_model: String::new(),
                },
            );
            continue;
        }

        let model = select_model(agent, &stdin)?;

        agents.insert(
            agent.to_string(),
            crate::config::AgentConfig {
                command: agent.to_string(),
                enabled: true,
                default_model: model,
            },
        );
    }

    // disabled entries for agents not in PATH
    for &agent in KNOWN_AGENTS {
        if !available.contains(&agent) {
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

    let config = crate::config::RelayConfig { agents, max_concurrent_jobs: 4 };
    config.save()?;
    println!("\nrelay.config.yaml created.");
    Ok(())
}

fn select_model(agent: &str, stdin: &std::io::Stdin) -> Result<String> {
    use std::io::{BufRead, Write};
    use std::process::Command;

    match agent {
        "opencode" => {
            let output = Command::new("opencode").arg("models").output()?;
            let raw = String::from_utf8_lossy(&output.stdout);
            let models: Vec<&str> = raw.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

            println!("Available opencode models:");
            for (i, m) in models.iter().enumerate() {
                println!("  [{}] {}", i + 1, m);
            }

            loop {
                print!("Select model (number or type name) [default: 1]: ");
                std::io::stdout().flush()?;
                let mut line = String::new();
                stdin.lock().read_line(&mut line)?;
                let input = line.trim();

                if input.is_empty() {
                    return Ok(models.first().copied().unwrap_or("opencode/claude-sonnet-4-6").to_string());
                }

                if let Ok(n) = input.parse::<usize>() {
                    if n >= 1 && n <= models.len() {
                        return Ok(models[n - 1].to_string());
                    }
                } else if !input.is_empty() {
                    return Ok(input.to_string());
                }

                println!("Invalid selection.");
            }
        }

        "copilot" => {
            let choices = &["claude-sonnet-4.5", "claude-sonnet-4", "claude-haiku-4.5", "gpt-5"];
            println!("Available copilot models:");
            for (i, m) in choices.iter().enumerate() {
                println!("  [{}] {}", i + 1, m);
            }

            loop {
                print!("Select model [default: 1]: ");
                std::io::stdout().flush()?;
                let mut line = String::new();
                stdin.lock().read_line(&mut line)?;
                let input = line.trim();

                if input.is_empty() {
                    return Ok(choices[0].to_string());
                }

                if let Ok(n) = input.parse::<usize>() {
                    if n >= 1 && n <= choices.len() {
                        return Ok(choices[n - 1].to_string());
                    }
                }

                println!("Invalid selection.");
            }
        }

        "pi" => {
            let output = Command::new("pi").arg("--list-models").output()?;
            let raw = String::from_utf8_lossy(&output.stdout);
            let models: Vec<&str> = raw.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

            if models.is_empty() {
                print!("Model for pi [default: anthropic/claude-sonnet-4-6]: ");
                std::io::stdout().flush()?;
                let mut line = String::new();
                stdin.lock().read_line(&mut line)?;
                let input = line.trim();
                return Ok(if input.is_empty() {
                    "anthropic/claude-sonnet-4-6".to_string()
                } else {
                    input.to_string()
                });
            }

            println!("Available pi models:");
            for (i, m) in models.iter().enumerate() {
                println!("  [{}] {}", i + 1, m);
            }

            loop {
                print!("Select model (number or type name) [default: 1]: ");
                std::io::stdout().flush()?;
                let mut line = String::new();
                stdin.lock().read_line(&mut line)?;
                let input = line.trim();

                if input.is_empty() {
                    return Ok(models.first().copied().unwrap_or("anthropic/claude-sonnet-4-6").to_string());
                }

                if let Ok(n) = input.parse::<usize>() {
                    if n >= 1 && n <= models.len() {
                        return Ok(models[n - 1].to_string());
                    }
                } else if !input.is_empty() {
                    return Ok(input.to_string());
                }

                println!("Invalid selection.");
            }
        }

        // codex & unknown: free-text input
        _ => {
            let default = if agent == "codex" { "o4-mini" } else { "" };
            print!("Model for {} [default: {}]: ", agent, default);
            std::io::stdout().flush()?;
            let mut line = String::new();
            stdin.lock().read_line(&mut line)?;
            let input = line.trim();
            if input.is_empty() {
                Ok(default.to_string())
            } else {
                Ok(input.to_string())
            }
        }
    }
}
