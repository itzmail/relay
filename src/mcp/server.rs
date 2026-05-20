use anyhow::Result;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, StreamableHttpServerConfig, session::local::LocalSessionManager,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::config::RelayConfig;
use super::spawn::{JobRegistry, monitor_loop};
use super::tools::RelayService;
use super::RelayPaths;

pub async fn run_server(
    port: u16,
    cancel: CancellationToken,
    db: Arc<Mutex<Connection>>,
    registry: Arc<JobRegistry>,
    config: Arc<RelayConfig>,
    paths: Arc<RelayPaths>,
) -> Result<()> {
    // Start heuristic status monitor
    let monitor_cancel = cancel.child_token();
    let db_monitor = db.clone();
    tokio::spawn(monitor_loop(db_monitor, monitor_cancel));

    let service: StreamableHttpService<RelayService, LocalSessionManager> =
        StreamableHttpService::new(
            {
                let db = db.clone();
                let registry = registry.clone();
                let config = config.clone();
                let paths = paths.clone();
                move || Ok(RelayService::new(db.clone(), registry.clone(), config.clone(), paths.clone()))
            },
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
        .with_graceful_shutdown(async move {
            cancel.cancelled_owned().await;
            // Kill all active jobs with checkpoint before shutting down
            registry.kill_all(true, db).await;
        })
        .await?;

    Ok(())
}

async fn health_handler() -> &'static str {
    "ok"
}
