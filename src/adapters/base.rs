use anyhow::Result;

pub struct RawOutput {
    pub exit_code: i32,
    pub output: String,
}

pub trait Agent {
    fn run(&self, task: &str, context: &str) -> Result<RawOutput>;
    /// Returns (command, args) without executing. Used by MCP spawn for async process control.
    fn spawn_args(&self, task: &str, context: &str) -> (String, Vec<String>);
}
