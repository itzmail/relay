use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

const CLAUDE_CODE_MARKER_START: &str = "<!-- relay:start -->";
const CLAUDE_CODE_MARKER_END: &str = "<!-- relay:end -->";

const SKILL_CONTENT: &str = r#"---
name: relay-init
description: Initialize Relay for this project — detect available agents, select models, generate relay.config.yaml, and inject relay instructions into CLAUDE.md
---

## Steps

1. Run `relay agent check` via Bash. Show user which agents are available in PATH.

2. Ask user (via AskUserQuestion):
   - Which agents to enable (multi-select from available list)

3. For each selected agent, ask which model to use:
   - opencode: run `opencode models` and show list
   - codex: free-text input, suggest `o4-mini`
   - copilot: choices — claude-sonnet-4.5, claude-sonnet-4, claude-haiku-4.5, gpt-5
   - pi: run `pi --list-models` and show list, fallback `anthropic/claude-sonnet-4-6`

4. Run `relay init` via Bash using a heredoc that answers the prompts automatically based on user choices. Example:
   ```bash
   printf "y\ny\nn\n" | relay init
   ```
   Construct the answer string from user's selections (y/n per agent in KNOWN_AGENTS order: opencode, codex, copilot, pi).

5. Show `relay agent list` to confirm setup.

## Notes

- `relay.config.yaml` is created in current working directory (project root).
- If relay.config.yaml already exists, ask user whether to overwrite.
- KNOWN_AGENTS order for `relay init` prompts: opencode, codex, copilot, pi.
"#;

pub fn setup_claude_code(global: bool) -> Result<()> {
    let claude_md_path = claude_md_path(global)?;

    // read existing content or empty
    let existing = if claude_md_path.exists() {
        fs::read_to_string(&claude_md_path)?
    } else {
        String::new()
    };

    // build relay block
    let relay_block = relay_instructions_block();

    let new_content = if existing.contains(CLAUDE_CODE_MARKER_START) {
        // replace existing block
        replace_between_markers(&existing, &relay_block)
    } else {
        // append
        if existing.is_empty() {
            relay_block
        } else {
            format!("{}\n\n{}", existing.trim_end(), relay_block)
        }
    };

    if let Some(parent) = claude_md_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&claude_md_path, new_content)?;
    println!("  ✓ Injected relay instructions → {}", claude_md_path.display());

    // install skill
    install_relay_init_skill(global)?;

    Ok(())
}

fn claude_md_path(global: bool) -> Result<PathBuf> {
    if global {
        let home = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("$HOME not set"))?;
        Ok(PathBuf::from(home).join(".claude").join("CLAUDE.md"))
    } else {
        // project-local: find git root or use cwd
        let cwd = std::env::current_dir()?;
        Ok(cwd.join("CLAUDE.md"))
    }
}

fn relay_instructions_block() -> String {
    format!(
        "{}\n## Relay (AI Agent Executor)\n\
Relay delegates coding tasks to other AI agents (opencode, codex, copilot, pi).\n\
Use it when a task benefits from a specialized agent or parallel execution.\n\
\n\
```bash\n\
relay run <agent> --task \"<task>\" --context \"<context>\"  # run agent\n\
relay plan <agent> --task \"<task>\"                        # dry-run preview (JSON)\n\
relay agent list                                          # list registered agents\n\
relay agent check                                         # check PATH availability\n\
relay config show                                         # show relay.config.yaml\n\
```\n\
\n\
Context format:\n\
```\n\
Goal: <overall goal>\n\
Done: <what's already done>\n\
Why: <key decisions made>\n\
Modified: <files already changed>\n\
Avoid: <things that failed, don't retry>\n\
```\n\
\n\
Run `relay init` in the project root to create relay.config.yaml.\n\
Run `/relay-init` in Claude Code to set up interactively.\n\
{}\n",
        CLAUDE_CODE_MARKER_START, CLAUDE_CODE_MARKER_END
    )
}

fn replace_between_markers(content: &str, new_block: &str) -> String {
    let start = content.find(CLAUDE_CODE_MARKER_START);
    let end = content.find(CLAUDE_CODE_MARKER_END);

    match (start, end) {
        (Some(s), Some(e)) => {
            let end_pos = e + CLAUDE_CODE_MARKER_END.len();
            format!("{}{}{}", &content[..s], new_block, &content[end_pos..])
        }
        _ => format!("{}\n\n{}", content.trim_end(), new_block),
    }
}

fn install_relay_init_skill(global: bool) -> Result<()> {
    let skill_dir = if global {
        let home = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("$HOME not set"))?;
        PathBuf::from(home).join(".claude").join("skills").join("relay-init")
    } else {
        let cwd = std::env::current_dir()?;
        cwd.join(".claude").join("skills").join("relay-init")
    };

    fs::create_dir_all(&skill_dir)?;
    let skill_path = skill_dir.join("SKILL.md");
    fs::write(&skill_path, SKILL_CONTENT)?;
    println!("  ✓ Installed /relay-init skill → {}", skill_path.display());

    Ok(())
}

pub fn list_targets() {
    println!("Supported targets:");
    println!("  claude-code    GitHub Copilot for Claude Code (CLAUDE.md + /relay-init skill)");
    println!();
    println!("Usage:");
    println!("  relay setup claude-code           # inject into project CLAUDE.md");
    println!("  relay setup claude-code --global  # inject into ~/.claude/CLAUDE.md");
}

pub fn run_setup(target: &str, global: bool) -> Result<()> {
    match target {
        "claude-code" => {
            println!("Setting up relay for Claude Code{}...", if global { " (global)" } else { "" });
            setup_claude_code(global)?;
            println!("\nDone! Next steps:");
            if !global {
                println!("  1. Run `relay init` in this project to create relay.config.yaml");
                println!("  2. Use `/relay-init` in Claude Code to set up interactively");
            } else {
                println!("  1. In any project, run `relay init` to create relay.config.yaml");
                println!("  2. Use `/relay-init` in Claude Code to set up interactively");
            }
            Ok(())
        }
        other => bail!(
            "Unknown target '{}'. Run `relay setup --list` to see supported targets.",
            other
        ),
    }
}
