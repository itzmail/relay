use anyhow::Result;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SESSION_DIR: &str = "/tmp/relay-sessions";

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct Session {
    pub pid: u32,
    pub workspace: String,
    pub tool: String,
    pub role: String,
    pub goal: String,
    pub done: Vec<String>,
    pub modified: Vec<String>,
    pub status: String, // "idle" | "working"
    pub started_at: u64,
}

pub fn session_dir() -> PathBuf {
    PathBuf::from(SESSION_DIR)
}

pub fn session_path(pid: u32) -> PathBuf {
    PathBuf::from(SESSION_DIR).join(format!("{pid}.json"))
}

pub fn write_session(role: &str) -> Result<PathBuf> {
    let dir = session_dir();
    fs::create_dir_all(&dir)?;

    let pid = std::process::id();
    let workspace = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // read role from arg, then fall back to .relay-role file in cwd
    let resolved_role = if !role.is_empty() {
        role.to_string()
    } else {
        std::env::current_dir()
            .ok()
            .map(|p| p.join(".relay-role"))
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };

    let session = Session {
        pid,
        workspace,
        tool: "claude-code".to_string(),
        role: resolved_role,
        goal: String::new(),
        done: vec![],
        modified: vec![],
        status: "idle".to_string(),
        started_at: now_secs(),
    };

    let path = session_path(pid);
    fs::write(&path, serde_json::to_string_pretty(&session)?)?;
    Ok(path)
}

pub fn update_status(status: &str) -> Result<()> {
    let pid = std::process::id();
    let path = session_path(pid);
    if !path.exists() {
        return Ok(());
    }
    let mut s: Session = serde_json::from_str(&fs::read_to_string(&path)?)?;
    s.status = status.to_string();

    // update modified files from git
    if status == "idle" {
        s.modified = git_modified();
    }

    fs::write(&path, serde_json::to_string_pretty(&s)?)?;
    Ok(())
}

pub fn delete_session() -> Result<()> {
    let path = session_path(std::process::id());
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn list_sessions() -> Vec<Session> {
    let dir = session_dir();
    if !dir.exists() {
        return vec![];
    }

    fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .filter_map(|e| {
            let content = fs::read_to_string(e.path()).ok()?;
            let s: Session = serde_json::from_str(&content).ok()?;
            // validate PID still alive, cleanup stale
            if is_pid_alive(s.pid) {
                Some(s)
            } else {
                let _ = fs::remove_file(e.path());
                None
            }
        })
        .collect()
}

fn is_pid_alive(pid: u32) -> bool {
    // kill -0 checks existence without sending a signal
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git_modified() -> Vec<String> {
    std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
