use anyhow::{Result, bail};
use clap::Subcommand;
use rusqlite::Connection;
use std::fs;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use super::{daemon, db, paths, server, status};

#[derive(Subcommand)]
pub enum McpCommands {
    /// Start the Relay MCP daemon
    Start {
        #[arg(long, default_value_t = 7777)]
        port: u16,
        /// Run in foreground (no daemonize)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the Relay MCP daemon
    Stop,
    /// Show daemon status
    Status,
    /// Generate MCP client config for AI coding agents
    Install,
}

pub async fn dispatch(cmd: McpCommands) -> Result<()> {
    match cmd {
        McpCommands::Start { port, foreground } => start(port, foreground).await,
        McpCommands::Stop => stop(),
        McpCommands::Status => show_status(),
        McpCommands::Install => install(),
    }
}

async fn start(port: u16, foreground: bool) -> Result<()> {
    let p = paths();

    if foreground {
        // Child re-exec path: parent already wrote PID file, skip guard.
        setup_tracing_stderr();
        tracing::info!("Starting Relay MCP server on port {port} (foreground)");

        let conn = db::open_or_init(&p.db)?;
        let db_handle: Arc<Mutex<Connection>> = Arc::new(Mutex::new(conn));

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            cancel_clone.cancel();
        });

        server::run_server(port, cancel, db_handle).await?;
        daemon::cleanup(&p.pid, &p.port);
        return Ok(());
    }

    // Guard: already running?
    if let Some(pid) = daemon::read_pid(&p.pid) {
        if daemon::is_pid_alive(pid) {
            bail!("Relay MCP daemon already running (PID {pid}). Run `relay mcp stop` first.");
        }
        daemon::cleanup(&p.pid, &p.port);
    }

    fs::create_dir_all(&p.dir)?;
    db::open_or_init(&p.db)?;

    let child_pid = daemon::spawn_daemon(port, &p.log)?;
    daemon::write_pid(&p.pid, child_pid)?;
    daemon::write_port(&p.port, port)?;
    println!("Relay MCP daemon started (PID {child_pid}, port {port})");
    println!("Log: {}", p.log.display());

    Ok(())
}

fn stop() -> Result<()> {
    let p = paths();

    let pid = daemon::read_pid(&p.pid)
        .ok_or_else(|| anyhow::anyhow!("No PID file found. Is the daemon running?"))?;

    if !daemon::is_pid_alive(pid) {
        println!("Process {pid} not alive. Cleaning up stale files.");
        daemon::cleanup(&p.pid, &p.port);
        return Ok(());
    }

    print!("Stopping relay MCP daemon (PID {pid})...");
    daemon::stop_daemon(pid)?;
    daemon::cleanup(&p.pid, &p.port);
    println!(" done.");

    Ok(())
}

fn show_status() -> Result<()> {
    let p = paths();
    let s = status::get_status(&p.pid, &p.port, &p.log);
    println!("{}", status::format_status(&s));
    Ok(())
}

fn install() -> Result<()> {
    let p = paths();
    let port = fs::read_to_string(&p.port)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(7777);

    let url = format!("http://localhost:{port}/mcp");

    println!("Relay MCP server URL: {url}");
    println!();
    println!("# Claude Code / Pi (.mcp.json):");
    println!(
        r#"{{
  "mcpServers": {{
    "relay": {{
      "url": "{url}"
    }}
  }}
}}"#
    );
    println!();
    println!("# Codex (~/.codex/config.toml):");
    println!("[mcp_servers.relay]\nurl = \"{url}\"");
    println!();
    println!("# Copilot CLI (~/.copilot/mcp-config.json):");
    println!(
        r#"{{
  "mcpServers": {{
    "relay": {{
      "type": "http",
      "url": "{url}",
      "tools": ["*"]
    }}
  }}
}}"#
    );

    Ok(())
}

fn setup_tracing_stderr() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("relay=info".parse().unwrap()))
        .with_target(false)
        .init();
}
