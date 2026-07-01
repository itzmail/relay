use anyhow::Result;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct Session {
    pub pid: u32,
    pub session_id: String,
    pub workspace: String,
    pub name: String,
    pub status: String,
    pub started_at: u64,
}

fn claude_sessions_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".claude").join("sessions")
}

fn is_pid_alive(pid: u32) -> bool {
    // "kill" is a shell builtin — use /bin/kill explicitly
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn list_sessions() -> Vec<Session> {
    let dir = claude_sessions_dir();
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
            let raw: serde_json::Value = serde_json::from_str(&content).ok()?;
            let pid = raw["pid"].as_u64()? as u32;
            if !is_pid_alive(pid) {
                return None;
            }
            Some(Session {
                pid,
                session_id: raw["sessionId"].as_str().unwrap_or("").to_string(),
                workspace: raw["cwd"].as_str().unwrap_or("").to_string(),
                name: raw["name"].as_str().unwrap_or("").to_string(),
                status: raw["status"].as_str().unwrap_or("idle").to_string(),
                started_at: raw["startedAt"].as_u64().unwrap_or(0) / 1000,
            })
        })
        .collect()
}

pub fn write_session(_role: &str) -> Result<PathBuf> {
    Ok(claude_sessions_dir())
}

pub fn update_status(_status: &str) -> Result<()> {
    Ok(())
}

pub fn delete_session() -> Result<()> {
    Ok(())
}

fn join_dir() -> PathBuf {
    PathBuf::from("/tmp/relay-joined")
}

pub fn join_flag_path(pid: u32) -> PathBuf {
    join_dir().join(format!("{}.join", pid))
}

pub fn is_joined(pid: u32) -> bool {
    let flag = join_flag_path(pid);
    if !flag.exists() {
        return false;
    }
    if !is_pid_alive(pid) {
        let _ = fs::remove_file(&flag);
        return false;
    }
    true
}

pub fn join_session(cwd: &str) -> Result<()> {
    let sessions = list_sessions();
    let matches: Vec<&Session> = sessions.iter().filter(|s| s.workspace == cwd).collect();

    match matches.len() {
        0 => anyhow::bail!("No active Claude Code session found in {}", cwd),
        1 => {
            let s = matches[0];
            fs::create_dir_all(join_dir())?;
            fs::write(join_flag_path(s.pid), &s.session_id)?;
            let name = if s.name.is_empty() { &s.session_id[..8] } else { s.name.as_str() };
            println!("Joined relay mesh as \"{}\" (pid {})", name, s.pid);
        }
        _ => {
            println!("Multiple sessions found in {}:", cwd);
            for (i, s) in matches.iter().enumerate() {
                let name = if s.name.is_empty() { &s.session_id[..8] } else { s.name.as_str() };
                println!("  [{}] pid:{} name=\"{}\" status={}", i + 1, s.pid, name, s.status);
            }
            print!("Select [1-{}]: ", matches.len());
            use std::io::{BufRead, Write};
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().lock().read_line(&mut input)?;
            let idx: usize = input.trim().parse::<usize>().unwrap_or(0);
            if idx < 1 || idx > matches.len() {
                anyhow::bail!("Invalid selection");
            }
            let s = matches[idx - 1];
            fs::create_dir_all(join_dir())?;
            fs::write(join_flag_path(s.pid), &s.session_id)?;
            let name = if s.name.is_empty() { &s.session_id[..8] } else { s.name.as_str() };
            println!("Joined relay mesh as \"{}\" (pid {})", name, s.pid);
        }
    }
    Ok(())
}

pub fn leave_session(cwd: &str) -> Result<()> {
    let sessions = list_sessions();
    let joined: Vec<&Session> = sessions
        .iter()
        .filter(|s| s.workspace == cwd && is_joined(s.pid))
        .collect();

    if joined.is_empty() {
        println!("No joined session found in {}", cwd);
        return Ok(());
    }

    for s in joined {
        let _ = fs::remove_file(join_flag_path(s.pid));
        let name = if s.name.is_empty() { &s.session_id[..8] } else { s.name.as_str() };
        println!("Left relay mesh: \"{}\" (pid {})", name, s.pid);
    }
    Ok(())
}
