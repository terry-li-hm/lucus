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
- [x] 6. Stale worktree metadata recovery
- [x] 7. Failed creation rollback
- [x] 8. Final adversarial audit + RESULT.md

**Current milestone:** DONE — all 8 milestones complete, 23:20 HKT, ~7.7h before deadline

## Milestone 8 — complete (23:21 HKT)

- Re-read all 9 git subprocess sites, the `sh -c` hook boundary, and every filesystem boundary in
  `src/`. No remaining force flags, missing `--` on user-controlled refs, implicit secret copying,
  unchecked traversal, partial destructive sequences, stale-metadata failure, or invalid NDJSON.
  No new causally-connected defect — no fix made (hardening-only `rev_parse --` noted in RESULT).
- Final checks: fmt PASS · bin 84/84 · cli 39/39 · clippy -D warnings PASS · `git diff --check` PASS.
- `RESULT.md` written: full predicate→test→commit map, unresolved risks, Linux verification notes.

## Outcome

8 commits on `lane/lucus-safety-overnight-local`, base `d1eb5af` (no drift, no rewrites, no push).
All 8 defect-documenting tests converted to regressions; 123 tests green offline.

## Milestone 7 — complete (23:47 HKT)

- `new` captures the base SHA before `git worktree add`; copy/`ensure_gitignore`/hooks moved into
  `finalize_creation`. On failure, `git::creation_is_rollback_eligible` deterministically decides:
  worktree still registered+attached to the branch, branch tip == base SHA, and every dirty/untracked
  path is a lucus-copied file still byte-identical to its source in the main checkout. Eligible →
  `git::rollback_clean_creation` removes only the identical copies, then non-force
  `worktree remove` + `branch -d`. Ineligible (hook commit, hook edit, hook file, moved worktree) →
  everything preserved with exact recovery steps naming `lucus remove <branch>`. Force never used.
- New helpers: `rev_parse` (with `--`), `dirty_paths` (porcelain parser), `branch_exists`, plus the
  two rollback functions above.
- Unit regressions: pristine rollback (no force), identical-vs-diverged copies, advanced branch,
  extra untracked file.
- e2e regressions: copy failure midway (chmod 000 second entry; first copy cleaned), invalid copy
  entry (directory), clean hook failure (`["false"]`), hook-commit preservation, hook-untracked-file
  preservation — each asserting rollback or preservation plus branch state.
- Note: `ensure_gitignore` no-ops for `new` paths since milestone 2 guarantees outside-repo targets,
  so no `.gitignore` rollback is needed.
- Gate: fmt PASS · bin 84/84 · cli 39/39 · clippy PASS.

## Milestone 6 — complete (23:34 HKT)

- `list_worktrees` now skips registrations whose directory is missing (previously one lost directory
  bricked `list`, `query`, `remove`, and `new` through the shared listing error).
- New `git::stale_worktrees` (detects missing-dir registrations; reads branch from admin `HEAD`
  file) and bounded `git::prune_stale_worktrees` (plain `git worktree prune` — metadata only, never
  a live directory or branch).
- `remove` recovery: stale registration for the named branch → prune then delete branch with the
  ordinary `-d`/`-D` semantics; branch-only completion only under explicit `--force` (covers the
  refused-recovery retry). All no-force refusals (detached/moved worktrees) unchanged. New
  `git::branch_exists` helper (with `--` separator).
- Ordering lesson (recorded): git refuses `branch -d` while a registered worktree still uses the
  branch, so recovery must prune first; the failed `-d` then leaves the branch — retryable only
  via the force-gated branch-only path.
- Unit regressions: `list_and_find_survive_missing_worktree_directory`,
  `stale_worktrees_reports_missing_directory_and_branch`, `prune_never_removes_live_worktree_or_branch`.
- e2e regressions: `list_query_and_remove_survive_out_of_band_directory_loss` (repo reusable
  afterwards — `new` again), `stale_worktree_recovery_refuses_to_delete_unmerged_branch` (unique
  commit survives; `--force` retry completes).
- Gate: fmt PASS · bin 81/81 · cli 34/34 · clippy PASS.

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

### PAUSED FOR GOAL RESTART — 2026-08-26 15:17 UTC (23:17 HKT)

- **State:** all 8 milestones complete. No edit in flight; pause taken at the final atomic
  boundary. **Next milestone: none** — do not start new work without a new contract/extension.
- **HEAD:** `0594024` (docs: record overnight completion map in RESULT.md); 9 commits total on
  `lane/lucus-safety-overnight-local`, base `d1eb5af` = `origin/main`.
- **Gates re-verified at pause:** fmt PASS · bin 84/84 · cli 39/39 · clippy -D warnings PASS ·
  `git diff --check` PASS. Deadline (23:00 UTC) not reached — ~7.7h remaining.
- **Untracked (intentionally not committed):** `CONTRACT.md` (the contract itself), `.pi/`
  (session artifacts). Neither is contract-editable.
- **For a continuation session:** re-read CONTRACT.md + this file + RESULT.md; verify base and
  HEAD; every predicate→test→commit mapping is in RESULT.md; unresolved risks and Linux
  verification needs are listed there. No pushes, no PRs, no history rewrites.

### GOAL RESTART VERIFICATION — 2026-08-26 15:20 UTC (23:20 HKT)

Resumed from the pause record above under the reloaded goal. No milestone remained; performed a
second adversarial sweep of the audit's borderline items instead of manufacturing work:

- `lucus new HEAD` (branch literally named `HEAD`): probed in a scratch repo — git itself refuses
  `worktree add -b HEAD` ("not a valid branch name"), so creation fails before any state exists.
  The destructive conflation path (`remove HEAD` matching a detached worktree listed as `HEAD`)
  is independently refused by the M5 revalidation (`head.is_branch()` false), covered by
  `ensure_worktree_attached_rejects_detached_head`. No defect.
- `dirty_paths` porcelain parsing (quoted paths, `R old -> new` renames, `?? dir/` untracked
  dirs): every non-trivial shape fails to match a copied basename → rollback eligibility refuses →
  state preserved. Parser errs safe. No defect.
- Nested copy entries (`conf/.env` copied flat as `.env`): byte-identity check reads the wrong
  source path → eligibility refuses → worktree preserved (conservative, never destructive).
  Acceptable limitation, noted.
- `init.rs` rc-file write follows symlinks under `$HOME`: standard editor semantics in the user's
  own home, not a worktree-lifecycle boundary; out of causal scope for this contract.

Final gates re-run at 15:19 UTC: fmt PASS · bin 84/84 · cli 39/39 · clippy -D warnings PASS ·
`git diff --check` PASS · 10 commits, base `d1eb5af` unchanged. **Campaign complete.**

If this session dies: re-read this file, then CONTRACT.md. Each completed milestone is one
local commit on this lane; do not rewrite them. Work only inside this worktree.
