# Relay

Relay is a **mesh coordinator for AI coding sessions** — a Rust CLI + MCP server that lets multiple Claude Code (or other AI coding agent) sessions discover each other, share context, and route tasks by role.

**Relay does not spawn agents. The user opens agents. Relay connects them.**

---

## Architecture

```
User opens Claude Code session A (master)
User opens Claude Code session B (backend)
User opens Claude Code session C (reviewer)
         ↓
Each session runs relay init once
         ↓
SessionStart hook → writes /tmp/relay-sessions/<pid>.json
         ↓
relay_sessions MCP tool → all sessions visible to each other
         ↓
relay_send / relay_read / relay_clarify → context flows between sessions
```

---

## Session File (`/tmp/relay-sessions/<pid>.json`)

```json
{
  "pid": 12345,
  "workspace": "/path/to/project",
  "tool": "claude-code",
  "role": "review the plan for gaps and clarity",
  "goal": "",
  "done": [],
  "modified": [],
  "status": "idle",
  "started_at": 1719360000
}
```

- Role is **free text** — not a fixed enum. Describe the session's purpose.
- `status` auto-updates via hooks: `working` (PreToolUse) → `idle` (PostToolUse)
- `modified` auto-updates from `git diff HEAD` after each tool use
- File is deleted on `SessionEnd`. Stale entries (dead PIDs) are cleaned on list.

---

## Relay Role (`.relay-role`)

`relay init` saves the session role to `.relay-role` in the project root. The `SessionStart` hook reads this file to populate the session file on each open.

---

## CLI

```bash
relay init                    # set up mesh: inject hooks, MCP config, CLAUDE.md
relay session list            # list active sessions (validates PID liveness)
relay session write [--role]  # write session file (SessionStart hook)
relay session delete          # delete session file (SessionEnd hook)
relay session status <value>  # update status: "working" | "idle"

relay mcp start               # start MCP daemon (default port 7777)
relay mcp stop                # stop daemon
relay mcp status              # show daemon status
relay mcp install             # write MCP config for Claude Code / Codex / Copilot
relay mcp uninstall           # remove MCP config

relay update                  # self-update to latest release
```

---

## MCP Tools

| Tool | Description |
|---|---|
| `relay_ping` | Health check |
| `relay_sessions` | List all active sessions (pid, role, workspace, status) |
| `relay_send` | Send message/context to a session by role or broadcast |
| `relay_read` | Read incoming messages for this session |
| `relay_clarify` | Request clarification from a target role; escalates to master if unresolved |
| `relay_agents` | List agents registered in the message bus |

---

## Tech Stack

- **Language:** Rust
- **MCP server:** `rmcp` crate (streamable HTTP)
- **Message bus:** SQLite (`~/.relay/relay.db`)
- **Session discovery:** `/tmp/relay-sessions/` filesystem
- **Key crates:** `clap`, `serde`, `serde_json`, `tokio`, `rusqlite`, `axum`

---

## Project Structure

```
relay/
├── Cargo.toml
├── .relay-role                  # session role for this project (created by relay init)
└── src/
    ├── main.rs                  # CLI entry point (clap)
    ├── config.rs                # minimal config (MCP daemon settings)
    ├── session.rs               # session file read/write/list/cleanup
    ├── setup.rs                 # hook injection, CLAUDE.md injection
    ├── updater.rs               # self-update from GitHub Releases
    └── mcp/
        ├── mod.rs               # RelayPaths, paths()
        ├── cli.rs               # relay mcp subcommands
        ├── daemon.rs            # daemonize, PID file, stop
        ├── db.rs                # SQLite init
        ├── installer.rs         # write MCP config to claude/codex/copilot
        ├── server.rs            # axum HTTP server
        ├── status.rs            # daemon status display
        └── tools.rs             # MCP tool implementations
```

---

## Clarification Protocol

When a session receives an ambiguous task:
1. Call `relay_clarify` with the question and optional `target_role`
2. Relay finds the target role session and sends the message
3. If no matching role is found → escalates to `master`
4. Master resolves and sends back via `relay_send`

Sessions must NOT guess or assume on ambiguous input — `relay_clarify` first.

<!-- relay:start -->
## Relay Mesh

This session is part of a Relay mesh — a network of AI coding sessions sharing context.

### Clarification Protocol

When you receive a task via relay and something is ambiguous, call `relay_clarify` before starting work. Do NOT assume or guess — ask first.

If the target role cannot answer, the question escalates automatically to the master session.

### Session Commands (MCP tools)

- `relay_sessions` — list all active sessions in this mesh
- `relay_send` — send context or a task to another session by role
- `relay_read` — read incoming messages for this session
- `relay_clarify` — request clarification from a target role or master

### Context format (when sending via relay_send)

```
Goal: <overall goal>
Done: <what is already done>
Why: <key decisions made>
Modified: <files already changed>
Avoid: <things that failed, do not retry>
```

Run `relay init` in project root to set up or reconfigure this session.
<!-- relay:end -->


