---
title: "feat: lucus — git worktree manager for parallel AI agent workflows"
type: feat
status: active
date: 2026-03-02
deepened: 2026-03-02
---

# lucus — Git Worktree Manager for Parallel AI Agent Workflows

## Enhancement Summary

**Deepened on:** 2026-03-02
**Research agents run:** 8 (git2 API, shell wrapper/clap, progressive rendering, TOML merge, architecture, security, performance, grapho reference)

### Key Improvements

1. **Replace `reqwest` with `ureq`** — eliminates tokio, saves 3–5MB binary size, keeps us under the 5MB limit without heroics
2. **Hybrid git strategy** — git2 for read operations (list, find, validate), subprocess for write operations (`git worktree add -b`) — simpler and more reliable than pure git2
3. **Three Critical security fixes** — hook template injection (env vars, not string interpolation), path traversal validation, .lucus.toml trust boundary (must be explicitly trusted before execution)
4. **Shell integration redesign** — `lucus init zsh` subcommand + `lucus query <branch>` instead of `--print-path` flag — cleaner separation of concerns
5. **Non-TTY output is NDJSON** — not markdown table — agent-parseable and line-oriented

### New Considerations Discovered

- **Async runtime decision must be made before Phase 2**: ureq (sync, Phase 1) vs reqwest blocking (sync, heavier) — ureq chosen, decide before writing `llm.rs`
- **`.lucus.toml` is a trust boundary**: treat like `.envrc` — require explicit opt-in before running hooks from project config
- **Hook variables must use env vars, never string interpolation**: `{branch}` in shell commands is a shell injection vector
- **`allow_hyphen_values = true`** needed on clap positional args for `-` (previous worktree) shortcut to work

---

## Overview

`lucus` (Latin: "sacred grove") is a Rust CLI for managing git worktrees targeted at solo developers running parallel AI agent sessions (Claude Code, Codex, Gemini) on the same repo. Each worktree is an isolated working tree — no staging conflicts, no dirty index surprises.

Name reserved on crates.io (v0.1.0 placeholder). Repo: `~/code/lucus`.

## Problem Statement

Running multiple AI agent sessions (Claude Code orchestrator + Codex delegate + Gemini delegate) on the same git repo causes conflicts: `git add -A` in one session stages another session's changes. The existing `repo-autocommit` hook already hit this. The only clean fix is one worktree per session.

Existing tools:
- **workmux** — tight tmux coupling, fragile agent status via window title parsing. Fails if tmux isn't running.
- **worktrunk** (`wt`) — best command vocabulary and git integration, but zero multiplexer integration and no task persistence.

Neither is the right fit. `lucus` takes the best of both and adds task-first creation.

## Proposed Solution

A terminal-agnostic worktree CLI with:
1. **Task-first creation** — `lucus new "implement auth"` generates branch name, persists task, creates worktree, runs hooks.
2. **Rich list** — progressive rendering showing branch, git stats, task description, stale detection.
3. **Clean lifecycle** — create → work → merge → cleanup as one pipeline.
4. **Shell `cd` integration** — shell function wrapper so switching actually moves your shell.
5. **Agent-type hooks** — different `post-create` configs per agent (Claude/Codex/Gemini).
6. **TOML config** — two-level: `~/.config/lucus/config.toml` + per-project `.lucus.toml`.

## Command Vocabulary

```
lucus new <branch|"task prompt">   # create worktree (+ optional agent launch)
lucus switch <branch>              # switch to existing worktree (shell cd)
lucus list                         # list all worktrees with status
lucus remove <branch>              # remove worktree + branch
lucus merge [branch]               # squash+rebase+ff+cleanup pipeline
lucus status                       # show running processes per worktree
lucus clean                        # prune stale (no process, uncommitted changes flagged)
lucus init <shell>                 # install shell function to ~/.zshrc / ~/.bashrc
lucus query <branch>               # print worktree path (used by shell wrapper)
```

**Symbolic shortcuts** (from worktrunk):
- `lucus switch -` → previous worktree
- `lucus switch ^` → default branch
- `lucus switch @` → current worktree

### Research Insights: Command Design

**Shell integration redesign:**
- `--print-path` flag is clumsy and leaks implementation details into the public API.
- **Better:** `lucus init zsh` writes the wrapper; `lucus query <branch>` is the internal path-printer used by the wrapper. Users never call `query` directly.
- This matches how `direnv hook zsh` and `mise activate zsh` work — shell-specific init subcommand, clean internal protocol.

**Clap positional args for symbolic shortcuts:**
```rust
// Requires allow_hyphen_values = true on the positional arg
// so `-` (previous worktree) isn't eaten as a flag
#[arg(allow_hyphen_values = true)]
branch: BranchRef,
```

**`BranchRef` custom type:**
```rust
enum BranchRef {
    Previous,        // "-"
    DefaultBranch,   // "^"
    Current,         // "@"
    Named(String),
}

impl std::str::FromStr for BranchRef { ... }
```

This keeps match arms in switch.rs clean and avoids string comparison scattered across commands.

---

## Architecture

### File Layout

```
~/code/lucus/
├── src/
│   ├── main.rs              # CLI entry (clap derive)
│   ├── commands/
│   │   ├── new.rs           # create worktree + task persistence
│   │   ├── switch.rs        # switch worktree (prints path for shell wrapper)
│   │   ├── list.rs          # progressive parallel list
│   │   ├── remove.rs        # cleanup
│   │   ├── merge.rs         # squash+rebase+ff pipeline
│   │   ├── status.rs        # process inventory per worktree
│   │   ├── clean.rs         # stale detection + prune
│   │   ├── init.rs          # shell function install
│   │   └── query.rs         # path printer (used by shell wrapper)
│   ├── config.rs            # TOML config loading (global + project merge)
│   ├── git.rs               # git operations (split: read via git2, write via subprocess)
│   ├── hooks.rs             # hook execution (blocking vs background, env var injection)
│   ├── llm.rs               # branch name generation from task prompt (ureq, sync)
│   ├── tasks.rs             # task metadata persistence (.lucus/tasks/{branch}.md)
│   └── output.rs            # TTY detection, OutputFormat enum, agent-first formatting
├── Cargo.toml
├── .lucus.toml              # project-level config example
└── docs/plans/
```

### Key Dependencies

| Crate | Version | Use |
|---|---|---|
| `clap` | 4 | CLI (derive macros) |
| `anyhow` | 1 | Error handling |
| `serde` + `toml` | latest | Config parsing |
| `owo-colors` | 4 | Colour output (auto TTY) |
| `rayon` | 1 | Parallel git calls for list |
| `git2` | 0.20 | Git read operations (list, find, validate worktrees) |
| `directories` | 5 | Config path resolution (replaces `dirs`) |
| `ureq` | 2 | LLM API call — sync, no tokio, saves 3–5MB vs reqwest |
| `indicatif` | latest | Progressive list skeleton + MultiProgress |

**Removed:** `reqwest` — was pulling in tokio and adding 3–5MB. `ureq` is sync and sufficient for the one LLM call in `new`.
**Changed:** `dirs` → `directories` v5 (more complete XDG support, `ProjectDirs` type).

### Research Insights: git Strategy

**Hybrid approach (git2 for reads, subprocess for writes):**

git2 0.20+ has the `Worktree` type but creating a worktree with a new branch requires two separate git2 calls and is fragile. The one-liner subprocess is simpler and more reliable:

```rust
// WRITE: always subprocess
fn worktree_add(repo_root: &Path, branch: &str, path: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["-C", repo_root.to_str().unwrap()])
        .args(["worktree", "add", "-b", branch, path.to_str().unwrap()])
        .status()?;
    ensure!(status.success(), "git worktree add failed");
    Ok(())
}

// READ: git2 (fast, no subprocess overhead)
fn list_worktrees(repo: &Repository) -> Result<Vec<WorktreeInfo>> {
    let names = repo.worktrees()?;
    names.iter()
        .filter_map(|n| n)
        .map(|name| {
            let wt = repo.find_worktree(name)?;
            // ...
        })
        .collect()
}
```

**Operation split in git.rs:**
- `git.rs` exposes two modules: `read` (git2-backed) and `write` (subprocess-backed)
- This makes the boundary explicit and avoids mixing strategies

### Research Insights: Architecture

**`HookContext` struct for template variables:**
```rust
pub struct HookContext {
    pub branch: String,
    pub worktree_path: PathBuf,
    pub repo_root: PathBuf,
    pub default_branch: String,
    pub agent: Option<String>,
    pub task: Option<String>,
}
```
All hook execution goes through this struct. Never build env vars from free-form string interpolation — see Security section.

**`OutputFormat` enum (from grapho reference pattern):**
```rust
pub enum OutputFormat {
    Human,   // TTY: colour, symbols, tables
    Json,    // --json flag or non-TTY
    Ndjson,  // streaming output (list command)
}
```

**`lucus merge --dry-run`**: Show the rebase/ff plan without executing. Worth adding in Phase 3 — avoids surprises on repos with complex history.

**stdout-only-on-success rule for shell wrapper:**
- `lucus query <branch>` must print path to stdout **only** if the worktree exists and is valid
- All errors go to stderr
- Exit 1 on failure — the shell wrapper checks exit code and falls back to `command lucus "$@"`

---

### Config Schema

```toml
# ~/.config/lucus/config.toml (global)
[worktree]
path_template = "../{repo}.{branch}"  # where worktrees are created
default_branch = "main"
agent = "codex"                        # default agent to launch on new

[llm]
enabled = true
model = "claude-haiku-4-5-20251001"   # for branch name generation
max_tokens = 20

[hooks]
post_create = ["cp .env .env.local"]  # blocking by default
post_create_bg = []                   # background (fire and forget)
pre_merge = ["cargo test"]
post_remove = []

[hooks.claude]
post_create = ["echo 'claude worktree ready'"]

[hooks.codex]
post_create = ["export CODEX_UNSAFE_ALLOW_NO_SANDBOX=1"]

[files]
copy = [".env"]              # copied on worktree creation
symlink = ["node_modules", "target"]  # symlinked (saves disk)
```

```toml
# .lucus.toml (per-project, merges with global)
[worktree]
path_template = "../{branch}"

[hooks]
post_create = ["pnpm install"]  # appended to global hooks, not replaced
pre_merge = ["pnpm test"]
```

### Research Insights: Config Merge

**Merge semantics for hook arrays:**
```rust
// None = inherit from global (not set in project config)
// Some([]) = explicitly suppress global hooks
// Some([...]) = append to global hooks
struct HookConfig {
    post_create: Option<Vec<String>>,
    pre_merge: Option<Vec<String>>,
    // ...
}

fn merge_hooks(global: &HookConfig, project: &HookConfig) -> Vec<String> {
    match &project.post_create {
        None => global.post_create.clone().unwrap_or_default(),
        Some(v) if v.is_empty() => vec![],  // suppress
        Some(v) => {
            let mut merged = global.post_create.clone().unwrap_or_default();
            merged.extend(v.iter().cloned());
            merged
        }
    }
}
```

**Config discovery:**
```rust
// Walk ancestors until git root, load first .lucus.toml found
fn find_project_config(start: &Path) -> Option<PathBuf> {
    let git_root = find_git_root(start)?;
    start.ancestors()
        .take_while(|p| p.starts_with(&git_root))
        .map(|p| p.join(".lucus.toml"))
        .find(|p| p.exists())
}
```
Stop at git root — don't walk into `~` looking for config.

**`directories` crate for config path:**
```rust
use directories::ProjectDirs;

let dirs = ProjectDirs::from("", "", "lucus")
    .ok_or_else(|| anyhow!("Cannot determine config directory"))?;
let config_path = dirs.config_dir().join("config.toml");
```

---

### Task Persistence

On `lucus new "implement auth feature"`:
1. Calls LLM → branch name: `feat/auth`
2. Creates `.lucus/tasks/feat-auth.md` with frontmatter:
   ```markdown
   ---
   branch: feat/auth
   created: 2026-03-02T09:30:00+08:00
   agent: codex
   ---
   implement auth feature
   ```
3. `lucus list` reads these files and shows task column.
4. Stale detection: no running process + uncommitted changes → `⚠ stale` flag in list.

---

### Shell Integration

`lucus switch` cannot `cd` the parent shell on its own. Solution: shell function wrapper installed via `lucus init zsh`.

```bash
# Written to ~/.zshrc by `lucus init zsh`
lucus() {
  if [[ "$1" == "switch" || "$1" == "new" ]]; then
    local dir
    dir=$(command lucus query "${@:2}" 2>/dev/null)
    if [[ -n "$dir" ]]; then
      cd "$dir" || return 1
    else
      command lucus "$@"
    fi
  else
    command lucus "$@"
  fi
}
```

**`lucus query <branch>`** is the internal protocol: prints path to stdout on success, stderr on error, exits 1 on failure. The shell wrapper uses exit code to decide whether to `cd` or fall through.

### Research Insights: Progressive List Rendering

**Two-phase rendering with MultiProgress:**
```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

// Phase 1: fast skeleton from git worktree list --porcelain (subprocess)
let mp = MultiProgress::new();
let bars: Vec<ProgressBar> = worktrees.iter().map(|wt| {
    let pb = mp.add(ProgressBar::new_spinner());
    pb.set_message(wt.branch.clone());
    pb
}).collect();

// Phase 2: rayon parallel enrichment
worktrees.par_iter().zip(bars.par_iter()).for_each(|(wt, pb)| {
    let stats = get_git_stats(&wt.path);  // subprocess: git status --porcelain=v2 --branch
    mp.suspend(|| {
        // update the line without flicker
        pb.finish_with_message(format_row(&wt, &stats));
    });
});
```

**Use `git status --porcelain=v2 --branch` for enrichment** — faster than git2 object traversal for this use case, and `--porcelain=v2` gives structured output that's easy to parse.

**Non-TTY output: NDJSON** (not markdown table):
```
{"branch":"feat/auth","path":"/Users/terry/code/myapp.feat-auth","ahead":2,"behind":0,"uncommitted":3,"task":"implement auth feature","stale":false}
{"branch":"fix/header","path":"/Users/terry/code/myapp.fix-header","ahead":0,"behind":1,"uncommitted":0,"task":null,"stale":false}
```
Markdown tables require column width scanning and break piping to `jq`. NDJSON is line-oriented and agent-parseable.

---

### Agent-First Output

Follow `~/docs/solutions/patterns/agent-first-cli.md`:
- `std::io::stdout().is_terminal()` → TTY: colour + symbols. Non-TTY: NDJSON.
- All status output to stderr; data to stdout.
- `--json` flag forces JSON even on TTY (useful for scripting from a terminal).

```rust
use std::io::IsTerminal;

pub fn output_format(json_flag: bool) -> OutputFormat {
    if json_flag { OutputFormat::Json }
    else if std::io::stdout().is_terminal() { OutputFormat::Human }
    else { OutputFormat::Ndjson }
}
```

### tmux Integration (Optional, Not Required)

`lucus` does not require tmux. But if `$TMUX` is set, `lucus new` can optionally create a tmux window:

```toml
[tmux]
enabled = true               # only when $TMUX is set
new_window_on_create = true
window_name_template = "{branch}"
```

No hex colours in tmux config. `colour` numbers only (Ghostty/Blink compatibility).

---

## Hook System

9 hook types (adopted from worktrunk), each with blocking/background variant:

| Hook | Blocking | Background |
|---|---|---|
| `pre-create` | ✓ | |
| `post-create` | ✓ | ✓ |
| `pre-switch` | ✓ | |
| `post-switch` | | ✓ |
| `pre-merge` | ✓ | |
| `post-merge` | | ✓ |
| `pre-remove` | ✓ | |
| `post-remove` | | ✓ |
| `post-start` (agent launch) | | ✓ |

### Research Insights: Hook Security — CRITICAL

**F-01: Hook template injection (CRITICAL)**

Never interpolate `{branch}` into shell command strings. Branch names can contain shell metacharacters:
```
branch name: "$(rm -rf ~)"  →  post_create: "echo $(rm -rf ~)"  → RCE
```

**Fix: always pass template variables as environment variables:**
```rust
fn run_hook(cmd: &str, ctx: &HookContext) -> Result<()> {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)  // cmd is never modified — branch name is in env, not the command
        .env("LUCUS_BRANCH", &ctx.branch)
        .env("LUCUS_WORKTREE_PATH", &ctx.worktree_path)
        .env("LUCUS_REPO", &ctx.repo_root)
        .env("LUCUS_AGENT", ctx.agent.as_deref().unwrap_or(""))
        .env("LUCUS_TASK", ctx.task.as_deref().unwrap_or(""))
        .status()?;
    Ok(())
}
```

Hook authors use `$LUCUS_BRANCH` in their scripts, not `{branch}`. The TOML config docs must reflect this.

**F-02: Path traversal in template substitution (HIGH)**

`path_template = "../{repo}.{branch}"` — validate resolved path stays within allowed parent:
```rust
fn resolve_worktree_path(template: &str, ctx: &TemplateCtx) -> Result<PathBuf> {
    let rendered = template
        .replace("{repo}", &ctx.repo)
        .replace("{branch}", &ctx.branch_safe());  // sanitized: alphanumeric + - + /
    let path = PathBuf::from(rendered).canonicalize()?;
    // Validate: must not resolve to inside the source repo
    ensure!(!path.starts_with(&ctx.repo_root), "Worktree path resolves inside source repo");
    Ok(path)
}
```

`branch_safe()` strips anything outside `[a-zA-Z0-9/_-]`.

**F-03: .lucus.toml trust boundary (HIGH)**

`.lucus.toml` can contain arbitrary hook commands. Treat like `.envrc`:
- On first encounter, print: `lucus: untrusted .lucus.toml at /path/to/.lucus.toml — run 'lucus trust' to allow`
- Store trusted paths in `~/.config/lucus/trusted.toml` (SHA256 of path + mtime)
- Only load project hooks if path is trusted

**F-04: Shell wrapper injection (MEDIUM)**

The shell function calls `command lucus query "${@:2}"`. The `"${@:2}"` is safe in zsh/bash with double-quotes. Document the double-quote requirement explicitly in the generated wrapper.

**F-05: LLM branch name sanitization (LOW)**

LLM can return anything. Strip to `[a-zA-Z0-9/_-]`, max 60 chars, before creating branch:
```rust
fn sanitize_branch_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_alphanumeric() || *c == '/' || *c == '-' || *c == '_')
        .take(60)
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
```

---

## Acceptance Criteria

- [ ] `lucus new feat/auth` creates worktree at path from template, runs `post-create` hooks (via env vars), prints path
- [ ] `lucus new "implement OAuth login"` calls Haiku via ureq → branch name → creates worktree, persists task to `.lucus/tasks/`
- [ ] `lucus switch feat/auth` prints path; shell wrapper `cd`s to it
- [ ] `lucus list` renders progressively: branch names immediately, git stats filled in via rayon
- [ ] `lucus list` shows task description column if `.lucus/tasks/{branch}.md` exists
- [ ] `lucus list` flags stale worktrees (uncommitted changes + no running agent process)
- [ ] `lucus list` non-TTY output is NDJSON
- [ ] `lucus merge feat/auth` squashes commits, rebases onto main, fast-forwards, removes worktree + branch
- [ ] `lucus merge --dry-run` shows plan without executing
- [ ] `lucus remove feat/auth` removes worktree and branch
- [ ] `lucus status` shows pid + runtime for any running agent process per worktree
- [ ] Global config merges with per-project config (hooks append, not replace; None=inherit, Some([])=suppress)
- [ ] Per-project `.lucus.toml` hooks require `lucus trust` before execution
- [ ] Hook template variables passed as env vars (`$LUCUS_BRANCH` etc), never string-interpolated into commands
- [ ] Path from template validated against traversal before creating worktree
- [ ] Agent-first output: TTY → colour, non-TTY → NDJSON
- [ ] `lucus init zsh` writes shell function to `~/.zshrc`
- [ ] `lucus query <branch>` prints path to stdout; stderr on error; exit 1 on failure
- [ ] Binary size under 5MB after release profile (opt-level = "z", lto, strip) — ureq not reqwest
- [ ] `cargo test` green, `cargo clippy` clean, `cargo machete` no unused deps

## Implementation Phases

### Phase 1: Core (ship-worthy v0.2.0)

- `lucus new <branch>` — create worktree (subprocess: `git worktree add -b`), run `post-create` hooks via env vars
- `lucus list` — basic list (branch, path, ahead/behind, uncommitted count) with NDJSON non-TTY
- `lucus remove <branch>` — remove worktree + branch
- `lucus switch <branch>` / `lucus query <branch>` — print path for shell wrapper
- `lucus init <shell>` — write shell function
- TOML config (global only, no per-project merge yet)
- Security fixes F-01 (env vars) and F-02 (path validation) — in Phase 1, not deferred

### Phase 2: Task-first + rich list (v0.3.0)

- `lucus new "natural language prompt"` — ureq LLM branch naming + task persistence
- `lucus list` — progressive rendering (indicatif MultiProgress + rayon), task column, stale flag
- Per-project `.lucus.toml` merging with global (trust model from F-03)
- Agent-type hook disambiguation (`[hooks.codex]`, `[hooks.claude]`)
- `files.copy` + `files.symlink` on create
- LLM branch name sanitization (F-05)

### Phase 3: Polish (v0.4.0)

- `lucus merge` — squash+rebase+ff+cleanup pipeline + `--dry-run`
- `lucus status` — process inventory per worktree
- `lucus clean` — prune stale
- tmux optional integration (`[tmux]` config block)
- Shell completions (zsh + bash)
- JSON schema for config

## Risks & Dependencies

| Risk | Mitigation |
|---|---|
| `git2` missing some worktree write APIs | Already mitigated: use subprocess for all writes |
| LLM latency on `lucus new "prompt"` | ureq is sync — no async overhead. 1–2s acceptable for one-time create |
| Shell function wrapper conflicts with existing `lucus` alias | Detect and warn on `lucus init` |
| `rayon` + subprocess threading in list command | Scope rayon threads to list command; no global threadpool |
| Binary over 5MB | ureq instead of reqwest eliminates the tokio weight; release profile strips the rest |
| `.lucus.toml` trust UX friction | Prompt is shown once per path; trust is stored persistently |

## Sources & References

- [workmux](https://github.com/raine/workmux) — tmux-coupled design, pane layouts, task prompts
- [worktrunk](https://github.com/max-sixty/worktrunk) — command vocabulary, hook system, merge pipeline, progressive list
- [workmux intro blog](https://raine.dev/blog/introduction-to-workmux/)
- [worktrunk docs](https://worktrunk.dev)
- `~/docs/solutions/patterns/agent-first-cli.md` — TTY detection, agent-readable output
- `~/docs/solutions/rust-toolchain-setup.md` — deps, release profile, pre-publish checklist
- `~/code/grapho/` — reference Rust CLI structure: clap derive, atomic write, OutputFormat enum, TTY detection pattern
