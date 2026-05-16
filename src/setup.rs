use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

const CLAUDE_CODE_MARKER_START: &str = "<!-- relay:start -->";
const CLAUDE_CODE_MARKER_END: &str = "<!-- relay:end -->";

const SKILL_RELAY_INIT: &str = r#"---
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

const SKILL_RELAY_PLAN: &str = r#"---
name: relay-plan
description: Orchestrate a task using Relay — break into subtasks, assign agents, get user approval, execute in parallel, review results, and close or fix.
---

## Steps

### 1. Generate plan

First, run `relay agent list` to see which agents are enabled and available.

Then, using your own reasoning, break the user's task into subtasks. For each subtask, decide:
- Which agent is best suited (`opencode`, `codex`, `copilot`, or `pi`)
- What the specific task is
- Why that agent was chosen

### 2. Present plan to user

Show the plan clearly — list each subtask, which agent will handle it, and why.
Ask for approval via AskUserQuestion before proceeding.

**Wait for user confirmation. Do not proceed without approval.**

### 3. Execute in parallel

After approval, spawn all agents in a **single response** using multiple Bash tool calls simultaneously (not sequentially):

```bash
relay run <agent1> --task "<task1>" --context "<context>"
relay run <agent2> --task "<task2>" --context "<context>"
```

Each call blocks until the agent finishes. Running them in parallel cuts total wall time.

### 4. Summarize and review

After all agents finish:
- Briefly summarize each agent's output (use your built-in summarization — no separate LLM call needed)
- Read the modified files
- Verify correctness against the original task

### 5. Fix or close

- **Incorrect or incomplete**: fix it yourself, or spawn a targeted agent for the specific issue
- **All correct**: report results to user and close the session

## Notes

- Context format for `--context`:
  ```
  Goal: <overall goal>
  Done: <what's already done>
  Why: <key decisions made>
  Modified: <files already changed>
  Avoid: <things that failed, don't retry>
  ```
- You are the decision maker. Relay is the executor.
- Never delegate decision-making to agents — only delegate implementation.
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
\n\
Relay delegates coding tasks to specialized AI agents (opencode, codex, copilot, pi).\n\
You are the **decision maker**. Relay is your executor.\n\
\n\
### When to use Relay\n\
\n\
- Task is clearly scoped and delegatable (implement X, write tests for Y, refactor Z)\n\
- Task can run independently without your direct involvement\n\
- Multiple independent subtasks exist that can run in parallel\n\
\n\
### Orchestration Flow\n\
\n\
**1. Plan** — run `/relay-plan` or manually:\n\
```bash\n\
relay plan --task \"<task>\" --context \"<context>\"\n\
```\n\
Output: JSON list of subtasks with `agent`, `task`, `reason` per entry.\n\
\n\
**2. Show plan to user, ask approval.**\n\
Present clearly. Wait for confirmation before proceeding.\n\
\n\
**3. Execute (parallel)** — after approval, spawn all agents in a single response\n\
using multiple Bash tool calls simultaneously:\n\
```bash\n\
relay run <agent1> --task \"<task1>\" --context \"<context>\"\n\
relay run <agent2> --task \"<task2>\" --context \"<context>\"\n\
```\n\
Each call blocks until agent finishes. Parallel = faster wall time.\n\
\n\
**4. Review** — after all agents finish:\n\
- Summarize each agent output (built-in — no separate LLM call)\n\
- Read modified files\n\
- Verify correctness against original task\n\
\n\
**5. Fix or close**\n\
- Incorrect/incomplete → fix yourself or spawn targeted agent\n\
- All correct → report to user, close session\n\
\n\
### Commands\n\
\n\
```bash\n\
relay plan --task \"<task>\" --context \"<context>\"         # generate task plan (JSON)\n\
relay run <agent> --task \"<task>\" --context \"<context>\"  # run agent (blocking)\n\
relay agent list                                           # list registered agents\n\
relay agent check                                          # check PATH availability\n\
relay config show                                          # show relay.config.yaml\n\
```\n\
\n\
### Context format\n\
\n\
```\n\
Goal: <overall goal>\n\
Done: <what's already done>\n\
Why: <key decisions made>\n\
Modified: <files already changed>\n\
Avoid: <things that failed, don't retry>\n\
```\n\
\n\
Run `relay init` in project root to create relay.config.yaml.\n\
Run `/relay-init` in Claude Code to set up interactively.\n\
Run `/relay-plan` in Claude Code to orchestrate a task end-to-end.\n\
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
    let skills_root = if global {
        let home = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("$HOME not set"))?;
        PathBuf::from(home).join(".claude").join("skills")
    } else {
        std::env::current_dir()?.join(".claude").join("skills")
    };

    let skills = [
        ("relay-init", SKILL_RELAY_INIT),
        ("relay-plan", SKILL_RELAY_PLAN),
    ];

    for (name, content) in &skills {
        let dir = skills_root.join(name);
        fs::create_dir_all(&dir)?;
        let path = dir.join("SKILL.md");
        fs::write(&path, content)?;
        println!("  ✓ Installed /{} skill → {}", name, path.display());
    }

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
