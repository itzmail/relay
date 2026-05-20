use rmcp::{
    ErrorData,
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::Parameters,
    },
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::config::RelayConfig;
use super::jobs;
use super::spawn::JobRegistry;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn validate_agent_id(id: &str) -> Result<(), ErrorData> {
    if id.is_empty() || id.len() > 128 {
        return Err(ErrorData::invalid_params("agent id must be 1..=128 chars", None));
    }
    Ok(())
}

fn upsert_agent(conn: &Connection, id: &str, now: u64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO agents(id, kind, started_at, status, last_read_id, last_seen)
         VALUES (?, 'unknown', ?, 'active', 0, ?)",
        rusqlite::params![id, now, now],
    )?;
    conn.execute(
        "UPDATE agents SET last_seen = ? WHERE id = ?",
        rusqlite::params![now, id],
    )?;
    Ok(())
}

// ─── Ping ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PingResponse {
    pub pong: bool,
    pub timestamp: u64,
    pub version: String,
}

// ─── Send ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendArgs {
    pub from: String,
    pub to: Option<String>,
    pub topic: Option<String>,
    pub payload: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SendResponse {
    pub id: i64,
    pub created_at: u64,
}

// ─── Read ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadArgs {
    pub agent_id: String,
    pub topic: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct MessageItem {
    pub id: i64,
    pub from: String,
    pub to: Option<String>,
    pub topic: Option<String>,
    pub payload: String,
    pub created_at: u64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ReadResponse {
    pub messages: Vec<MessageItem>,
    pub new_cursor: i64,
}

// ─── Agents ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AgentInfo {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub started_at: u64,
    pub last_seen: u64,
    pub last_read_id: i64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AgentsResponse {
    pub agents: Vec<AgentInfo>,
}

// ─── Spawn ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SpawnArgs {
    /// Agent name: "opencode" | "codex" | "copilot" | "pi"
    pub agent: String,
    /// Task description for the agent
    pub task: String,
    /// Optional context string (goal/done/why/avoid)
    pub context: Option<String>,
    /// Override the default model for this agent
    pub model: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SpawnResponse {
    pub job_id: String,
    pub pid: u32,
    pub log_path: String,
    pub status: String,
}

// ─── Job Status ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobStatusArgs {
    pub job_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct LogLine {
    pub line_no: i64,
    pub stream: String,
    pub content: String,
    pub ts: u64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct JobStatusResponse {
    pub job_id: String,
    pub agent: String,
    pub status: String,
    pub pid: Option<u32>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub last_stdout_at: u64,
    pub exit_code: Option<i32>,
    pub modified_files: Option<Vec<String>>,
    pub tail: Vec<LogLine>,
}

// ─── Job Logs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobLogsArgs {
    pub job_id: String,
    /// Byte offset to start reading from (default 0)
    pub offset: Option<u64>,
    /// Max bytes to return (default 65536, max 1048576)
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct JobLogsResponse {
    pub content: String,
    pub next_offset: u64,
    pub eof: bool,
}

// ─── Job Kill ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobKillArgs {
    pub job_id: String,
    /// Send checkpoint prompt to agent via stdin before killing (default true)
    pub checkpoint: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct JobKillResponse {
    pub job_id: String,
    pub status: String,
    pub checkpoint_response: Option<String>,
}

// ─── Service ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RelayService {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    db: Arc<Mutex<Connection>>,
    registry: Arc<JobRegistry>,
    config: Arc<RelayConfig>,
    paths: Arc<super::RelayPaths>,
}

impl RelayService {
    pub fn new(
        db: Arc<Mutex<Connection>>,
        registry: Arc<JobRegistry>,
        config: Arc<RelayConfig>,
        paths: Arc<super::RelayPaths>,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            db,
            registry,
            config,
            paths,
        }
    }
}

#[tool_router]
impl RelayService {
    #[tool(description = "Health check — returns pong with server timestamp and version")]
    fn relay_ping(&self) -> String {
        let response = PingResponse {
            pong: true,
            timestamp: now_secs(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        serde_json::to_string(&response).unwrap_or_else(|_| r#"{"pong":true}"#.to_string())
    }

    #[tool(description = "Send a message from one agent to another (or broadcast). Returns the message id.")]
    fn relay_send(
        &self,
        Parameters(args): Parameters<SendArgs>,
    ) -> Result<String, ErrorData> {
        validate_agent_id(&args.from)?;
        if let Some(ref to) = args.to {
            validate_agent_id(to)?;
        }

        let conn = self.db.lock().map_err(|_| {
            ErrorData::internal_error("db lock poisoned", None)
        })?;

        let now = now_secs();

        upsert_agent(&conn, &args.from, now).map_err(|e| {
            tracing::error!("upsert_agent error: {e}");
            ErrorData::internal_error("db error", None)
        })?;

        conn.execute(
            "INSERT INTO messages(from_agent, to_agent, topic, payload, created_at)
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![args.from, args.to, args.topic, args.payload, now],
        )
        .map_err(|e| {
            tracing::error!("insert message error: {e}");
            ErrorData::internal_error("db error", None)
        })?;

        let id = conn.last_insert_rowid();
        let resp = SendResponse { id, created_at: now };
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tool(description = "Read unread messages for an agent. Automatically advances the read cursor.")]
    fn relay_read(
        &self,
        Parameters(args): Parameters<ReadArgs>,
    ) -> Result<String, ErrorData> {
        validate_agent_id(&args.agent_id)?;
        let limit = args.limit.unwrap_or(50).min(200) as i64;

        let conn = self.db.lock().map_err(|_| {
            ErrorData::internal_error("db lock poisoned", None)
        })?;

        let now = now_secs();

        upsert_agent(&conn, &args.agent_id, now).map_err(|e| {
            tracing::error!("upsert_agent error: {e}");
            ErrorData::internal_error("db error", None)
        })?;

        let last_read_id: i64 = conn
            .query_row(
                "SELECT last_read_id FROM agents WHERE id = ?",
                rusqlite::params![args.agent_id],
                |r| r.get(0),
            )
            .map_err(|e| {
                tracing::error!("last_read_id query error: {e}");
                ErrorData::internal_error("db error", None)
            })?;

        let mut stmt = conn
            .prepare(
                "SELECT id, from_agent, to_agent, topic, payload, created_at
                 FROM messages
                 WHERE id > ?
                   AND from_agent != ?
                   AND (to_agent IS NULL OR to_agent = ?)
                   AND (? IS NULL OR topic = ?)
                 ORDER BY id ASC
                 LIMIT ?",
            )
            .map_err(|e| {
                tracing::error!("prepare error: {e}");
                ErrorData::internal_error("db error", None)
            })?;

        let topic_param = args.topic.as_deref();
        let messages: Vec<MessageItem> = stmt
            .query_map(
                rusqlite::params![
                    last_read_id,
                    args.agent_id,
                    args.agent_id,
                    topic_param,
                    topic_param,
                    limit
                ],
                |r| {
                    Ok(MessageItem {
                        id: r.get(0)?,
                        from: r.get::<_, String>(1)?,
                        to: r.get(2)?,
                        topic: r.get(3)?,
                        payload: r.get(4)?,
                        created_at: r.get(5)?,
                    })
                },
            )
            .map_err(|e| {
                tracing::error!("query error: {e}");
                ErrorData::internal_error("db error", None)
            })?
            .filter_map(|r| r.ok())
            .collect();

        let new_cursor = messages.last().map(|m| m.id).unwrap_or(last_read_id);

        if new_cursor > last_read_id {
            conn.execute(
                "UPDATE agents SET last_read_id = ?, last_seen = ? WHERE id = ?",
                rusqlite::params![new_cursor, now, args.agent_id],
            )
            .map_err(|e| {
                tracing::error!("cursor update error: {e}");
                ErrorData::internal_error("db error", None)
            })?;
        }

        let resp = ReadResponse { messages, new_cursor };
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tool(description = "List all agents that have sent or received messages, ordered by last activity.")]
    fn relay_agents(&self) -> Result<String, ErrorData> {
        let conn = self.db.lock().map_err(|_| {
            ErrorData::internal_error("db lock poisoned", None)
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT id, kind, status, started_at, last_seen, last_read_id
                 FROM agents
                 ORDER BY last_seen DESC",
            )
            .map_err(|e| {
                tracing::error!("prepare agents error: {e}");
                ErrorData::internal_error("db error", None)
            })?;

        let agents: Vec<AgentInfo> = stmt
            .query_map([], |r| {
                Ok(AgentInfo {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    status: r.get(2)?,
                    started_at: r.get(3)?,
                    last_seen: r.get(4)?,
                    last_read_id: r.get(5)?,
                })
            })
            .map_err(|e| {
                tracing::error!("query agents error: {e}");
                ErrorData::internal_error("db error", None)
            })?
            .filter_map(|r| r.ok())
            .collect();

        let resp = AgentsResponse { agents };
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tool(description = "Spawn an AI coding agent (opencode/codex/copilot/pi) as a background job. Returns job_id for tracking.")]
    fn relay_spawn(
        &self,
        Parameters(args): Parameters<SpawnArgs>,
    ) -> Result<String, ErrorData> {
        let registry = self.registry.clone();
        let db = self.db.clone();
        let config = self.config.clone();
        let paths = self.paths.clone();
        let context = args.context.unwrap_or_default();
        let model = args.model.clone();

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                registry.spawn(
                    &args.agent,
                    &args.task,
                    &context,
                    model.as_deref(),
                    &config,
                    db,
                    &paths,
                ).await
            })
        })?;

        let resp = SpawnResponse {
            job_id: result.job_id,
            pid: result.pid,
            log_path: result.log_path,
            status: "working".to_string(),
        };
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tool(description = "Get status and last 50 lines of output for a spawned job.")]
    fn relay_job_status(
        &self,
        Parameters(args): Parameters<JobStatusArgs>,
    ) -> Result<String, ErrorData> {
        let conn = self.db.lock().map_err(|_| {
            ErrorData::internal_error("db lock poisoned", None)
        })?;

        let job = jobs::get_job(&conn, &args.job_id)
            .map_err(|e| ErrorData::internal_error(format!("db error: {e}"), None))?
            .ok_or_else(|| ErrorData::invalid_params(format!("job '{}' not found", args.job_id), None))?;

        let tail_lines = jobs::get_tail(&conn, &args.job_id)
            .unwrap_or_default()
            .into_iter()
            .map(|l| LogLine {
                line_no: l.line_no,
                stream: l.stream,
                content: l.content,
                ts: l.ts,
            })
            .collect();

        let modified_files: Option<Vec<String>> = job
            .modified_files
            .and_then(|s| serde_json::from_str(&s).ok());

        let resp = JobStatusResponse {
            job_id: job.id,
            agent: job.agent,
            status: job.status,
            pid: job.pid,
            started_at: job.started_at,
            finished_at: job.finished_at,
            last_stdout_at: job.last_stdout_at,
            exit_code: job.exit_code,
            modified_files,
            tail: tail_lines,
        };
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tool(description = "Read raw log output for a job from byte offset. Use next_offset for pagination.")]
    fn relay_job_logs(
        &self,
        Parameters(args): Parameters<JobLogsArgs>,
    ) -> Result<String, ErrorData> {
        let conn = self.db.lock().map_err(|_| {
            ErrorData::internal_error("db lock poisoned", None)
        })?;

        let job = jobs::get_job(&conn, &args.job_id)
            .map_err(|e| ErrorData::internal_error(format!("db error: {e}"), None))?
            .ok_or_else(|| ErrorData::invalid_params(format!("job '{}' not found", args.job_id), None))?;

        drop(conn); // release lock before file IO

        let offset = args.offset.unwrap_or(0);
        let max_bytes = args.max_bytes.unwrap_or(65536).min(1048576);

        let file_result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let mut file = tokio::fs::File::open(&job.log_path).await?;
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                let mut buf = vec![0u8; max_bytes as usize];
                let n = file.read(&mut buf).await?;
                buf.truncate(n);
                Ok::<(Vec<u8>, bool), std::io::Error>((buf, n == 0 && job.finished_at.is_some()))
            })
        });

        let (bytes, eof) = file_result.map_err(|e: std::io::Error| {
            ErrorData::internal_error(format!("log read error: {e}"), None)
        })?;

        let content = String::from_utf8_lossy(&bytes).to_string();
        let next_offset = offset + bytes.len() as u64;

        let resp = JobLogsResponse { content, next_offset, eof };
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tool(description = "Kill a running job. Optionally send checkpoint prompt via stdin first (default: true).")]
    fn relay_job_kill(
        &self,
        Parameters(args): Parameters<JobKillArgs>,
    ) -> Result<String, ErrorData> {
        let registry = self.registry.clone();
        let db = self.db.clone();
        let checkpoint = args.checkpoint.unwrap_or(true);
        let job_id = args.job_id.clone();

        let checkpoint_response = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                registry.kill(&job_id, checkpoint, db).await
            })
        })?;

        let resp = JobKillResponse {
            job_id: args.job_id,
            status: "killed".to_string(),
            checkpoint_response,
        };
        Ok(serde_json::to_string(&resp).unwrap())
    }
}

#[tool_handler]
impl ServerHandler for RelayService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_instructions("Relay MCP — AI agent coordination hub")
    }
}
