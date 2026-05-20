use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PingResponse {
    pub pong: bool,
    pub timestamp: u64,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct RelayService {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl RelayService {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl RelayService {
    #[tool(description = "Health check — returns pong with server timestamp and version")]
    fn relay_ping(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let response = PingResponse {
            pong: true,
            timestamp,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        serde_json::to_string(&response).unwrap_or_else(|_| r#"{"pong":true}"#.to_string())
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
