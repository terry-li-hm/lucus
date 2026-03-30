---
title: "feat: copy .env files and manage .gitignore on lucus new"
type: feat
status: active
date: 2026-03-02
issue: https://github.com/terry-li-hm/lucus/issues/1
---

# feat: copy .env files and manage .gitignore on `lucus new`

## Problem

When `lucus new <branch>` creates a worktree, delegates (Codex, Gemini, OpenCode) running inside it fail silently or use wrong config because `.env*` files aren't present. Additionally, users who configure an in-repo path template (e.g. `.worktrees/{branch}`) get untracked noise in `git status` unless `.worktrees/` is in `.gitignore`.

Currently achievable via `post_create` hooks, but that's too much setup friction — it should be native behaviour.

## Proposed Solution

Two changes to `lucus new`, inserted after `git::worktree_add` succeeds and before hooks run:

1. **Copy `.env*` files** from repo root into the new worktree
2. **Manage `.gitignore`** — if the worktree is inside the repo (in-repo template), ensure the parent dir is in the repo's `.gitignore`

### Config (optional override)

Add a `[files]` section to `~/.config/lucus/config.toml` with sensible defaults. If no config, default behaviour applies automatically.

```toml
[files]
# Glob patterns relative to repo root to copy on worktree creation
# Default: all .env* files in repo root
copy = [".env", ".env.local", ".env.test", ".env.production"]
```

Default when `[files]` section absent: copy all `.env*` files found in repo root (glob `repo_root/.env*`).

## Acceptance Criteria

- [ ] `lucus new <branch>` copies all `.env*` files from repo root into the new worktree
- [ ] If no `.env*` files exist in repo root, proceeds silently (no error)
- [ ] `[files] copy` in config overrides which files are copied
- [ ] If worktree path is inside the repo, parent dir is added to repo's `.gitignore` (if not already present)
- [ ] If worktree path is outside the repo (default template `../repo.branch`), no `.gitignore` changes made
- [ ] `cargo clippy` clean
- [ ] `cargo test` green (new unit tests for both helpers)

## Implementation

### Files changed

| File | Change |
|------|--------|
| `src/config.rs` | Add `FilesConfig` struct + `files: FilesConfig` field on `Config` |
| `src/files.rs` | New module: `copy_env_files()` + `ensure_gitignore()` |
| `src/main.rs` | Add `mod files;` |
| `src/commands/new.rs` | Wire in both helpers after `worktree_add` |

### `src/config.rs` addition

```rust
#[derive(Clone, Debug, Deserialize)]
pub struct FilesConfig {
    #[serde(default = "default_copy_patterns")]
    pub copy: Vec<String>,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self { copy: default_copy_patterns() }
    }
}

fn default_copy_patterns() -> Vec<String> {
    // Empty = auto-discover .env* at runtime
    vec![]
}

// In Config struct:
pub struct Config {
    pub worktree: WorktreeConfig,
    pub hooks: HookConfig,
    #[serde(default)]
    pub files: FilesConfig,
}
```

### `src/files.rs`

```rust
use anyhow::{Context, Result};
use std::path::Path;

/// Copy .env* files from repo_root into target_path.
/// If config.copy is empty, auto-discover all .env* in repo root.
pub fn copy_env_files(
    repo_root: &Path,
    target_path: &Path,
    patterns: &[String],
) -> Result<()> {
    let candidates: Vec<_> = if patterns.is_empty() {
        // Auto-discover: all .env* files in repo root (non-recursive)
        std::fs::read_dir(repo_root)
            .context("failed to read repo root")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".env")
            })
            .map(|e| e.path())
            .collect()
    } else {
        patterns
            .iter()
            .map(|p| repo_root.join(p))
            .filter(|p| p.exists())
            .collect()
    };

    for src in candidates {
        let filename = src.file_name().expect("file has name");
        let dst = target_path.join(filename);
        std::fs::copy(&src, &dst)
            .with_context(|| format!("failed to copy {} to {}", src.display(), dst.display()))?;
    }
    Ok(())
}

/// If the worktree is inside the repo, ensure the parent dir is in .gitignore.
/// No-op if worktree is outside the repo (default template).
pub fn ensure_gitignore(repo_root: &Path, target_path: &Path) -> Result<()> {
    // Only act if worktree is inside the repo
    if !target_path.starts_with(repo_root) {
        return Ok(());
    }

    let parent = match target_path.parent() {
        Some(p) => p,
        None => return Ok(()),
    };

    // Get relative path of parent from repo root for .gitignore entry
    let relative = parent.strip_prefix(repo_root)
        .context("failed to strip repo root prefix")?;
    let entry = format!("/{}/", relative.display());

    let gitignore_path = repo_root.join(".gitignore");
    let existing = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path)
            .context("failed to read .gitignore")?
    } else {
        String::new()
    };

    if !existing.lines().any(|l| l.trim() == entry.trim()) {
        let mut updated = existing;
        if !updated.ends_with('\n') && !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str(&entry);
        updated.push('\n');
        std::fs::write(&gitignore_path, updated)
            .context("failed to write .gitignore")?;
    }

    Ok(())
}
```

### `src/commands/new.rs` insertion point

After `git::worktree_add(...)` and before `let hook_context = ...`:

```rust
crate::files::copy_env_files(&repo_root, &target_path, &config.files.copy)?;
crate::files::ensure_gitignore(&repo_root, &target_path)?;
```

## Delegate Decomposition (for /xlfg)

These streams are **file-independent** — safe to run in parallel:

| Stream | Tool | Files touched | Task |
|--------|------|---------------|------|
| A — Rust core | **Codex** | `config.rs`, `src/files.rs` (new), `src/main.rs`, `commands/new.rs` | Implement `FilesConfig`, `files.rs` module, wire into `new.rs`, add `#[cfg(test)]` unit tests |
| B — Docs | **OpenCode** | `~/skills/lucus/SKILL.md`, `README.md` (if exists) | Document new behaviour: which files get copied, gitignore note, config override example |

Stream B has zero overlap with Stream A.

## Verification

```bash
# In a test repo:
cd /tmp && mkdir test-repo && cd test-repo && git init
echo "SECRET=123" > .env
echo "LOCAL=456" > .env.local
lucus new feat-test
ls ../test-repo.feat-test/.env*        # should show both files
cat .gitignore                          # should NOT be modified (default template is ../)

# With in-repo template:
# Set path_template = ".worktrees/{branch}" in config
lucus new feat-inner
cat .gitignore                          # should contain /.worktrees/

cargo clippy -- -D warnings
cargo test
```

## Notes

- `copy_env_files` is intentionally non-recursive — only repo root, not subdirs
- Missing `.env*` files are silently skipped (not an error)
- `.gitignore` check is idempotent — safe to run `lucus new` multiple times
- Future: `files.symlink` for `node_modules`, `target/` (Phase 2 from original plan)
