use anyhow::Result;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, StreamableHttpServerConfig, session::local::LocalSessionManager,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use super::tools::RelayService;

pub async fn run_server(port: u16, cancel: CancellationToken, db: Arc<Mutex<Connection>>) -> Result<()> {
    let service: StreamableHttpService<RelayService, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(RelayService::new(db.clone())),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_cancellation_token(cancel.child_token()),
        );

    let router = Router::new()
        .nest_service("/mcp", service)
        .route("/health", axum::routing::get(health_handler));

    let bind_addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Relay MCP server listening on {bind_addr}");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move { cancel.cancelled_owned().await })
        .await?;

    Ok(())
}

async fn health_handler() -> &'static str {
    "ok"
}
