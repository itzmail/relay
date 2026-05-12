use anyhow::Result;
use clap::{Parser, Subcommand};

mod adapters;
mod config;
mod context;
mod runner;

#[derive(Parser)]
#[command(name = "relay", about = "AI agent executor for Claude Code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize relay.config.yaml interactively
    Init,

    /// Show execution plan and ask for confirmation before running
    Plan {
        agent: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "")]
        context: String,
    },

    /// Run an agent with a task and context (non-interactive)
    Run {
        agent: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "")]
        context: String,
        /// Override model from config
        #[arg(long)]
        model: Option<String>,
    },

    /// Agent management subcommands
    Agent {
        #[command(subcommand)]
        cmd: AgentCommands,
    },

    /// Config subcommands
    Config {
        #[command(subcommand)]
        cmd: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    List,
    Check,
}

#[derive(Subcommand)]
enum ConfigCommands {
    Show,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            runner::init().await?;
        }
        Commands::Plan { agent, task, context } => {
            runner::plan(&agent, &task, &context).await?;
        }
        Commands::Run { agent, task, context, model } => {
            let output = runner::run(&agent, &task, &context, model.as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Commands::Agent { cmd } => match cmd {
            AgentCommands::List => runner::agent_list()?,
            AgentCommands::Check => runner::agent_check()?,
        },
        Commands::Config { cmd } => match cmd {
            ConfigCommands::Show => runner::config_show()?,
        },
    }

    Ok(())
}
