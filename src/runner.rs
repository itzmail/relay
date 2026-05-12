use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::adapters::{get_adapter, KNOWN_AGENTS};
use crate::config::RelayConfig;

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentOutput {
    pub agent: String,
    pub status: String,
    pub exit_code: i32,
    pub output: String,
    pub modified_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub agent: String,
    pub model: String,
    pub task: String,
    pub context: String,
    pub available_agents: Vec<String>,
}

pub async fn plan(agent_name: &str, task: &str, context: &str) -> Result<()> {
    let config = RelayConfig::load()?;
    let agent_cfg = config.get_agent(agent_name)?;

    if !agent_cfg.enabled {
        bail!(
            "Agent '{}' is disabled. Run `relay init` to reconfigure.",
            agent_name
        );
    }

    let available_agents: Vec<String> = config
        .agents
        .iter()
        .filter(|(_, cfg)| cfg.enabled)
        .map(|(name, _)| name.clone())
        .collect();

    let plan = ExecutionPlan {
        agent: agent_name.to_string(),
        model: agent_cfg.default_model.clone(),
        task: task.to_string(),
        context: context.to_string(),
        available_agents,
    };

    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

pub async fn run(agent_name: &str, task: &str, context: &str, model_override: Option<&str>) -> Result<AgentOutput> {
    let config = RelayConfig::load()?;
    let mut agent_cfg = config.get_agent(agent_name)?.clone();

    if !agent_cfg.enabled {
        bail!(
            "Agent '{}' is disabled. Run `relay init` to reconfigure.",
            agent_name
        );
    }

    if let Some(m) = model_override {
        agent_cfg.default_model = m.to_string();
    }

    let adapter = get_adapter(agent_name, &agent_cfg)?;

    let before = git_snapshot();
    let result = adapter.run(task, context)?;
    let after = git_snapshot();
    let modified_files = compute_modified(before, after);

    Ok(AgentOutput {
        agent: agent_name.to_string(),
        status: if result.exit_code == 0 {
            "done".to_string()
        } else {
            "error".to_string()
        },
        exit_code: result.exit_code,
        output: result.output,
        modified_files,
    })
}

pub async fn init() -> Result<()> {
    crate::adapters::init_interactive().await
}

pub fn agent_list() -> Result<()> {
    let config = RelayConfig::load()?;
    println!("Registered agents:");
    for (name, cfg) in &config.agents {
        let status = if cfg.enabled { "enabled" } else { "disabled" };
        let available = which_available(&cfg.command);
        println!(
            "  {} {} [{}] model={}{}",
            if available { "✓" } else { "✗" },
            name,
            status,
            cfg.default_model,
            if !available { " (binary not found)" } else { "" }
        );
    }
    Ok(())
}

pub fn agent_check() -> Result<()> {
    println!("Checking agent availability:");
    for agent_name in KNOWN_AGENTS {
        let found = which_available(agent_name);
        println!("  {} {}", if found { "✓" } else { "✗" }, agent_name);
    }
    Ok(())
}

pub fn config_show() -> Result<()> {
    let content = std::fs::read_to_string(crate::config::CONFIG_FILE)
        .map_err(|_| anyhow::anyhow!("Config not found. Run `relay init` first."))?;
    print!("{}", content);
    Ok(())
}

fn which_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git_snapshot() -> Vec<String> {
    Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .map(|s| s.to_string())
                        .collect(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn compute_modified(before: Vec<String>, after: Vec<String>) -> Vec<String> {
    after
        .into_iter()
        .filter(|f| !before.contains(f))
        .collect()
}
