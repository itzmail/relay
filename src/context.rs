use anyhow::Result;
use std::fs;
use std::path::PathBuf;

const RELAY_DIR: &str = ".relay";

pub fn build_prompt(context: &str, task: &str) -> String {
    if context.is_empty() {
        return task.to_string();
    }

    format!(
        "[RELAY CONTEXT]\n{}\n[END CONTEXT]\n\n{}",
        context, task
    )
}

pub fn write_temp_context(context: &str) -> Result<PathBuf> {
    fs::create_dir_all(RELAY_DIR)?;
    let path = PathBuf::from(RELAY_DIR).join(format!("ctx_{}.tmp", std::process::id()));
    fs::write(&path, context)?;
    Ok(path)
}

pub fn delete_temp_context(path: &PathBuf) {
    let _ = fs::remove_file(path);
}
