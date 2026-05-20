use anyhow::{Result, bail};
use clap::Subcommand;
use rusqlite::Connection;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::config::RelayConfig;
use super::{daemon, db, installer, paths, server, spawn, status};

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
    /// Write MCP client config for AI coding agents to disk
    Install {
        /// Skip prompts; install all detected agents with defaults
        #[arg(long)]
        yes: bool,
        /// Print changes without writing to disk
        #[arg(long)]
        dry_run: bool,
        /// Target specific agents: claude,codex,copilot
        #[arg(long, value_delimiter = ',')]
        target: Vec<String>,
    },
}

pub async fn dispatch(cmd: McpCommands) -> Result<()> {
    match cmd {
        McpCommands::Start { port, foreground } => start(port, foreground).await,
        McpCommands::Stop => stop().await,
        McpCommands::Status => show_status(),
        McpCommands::Install { yes, dry_run, target } => install(yes, dry_run, target),
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

        // Load config (optional — daemon continues with defaults if no relay.config.yaml)
        let config = RelayConfig::load().unwrap_or_else(|_| RelayConfig {
            agents: Default::default(),
            max_concurrent_jobs: 4,
        });

        let registry = Arc::new(spawn::JobRegistry::new());
        let paths_arc = Arc::new(p);
        let config_arc = Arc::new(config);

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            cancel_clone.cancel();
        });

        server::run_server(port, cancel, db_handle, registry, config_arc, paths_arc).await?;
        let p = paths(); // re-derive after move
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
    fs::create_dir_all(&p.jobs_dir)?;
    db::open_or_init(&p.db)?;

    let child_pid = daemon::spawn_daemon(port, &p.log)?;
    daemon::write_pid(&p.pid, child_pid)?;
    daemon::write_port(&p.port, port)?;
    println!("Relay MCP daemon started (PID {child_pid}, port {port})");
    println!("Log: {}", p.log.display());

    Ok(())
}

async fn stop() -> Result<()> {
    let p = paths();

    let pid = daemon::read_pid(&p.pid)
        .ok_or_else(|| anyhow::anyhow!("No PID file found. Is the daemon running?"))?;

    if !daemon::is_pid_alive(pid) {
        println!("Process {pid} not alive. Cleaning up stale files.");
        daemon::cleanup(&p.pid, &p.port);
        return Ok(());
    }

    // Check for active jobs
    if let Ok(conn) = db::open_or_init(&p.db) {
        if let Ok(active) = super::jobs::list_active(&conn) {
            if !active.is_empty() {
                println!("{} job(s) still running:", active.len());
                for (id, agent) in &active {
                    println!("  - {} (job {})", agent, id);
                }
                print!("Kill all & checkpoint? [y/N]: ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    bail!("Stop aborted. Daemon still running.");
                }
                // Daemon itself will handle checkpoint kill via shutdown hook
            }
        }
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

fn install(yes: bool, dry_run: bool, targets: Vec<String>) -> Result<()> {
    let p = paths();
    let port = fs::read_to_string(&p.port)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(7777);
    let url = format!("http://localhost:{port}/mcp");

    if !p.port.exists() {
        eprintln!("Warning: daemon has not been started yet. Using default port 7777.");
        eprintln!("Run `relay mcp start` first, then `relay mcp install`.");
    }

    let detected = installer::detect_installed();
    println!("Relay MCP URL: {url}");
    println!(
        "Detected in PATH: {}",
        if detected.is_empty() { "none".to_string() } else { detected.join(", ") }
    );
    println!();

    let chosen: Vec<&'static str> = if !targets.is_empty() {
        targets
            .iter()
            .filter_map(|t| match t.as_str() {
                "claude" => Some("claude"),
                "codex" => Some("codex"),
                "copilot" => Some("copilot"),
                other => { eprintln!("Unknown target '{other}'. Valid: claude, codex, copilot"); None }
            })
            .collect()
    } else if yes {
        detected.clone()
    } else {
        prompt_targets(&detected)?
    };

    if chosen.is_empty() {
        println!("Nothing to install.");
        return Ok(());
    }

    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot resolve home dir"))?;

    for agent in chosen {
        let (config_path, label) = match agent {
            "claude" => resolve_claude_path(yes || !targets.is_empty(), &home)?,
            "codex" => (home.join(".codex").join("config.toml"), "Codex".to_string()),
            "copilot" => (home.join(".copilot").join("mcp-config.json"), "Copilot".to_string()),
            _ => continue,
        };

        let result = match agent {
            "claude" => installer::install_claude(&url, &config_path, dry_run),
            "codex" => installer::install_codex(&url, &config_path, dry_run),
            "copilot" => installer::install_copilot(&url, &config_path, dry_run),
            _ => unreachable!(),
        };

        match result {
            Ok(content) if dry_run => {
                println!("--- {label} (dry-run): {} ---", config_path.display());
                println!("{content}");
            }
            Ok(_) => println!("  ✓ {label}: wrote {}", config_path.display()),
            Err(e) => eprintln!("  ✗ {label}: {e}"),
        }
    }

    Ok(())
}

fn prompt_targets(detected: &[&'static str]) -> Result<Vec<&'static str>> {
    let stdin = io::stdin();
    let mut chosen = Vec::new();

    for &agent in &["claude", "codex", "copilot"] {
        let installed = detected.contains(&agent);
        let default_hint = if installed { "Y/n" } else { "y/N" };
        print!("Install for {agent}? [{default_hint}] ");
        io::stdout().flush()?;
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let trimmed = line.trim();
        let pick = if trimmed.is_empty() { installed } else { trimmed.eq_ignore_ascii_case("y") };
        if pick {
            chosen.push(agent);
        }
    }
    Ok(chosen)
}

fn resolve_claude_path(yes: bool, home: &Path) -> Result<(PathBuf, String)> {
    if yes {
        let path = std::env::current_dir()?.join(".mcp.json");
        return Ok((path, "Claude Code (project)".to_string()));
    }
    println!("Claude Code config scope:");
    println!("  [1] Project (./.mcp.json)  <- recommended");
    println!("  [2] Global  (~/.claude.json)");
    print!("Select [1]: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    match line.trim() {
        "" | "1" => Ok((std::env::current_dir()?.join(".mcp.json"), "Claude Code (project)".to_string())),
        "2" => Ok((home.join(".claude.json"), "Claude Code (global)".to_string())),
        _ => bail!("Invalid selection"),
    }
}

fn setup_tracing_stderr() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("relay=info".parse().unwrap()))
        .with_target(false)
        .init();
}
