# Lucus overnight safety completion — progress log

- **Base SHA (verified, no drift):** `d1eb5af60304fcfa248ddc5bd946ccd9eb13f66c` = `origin/main`
- **Lane:** `lane/lucus-safety-overnight-local` (local commits only, no push)
- **Deadline:** 2026-08-27 07:00 HKT / 2026-08-26 23:00 UTC
- **Session start:** 2026-08-26 22:38 HKT (14:38 UTC)

## Milestone checklist

- [x] 0. Linux offline verification path (disable unused git2 network features; prove offline suite)
- [x] 1. No implicit or escaping environment-file copies
- [ ] 2. Canonical in-repo worktree guard
- [ ] 3. Branch names and Git option safety
- [ ] 4. Valid NDJSON for every control character
- [ ] 5. Branch/worktree ownership revalidation before removal
- [ ] 6. Stale worktree metadata recovery
- [ ] 7. Failed creation rollback
- [ ] 8. Final adversarial audit + RESULT.md

**Current milestone:** 2

## Milestone 1 — complete (22:55 HKT)

- `files::env_sources` rewritten as a strict allowlist: empty list → zero side effects (no implicit
  `.env*` discovery, no target mkdir); non-empty entries must be repository-relative regular files —
  absolute paths, any `..` component, symlinks (final component), directories, and canonical-path
  escapes via intermediate symlinks are all rejected with explicit errors. Missing entries still skip.
- Destination behavior unchanged (flat `target/basename`).
- Converted all 4 `documents_defect_copy_env_files_*` tests to regressions; added
  `copy_pattern_escaping_repo_through_intermediate_symlink_is_rejected`.
- Legacy unsafe unit tests flipped (`auto_discovers_root_env_files`, `creates_missing_target_dir`,
  `skips_directories_named_env`, `unicode_env_suffix_is_copied`, strengthened `empty_repo_copies_nothing`).
- e2e: `new_creates_worktree_and_query_prints_path` no longer expects the default `.env` copy; added
  `new_without_copy_config_does_not_copy_env_files` and `new_copies_only_explicitly_configured_env_files`.
- Gate: fmt PASS · clippy -D warnings PASS · bin 67/68 (only known milestone-2 red) · cli 27/27.

## Milestone 0 — complete (22:47 HKT)

- Change: `git2 = { version = "0.20", default-features = false }` (ssh/https transports unused;
  git2 used only for local discover/open/worktrees/HEAD). `cargo update --offline -p git2` removed
  `openssl-sys`, `libssh2-sys`, `openssl-probe` from the graph and lockfile.
- Gate: `cargo fmt --all -- --check` PASS · `cargo clippy --offline --all-targets -- -D warnings` PASS ·
  `cargo test --offline --all-targets` 66/67 green.
- One red: `git::tests::documents_defect_in_repo_worktree_path_rejected` — pre-existing on this macOS
  host (proved by stash → rerun on pristine base: same failure). Cause: fixture under `/var/folders/…`
  TMPDIR; canonical form `/private/var/…`; lexical guard misses in-repo path. This is exactly
  milestone 2's defect and its conversion target. Full suite green is restored at milestone 2.

## Commands run

| Time (HKT) | Command | Result |
|---|---|---|
| 22:38 | `git rev-parse HEAD` / `origin/main` | both `d1eb5af…` — base verified, no drift |

## Findings

(none yet)

## Recovery notes

If this session dies: re-read this file, then CONTRACT.md. Each completed milestone is one
local commit on this lane; do not rewrite them. Work only inside this worktree.
