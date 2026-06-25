---
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
