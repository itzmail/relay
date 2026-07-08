use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;
use serde_json;

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
        "{}\n## Relay Mesh\n\
\n\
This session is part of a Relay mesh — a network of AI coding sessions sharing context.\n\
\n\
### Clarification Protocol\n\
\n\
When you receive a task via relay and something is ambiguous, call `relay_clarify` \
before starting work. Do NOT assume or guess — ask first.\n\
\n\
If the target role cannot answer, the question escalates automatically to the master session.\n\
\n\
### Session Commands (MCP tools)\n\
\n\
- `relay_sessions` — list all active sessions in this mesh\n\
- `relay_send` — send context or a task to another session by role\n\
- `relay_read` — read incoming messages for this session\n\
- `relay_clarify` — request clarification from a target role or master\n\
\n\
### Context format (when sending via relay_send)\n\
\n\
```\n\
Goal: <overall goal>\n\
Done: <what is already done>\n\
Why: <key decisions made>\n\
Modified: <files already changed>\n\
Avoid: <things that failed, do not retry>\n\
```\n\
\n\
Run `relay init` in project root to set up or reconfigure this session.\n\
{}\n",
        CLAUDE_CODE_MARKER_START, CLAUDE_CODE_MARKER_END
    )
}

const RELAY_HOOK_SCRIPT: &str = r#"
# relay: inject pending replies and notify unread messages
if [ "${RELAY_IGNORE}" = "1" ]; then exit 0; fi
PROJECT_CWD=$(pwd)
SESSION_FILE=$(python3 -c "
import json, os, glob
sid = os.environ.get('CLAUDE_CODE_SESSION_ID', '')
cwd = os.environ.get('PROJECT_CWD', '')
for f in glob.glob(os.path.expanduser('~/.claude/sessions/*.json')):
    try:
        d = json.load(open(f))
        if d.get('sessionId') == sid and d.get('cwd') == cwd:
            print(f)
            break
    except: pass
" PROJECT_CWD="$PROJECT_CWD" 2>/dev/null)
if [ -z "$SESSION_FILE" ]; then exit 0; fi
PID=$(python3 -c "import json,sys; print(json.load(open('$SESSION_FILE'))['pid'])" 2>/dev/null)
if [ -z "$PID" ]; then exit 0; fi
FLAG="/tmp/relay-joined/${PID}.join"
if [ ! -f "$FLAG" ]; then exit 0; fi
if ! /bin/kill -0 "$PID" 2>/dev/null; then rm -f "$FLAG"; exit 0; fi
SESSION_NAME=$(python3 -c "import json; d=json.load(open('$SESSION_FILE')); print(d.get('name',''))" 2>/dev/null)
if [ -z "$SESSION_NAME" ]; then exit 0; fi
# Check for pending reply file (written by relay watch)
REPLY_FILE="$HOME/.relay/pending-reply-${SESSION_NAME}.txt"
if [ -f "$REPLY_FILE" ]; then
  REPLY_CONTENT=$(cat "$REPLY_FILE")
  rm -f "$REPLY_FILE"
  python3 -c "
import json, sys
content = sys.argv[1]
print(json.dumps({'hookSpecificOutput': {'additionalContext': content}}))
" "$REPLY_CONTENT"
  exit 0
fi
DB="$HOME/.relay/relay.db"
if [ ! -f "$DB" ]; then exit 0; fi
UNREAD=$(sqlite3 "$DB" "
  SELECT COUNT(*) FROM messages m
  LEFT JOIN agents a ON a.id = '${SESSION_NAME}'
  WHERE m.id > COALESCE((SELECT last_read_id FROM agents WHERE id = '${SESSION_NAME}'), 0)
    AND m.from_agent != '${SESSION_NAME}'
    AND (m.to_agent IS NULL OR m.to_agent = '${SESSION_NAME}')
" 2>/dev/null || echo 0)
if [ "$UNREAD" -gt 0 ]; then
  python3 -c "
import json, sys
msg = '⚡ relay: {} unread message(s) for \"{}\". Call relay_read to process.'.format(sys.argv[1], sys.argv[2])
print(json.dumps({'hookSpecificOutput': {'additionalContext': msg}}))
" "$UNREAD" "$SESSION_NAME"
fi
"#;

pub fn inject_hooks(global: bool) -> Result<()> {
    let settings_path = if global {
        let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("$HOME not set"))?;
        PathBuf::from(home).join(".claude").join("settings.json")
    } else {
        std::env::current_dir()?.join(".claude").join("settings.json")
    };

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let hooks = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json root must be object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    hooks["UserPromptSubmit"] = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": RELAY_HOOK_SCRIPT.trim()
        }]
    }]);

    fs::write(&settings_path, serde_json::to_string_pretty(&root)?)?;
    println!("  ✓ Injected UserPromptSubmit hook → {}", settings_path.display());
    Ok(())
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
