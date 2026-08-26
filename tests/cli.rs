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
    assert_eq!(
        fs::read_to_string(Path::new(&path).join(".env")).expect("copied env"),
        "FAKECREDENTIAL=1"
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

#[test]
fn new_duplicate_branch_fails() {
    let fx = Fixture::new("new-dup");
    assert_success(&fx.lucus(&["new", "feat-dup"]));
    let output = fx.lucus(&["new", "feat-dup"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("worktree already exists"));
}

#[test]
fn new_rejects_branch_that_sanitizes_to_empty() {
    let fx = Fixture::new("new-empty");
    let output = fx.lucus(&["new", "!!!"]);
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("empty after sanitization"));
}

#[test]
fn new_unicode_branch_is_stripped() {
    let fx = Fixture::new("new-unicode");
    let output = fx.lucus(&["new", "feat/你好"]);
    // Sanitizes to "feat/", which git typically rejects.
    let code = assert_failure(&output);
    assert_eq!(code, 1);
    assert!(stderr(&output).contains("lucus:"));
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
