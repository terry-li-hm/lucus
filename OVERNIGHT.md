# Lucus overnight safety completion — progress log

- **Base SHA (verified, no drift):** `d1eb5af60304fcfa248ddc5bd946ccd9eb13f66c` = `origin/main`
- **Lane:** `lane/lucus-safety-overnight-local` (local commits only, no push)
- **Deadline:** 2026-08-27 07:00 HKT / 2026-08-26 23:00 UTC
- **Session start:** 2026-08-26 22:38 HKT (14:38 UTC)

## Milestone checklist

- [x] 0. Linux offline verification path (disable unused git2 network features; prove offline suite)
- [x] 1. No implicit or escaping environment-file copies
- [x] 2. Canonical in-repo worktree guard
- [x] 3. Branch names and Git option safety
- [x] 4. Valid NDJSON for every control character
- [x] 5. Branch/worktree ownership revalidation before removal
- [ ] 6. Stale worktree metadata recovery
- [ ] 7. Failed creation rollback
- [ ] 8. Final adversarial audit + RESULT.md

**Current milestone:** 6

## Milestone 5 — complete (23:24 HKT)

- Added `git::ensure_worktree_attached_to_branch`: after `pre_remove` (and before any destructive
  step) `remove` re-resolves branch→worktree and requires the same canonical path, then re-opens the
  worktree repo and requires HEAD to still be that branch. Hook/concurrent switch, detach, or
  unregistration → explicit refusal with all state preserved. Detached-before-invocation is caught
  at initial resolution ("no worktree found") — also a safe refusal.
- Unit regressions in `git.rs`: accept-matching, reject-detached, reject-switched (each asserts
  worktree+branch preserved on refusal).
- e2e regressions: `remove_refuses_when_pre_remove_hook_switches_branch` (deterministic hook race;
  also proves the unique commit object survives), `remove_refuses_when_pre_remove_hook_detaches_head`,
  `remove_refuses_when_worktree_detached_out_of_band`.
- Gate: fmt PASS · bin 78/78 · cli 32/32 · clippy PASS.

## Milestone 4 — complete (23:15 HKT)

- `escape_json` now escapes every C0 control (serde_json-compatible policy: `\b \f \n \r \t`
  short escapes, `\u00xx` for the rest). Emission refactored into `ndjson_lines(rows)` so tests can
  validate the exact strings.
- Converted `documents_defect_ndjson_skips_control_character_escaping` → regression covering the
  full 0x00–0x1f range.
- Added a test-only strict JSON parser (objects, strings, integers; rejects raw control bytes,
  invalid escapes, malformed `\u`, trailing bytes) and a proof test that every emitted line for
  control-heavy branch/path rows parses strictly and round-trips to the original values. No
  runtime dependency added.
- Gate: fmt PASS · bin 75/75 · cli 29/29 · clippy PASS.

## Milestone 3 — complete (23:08 HKT)

- `sanitize_branch` (silent rewrite) replaced by `validate_branch_name` (identity-preserving,
  explicit rejection): charset `[A-Za-z0-9/._-]`; rejects leading `-` (option injection), leading
  `.`/`/`, `..` traversal, `//`, dot-leading components, trailing `/`/`.`/`.lock`, >60 chars.
  Dots preserved: `release/1.2.3` created unchanged (was `release/123`); no silent truncation
  collapse. `resolve_worktree_path` re-validates as defense in depth. `--` separators added to
  `git worktree add`, `git worktree remove`, and `git branch -d/-D` (verified accepted in a scratch
  repo first).
- Converted both `documents_defect_sanitize_*` tests into regressions; rewrote the strip-semantics
  unit tests as rejection tests; added `branch_delete_removes_dotted_branch_after_dashdash_separator`.
- e2e: `new_rejects_branch_name_with_invalid_characters`, `new_rejects_unicode_branch_name`,
  `new_preserves_dotted_branch_names` (also proves remove/query of dotted names),
  `new_rejects_leading_dash_branch_name` (via `--` passthrough).
- Gate: fmt PASS · bin 72/72 · cli 29/29 · clippy PASS · `git diff --check` clean.

## Milestone 2 — complete (23:01 HKT)

- `resolve_worktree_path` now canonicalizes the deepest existing ancestor of the resolved target
  (new helper `canonicalize_deepest_existing`) and performs the in-repo containment check on that
  physically resolved form vs the canonical repo root. Fixes the lexical-vs-canonical mismatch that
  let in-repo paths through behind symlinked prefixes (`/var`→`/private/var` TMPDIR fixtures,
  symlinked checkouts). Returned path unchanged (lexical), so outside-repo behavior is untouched.
- `documents_defect_in_repo_worktree_path_rejected` → regression
  `in_repo_worktree_path_is_rejected_through_symlinked_temp_prefix` (red on macOS base, now green).
- Added `in_repo_worktree_path_through_symlinked_repo_dir_is_rejected` (synthetic symlink, runs on
  Linux) and `symlinked_repo_dir_default_template_still_resolves_outside` (outside default intact).
- Gate (first fully green suite of the night): fmt PASS · bin 70/70 · cli 27/27 · clippy PASS.

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
