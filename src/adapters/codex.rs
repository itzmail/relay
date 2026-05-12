use anyhow::Result;
use std::process::Command;

use super::base::{Agent, RawOutput};
use crate::config::AgentConfig;
use crate::context::build_prompt;

pub struct CodexAdapter {
    command: String,
    model: String,
}

impl CodexAdapter {
    pub fn new(cfg: &AgentConfig) -> Self {
        Self {
            command: cfg.command.clone(),
            model: cfg.default_model.clone(),
        }
    }
}

impl Agent for CodexAdapter {
    fn run(&self, task: &str, context: &str) -> Result<RawOutput> {
        let prompt = build_prompt(context, task);

        let output = Command::new(&self.command)
            .args(["exec", "-m", &self.model, "--sandbox", "workspace-write", &prompt])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    anyhow::anyhow!(
                        "Binary '{}' not found in PATH. Please install it first.",
                        self.command
                    )
                } else {
                    anyhow::anyhow!(e)
                }
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = if stderr.is_empty() {
            stdout
        } else {
            format!("{}\n[stderr]\n{}", stdout, stderr)
        };

        Ok(RawOutput {
            exit_code: output.status.code().unwrap_or(-1),
            output: combined,
        })
    }
}
