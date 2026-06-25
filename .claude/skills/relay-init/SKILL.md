---
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
