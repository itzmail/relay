use anyhow::Result;

pub struct RawOutput {
    pub exit_code: i32,
    pub output: String,
}

pub trait Agent {
    fn run(&self, task: &str, context: &str) -> Result<RawOutput>;
}
