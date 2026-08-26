use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let mut root = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        root.push(format!(
            "lucus-cli-{label}-{}-{stamp}-{n}",
            std::process::id()
        ));
        let repo = root.join("repo");
        fs::create_dir_all(&repo).expect("fixture dirs");
        write_gitconfig(&root);
        git_init(&repo, &root);
        Self { root, repo }
    }

    fn lucus(&self, args: &[&str]) -> Output {
        self.lucus_in(&self.repo, args)
    }

    fn lucus_in(&self, cwd: &Path, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lucus"));
        cmd.current_dir(cwd).args(args);
        apply_isolation(&mut cmd, &self.root);
        cmd.output().expect("lucus should run")
    }

    fn write_config(&self, body: &str) {
        let xdg = self.root.join(".config").join("lucus");
        let macos = self
            .root
            .join("Library")
            .join("Application Support")
            .join("lucus");
        fs::create_dir_all(&xdg).expect("linux config dir");
        fs::create_dir_all(&macos).expect("macos config dir");
        fs::write(xdg.join("config.toml"), body).expect("write linux config");
        fs::write(macos.join("config.toml"), body).expect("write macos config");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn apply_isolation(cmd: &mut Command, home: &Path) {
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", home.join("gitconfig"))
        .env("GIT_AUTHOR_NAME", "Lucus Test")
        .env("GIT_AUTHOR_EMAIL", "lucus-test@example.com")
        .env("GIT_COMMITTER_NAME", "Lucus Test")
        .env("GIT_COMMITTER_EMAIL", "lucus-test@example.com")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_TEMPLATE_DIR");
}

fn write_gitconfig(home: &Path) {
    fs::write(
        home.join("gitconfig"),
        "[user]\n    name = Lucus Test\n    email = lucus-test@example.com\n[commit]\n    gpgsign = false\n[init]\n    defaultBranch = main\n",
    )
    .expect("write gitconfig");
}

fn git(dir: &Path, home: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new("git");
    cmd.args(["-C"]).arg(dir).args(args);
    apply_isolation(&mut cmd, home);
    let output = cmd.output().expect("git should run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git_init(repo: &Path, home: &Path) {
    let mut cmd = Command::new("git");
    cmd.args(["init", "-b", "main"]).arg(repo);
    apply_isolation(&mut cmd, home);
    let output = cmd.output().expect("git init should run");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    git(repo, home, &["config", "user.name", "Lucus Test"]);
    git(
        repo,
        home,
        &["config", "user.email", "lucus-test@example.com"],
    );
    git(repo, home, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("README.md"), "hello\n").expect("write readme");
    git(repo, home, &["add", "README.md"]);
    git(repo, home, &["commit", "-m", "init"]);
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success, got {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout(output),
        stderr(output)
    );
}

fn assert_failure(output: &Output) -> i32 {
    assert!(
        !output.status.success(),
        "expected failure, got success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
    output.status.code().expect("exit code")
}

fn branch_exists(fx: &Fixture, branch: &str) -> bool {
    Command::new("git")
        .args(["-C"])
        .arg(&fx.repo)
        .args(["show-ref", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .status()
        .expect("git show-ref")
        .success()
}

#[test]
fn help_and_version_succeed() {
    let fx = Fixture::new("help");
    let help = fx.lucus(&["--help"]);
    assert_success(&help);
    assert!(stdout(&help).contains("Git worktree manager"));
    let version = fx.lucus(&["--version"]);
    assert_success(&version);
    assert!(stdout(&version).contains("lucus"));
}

#[test]
fn missing_subcommand_fails() {
    let fx = Fixture::new("no-cmd");
    let code = assert_failure(&fx.lucus(&[]));
    assert_ne!(code, 0);
}

#[test]
fn list_emits_ndjson_for_main_when_stdout_is_piped() {
    let fx = Fixture::new("list");
    let output = fx.lucus(&["list"]);
    assert_success(&output);
    let listed = stdout(&output);
    let line = listed.lines().next().expect("one line");
    assert!(line.contains("\"branch\":\"main\""));
    assert!(line.contains("\"ahead\":0"));
    assert!(line.contains("\"uncommitted\":0"));
}

#[test]
fn list_json_flag_is_ndjson() {
    let fx = Fixture::new("list-json");
    let output = fx.lucus(&["--json", "list"]);
    assert_success(&output);
    assert!(stdout(&output).contains("\"branch\":\"main\""));
}

#[test]
fn list_fails_outside_a_git_repository() {
    let fx = Fixture::new("list-nogit");
    let empty = fx.root.join("empty");
    fs::create_dir_all(&empty).expect("empty dir");
    let output = fx.lucus_in(&empty, &["list"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("not inside a git repository"));
}

#[test]
fn new_creates_worktree_and_query_prints_path() {
    let fx = Fixture::new("new");
    fs::write(fx.repo.join(".env"), "FAKECREDENTIAL=1").expect("write env");
    let created = fx.lucus(&["new", "feat-auth"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    assert!(Path::new(&path).is_dir());
    assert!(Path::new(&path).join("README.md").exists());
    // No [files] copy allowlist: credentials must stay in the source checkout.
    assert!(
        !Path::new(&path).join(".env").exists(),
        "root .env must not be copied without an explicit files.copy entry"
    );
    assert!(
        !fx.repo.join(".gitignore").exists(),
        "outside worktrees should not touch gitignore"
    );

    let queried = fx.lucus(&["query", "feat-auth"]);
    assert_success(&queried);
    assert_eq!(
        Path::new(stdout(&queried).trim())
            .canonicalize()
            .expect("canonicalize query path"),
        Path::new(&path)
            .canonicalize()
            .expect("canonicalize new path")
    );

    let switched = fx.lucus(&["switch", "feat-auth"]);
    assert_success(&switched);
    assert_eq!(
        Path::new(stdout(&switched).trim())
            .canonicalize()
            .expect("canonicalize switch path"),
        Path::new(&path)
            .canonicalize()
            .expect("canonicalize new path")
    );
}

/// Regression: plain `lucus new` with no `[files]` configuration copies no
/// environment files into the new worktree — not even a root `.env`.
#[test]
fn new_without_copy_config_does_not_copy_env_files() {
    let fx = Fixture::new("new-no-copy");
    fs::write(fx.repo.join(".env"), "FAKECREDENTIAL=1").expect("write .env");
    fs::write(fx.repo.join(".env.local"), "FAKECREDENTIAL=2").expect("write .env.local");
    fs::write(fx.repo.join(".envrc"), "FAKECREDENTIAL=3").expect("write .envrc");

    let created = fx.lucus(&["new", "feat-no-copy"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();

    assert!(
        !Path::new(&path).join(".env").exists(),
        ".env must not be copied"
    );
    assert!(
        !Path::new(&path).join(".env.local").exists(),
        ".env.local must not be copied"
    );
    assert!(
        !Path::new(&path).join(".envrc").exists(),
        ".envrc must not be copied"
    );
}

/// The explicit allowlist copies exactly the configured files and nothing else.
#[test]
fn new_copies_only_explicitly_configured_env_files() {
    let fx = Fixture::new("new-copy-allowlist");
    fs::write(fx.repo.join(".env"), "FAKECREDENTIAL=1").expect("write .env");
    fs::write(fx.repo.join(".envrc"), "FAKECREDENTIAL=2").expect("write .envrc");
    fx.write_config("[files]\ncopy = [\".env\"]\n");

    let created = fx.lucus(&["new", "feat-copy-allowlist"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();

    assert_eq!(
        fs::read_to_string(Path::new(&path).join(".env")).expect("listed .env should be copied"),
        "FAKECREDENTIAL=1"
    );
    assert!(
        !Path::new(&path).join(".envrc").exists(),
        "unlisted files must not be copied"
    );
}

#[test]
fn new_duplicate_branch_fails() {
    let fx = Fixture::new("new-dup");
    assert_success(&fx.lucus(&["new", "feat-dup"]));
    let output = fx.lucus(&["new", "feat-dup"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("worktree already exists"));
}

/// Regression: names made only of invalid characters are rejected up front
/// with an explicit error instead of silently "sanitizing to empty".
#[test]
fn new_rejects_branch_name_with_invalid_characters() {
    let fx = Fixture::new("new-invalid");
    let output = fx.lucus(&["new", "!!!"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("characters not allowed"),
        "unexpected stderr: {}",
        stderr(&output)
    );
}

#[test]
fn new_rejects_unicode_branch_name() {
    let fx = Fixture::new("new-unicode");
    let output = fx.lucus(&["new", "feat/你好"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("characters not allowed"),
        "unexpected stderr: {}",
        stderr(&output)
    );
}

/// Regression: `release/1.2.3` used to collapse to `release/123`; valid
/// dotted names are now created unchanged.
#[test]
fn new_preserves_dotted_branch_names() {
    let fx = Fixture::new("new-dotted");
    let created = fx.lucus(&["new", "release/1.2.3"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    assert!(Path::new(&path).is_dir());
    assert!(branch_exists(&fx, "release/1.2.3"));
    // The collapsed variant must not exist as a side effect.
    assert!(!branch_exists(&fx, "release/123"));

    let queried = fx.lucus(&["query", "release/1.2.3"]);
    assert_success(&queried);

    let removed = fx.lucus(&["remove", "release/1.2.3"]);
    assert_success(&removed);
    assert!(!branch_exists(&fx, "release/1.2.3"));
}

/// Regression: option-like branch names starting with `-` are rejected by
/// lucus itself (passed via `--` so clap does not intercept them first).
#[test]
fn new_rejects_leading_dash_branch_name() {
    let fx = Fixture::new("new-dash");
    let output = fx.lucus(&["new", "--", "--force"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("could be parsed as a git option"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(
        stdout(&output).trim().is_empty(),
        "nothing should be created for a rejected branch"
    );
}

#[test]
fn query_at_prints_current_worktree() {
    let fx = Fixture::new("query-at");
    let output = fx.lucus(&["query", "@"]);
    assert_success(&output);
    let printed = stdout(&output);
    let canonical = fx.repo.canonicalize().expect("canonicalize repo");
    assert_eq!(Path::new(printed.trim()), canonical.as_path());
}

#[test]
fn query_caret_uses_configured_default_branch() {
    let fx = Fixture::new("query-caret");
    let output = fx.lucus(&["query", "^"]);
    assert_success(&output);
    let canonical = fx.repo.canonicalize().expect("canonicalize repo");
    assert_eq!(stdout(&output).trim(), canonical.to_string_lossy());
}

#[test]
fn query_missing_branch_fails() {
    let fx = Fixture::new("query-missing");
    let output = fx.lucus(&["query", "no-such-branch"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("no worktree found"));
}

#[test]
fn query_previous_fails_without_checkout_history() {
    let fx = Fixture::new("query-prev");
    let output = fx.lucus(&["query", "-"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("unable to resolve previous branch"));
}

#[test]
fn remove_missing_branch_fails() {
    let fx = Fixture::new("remove-missing");
    let output = fx.lucus(&["remove", "ghost"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("no worktree found"));
}

#[test]
fn remove_clean_worktree() {
    let fx = Fixture::new("remove-clean");
    let created = fx.lucus(&["new", "feat-clean"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    let removed = fx.lucus(&["remove", "feat-clean"]);
    assert_success(&removed);
    assert!(stderr(&removed).contains("removed worktree"));
    assert!(!Path::new(&path).exists());
    assert!(!branch_exists(&fx, "feat-clean"));
}

#[test]
fn remove_dirty_without_force_fails_and_keeps_branch() {
    let fx = Fixture::new("remove-dirty");
    let created = fx.lucus(&["new", "feat-dirty"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(Path::new(&path).join("dirty.txt"), "x").expect("dirty file");
    let output = fx.lucus(&["remove", "feat-dirty"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("git worktree remove failed"));
    assert!(Path::new(&path).exists());
    assert!(branch_exists(&fx, "feat-dirty"));
}

#[test]
fn remove_dirty_with_force_succeeds() {
    let fx = Fixture::new("remove-force");
    let created = fx.lucus(&["new", "feat-force"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(Path::new(&path).join("dirty.txt"), "x").expect("dirty file");
    let removed = fx.lucus(&["remove", "--force", "feat-force"]);
    assert_success(&removed);
    assert!(!Path::new(&path).exists());
    assert!(!branch_exists(&fx, "feat-force"));
}

/// `lucus remove` without `--force` uses `git branch -d` and refuses to
/// delete a branch that still has unique commits.
#[test]
fn remove_without_force_refuses_unmerged_branch() {
    let fx = Fixture::new("remove-unmerged");
    let created = fx.lucus(&["new", "feat-unique"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(Path::new(&path).join("unique.txt"), "only-here").expect("unique file");
    git(Path::new(&path), &fx.root, &["add", "unique.txt"]);
    git(Path::new(&path), &fx.root, &["commit", "-m", "unique"]);
    let head = git(Path::new(&path), &fx.root, &["rev-parse", "HEAD"]);
    let sha = String::from_utf8_lossy(&head.stdout).trim().to_owned();

    let output = fx.lucus(&["remove", "feat-unique"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("git branch -d failed"),
        "unexpected stderr:\n{}",
        stderr(&output)
    );
    assert!(branch_exists(&fx, "feat-unique"));

    let mut cat_cmd = Command::new("git");
    cat_cmd
        .args(["-C"])
        .arg(&fx.repo)
        .args(["cat-file", "-t", &sha]);
    let cat = cat_cmd.output().expect("cat-file");
    assert!(
        cat.status.success(),
        "unique commit object should still exist until gc"
    );
}

#[test]
fn remove_with_force_deletes_unmerged_branch() {
    let fx = Fixture::new("remove-unmerged-force");
    let created = fx.lucus(&["new", "feat-unique-force"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(Path::new(&path).join("unique.txt"), "only-here").expect("unique file");
    git(Path::new(&path), &fx.root, &["add", "unique.txt"]);
    git(Path::new(&path), &fx.root, &["commit", "-m", "unique"]);

    let removed = fx.lucus(&["remove", "--force", "feat-unique-force"]);
    assert_success(&removed);
    assert!(!Path::new(&path).exists());
    assert!(!branch_exists(&fx, "feat-unique-force"));
}

/// Regression: a `pre_remove` hook that switches the worktree to another
/// branch must abort the removal; the worktree, branch, and unique commits
/// are preserved untouched.
#[test]
fn remove_refuses_when_pre_remove_hook_switches_branch() {
    let fx = Fixture::new("remove-hook-race");
    let created = fx.lucus(&["new", "feat-race"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(Path::new(&path).join("race.txt"), "unique").expect("unique file");
    git(Path::new(&path), &fx.root, &["add", "race.txt"]);
    git(Path::new(&path), &fx.root, &["commit", "-m", "race"]);
    let head = git(Path::new(&path), &fx.root, &["rev-parse", "HEAD"]);
    let sha = String::from_utf8_lossy(&head.stdout).trim().to_owned();

    git(&fx.repo, &fx.root, &["branch", "hook-target"]);
    let quote = char::from_u32(0x22).expect("quote");
    let backslash = char::from_u32(0x5c).expect("backslash");
    let hook = format!(
        "git -C {backslash}{quote}$LUCUS_WORKTREE_PATH{backslash}{quote} checkout hook-target"
    );
    fx.write_config(&format!("[hooks]\npre_remove = [\"{hook}\"]\n"));

    let output = fx.lucus(&["remove", "feat-race"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("no longer attached"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(Path::new(&path).is_dir(), "worktree must be preserved");
    assert!(branch_exists(&fx, "feat-race"), "branch must be preserved");

    let mut cat_cmd = Command::new("git");
    cat_cmd
        .args(["-C"])
        .arg(&fx.repo)
        .args(["cat-file", "-t", &sha]);
    let cat = cat_cmd.output().expect("cat-file");
    assert!(
        cat.status.success(),
        "unique commit object must survive the refused removal"
    );
}

/// Regression: a `pre_remove` hook that detaches the worktree HEAD must
/// abort the removal with all state preserved.
#[test]
fn remove_refuses_when_pre_remove_hook_detaches_head() {
    let fx = Fixture::new("remove-hook-detach");
    let created = fx.lucus(&["new", "feat-detach"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    let quote = char::from_u32(0x22).expect("quote");
    let backslash = char::from_u32(0x5c).expect("backslash");
    let hook = format!(
        "git -C {backslash}{quote}$LUCUS_WORKTREE_PATH{backslash}{quote} checkout --detach"
    );
    fx.write_config(&format!("[hooks]\npre_remove = [\"{hook}\"]\n"));

    let output = fx.lucus(&["remove", "feat-detach"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("no longer attached"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(Path::new(&path).is_dir(), "worktree must be preserved");
    assert!(
        branch_exists(&fx, "feat-detach"),
        "branch must be preserved"
    );
}

/// Regression: a worktree detached out of band (no hook) is refused too.
#[test]
fn remove_refuses_when_worktree_detached_out_of_band() {
    let fx = Fixture::new("remove-oob-detach");
    let created = fx.lucus(&["new", "feat-oob"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    git(Path::new(&path), &fx.root, &["checkout", "--detach"]);

    let output = fx.lucus(&["remove", "feat-oob"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    // Detached before lucus even resolves: the initial lookup already
    // refuses because the branch is no longer checked out anywhere.
    assert!(
        stderr(&output).contains("no worktree found for branch feat-oob"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(Path::new(&path).is_dir(), "worktree must be preserved");
    assert!(branch_exists(&fx, "feat-oob"), "branch must be preserved");
}

/// Regression: out-of-band loss of a registered worktree directory must not
/// brick `list` or `query`, and `remove` recovers by pruning the stale
/// registration and deleting the branch with ordinary merged-state
/// protection.
#[test]
fn list_query_and_remove_survive_out_of_band_directory_loss() {
    let fx = Fixture::new("stale-recovery");
    let created = fx.lucus(&["new", "feat-stale"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::remove_dir_all(&path).expect("out-of-band directory loss");

    let listed = fx.lucus(&["list"]);
    assert_success(&listed);
    assert!(
        stdout(&listed).contains("\"branch\":\"main\""),
        "live entries must still be listed: {}",
        stdout(&listed)
    );
    assert!(
        !stdout(&listed).contains("feat-stale"),
        "stale registration must not be listed"
    );

    let queried = fx.lucus(&["query", "feat-stale"]);
    let code = assert_failure(&queried);
    assert_eq!(code, 1);
    assert!(stderr(&queried).contains("no worktree found"));

    let removed = fx.lucus(&["remove", "feat-stale"]);
    assert_success(&removed);
    assert!(
        stderr(&removed).contains("pruned stale worktree registration"),
        "unexpected stderr: {}",
        stderr(&removed)
    );
    assert!(!branch_exists(&fx, "feat-stale"));

    // The repo is fully usable afterwards.
    let again = fx.lucus(&["new", "feat-stale"]);
    assert_success(&again);
}

/// Regression: recovery of a stale worktree for a branch with unique commits
/// still refuses to discard them without `--force`.
#[test]
fn stale_worktree_recovery_refuses_to_delete_unmerged_branch() {
    let fx = Fixture::new("stale-unmerged");
    let created = fx.lucus(&["new", "feat-stale-unique"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(Path::new(&path).join("unique.txt"), "only-here").expect("unique file");
    git(Path::new(&path), &fx.root, &["add", "unique.txt"]);
    git(Path::new(&path), &fx.root, &["commit", "-m", "unique"]);
    let head = git(Path::new(&path), &fx.root, &["rev-parse", "HEAD"]);
    let sha = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    fs::remove_dir_all(&path).expect("out-of-band directory loss");

    let output = fx.lucus(&["remove", "feat-stale-unique"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("pruned stale worktree registration"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("git branch -d failed"),
        "merged-state protection must apply in recovery: {}",
        stderr(&output)
    );
    assert!(
        branch_exists(&fx, "feat-stale-unique"),
        "branch must be preserved"
    );

    let mut cat_cmd = Command::new("git");
    cat_cmd
        .args(["-C"])
        .arg(&fx.repo)
        .args(["cat-file", "-t", &sha]);
    let cat = cat_cmd.output().expect("cat-file");
    assert!(
        cat.status.success(),
        "unique commit object must survive the refused recovery"
    );

    // Explicit force completes the removal, as in the normal path.
    let forced = fx.lucus(&["remove", "--force", "feat-stale-unique"]);
    assert_success(&forced);
    assert!(!branch_exists(&fx, "feat-stale-unique"));
}

/// Regression: a copy failure midway through copying (first file copied,
/// second unreadable) rolls back the freshly created clean worktree and
/// branch without force; the already-copied file is removed as part of it.
#[test]
fn new_rolls_back_when_copy_fails_midway() {
    let fx = Fixture::new("rollback-copy-midway");
    fs::write(fx.repo.join(".env"), "FAKECREDENTIAL=1").expect("write .env");
    fs::write(fx.repo.join("locked.env"), "FAKECREDENTIAL=2").expect("write locked.env");
    fs::set_permissions(
        fx.repo.join("locked.env"),
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .expect("lock file");
    fx.write_config("[files]\ncopy = [\".env\", \"locked.env\"]\n");

    let output = fx.lucus(&["new", "feat-copy-fail"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("failed to copy env file"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("rolled back"),
        "rollback must be reported: {}",
        stderr(&output)
    );
    let path = fx.root.join("repo.feat-copy-fail");
    assert!(!path.exists(), "worktree must be rolled back");
    assert!(
        !branch_exists(&fx, "feat-copy-fail"),
        "branch must be rolled back"
    );

    fs::set_permissions(
        fx.repo.join("locked.env"),
        std::os::unix::fs::PermissionsExt::from_mode(0o644),
    )
    .expect("unlock for fixture cleanup");
}

/// Regression: an invalid copy entry (a directory) fails creation and rolls
/// everything back.
#[test]
fn new_rolls_back_when_copy_entry_is_invalid() {
    let fx = Fixture::new("rollback-copy-invalid");
    fs::create_dir_all(fx.repo.join("notes")).expect("directory entry");
    fx.write_config("[files]\ncopy = [\"notes\"]\n");

    let output = fx.lucus(&["new", "feat-copy-invalid"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("not a regular file"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("rolled back"));
    assert!(!fx.root.join("repo.feat-copy-invalid").exists());
    assert!(!branch_exists(&fx, "feat-copy-invalid"));
}

/// Regression: a failing post_create hook rolls back the clean worktree and
/// branch (never with force).
#[test]
fn new_rolls_back_when_post_create_hook_fails_cleanly() {
    let fx = Fixture::new("rollback-hook");
    fx.write_config("[hooks]\npost_create = [\"false\"]\n");

    let output = fx.lucus(&["new", "feat-hook-clean"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("hook command failed"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("rolled back"));
    assert!(!fx.root.join("repo.feat-hook-clean").exists());
    assert!(!branch_exists(&fx, "feat-hook-clean"));
}

/// Regression: a hook that commits before failing creates unique state —
/// the worktree and branch are preserved with recovery instructions.
#[test]
fn new_preserves_worktree_when_hook_makes_commit() {
    let fx = Fixture::new("preserve-hook-commit");
    fx.write_config(
        "[hooks]\npost_create = ['git -C \"$LUCUS_WORKTREE_PATH\" commit --allow-empty -m hook', 'false']\n",
    );

    let output = fx.lucus(&["new", "feat-hook-commit"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("may contain unique state"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("lucus remove feat-hook-commit"),
        "recovery steps must name the exact command: {}",
        stderr(&output)
    );
    let path = fx.root.join("repo.feat-hook-commit");
    assert!(path.is_dir(), "worktree must be preserved");
    assert!(
        branch_exists(&fx, "feat-hook-commit"),
        "branch must be preserved"
    );

    // The hook's commit is intact and the removal path still works.
    git(&path, &fx.root, &["log", "-1", "--oneline"]);
    let removed = fx.lucus(&["remove", "--force", "feat-hook-commit"]);
    assert_success(&removed);
}

/// Regression: a hook that leaves untracked files before failing creates
/// unique state — everything is preserved with recovery instructions.
#[test]
fn new_preserves_worktree_when_hook_leaves_untracked_file() {
    let fx = Fixture::new("preserve-hook-untracked");
    fx.write_config(
        "[hooks]\npost_create = ['touch \"$LUCUS_WORKTREE_PATH/leftover.txt\"', 'false']\n",
    );

    let output = fx.lucus(&["new", "feat-hook-leftover"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("may contain unique state"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    let path = fx.root.join("repo.feat-hook-leftover");
    assert!(path.is_dir(), "worktree must be preserved");
    assert!(path.join("leftover.txt").exists(), "hook file must survive");
    assert!(branch_exists(&fx, "feat-hook-leftover"));

    let removed = fx.lucus(&["remove", "--force", "feat-hook-leftover"]);
    assert_success(&removed);
}

/// Regression: a configured copy entry that names a tracked file must not
/// overwrite the checked-out copy in the new worktree. Creation fails with
/// an overwrite refusal, the tracked content stays intact, and because the
/// worktree is otherwise clean the rollback may remove it and the branch.
#[test]
fn new_refuses_to_overwrite_tracked_copy_destination_and_rolls_back_clean() {
    let fx = Fixture::new("tracked-copy-destination");
    fs::write(fx.repo.join("tracked.txt"), "tracked content").expect("write tracked file");
    git(&fx.repo, &fx.root, &["add", "tracked.txt"]);
    git(&fx.repo, &fx.root, &["commit", "-m", "add tracked"]);
    fx.write_config("[files]\ncopy = [\"tracked.txt\"]\n");

    let output = fx.lucus(&["new", "feat-tracked-copy"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("refusing to overwrite existing file"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("rolled back"),
        "clean creation may be rolled back: {}",
        stderr(&output)
    );
    // The tracked content was never overwritten.
    assert_eq!(
        fs::read_to_string(fx.repo.join("tracked.txt")).expect("tracked file intact"),
        "tracked content"
    );
    assert!(
        !fx.root.join("repo.feat-tracked-copy").exists(),
        "clean worktree must be rolled back"
    );
    assert!(
        !branch_exists(&fx, "feat-tracked-copy"),
        "branch must be rolled back"
    );
}

/// Regression: `git worktree add` runs repository post-checkout hooks before
/// `files.copy`, so a hook can create a unique ignored destination first.
/// lucus must refuse to overwrite its distinct contents and preserve it.
#[test]
fn new_refuses_to_overwrite_ignored_file_created_by_post_checkout_hook() {
    let fx = Fixture::new("post-checkout-hook-env");
    fs::write(fx.repo.join(".env"), "LUCUS=source").expect("write source env");
    fs::write(fx.repo.join(".gitignore"), ".env\n").expect("write gitignore");
    git(&fx.repo, &fx.root, &["add", ".gitignore"]);
    git(&fx.repo, &fx.root, &["commit", "-m", "ignore env"]);
    fx.write_config("[files]\ncopy = [\".env\"]\n");

    // A synthetic repository hook runs during the checkout that `git
    // worktree add` performs, before lucus copies anything.
    let hooks_dir = fx.repo.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir).expect("hooks dir exists");
    fs::write(
        hooks_dir.join("post-checkout"),
        "#!/bin/sh\nprintf 'HOOK-OWNED' > ./.env\n",
    )
    .expect("write post-checkout hook");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        hooks_dir.join("post-checkout"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("make hook executable");

    let output = fx.lucus(&["new", "feat-hook-env"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("refusing to overwrite existing file"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    let path = fx.root.join("repo.feat-hook-env");
    assert!(path.is_dir(), "worktree must be preserved");
    assert_eq!(
        fs::read_to_string(path.join(".env")).expect("hook file survives"),
        "HOOK-OWNED",
        "the hook's distinct contents must never be overwritten"
    );
    assert!(
        branch_exists(&fx, "feat-hook-env"),
        "branch must be preserved"
    );

    let removed = fx.lucus(&["remove", "--force", "feat-hook-env"]);
    assert_success(&removed);
}

/// Regression: a configured copy entry that is missing from the repo copies
/// nothing; a hook-created ignored file with the same basename is unique
/// state and must survive a failed creation instead of being rolled back
/// as if lucus had copied it.
#[test]
fn new_preserves_ignored_hook_file_matching_missing_copy_entry() {
    let fx = Fixture::new("preserve-ignored-hook-file");
    fs::write(fx.repo.join(".gitignore"), ".env\n").expect("write gitignore");
    git(&fx.repo, &fx.root, &["add", ".gitignore"]);
    git(&fx.repo, &fx.root, &["commit", "-m", "ignore env"]);
    fx.write_config(
        "[files]\ncopy = [\".env\"]\n[hooks]\npost_create = ['touch \"$LUCUS_WORKTREE_PATH/.env\"', 'false']\n",
    );

    let output = fx.lucus(&["new", "feat-ignored-env"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(
        stderr(&output).contains("may contain unique state"),
        "unexpected stderr: {}",
        stderr(&output)
    );
    let path = fx.root.join("repo.feat-ignored-env");
    assert!(path.is_dir(), "worktree must be preserved");
    assert!(
        path.join(".env").is_file(),
        "hook-created ignored file must be preserved"
    );
    assert!(
        branch_exists(&fx, "feat-ignored-env"),
        "branch must be preserved"
    );

    let removed = fx.lucus(&["remove", "--force", "feat-ignored-env"]);
    assert_success(&removed);
}

#[test]
fn init_zsh_writes_wrapper_under_isolated_home() {
    let fx = Fixture::new("init");
    let output = fx.lucus(&["init", "zsh"]);
    assert_success(&output);
    assert!(stderr(&output).contains("installed lucus shell wrapper"));
    let rc = fs::read_to_string(fx.root.join(".zshrc")).expect("zshrc");
    assert!(rc.contains("lucus() {"));
    assert!(rc.contains(r#"command lucus query "${@:2}""#));

    let again = fx.lucus(&["init", "zsh"]);
    assert_success(&again);
    assert!(stderr(&again).contains("already exists"));
}

#[test]
fn init_unsupported_shell_fails() {
    let fx = Fixture::new("init-fish");
    let output = fx.lucus(&["init", "fish"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("unsupported shell"));
}

#[test]
fn malformed_config_fails_non_list_commands() {
    let fx = Fixture::new("bad-config");
    fx.write_config("[worktree\n");
    let output = fx.lucus(&["query", "@"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("failed to parse config file"));

    let list = fx.lucus(&["list"]);
    assert_success(&list);
}

#[test]
fn new_with_in_repo_template_fails() {
    let fx = Fixture::new("in-repo-template");
    fx.write_config("[worktree]\npath_template = \".worktrees/{branch}\"\n");
    let output = fx.lucus(&["new", "feat-inside"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("inside source repository"));
}

#[test]
fn new_runs_post_create_hook_with_env() {
    let fx = Fixture::new("hooks");
    let marker = fx.root.join("hook.txt");
    fx.write_config(&format!(
        "[hooks]\npost_create = [\"printf '%s' \\\"$LUCUS_BRANCH\\\" > '{}'\"]\n",
        marker.display()
    ));
    let created = fx.lucus(&["new", "feat-hook"]);
    assert_success(&created);
    assert_eq!(
        fs::read_to_string(&marker).expect("hook marker"),
        "feat-hook"
    );
}

#[test]
fn failing_post_create_hook_exits_one() {
    let fx = Fixture::new("hook-fail");
    fx.write_config("[hooks]\npost_create = [\"false\"]\n");
    let output = fx.lucus(&["new", "feat-hook-fail"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("hook command failed"));
}
