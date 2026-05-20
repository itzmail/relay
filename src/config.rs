use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

pub const CONFIG_FILE: &str = "relay.config.yaml";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentConfig {
    pub command: String,
    pub enabled: bool,
    pub default_model: String,
}

fn default_max_concurrent_jobs() -> usize { 4 }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelayConfig {
    pub agents: HashMap<String, AgentConfig>,
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,
}

impl RelayConfig {
    pub fn load() -> Result<Self> {
        let content = fs::read_to_string(CONFIG_FILE)
            .with_context(|| format!("Config not found. Run `relay init` first."))?;
        let config: RelayConfig = serde_yaml::from_str(&content)
            .with_context(|| "Invalid relay.config.yaml")?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(CONFIG_FILE, content)?;
        Ok(())
    }

    pub fn get_agent(&self, name: &str) -> Result<&AgentConfig> {
        self.agents.get(name).with_context(|| {
            format!(
                "Agent '{}' not found. Run `relay agent list` to see available agents.",
                name
            )
        })
    }
}
