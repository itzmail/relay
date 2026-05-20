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
    /// Identifier of the sending agent (non-empty, max 128 chars)
    pub from: String,
    /// Target agent id. Omit for broadcast.
    pub to: Option<String>,
    /// Optional topic tag (e.g. "task", "alerts")
    pub topic: Option<String>,
    /// Message body (plain text or JSON string)
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
    /// Agent reading messages — only receives its DMs + broadcasts
    pub agent_id: String,
    /// Filter by topic. Omit for all topics.
    pub topic: Option<String>,
    /// Max messages to return (default 50, max 200)
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
