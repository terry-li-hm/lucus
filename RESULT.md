# Lucus overnight safety completion — result

- **Base:** `d1eb5af60304fcfa248ddc5bd946ccd9eb13f66c` (`origin/main`, verified before any edit)
- **Lane:** `lane/lucus-safety-overnight-local` plus one focused follow-up, 14 commits at final acceptance, no history rewrite
- **Initial completion:** 2026-08-26 15:20 UTC (23:20 HKT) — all milestones complete before the deadline
- **Final cockpit acceptance:** 2026-08-27 08:03 HKT at `acc05f4`, after post-review hardening, installed-surface repair, and Linux verification
- **Final checks (all PASS on macOS and Linux):** `cargo fmt --all -- --check` ·
  `cargo test --offline --all-targets` (87 bin + 42 integration) ·
  `cargo clippy --offline --all-targets -- -D warnings` · `git diff --check` · Chaperone strict review

## Acceptance predicates → tests → commits

### 0. Linux offline verification path — `a9659b8`
- `git2` reduced to `default-features = false` (ssh/https transports unused); lockfile updated
  offline (`openssl-sys`, `libssh2-sys`, `openssl-probe` removed; `libz-sys` retained — not a
  network feature).
- Local discover/open/worktree/HEAD coverage proven offline by `git::tests::list_worktrees_includes_main_checkout`,
  `current_branch_reads_head`, `find_worktree_by_branch_none_for_unknown`,
  `worktree_add_and_stats_and_remove`, plus the whole suite running `--offline`.
- Known red at this milestone: `documents_defect_in_repo_worktree_path_rejected` failed on the
  pristine macOS base (proved via stash → rerun); it was milestone 2's target and went green at
  `952c073`. Suite has been fully green from milestone 2 onward.

### 1. No implicit or escaping environment-file copies — `94dd4c9`
- Empty `[files].copy` copies nothing (and creates nothing):
  `files::tests::empty_copy_list_copies_nothing_and_never_discovers_env_files`,
  `copy_env_files_empty_repo_copies_nothing`, e2e `new_without_copy_config_does_not_copy_env_files`,
  `new_creates_worktree_and_query_prints_path` (legacy unsafe assertion flipped).
- Repository-relative allowlist only — rejects absolute (`absolute_copy_pattern_is_rejected`),
  `..` (`parent_traversal_copy_pattern_is_rejected`), directories (`copy_env_files_rejects_directory_entry`),
  symlinks (`symlink_copy_pattern_is_rejected`), canonical-path escape via intermediate symlink
  (`copy_pattern_escaping_repo_through_intermediate_symlink_is_rejected`).
- All four `documents_defect_copy_env_files_*` tests converted to regressions; explicit-copy e2e:
  `new_copies_only_explicitly_configured_env_files`. Destination behavior unchanged (flat basename).

### 2. Canonical in-repo worktree guard — `952c073`
- `resolve_worktree_path` compares the physically resolved target (canonical deepest existing
  ancestor + tail) against the canonical repo root. In-repo rejected on macOS `/var`→`/private/var`
  (`in_repo_worktree_path_is_rejected_through_symlinked_temp_prefix`) and through a synthetic
  symlinked repo dir that runs on Linux (`in_repo_worktree_path_through_symlinked_repo_dir_is_rejected`).
- Outside-repository default unchanged: `symlinked_repo_dir_default_template_still_resolves_outside`,
  `resolve_worktree_path_renders_default_template_outside_repo`,
  `resolve_worktree_path_accepts_absolute_template_outside_repo`, e2e `new_with_in_repo_template_fails`.

### 3. Branch names and Git option safety — `c1d270b`
- Dots preserved, no silent collapse: `validate_branch_name` is identity-preserving
  (`branch_validation_preserves_dots_in_user_branch`, `validate_branch_name_never_rewrites_distinct_names`,
  rejects >60 chars and invalid characters explicitly), e2e `new_preserves_dotted_branch_names`
  (create/query/remove of `release/1.2.3`; collapsed variant never exists) and
  `branch_delete_removes_dotted_branch_after_dashdash_separator`.
- Leading `-` rejected, never reinterpreted: `branch_validation_rejects_leading_dashes`,
  e2e `new_rejects_leading_dash_branch_name` (via clap `--` passthrough).
- `--` separators added to `git worktree add`, `git worktree remove`, `git branch -d/-D`, and
  `git show-ref --verify` (verified against a scratch repo before adopting).
- CLI unchanged; invalid-character rejections replace silent rewriting
  (`new_rejects_branch_name_with_invalid_characters`, `new_rejects_unicode_branch_name`).

### 4. Valid NDJSON for every control character — `773eee1`
- Full C0 range escaped (`escape_json_escapes_all_control_characters`; `\b`/`\f` short escapes,
  `\u00xx` otherwise — serde_json-compatible).
- Every emitted line parses as strict JSON via a test-only recursive-descent parser (no runtime
  dependency): `ndjson_lines_with_control_characters_parse_as_strict_json` (round-trips control-heavy
  branch/path rows) and `strict_parser_rejects_raw_controls_and_bad_escapes` (parser itself proven
  strict). Physical single lines: `ndjson_lines_are_single_physical_lines`.

### 5. Branch/worktree ownership revalidation — `a8f64e9`
- `git::ensure_worktree_attached_to_branch` runs after `pre_remove` and before any destructive
  step: branch→worktree must re-resolve to the same canonical path and the worktree HEAD must
  still be that branch. Detached HEAD, ambiguous HEAD (unreadable HEAD propagates as refusal),
  and hook/concurrent changes all abort with state preserved.
- Unit: `ensure_worktree_attached_accepts_matching_worktree`, `..._rejects_detached_head`,
  `..._rejects_switched_branch`. e2e: `remove_refuses_when_pre_remove_hook_switches_branch`
  (deterministic race; unique commit object survives), `remove_refuses_when_pre_remove_hook_detaches_head`,
  `remove_refuses_when_worktree_detached_out_of_band`.

### 6. Stale worktree metadata recovery — `01bea19`
- `list`/`query` skip missing-directory registrations instead of failing wholesale; `remove`
  recovers: `git::stale_worktrees` detects them (branch read from administrative `HEAD` file),
  bounded `git worktree prune` clears metadata (never a live directory or branch), then ordinary
  `-d`/`-D` branch deletion. e2e `list_query_and_remove_survive_out_of_band_directory_loss`
  (including full repo reusability afterwards).
- Safety of the bounded path: unit `prune_never_removes_live_worktree_or_branch`,
  `stale_worktrees_reports_missing_directory_and_branch`,
  `list_and_find_survive_missing_worktree_directory`; unmerged-branch refusal with surviving
  commit and `--force` retry: `stale_worktree_recovery_refuses_to_delete_unmerged_branch`.

### 7. Failed creation rollback — `9b5ccd2`
- Deterministic eligibility (`git::creation_is_rollback_eligible`): still registered+attached,
  branch tip == pre-add base SHA, every dirty path is a lucus-copied file still byte-identical to
  its source. Rollback (`git::rollback_clean_creation`) removes only those copies, then non-force
  `worktree remove` + `branch -d` — force is never the rollback path.
- Rollback regressions: e2e `new_rolls_back_when_copy_fails_midway` (first copy cleaned, second
  unreadable), `new_rolls_back_when_copy_entry_is_invalid`, `new_rolls_back_when_post_create_hook_fails_cleanly`;
  unit `pristine_creation_is_rollback_eligible_and_rolls_back_without_force`,
  `rollback_eligibility_survives_identical_copies_but_not_divergence`.
- Preservation regressions: e2e `new_preserves_worktree_when_hook_makes_commit` (recovery steps
  name the exact `lucus remove` command), `new_preserves_worktree_when_hook_leaves_untracked_file`;
  unit `rollback_eligibility_refuses_advanced_branch_and_extra_files`.

### 8. Final adversarial audit — this file
- All 9 `git` subprocess sites re-read: every user-influenced ref is `--`-separated
  (`worktree add/remove`, `branch -d/-D`, `show-ref`) or internally constructed from validated
  names (`rev-parse --verify`, `@{-1}`, fixed `status`/`prune` args). `sh -c` hooks run only
  user-configured commands with env-injected values (no interpolation).
- Filesystem boundaries re-read: copy allowlist validation, physical in-repo guard, canonical
  containment in `ensure_gitignore`, rollback's byte-identical-only deletions, stale-metadata
  reads. No remaining force flags, implicit secret copying, unchecked traversal, or invalid
  NDJSON found. No new causally-connected defect; no fix required.

## Post-review hardening — `c08136a`

Cockpit review found that the initial rollback inferred copied names from configuration rather than
recording successful copies. It could therefore remove a tracked destination or miss an ignored
hook-created file. A direct probe also confirmed that `git worktree add` runs repository
`post-checkout` hooks before Lucus copies files, so ordinary overwrite semantics could replace
hook-created state.

The accepted repair records each successful copy with its exact source and destination, includes
ignored paths in rollback eligibility, plans every deletion before acting, revalidates attachment
and branch tip, and refuses tracked or uncertain destinations. Copy sources must now be regular
files, metadata errors other than `NotFound` propagate, and destinations use atomic no-clobber
creation with streamed I/O. New regressions cover Unix sockets, exact copy records, tracked
copy destinations, missing configured files followed by ignored hook output, and pre-copy
`post-checkout` hook output.

## Installed-surface follow-up — `acc05f4`

The rebuilt binary exposed one further live defect: an unrelated registered worktree whose directory
still existed but whose linked `.git` target was broken caused `list`, `query`, and `remove` to fail
for every healthy branch. Lucus now skips that malformed registration during listing without
classifying it as stale, pruning it, or touching its directory, files, or branch. A synthetic
regression preserves a sentinel file and branch while proving healthy lookup remains available.
The exact revision passed the full 129-test suite and Clippy on macOS and Linux, then passed a live
`list` and `query main` smoke against the previously blocking Germline registration.

## Unresolved risks (accepted, with reasoning)

1. **TOCTOU windows are narrowed, not eliminated.** Ownership is revalidated immediately before
   destruction (M5) and rollback eligibility re-checked before rollback (M7), but a concurrent
   actor can still race between check and act. Fully closing this needs lockfiles/atomicity in
   git itself — out of scope.
2. **Partial remove sequence.** If `worktree remove` succeeds but `branch -d` refuses (unmerged),
   the branch remains without a worktree; recovery is `lucus remove --force <branch>` (the
   force-gated branch-only completion) or merging first. Unique state is never silently lost.
3. **`post_remove` hook failure after successful removal** exits 1 even though the removal is
   complete; state is consistent, only the summary line is missing.
4. **`git::rev_parse` has no `--` separator.** Its revision argument is always constructed
   internally from a validated branch name or the literal `HEAD`, so no causal defect exists;
   adding `--` would be hardening only.
5. **Force-gated branch-only deletion** deletes a named branch when no worktree holds it — only
   under the user's explicit `--force`, with `-D` semantics. Narrow, documented, tested.
6. **`ensure_gitignore` retains in-repo write logic** that is unreachable for `new` targets since
   M2 (outside-repo invariant); kept for API completeness, noted for future pruning.
7. **Hooks execute `sh -c` strings from user config** — a trusted-config capability by design;
   lucus only guarantees its own values arrive via environment variables, never interpolated.

## Platform verification completed

- **Linux:** the exact `acc05f4` bundle passed the full offline fmt, 129-test, Clippy, and diff
  checks on Ganglion (Linux 6.17, AArch64, rustc 1.98.0, Git 2.43.0). This exercised synthetic
  symlink handling, `git worktree prune`, ignored-path porcelain output, the chmod-000 copy-failure
  fixture, Unix sockets, and post-checkout hooks as a non-root user.
- **macOS:** the same revision passed the full checks on macOS 26, including the
  `/var`→`/private/var` canonical mismatch regression and symlink-permissive temporary layouts.
