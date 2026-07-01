# Relay

**Relay is a mesh coordinator for AI coding sessions.** Open multiple Claude Code sessions, connect them via Relay, and route tasks between them.

Each session has a **name** (set via `/rename` in Claude Code). Sessions discover each other automatically. Context flows between them via the Relay MCP server.

---

## How It Works

```
You open Claude Code session A
        ↓
   /rename master       ← name this session
        ↓
You open Claude Code session B
        ↓
   /rename backend      ← name this session
        ↓
   relay init           ← inject MCP config + hook (run once per project)
        ↓
   relay mcp start      ← start the MCP daemon
        ↓
   relay session join   ← attach this session to the mesh
        ↓
   relay_sessions MCP tool → both sessions visible to each other
        ↓
   relay_send / relay_read → share context, tasks, clarifications
```

Sessions are discovered from Claude Code's native session files (`~/.claude/sessions/`). No hooks required for discovery — just join the mesh when you're ready.

---

## Installation

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/itzmail/relay/master/install.sh | sh
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

### 2. Initialize the project

Run once per project:

```bash
relay init
```

This:
- Injects `UserPromptSubmit` hook into `.claude/settings.json` (unread message notifications)
- Writes `.mcp.json` with the Relay MCP server URL
- Injects Relay mesh instructions into `CLAUDE.md`

### 3. Name your sessions

In each Claude Code session, use `/rename` to give it a meaningful name:
```
/rename master
/rename backend
/rename reviewer
```

### 4. Join the mesh

In the terminal, from the project directory:

```bash
relay session join    # attach this Claude session to the mesh
```

Now other sessions can see you and send you messages. You'll get an `⚡ relay:` notification prepended to your next prompt when unread messages arrive — zero token cost when idle.

### 5. Discover and connect

Inside Claude Code, use MCP tools:

```
relay_sessions   → list all active sessions
relay_send       → send context or a task to a session by name
relay_read       → read incoming messages
relay_clarify    → ask for clarification (escalates to master if unresolved)
```

---

## CLI Reference

```bash
relay init                    # set up relay mesh for this project
relay session list            # list active sessions (shows [joined] status)
relay session join            # join the relay mesh from this project
relay session leave           # leave the relay mesh

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

| Tool | Description |
|---|---|
| `relay_ping` | Health check |
| `relay_sessions` | List all active sessions with name, workspace, status |
| `relay_send` | Send a message or context to another session by name |
| `relay_read` | Read incoming messages for this session |
| `relay_clarify` | Request clarification from a target role; escalates to master if unresolved |
| `relay_agents` | List agents that have used the message bus |

---

## Unread Message Notifications

When a session has unread messages, the `UserPromptSubmit` hook prepends a one-line notification to the next prompt:

```
⚡ relay: 2 unread message(s) for "backend". Call relay_read to process.
```

- **Zero token cost** when no messages are pending
- Only fires for sessions that have run `relay session join`
- Set `RELAY_IGNORE=1` to disable for a specific session

---

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/itzmail/relay/master/uninstall.sh | sh
```

Or manually:

```bash
rm /usr/local/bin/relay
```

---

## License

MIT
