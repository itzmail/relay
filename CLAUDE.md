# Relay

Relay adalah CLI tool berbasis Rust yang di-spawn oleh Claude Code untuk menjalankan AI coding agent lain (OpenCode, Codex, Copilot CLI) dengan context yang ter-inject, lalu mengembalikan raw output ke Claude Code.

**Relay bukan orchestrator — dia executor.**
Decision making, summarization, dan routing adalah urusan Claude Code.
Claude Code yang summarize output Relay menggunakan model Haiku (hemat token), lalu lanjut kerja dengan model utama.

---

## Positioning

```
User → Claude Code (decision maker)
              ↓
     memanggil Relay sebagai CLI tool
              ↓
     Relay spawn agent yang diminta
     dengan context ter-inject via temp file
              ↓
     Raw output dikembalikan ke Claude Code
              ↓
     Claude Code summarize pakai Haiku
     lalu lanjut kerja pakai model utama
```

---

## Tugas Relay (hanya 3)

1. **Baca config** — load agent registry dari `relay.config.yaml`
2. **Inject context** — tulis temp file, pass ke agent sebelum spawn
3. **Spawn & capture** — jalankan agent, tangkap raw output, return ke Claude Code

---

## Tech Stack

- **Language:** Rust
- **Config:** YAML (`relay.config.yaml`) — dibuat via `relay init`, disimpan di root project
- **Key crates:** `serde`, `serde_yaml`, `tokio`, `clap`

---

## CLI Interface

### Setup (jalankan sekali di root project)

```bash
relay init
```

Flow interaktif:

1. Relay detect agent mana yang ada di PATH
2. User pilih agent mana yang ingin diaktifkan
3. User input default model per agent
4. `relay.config.yaml` dibuat di root project

Contoh output `relay init`:

```
Checking available agents...
  ✓ opencode found
  ✓ codex found
  ✗ copilot not found in PATH

Select agents to enable:
  [x] opencode  → model: anthropic/claude-sonnet-4-5
  [x] codex     → model: o4-mini
  [ ] copilot   → not installed

relay.config.yaml created.
```

### Run (dipanggil Claude Code)

```bash
relay run <agent-name> --task "<task>" --context "<context>"
```

Contoh:

```bash
relay run opencode \
  --task "buatkan unit test untuk auth.py" \
  --context "Goal: setup auth module. Done: implementasi JWT di auth.py. Avoid: jangan pakai unittest, pakai pytest."
```

Output ke stdout (raw — Claude Code yang summarize):

```json
{
  "agent": "opencode",
  "status": "done",
  "exit_code": 0,
  "output": "<raw stdout dari agent>",
  "modified_files": ["test_auth.py"]
}
```

### Commands lainnya

```bash
relay agent list          # lihat agent yang terdaftar & statusnya
relay agent check         # re-check ketersediaan binary di PATH
relay config show         # print isi relay.config.yaml
```

---

## Agent Registry (`relay.config.yaml`)

Dibuat otomatis oleh `relay init`. Tidak perlu diedit manual.

```yaml
agents:
  opencode:
    command: "opencode"
    enabled: true
    default_model: "anthropic/claude-sonnet-4-5"

  codex:
    command: "codex"
    enabled: true
    default_model: "o4-mini"

  copilot:
    command: "copilot"
    enabled: false              # tidak ditemukan saat relay init
    default_model: "gpt-5"
```

---

## Adapter Spec (per Agent)

### OpenCode Adapter

```
Binary    : opencode
Non-interactive : opencode run -m <model> "<prompt>"
Model format    : provider/model (contoh: anthropic/claude-sonnet-4-5, openai/gpt-4o)
Context inject  : prepend context ke prompt sebagai string
Output          : stdout langsung
```

Command yang dijalankan Relay:

```bash
opencode run -m anthropic/claude-sonnet-4-5 "<context>\n\n<task>"
```

### Codex Adapter

```
Binary          : codex
Non-interactive : codex exec "<prompt>"
Model flag      : -m <model>
Model format    : nama model saja (contoh: o4-mini, gpt-4o)
Sandbox         : WAJIB --sandbox workspace-write agar bisa edit file
                  (default Codex adalah read-only — tidak bisa edit file)
Context inject  : prepend context ke prompt sebagai string
Output          : stdout (human-readable), atau --json untuk JSONL
```

Command yang dijalankan Relay:

```bash
codex exec -m o4-mini --sandbox workspace-write "<context>\n\n<task>"
```

⚠️ **Penting:** Tanpa `--sandbox workspace-write`, Codex berjalan read-only dan tidak bisa modifikasi file apapun.

### Copilot CLI Adapter

```
Binary          : copilot
Non-interactive : copilot -p "<prompt>" --allow-all-tools
Model flag      : --model <model>
Model format    : nama model (contoh: gpt-5, claude-sonnet-4.5, claude-haiku-4.5)
Context inject  : prepend context ke prompt sebagai string
Output          : stdout langsung
```

Command yang dijalankan Relay:

```bash
copilot -p "<context>\n\n<task>" --allow-all-tools --model gpt-5
```

⚠️ **Penting:** `--allow-all-tools` wajib untuk non-interactive mode. Tanpanya Copilot akan meminta konfirmasi manual.

---

## Context Injection

Context di-inject dengan cara **prepend ke prompt** sebelum task. Format context:

```
[RELAY CONTEXT]
Goal: <apa yang ingin dicapai overall>
Done: <apa yang sudah dikerjakan sebelumnya>
Why: <keputusan penting yang sudah dibuat>
Modified: <file yang sudah diubah>
Avoid: <hal yang sudah dicoba dan gagal, jangan diulangi>
[END CONTEXT]

<task spesifik untuk agent ini>
```

Context ditulis ke **temp file** sementara di `.relay/` sebelum agent dijalankan, lalu dibaca dan digabung ke prompt, kemudian **dihapus otomatis** setelah agent selesai.

---

## Modified Files Detection

Relay detect file yang berubah via git diff sebelum dan sesudah agent run:

```rust
// 1. snapshot git status sebelum spawn
let before = get_git_snapshot();

// 2. spawn agent...

// 3. snapshot git status setelah agent selesai
let after = get_git_snapshot();

// 4. diff → modified_files
let modified = compute_diff(before, after);
```

Selalu aktif, tidak opsional. Asumsi: project menggunakan git.

---

## Struktur Project

```
relay/
├── Cargo.toml
├── relay.config.yaml          # dibuat oleh relay init, per project
└── src/
    ├── main.rs                # CLI entry point, argument parsing (clap)
    ├── config.rs              # load & validate relay.config.yaml (serde_yaml)
    ├── context.rs             # build context string, write/delete temp file
    ├── runner.rs              # spawn process, capture stdout/stderr, git diff
    └── adapters/
        ├── mod.rs
        ├── base.rs            # trait Agent
        ├── opencode.rs        # implement trait Agent untuk OpenCode
        ├── codex.rs           # implement trait Agent untuk Codex
        └── copilot.rs         # implement trait Agent untuk Copilot CLI
```

---

## Adapter Trait

```rust
pub trait Agent {
    /// Build command dengan model & sandbox flag yang benar
    fn build_command(&self, task: &str, context: &str) -> Command;

    /// Spawn process, capture stdout, return output
    fn run(&self, task: &str, context: &str) -> Result<AgentOutput>;
}

pub struct AgentOutput {
    pub agent: String,
    pub status: String,       // "done" | "error"
    pub exit_code: i32,
    pub output: String,       // raw stdout
    pub modified_files: Vec<String>,
}
```

Output di-serialize ke JSON dan dicetak ke stdout.

---

## Error Handling

| Kondisi | Behavior |
|---------|----------|
| Agent tidak ada di `relay.config.yaml` | Error: "Agent 'X' not found. Run `relay agent list` to see available agents." |
| Agent `enabled: false` | Error: "Agent 'X' is disabled. Run `relay init` to reconfigure." |
| Binary tidak ada di PATH | Error: "Binary 'X' not found in PATH. Please install it first." |
| Agent exit non-zero | Return output tetap, `"status": "error"`, `exit_code` diisi |
| Bukan git repository | Warning di stderr, `modified_files: []` |
| Config YAML invalid | Error dengan baris yang bermasalah |

---

## Prinsip

- **Relay tidak membuat keputusan** — routing dan summarization urusan Claude Code
- **Setup via `relay init`** — tidak ada hardcode agent, semua dari config
- **Context singkat** — bukan raw history, cukup goal + done + why + avoid
- **Temp file auto-delete** — tidak ada sisa file setelah agent selesai
- **Raw output** — Relay return apa adanya, tidak diproses
- **Fast startup** — Rust binary, tidak ada runtime overhead
- **Git-aware** — selalu track file yang berubah via git diff
