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

use crate::session;

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

// ─── Sessions ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SessionsResponse {
    pub sessions: Vec<session::Session>,
}

// ─── Clarify ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClarifyArgs {
    /// ID or role of the session asking for clarification
    pub from: String,
    /// The ambiguous question
    pub question: String,
    /// Role to route to first; omit to escalate directly to master
    pub target_role: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ClarifyResponse {
    pub routed_to: String,
    pub message_id: i64,
}

// ─── Service ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RelayService {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    db: Arc<Mutex<Connection>>,
}

impl RelayService {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            db,
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

    #[tool(description = "Send a message or context from one session to another by role (or broadcast). Returns the message id.")]
    fn relay_send(
        &self,
        Parameters(args): Parameters<SendArgs>,
    ) -> Result<String, ErrorData> {
        validate_agent_id(&args.from)?;
        if let Some(ref to) = args.to {
            validate_agent_id(to)?;
        }

        let conn = self.db.lock().map_err(|_| ErrorData::internal_error("db lock poisoned", None))?;
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
        Ok(serde_json::to_string(&SendResponse { id, created_at: now }).unwrap())
    }

    #[tool(description = "Read unread messages for this session. Automatically advances the read cursor.")]
    fn relay_read(
        &self,
        Parameters(args): Parameters<ReadArgs>,
    ) -> Result<String, ErrorData> {
        validate_agent_id(&args.agent_id)?;
        let limit = args.limit.unwrap_or(50).min(200) as i64;

        let conn = self.db.lock().map_err(|_| ErrorData::internal_error("db lock poisoned", None))?;
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
                rusqlite::params![last_read_id, args.agent_id, args.agent_id, topic_param, topic_param, limit],
                |r| Ok(MessageItem {
                    id: r.get(0)?,
                    from: r.get::<_, String>(1)?,
                    to: r.get(2)?,
                    topic: r.get(3)?,
                    payload: r.get(4)?,
                    created_at: r.get(5)?,
                }),
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

        Ok(serde_json::to_string(&ReadResponse { messages, new_cursor }).unwrap())
    }

    #[tool(description = "List all sessions that have used the message bus, ordered by last activity.")]
    fn relay_agents(&self) -> Result<String, ErrorData> {
        let conn = self.db.lock().map_err(|_| ErrorData::internal_error("db lock poisoned", None))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, kind, status, started_at, last_seen, last_read_id
                 FROM agents ORDER BY last_seen DESC",
            )
            .map_err(|e| {
                tracing::error!("prepare agents error: {e}");
                ErrorData::internal_error("db error", None)
            })?;

        let agents: Vec<AgentInfo> = stmt
            .query_map([], |r| Ok(AgentInfo {
                id: r.get(0)?,
                kind: r.get(1)?,
                status: r.get(2)?,
                started_at: r.get(3)?,
                last_seen: r.get(4)?,
                last_read_id: r.get(5)?,
            }))
            .map_err(|e| {
                tracing::error!("query agents error: {e}");
                ErrorData::internal_error("db error", None)
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(serde_json::to_string(&AgentsResponse { agents }).unwrap())
    }

    #[tool(description = "List all active sessions in this relay mesh. Validates PID liveness and cleans up stale entries automatically.")]
    fn relay_sessions(&self) -> String {
        let sessions = session::list_sessions();
        serde_json::to_string(&SessionsResponse { sessions })
            .unwrap_or_else(|_| r#"{"sessions":[]}"#.to_string())
    }

    #[tool(description = "Request clarification from a target role. If the target role cannot be found, escalates to master automatically.")]
    fn relay_clarify(
        &self,
        Parameters(args): Parameters<ClarifyArgs>,
    ) -> Result<String, ErrorData> {
        let conn = self.db.lock().map_err(|_| ErrorData::internal_error("db lock poisoned", None))?;
        let now = now_secs();

        let sessions = session::list_sessions();
        let target = args.target_role.as_deref().unwrap_or("master");
        let routed_to = sessions
            .iter()
            .find(|s| s.role.to_lowercase().contains(&target.to_lowercase()))
            .map(|s| s.role.clone())
            .unwrap_or_else(|| "master".to_string());

        conn.execute(
            "INSERT INTO messages(from_agent, to_agent, topic, payload, created_at)
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![args.from, routed_to, "clarification", args.question, now],
        )
        .map_err(|e| {
            tracing::error!("clarify insert error: {e}");
            ErrorData::internal_error("db error", None)
        })?;

        let id = conn.last_insert_rowid();
        Ok(serde_json::to_string(&ClarifyResponse { routed_to, message_id: id }).unwrap())
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
        .with_instructions("Relay MCP — AI coding session mesh coordinator")
    }
}
