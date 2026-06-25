// ponytail: minimal config — only what MCP daemon needs; agent registry removed
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelayConfig {
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,
}

fn default_max_concurrent_jobs() -> usize { 4 }

impl RelayConfig {
    pub fn load() -> Self {
        std::fs::read_to_string("relay.config.yaml")
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_else(|| RelayConfig { max_concurrent_jobs: 4 })
    }
}
