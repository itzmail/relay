use anyhow::Result;
use dashmap::DashMap;
use rmcp::ErrorData;
use rusqlite::Connection;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::adapters;
use crate::config::RelayConfig;
use crate::runner::{compute_modified, git_snapshot};
use super::jobs;
use super::RelayPaths;

pub struct JobHandle {
    pub job_id: String,
    pub pid: u32,
    // tokio::sync::Mutex so we can .await inside async fn without Send issues
    stdin: Arc<tokio::sync::Mutex<Option<tokio::process::ChildStdin>>>,
    #[allow(dead_code)]
    cancel: CancellationToken,
}

pub struct SpawnResult {
    pub job_id: String,
    pub pid: u32,
    pub log_path: String,
}

#[derive(Clone)]
pub struct JobRegistry {
    jobs: Arc<DashMap<String, Arc<JobHandle>>>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self { jobs: Arc::new(DashMap::new()) }
    }

    pub fn count_active(&self) -> usize {
        self.jobs.len()
    }

    pub fn active_ids(&self) -> Vec<String> {
        self.jobs.iter().map(|e| e.key().clone()).collect()
    }

    pub async fn spawn(
        &self,
        agent_name: &str,
        task: &str,
        context: &str,
        model_override: Option<&str>,
        config: &RelayConfig,
        db: Arc<Mutex<Connection>>,
        paths: &RelayPaths,
    ) -> Result<SpawnResult, ErrorData> {
        // Concurrency check
        if self.jobs.len() >= config.max_concurrent_jobs {
            return Err(ErrorData::invalid_params(
                format!("max_concurrent_jobs ({}) reached", config.max_concurrent_jobs),
                None,
            ));
        }

        // Validate and get agent config
        let agent_cfg = config.get_agent(agent_name).map_err(|e| {
            ErrorData::invalid_params(e.to_string(), None)
        })?;

        if !agent_cfg.enabled {
            return Err(ErrorData::invalid_params(
                format!("Agent '{}' is disabled. Run `relay init` to reconfigure.", agent_name),
                None,
            ));
        }

        let mut agent_cfg = agent_cfg.clone();
        if let Some(m) = model_override {
            agent_cfg.default_model = m.to_string();
        }

        let adapter = adapters::get_adapter(agent_name, &agent_cfg).map_err(|e| {
            ErrorData::invalid_params(e.to_string(), None)
        })?;

        let (cmd, args) = adapter.spawn_args(task, context);

        // Setup job ID and paths
        let job_id = uuid::Uuid::new_v4().to_string();
        fs::create_dir_all(&paths.jobs_dir).map_err(|e| {
            ErrorData::internal_error(format!("cannot create jobs dir: {e}"), None)
        })?;
        let log_path = paths.jobs_dir.join(format!("{job_id}.log"));
        let log_path_str = log_path.display().to_string();

        // Spawn process with piped IO
        #[cfg(unix)]
        let mut child = {
            use std::os::unix::process::CommandExt;
            unsafe {
                Command::new(&cmd)
                    .args(&args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .pre_exec(|| { nix::libc::setsid(); Ok(()) })
                    .spawn()
                    .map_err(|e| ErrorData::internal_error(format!("spawn failed: {e}"), None))?
            }
        };

        #[cfg(not(unix))]
        let mut child = Command::new(&cmd)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ErrorData::internal_error(format!("spawn failed: {e}"), None))?;

        let pid = child.id().unwrap_or(0);

        // Extract IO handles before moving child
        let child_stdin = child.stdin.take();
        let child_stdout = child.stdout.take().unwrap();
        let child_stderr = child.stderr.take().unwrap();

        // Insert job into DB
        {
            let conn = db.lock().unwrap();
            jobs::insert_job(&conn, &job_id, agent_name, task, pid, &log_path_str)
                .map_err(|e| ErrorData::internal_error(format!("db error: {e}"), None))?;
        }

        let cancel = CancellationToken::new();
        let stdin_arc: Arc<tokio::sync::Mutex<Option<tokio::process::ChildStdin>>> =
            Arc::new(tokio::sync::Mutex::new(child_stdin));

        let handle = Arc::new(JobHandle {
            job_id: job_id.clone(),
            pid,
            stdin: stdin_arc.clone(),
            cancel: cancel.clone(),
        });

        self.jobs.insert(job_id.clone(), handle);

        // Background: stdout reader + tee to log file
        let db_out = db.clone();
        let jid_out = job_id.clone();
        let log_path_out = log_path.clone();
        tokio::spawn(async move {
            let mut log_file = match tokio::fs::OpenOptions::new()
                .create(true).append(true).open(&log_path_out).await
            {
                Ok(f) => f,
                Err(e) => { tracing::error!("cannot open log file: {e}"); return; }
            };

            let mut reader = BufReader::new(child_stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = log_file.write_all(line.as_bytes()).await;
                let _ = log_file.write_all(b"\n").await;
                let conn = db_out.lock().unwrap();
                let _ = jobs::append_tail_line(&conn, &jid_out, "stdout", &line);
                let _ = jobs::update_last_stdout_at(&conn, &jid_out);
            }
        });

        // Background: stderr reader + tee to log file
        let db_err = db.clone();
        let jid_err = job_id.clone();
        let log_path_err = log_path.clone();
        tokio::spawn(async move {
            let mut log_file = match tokio::fs::OpenOptions::new()
                .create(true).append(true).open(&log_path_err).await
            {
                Ok(f) => f,
                Err(e) => { tracing::error!("cannot open stderr log file: {e}"); return; }
            };

            let mut reader = BufReader::new(child_stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = log_file.write_all(line.as_bytes()).await;
                let _ = log_file.write_all(b"\n").await;
                let conn = db_err.lock().unwrap();
                let _ = jobs::append_tail_line(&conn, &jid_err, "stderr", &line);
                let _ = jobs::update_last_stdout_at(&conn, &jid_err);
            }
        });

        // Background: wait for process exit
        let db_wait = db.clone();
        let jid_wait = job_id.clone();
        let jobs_ref = self.jobs.clone();
        tokio::spawn(async move {
            let status = child.wait().await;
            let exit_code = match status {
                Ok(s) => s.code().unwrap_or(-1),
                Err(_) => -1,
            };
            let before = git_snapshot();
            // Small gap to let last stdout lines flush
            tokio::time::sleep(Duration::from_millis(300)).await;
            let after = git_snapshot();
            let modified = compute_modified(before, after);
            let modified_json = serde_json::to_string(&modified).unwrap_or_default();
            {
                let conn = db_wait.lock().unwrap();
                let _ = jobs::mark_finished(&conn, &jid_wait, exit_code, Some(&modified_json));
            }
            jobs_ref.remove(&jid_wait);
        });

        Ok(SpawnResult { job_id, pid, log_path: log_path_str })
    }

    pub async fn kill(
        &self,
        job_id: &str,
        checkpoint: bool,
        db: Arc<Mutex<Connection>>,
    ) -> Result<Option<String>, ErrorData> {
        let handle = self.jobs.get(job_id).map(|e| e.clone());
        let handle = match handle {
            Some(h) => h,
            None => return Err(ErrorData::invalid_params(
                format!("job '{job_id}' not found or already finished"),
                None,
            )),
        };

        if checkpoint {
            try_checkpoint_via_stdin(&handle).await;
        }

        // Snapshot tail before kill
        let tail_snapshot = {
            let conn = db.lock().unwrap();
            jobs::get_tail(&conn, job_id)
                .ok()
                .map(|lines| {
                    serde_json::to_string(
                        &lines.iter().map(|l| &l.content).collect::<Vec<_>>()
                    ).unwrap_or_default()
                })
        };

        // Send SIGTERM/SIGKILL
        super::daemon::kill_pid(handle.pid).map_err(|e| {
            ErrorData::internal_error(format!("kill error: {e}"), None)
        })?;

        {
            let conn = db.lock().unwrap();
            let _ = jobs::mark_killed(&conn, job_id, tail_snapshot.as_deref());
        }

        self.jobs.remove(job_id);
        Ok(None)
    }

    pub async fn kill_all(&self, checkpoint: bool, db: Arc<Mutex<Connection>>) {
        let ids: Vec<String> = self.active_ids();
        for id in ids {
            if let Err(e) = self.kill(&id, checkpoint, db.clone()).await {
                tracing::warn!("kill job {id} error: {e:?}");
            }
        }
    }
}

async fn try_checkpoint_via_stdin(handle: &JobHandle) {
    let msg = "\n[RELAY CHECKPOINT] Daemon shutting down. Briefly summarize: (1) what's done, (2) what's in progress, (3) blockers. You have 30 seconds.\n";

    let mut stdin_guard = handle.stdin.lock().await;
    if let Some(ref mut stdin) = *stdin_guard {
        let _ = stdin.write_all(msg.as_bytes()).await;
    } else {
        return;
    }
    drop(stdin_guard);

    // Wait up to 30s for additional stdout (captured by background reader → tail)
    tokio::time::sleep(Duration::from_secs(30)).await;
}

pub async fn monitor_loop(db: Arc<Mutex<Connection>>, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = interval.tick() => {
                let conn = db.lock().unwrap();
                let _ = jobs::tick_heuristic_status(&conn);
            }
        }
    }
}
