# Relay

**Relay is a mesh coordinator for AI coding sessions.** Open multiple Claude Code (or other AI coding agent) sessions, connect them via Relay, and share context across all of them.

Each session has a **role** — master, backend, frontend, reviewer, or any free-text description. Sessions discover each other automatically. Context flows between them via the Relay MCP server.

---

## How It Works

```
You open Claude Code (master)
        ↓
   relay init → sets role, injects hooks + MCP config
        ↓
   SessionStart hook fires → writes /tmp/relay-sessions/<pid>.json
        ↓
You open another Claude Code (backend)
        ↓
   relay init → sets role "backend"
        ↓
   relay_sessions MCP tool → both sessions are now visible to each other
        ↓
   relay_send / relay_read → share context, tasks, clarifications
```

Sessions auto-register on open, auto-cleanup on close. No manual wiring.

---

## Installation

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/itzmail/relay/master/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/itzmail/relay/master/install.ps1 | iex
```

### Build from source

```bash
cargo install --git https://github.com/itzmail/relay
```

---

## Quick Start

### 1. Start the MCP daemon

```bash
relay mcp start
```

### 2. Initialize each session

Run this once per project/session:

```bash
relay init
```

Interactive prompts:
- What is this session's role? (free text — e.g. `master`, `backend`, `review the plan for gaps`)

Then automatically:
- Injects `SessionStart` / `SessionEnd` / `PreToolUse` / `PostToolUse` hooks into `.claude/settings.json`
- Writes `.mcp.json` with the Relay MCP server URL
- Injects Relay mesh instructions into `CLAUDE.md`

### 3. Open another session

Open a second Claude Code in any project, run `relay init` with a different role. Both sessions are now in the mesh.

### 4. Discover and connect

Inside Claude Code, use MCP tools:

```
relay_sessions   → list all active sessions
relay_send       → send context or a task to a session by role
relay_read       → read incoming messages
relay_clarify    → ask for clarification from another role (escalates to master if unresolved)
```

---

## Session File

Each open session writes a file to `/tmp/relay-sessions/<pid>.json`:

```json
{
  "pid": 12345,
  "workspace": "/home/user/my-project",
  "tool": "claude-code",
  "role": "review the plan for gaps and clarity",
  "goal": "",
  "done": [],
  "modified": ["src/auth.rs"],
  "status": "idle",
  "started_at": 1719360000
}
```

- `status` is updated automatically via hooks (`working` during tool use, `idle` after)
- `modified` is updated from `git diff HEAD` after each tool use
- Stale entries (dead PIDs) are cleaned up automatically on `relay session list`

---

## CLI Reference

```bash
relay init                    # set up relay mesh for this project (interactive)
relay session list            # list active sessions in the mesh
relay session write           # manually write session file (called by SessionStart hook)
relay session delete          # manually delete session file (called by SessionEnd hook)
relay session status <value>  # update status: "working" or "idle"

# MCP daemon
relay mcp start               # start MCP server (default port 7777)
relay mcp stop                # stop MCP server
relay mcp status              # show daemon status
relay mcp install             # write MCP config to Claude Code / Codex / Copilot
relay mcp uninstall           # remove MCP config

relay update                  # update relay binary to latest release
```

---

## MCP Tools

Available inside Claude Code (and any MCP-compatible agent) once the daemon is running:

| Tool | Description |
|---|---|
| `relay_ping` | Health check |
| `relay_sessions` | List all active sessions with role, workspace, status |
| `relay_send` | Send a message or context to another session by role |
| `relay_read` | Read incoming messages for this session |
| `relay_clarify` | Request clarification from a target role; escalates to master if unresolved |
| `relay_agents` | List agents that have used the message bus |

---

## Clarification Flow

When a session receives an ambiguous task, it calls `relay_clarify`. Relay routes the question to the target role. If that role cannot answer, it escalates to `master`. If master cannot resolve it either, the user is asked directly.

This keeps sessions from silently guessing — ambiguity surfaces immediately.

---

## Uninstall

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/itzmail/relay/master/uninstall.sh | sh
```

### Windows

```powershell
irm https://raw.githubusercontent.com/itzmail/relay/master/uninstall.ps1 | iex
```

### via cargo

```bash
cargo uninstall relay
```

---

## License

MIT
