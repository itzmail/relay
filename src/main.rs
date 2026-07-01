use anyhow::Result;
use clap::{Parser, Subcommand};

mod config;
mod mcp;
mod session;
mod setup;
mod updater;

#[derive(Parser)]
#[command(name = "relay", about = "AI coding agent mesh coordinator", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up relay mesh for this project (hooks, MCP, CLAUDE.md)
    Init,

    /// Session management (mesh discovery)
    Session {
        #[command(subcommand)]
        cmd: SessionCommands,
    },

    /// MCP server daemon commands
    Mcp {
        #[command(subcommand)]
        cmd: mcp::cli::McpCommands,
    },

    /// Update relay to the latest version from GitHub Releases
    Update {
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Write session file for this process (called by SessionStart hook)
    Write {
        #[arg(long, default_value = "")]
        role: String,
    },
    /// Delete session file for this process (called by SessionEnd hook)
    Delete,
    /// Update session status (called by Pre/PostToolUse hooks)
    Status {
        value: String,
    },
    /// List active sessions in the mesh
    List,
    /// Join this project's relay mesh (enables unread message notifications)
    Join,
    /// Leave the relay mesh for this project
    Leave,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            init_interactive().await?;
        }
        Commands::Session { cmd } => match cmd {
            SessionCommands::Write { role } => {
                session::write_session(&role)?;
            }
            SessionCommands::Delete => {
                session::delete_session()?;
            }
            SessionCommands::Status { value } => {
                session::update_status(&value)?;
            }
            SessionCommands::List => {
                let sessions = session::list_sessions();
                if sessions.is_empty() {
                    println!("No active relay sessions.");
                } else {
                    println!("Active sessions:");
                    for s in &sessions {
                        let name = if s.name.is_empty() { &s.session_id[..8] } else { s.name.as_str() };
                        let joined = if session::is_joined(s.pid) { " [joined]" } else { "" };
                        println!(
                            "  [pid:{}] name=\"{}\"{} workspace={} status={}",
                            s.pid, name, joined, s.workspace, s.status
                        );
                    }
                }
            }
            SessionCommands::Join => {
                let cwd = std::env::current_dir()?.to_string_lossy().to_string();
                session::join_session(&cwd)?;
            }
            SessionCommands::Leave => {
                let cwd = std::env::current_dir()?.to_string_lossy().to_string();
                session::leave_session(&cwd)?;
            }
        },
        Commands::Mcp { cmd } => {
            mcp::cli::dispatch(cmd).await?;
        }
        Commands::Update { yes } => {
            run_update(yes).await?;
        }
    }

    Ok(())
}

async fn init_interactive() -> Result<()> {
    use std::io::{self, BufRead, Write};

    println!("Setting up Relay mesh for this project...");
    println!();

    print!("What is this session's role or task?\n(e.g. 'master', 'backend', 'review the plan for gaps')\n> ");
    io::stdout().flush()?;
    let mut role_input = String::new();
    io::stdin().lock().read_line(&mut role_input)?;
    let role = role_input.trim().to_string();

    print!("Install hooks globally (all projects) or project-only?\n  [1] Global (~/.claude/settings.json)\n  [2] This project only (.claude/settings.json)\n> ");
    io::stdout().flush()?;
    let mut scope_input = String::new();
    io::stdin().lock().read_line(&mut scope_input)?;
    let global = scope_input.trim() == "1";

    println!();
    println!("Injecting Claude Code hooks...");
    setup::inject_hooks(global)?;

    println!("Installing MCP config...");
    mcp::cli::install_for_init()?;

    println!("Injecting relay mesh instructions into CLAUDE.md...");
    setup::setup_claude_code(global)?;

    std::fs::create_dir_all("/tmp/relay-sessions")?;

    // persist role to .relay-role so hooks can read it on SessionStart
    if !role.is_empty() {
        std::fs::write(".relay-role", &role)?;
    }

    println!();
    println!("Done! Relay mesh is ready.");
    if !role.is_empty() {
        println!("  Role : {}", role);
    }
    println!("  Open another session and run `relay init` to join the mesh.");
    println!("  Use `relay session list` or MCP tool `relay_sessions` to see active sessions.");

    Ok(())
}

async fn run_update(yes: bool) -> Result<()> {
    println!("Checking for updates...");

    let info = match updater::force_check_latest_version().await? {
        Some(i) => i,
        None => {
            println!("Already up to date (v{}).", updater::CURRENT_VERSION);
            return Ok(());
        }
    };

    let asset_url = match &info.asset_url {
        Some(url) => url.clone(),
        None => {
            println!(
                "New version available: v{} → v{}\nRelease: {}\n\nNo pre-built binary found for this platform. Build from source:\n  cargo install --git https://github.com/itzmail/relay",
                info.current, info.latest, info.release_url
            );
            return Ok(());
        }
    };

    println!("New version available: v{} → v{}", info.current, info.latest);

    if !yes {
        print!("Download and install? [y/N] ");
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!("Downloading relay v{}...", info.latest);
    updater::download_and_install(&asset_url).await?;
    println!("Updated to v{}. Run `relay --version` to verify.", info.latest);

    Ok(())
}
