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

    /// Run an agent with a task and context
    Run {
        agent: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "")]
        context: String,
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
        Commands::Run { agent, task, context } => {
            let output = runner::run(&agent, &task, &context).await?;
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
