use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub fn open_or_init(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    run_migrations(&conn)?;
    Ok(conn)
}

fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            pid INTEGER,
            started_at INTEGER NOT NULL,
            status TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            from_agent TEXT,
            to_agent TEXT,
            topic TEXT,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_messages_topic ON messages(topic);

        INSERT OR IGNORE INTO schema_version(version) VALUES (1);
        ",
    )?;

    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )?;

    if version < 2 {
        conn.execute_batch(
            "
            ALTER TABLE agents ADD COLUMN last_read_id INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE agents ADD COLUMN last_seen INTEGER NOT NULL DEFAULT 0;

            CREATE INDEX IF NOT EXISTS idx_messages_to_agent ON messages(to_agent);

            INSERT OR IGNORE INTO schema_version(version) VALUES (2);
            ",
        )?;
    }

    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )?;

    if version < 3 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                agent TEXT NOT NULL,
                task TEXT NOT NULL,
                pid INTEGER,
                status TEXT NOT NULL,
                log_path TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                exit_code INTEGER,
                last_stdout_at INTEGER NOT NULL,
                modified_files TEXT,
                checkpoint TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
            CREATE INDEX IF NOT EXISTS idx_jobs_started_at ON jobs(started_at);

            CREATE TABLE IF NOT EXISTS job_logs_tail (
                job_id TEXT NOT NULL,
                line_no INTEGER NOT NULL,
                stream TEXT NOT NULL,
                content TEXT NOT NULL,
                ts INTEGER NOT NULL,
                PRIMARY KEY (job_id, line_no)
            );

            CREATE INDEX IF NOT EXISTS idx_job_logs_tail_job ON job_logs_tail(job_id, line_no);

            INSERT OR IGNORE INTO schema_version(version) VALUES (3);
            ",
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_bootstrap_in_memory() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let version: i64 = conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);

        // Idempotent — run again, should not error
        run_migrations(&conn).unwrap();

        let version2: i64 = conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version2, 3);
    }
}
