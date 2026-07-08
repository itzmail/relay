use anyhow::Result;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".relay").join("relay.db")
}

fn reply_file_path(session_name: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".relay")
        .join(format!("pending-reply-{}.txt", session_name))
}

pub async fn watch(session_name: &str, from_agent: &str, timeout_secs: u64) -> Result<()> {
    let db = db_path();
    if !db.exists() {
        anyhow::bail!("relay.db not found at {}. Is relay mcp start running?", db.display());
    }

    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    println!("Watching for reply from '{}' → '{}' (timeout: {}s)...", from_agent, session_name, timeout_secs);

    // Get the current max message id so we only watch for NEW messages
    let baseline_id: i64 = {
        let conn = rusqlite::Connection::open(&db)?;
        conn.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM messages",
            [],
            |r| r.get(0),
        )?
    };

    loop {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            - started;

        if elapsed >= timeout_secs {
            println!("Timeout reached. No reply from '{}'.", from_agent);
            return Ok(());
        }

        tokio::time::sleep(Duration::from_secs(5)).await;

        let conn = rusqlite::Connection::open(&db)?;
        let result: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, payload FROM messages
                 WHERE id > ?1
                   AND from_agent = ?2
                   AND (to_agent IS NULL OR to_agent = ?3)
                 ORDER BY id ASC
                 LIMIT 1",
                rusqlite::params![baseline_id, from_agent, session_name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        if let Some((_id, payload)) = result {
            let reply_path = reply_file_path(session_name);
            if let Some(parent) = reply_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = format!("[relay] Reply from '{}':\n{}", from_agent, payload);
            std::fs::write(&reply_path, &content)?;
            println!("Reply received. Written to {}", reply_path.display());
            return Ok(());
        }
    }
}
