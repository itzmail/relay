# Relay

**Relay is a CLI tool that lets Claude Code delegate coding tasks to other AI agents** — OpenCode, Codex, GitHub Copilot CLI, and Pi — then returns their raw output back to Claude Code for review.

Claude Code is the **decision maker**. Relay is the **executor**.

---

## Why Relay?

Claude Code is great at reasoning, planning, and reviewing — but sometimes you want to leverage specialized agents for implementation. The problem: there's no native way to orchestrate multiple AI coding agents from within Claude Code.

Relay bridges that gap.

### Before Relay

```
You → Claude Code
         ↓
    does everything alone
    (one model, one context window, sequential)
```

### After Relay

```
You → Claude Code (decision maker)
              ↓
     breaks task into subtasks
     assigns best agent per subtask
              ↓
     ┌────────────────────────────┐
     │  relay run opencode ...    │  ← parallel
     │  relay run codex    ...    │  ← parallel
     │  relay run copilot  ...    │  ← parallel
     └────────────────────────────┘
              ↓
     Claude Code reviews all outputs
     fixes what's wrong, closes session
```

| | Without Relay | With Relay |
|---|---|---|
| Parallelism | Sequential only | Multiple agents in parallel |
| Agent diversity | One model | Best agent per task type |
| Wall time | Long | Cuts proportionally with parallelism |
| Claude Code role | Does everything | Plans, delegates, reviews |

---

## Installation

### macOS / Linux (curl)

```bash
curl -fsSL https://raw.githubusercontent.com/itzmail/relay/master/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/itzmail/relay/master/install.ps1 | iex
```

### Manual (from GitHub Releases)

Download the binary for your platform from [Releases](https://github.com/itzmail/relay/releases), then add it to your PATH.

| Platform | File |
|---|---|
| macOS Apple Silicon | `relay-macos-aarch64.tar.gz` |
| macOS Intel | `relay-macos-x86_64.tar.gz` |
| Linux x86_64 | `relay-linux-x86_64.tar.gz` |
| Windows x86_64 | `relay-windows-x86_64.zip` |

### Build from source

```bash
cargo install --git https://github.com/itzmail/relay
```

---

## Uninstall

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/itzmail/relay/master/uninstall.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/itzmail/relay/master/uninstall.ps1 | iex
```

### Installed via cargo

```bash
cargo uninstall relay
```

---

## Quick Start

### 1. Set up Relay for Claude Code

```bash
# Inject relay instructions into your global CLAUDE.md
relay setup claude-code --global

# Or project-local only
relay setup claude-code
```

This injects orchestration instructions into CLAUDE.md and installs `/relay-init` and `/relay-plan` slash commands into Claude Code.

### 2. Initialize in your project

```bash
cd your-project
relay init
```

Interactive setup — detects which agents are in your PATH, lets you pick which to enable and which model each should use. Creates `relay.config.yaml` in project root.

```
Checking available agents...
  ✓ opencode found
  ✓ codex found
  ✗ copilot not found in PATH

Select agents to enable:
  [x] opencode  → model: anthropic/claude-sonnet-4-6
  [x] codex     → model: o4-mini
  [ ] copilot   → not installed

relay.config.yaml created.
```

### 3. Use `/relay-plan` in Claude Code

Open Claude Code and type:

```
/relay-plan implement user authentication with JWT
```

Claude Code will:
1. Break the task into subtasks and assign agents
2. Show you the plan and ask for approval
3. Execute all agents in parallel
4. Review the output and fix anything incorrect
5. Report results and close

---

## CLI Reference

```bash
relay init                                        # interactive setup
relay run <agent> --task "<task>" --context "<>" # run agent (blocking, returns JSON)
relay agent list                                  # list registered agents
relay agent check                                 # check binary availability in PATH
relay config show                                 # print relay.config.yaml
relay setup claude-code [--global]               # inject into CLAUDE.md + install skills
```

### relay run output

```json
{
  "agent": "opencode",
  "status": "done",
  "exit_code": 0,
  "output": "<raw stdout from agent>",
  "modified_files": ["src/auth.rs", "src/main.rs"]
}
```

---

## Supported Agents

| Agent | Binary | Install |
|---|---|---|
| [OpenCode](https://opencode.ai) | `opencode` | `npm i -g opencode-ai` |
| [Codex](https://github.com/openai/codex) | `codex` | `npm i -g @openai/codex` |
| [GitHub Copilot CLI](https://githubnext.com/projects/copilot-cli) | `copilot` | via GitHub CLI extension |
| [Pi](https://pi.ai) | `pi` | see Pi docs |

You don't need all of them — Relay only uses what you enable in `relay.config.yaml`.

---

## How Context Injection Works

When you run `relay run`, Relay prepends your context to the agent's prompt:

```
[RELAY CONTEXT]
Goal: <overall goal>
Done: <what's already done>
Why: <key decisions made>
Modified: <files already changed>
Avoid: <things that failed, don't retry>
[END CONTEXT]

<your specific task>
```

Context is written to a temp file under `.relay/`, injected before spawn, deleted after agent exits. No residue left behind.

---

## Configuration

`relay.config.yaml` (created by `relay init`):

```yaml
agents:
  opencode:
    command: opencode
    enabled: true
    default_model: anthropic/claude-sonnet-4-6

  codex:
    command: codex
    enabled: true
    default_model: o4-mini

  copilot:
    command: copilot
    enabled: false
    default_model: gpt-4o
```

Override model per-run:

```bash
relay run opencode --task "refactor auth module" --model anthropic/claude-opus-4-7
```

---

## Claude Code Skills

After `relay setup claude-code`, two slash commands are available:

| Command | What it does |
|---|---|
| `/relay-init` | Interactive setup — detect agents, pick models, create config |
| `/relay-plan` | Full orchestration — plan → approve → parallel execute → review → fix or close |

---

## Modified Files Detection

Relay automatically tracks which files changed during agent execution using git diff (before and after spawn). This is always active — assumes the project is a git repository.

---

## License

MIT
