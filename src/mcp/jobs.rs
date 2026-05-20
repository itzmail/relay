use anyhow::Result;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug)]
pub struct Job {
    pub id: String,
    pub agent: String,
    pub task: String,
    pub pid: Option<u32>,
    pub status: String,
    pub log_path: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i32>,
    pub last_stdout_at: u64,
    pub modified_files: Option<String>,
    pub checkpoint: Option<String>,
}

#[derive(Debug)]
pub struct TailLine {
    pub line_no: i64,
    pub stream: String,
    pub content: String,
    pub ts: u64,
}

pub fn insert_job(conn: &Connection, id: &str, agent: &str, task: &str, pid: u32, log_path: &str) -> Result<()> {
    let now = now_secs();
    conn.execute(
        "INSERT INTO jobs(id, agent, task, pid, status, log_path, started_at, last_stdout_at)
         VALUES (?, ?, ?, ?, 'working', ?, ?, ?)",
        rusqlite::params![id, agent, task, pid, log_path, now, now],
    )?;
    Ok(())
}

pub fn update_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET status = ? WHERE id = ?",
        rusqlite::params![status, id],
    )?;
    Ok(())
}

pub fn update_last_stdout_at(conn: &Connection, id: &str) -> Result<()> {
    let now = now_secs();
    conn.execute(
        "UPDATE jobs SET last_stdout_at = ? WHERE id = ?",
        rusqlite::params![now, id],
    )?;
    Ok(())
}

pub fn mark_finished(
    conn: &Connection,
    id: &str,
    exit_code: i32,
    modified_files_json: Option<&str>,
) -> Result<()> {
    let now = now_secs();
    let status = if exit_code == 0 { "done" } else { "error" };
    conn.execute(
        "UPDATE jobs SET status = ?, finished_at = ?, exit_code = ?, modified_files = ? WHERE id = ?",
        rusqlite::params![status, now, exit_code, modified_files_json, id],
    )?;
    Ok(())
}

pub fn mark_killed(conn: &Connection, id: &str, checkpoint_json: Option<&str>) -> Result<()> {
    let now = now_secs();
    conn.execute(
        "UPDATE jobs SET status = 'killed', finished_at = ?, checkpoint = ? WHERE id = ?",
        rusqlite::params![now, checkpoint_json, id],
    )?;
    Ok(())
}

pub fn get_job(conn: &Connection, id: &str) -> Result<Option<Job>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent, task, pid, status, log_path, started_at, finished_at,
                exit_code, last_stdout_at, modified_files, checkpoint
         FROM jobs WHERE id = ?",
    )?;
    let result = stmt.query_row(rusqlite::params![id], |r| {
        Ok(Job {
            id: r.get(0)?,
            agent: r.get(1)?,
            task: r.get(2)?,
            pid: r.get::<_, Option<u32>>(3)?,
            status: r.get(4)?,
            log_path: r.get(5)?,
            started_at: r.get(6)?,
            finished_at: r.get(7)?,
            exit_code: r.get(8)?,
            last_stdout_at: r.get(9)?,
            modified_files: r.get(10)?,
            checkpoint: r.get(11)?,
        })
    });
    match result {
        Ok(j) => Ok(Some(j)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn list_active(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent FROM jobs WHERE finished_at IS NULL ORDER BY started_at DESC",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

pub fn append_tail_line(conn: &Connection, job_id: &str, stream: &str, content: &str) -> Result<()> {
    let now = now_secs();
    let next_line_no: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(line_no), 0) + 1 FROM job_logs_tail WHERE job_id = ?",
            rusqlite::params![job_id],
            |r| r.get(0),
        )
        .unwrap_or(1);

    conn.execute(
        "INSERT INTO job_logs_tail(job_id, line_no, stream, content, ts) VALUES (?, ?, ?, ?, ?)",
        rusqlite::params![job_id, next_line_no, stream, content, now],
    )?;

    // Prune: keep last 50 lines
    conn.execute(
        "DELETE FROM job_logs_tail WHERE job_id = ? AND line_no <= (SELECT MAX(line_no) - 50 FROM job_logs_tail WHERE job_id = ?)",
        rusqlite::params![job_id, job_id],
    )?;

    Ok(())
}

pub fn get_tail(conn: &Connection, job_id: &str) -> Result<Vec<TailLine>> {
    let mut stmt = conn.prepare(
        "SELECT line_no, stream, content, ts FROM job_logs_tail WHERE job_id = ? ORDER BY line_no ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![job_id], |r| {
            Ok(TailLine {
                line_no: r.get(0)?,
                stream: r.get(1)?,
                content: r.get(2)?,
                ts: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

pub fn tick_heuristic_status(conn: &Connection) -> Result<()> {
    let now = now_secs() as i64;
    conn.execute(
        "UPDATE jobs SET status = CASE
            WHEN ? - last_stdout_at < 30 THEN 'working'
            WHEN ? - last_stdout_at < 120 THEN 'possibly_waiting'
            WHEN ? - last_stdout_at < 600 THEN 'idle'
            ELSE 'stuck'
         END
         WHERE finished_at IS NULL",
        rusqlite::params![now, now, now],
    )?;
    Ok(())
}
