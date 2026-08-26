use anyhow::{Context, Result, anyhow, ensure};
use git2::Repository;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct WorktreeEntry {
    pub branch: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct WorktreeStats {
    pub ahead: u32,
    pub behind: u32,
    pub uncommitted: u32,
}

pub fn repo_root_from_cwd() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let repo = Repository::discover(&cwd).context("not inside a git repository")?;
    let root = repo
        .workdir()
        .ok_or_else(|| anyhow!("repository has no working directory"))?;
    root.canonicalize()
        .with_context(|| format!("failed to canonicalize repo root: {}", root.display()))
}

pub fn current_branch(repo_root: &Path) -> Result<String> {
    let repo = Repository::open(repo_root)
        .with_context(|| format!("failed to open repository at {}", repo_root.display()))?;
    branch_for_repo(&repo)
}

pub fn previous_branch(repo_root: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--abbrev-ref", "@{-1}"])
        .output()
        .with_context(|| {
            format!(
                "failed to resolve previous branch for repository {}",
                repo_root.display()
            )
        })?;

    ensure!(output.status.success(), "unable to resolve previous branch");

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    ensure!(!value.is_empty(), "unable to resolve previous branch");
    Ok(value)
}

pub fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeEntry>> {
    let repo = Repository::open(repo_root)
        .with_context(|| format!("failed to open repository at {}", repo_root.display()))?;

    let mut entries = Vec::new();

    if let Some(main_workdir) = repo.workdir() {
        let branch = branch_for_repo(&repo)?;
        entries.push(WorktreeEntry {
            branch,
            path: main_workdir.canonicalize().with_context(|| {
                format!(
                    "failed to canonicalize workdir path {}",
                    main_workdir.display()
                )
            })?,
        });
    }

    let worktrees = repo.worktrees().context("failed to list worktrees")?;
    for name in worktrees.iter().flatten() {
        let wt = repo
            .find_worktree(name)
            .with_context(|| format!("failed to read worktree metadata for {name}"))?;
        let path = wt
            .path()
            .canonicalize()
            .with_context(|| format!("failed to canonicalize worktree path for {name}"))?;
        let wt_repo = Repository::open(wt.path())
            .with_context(|| format!("failed to open worktree repository for {name}"))?;
        let branch = branch_for_repo(&wt_repo)?;
        entries.push(WorktreeEntry { branch, path });
    }

    entries.sort_by(|left, right| left.branch.cmp(&right.branch));
    entries.dedup_by(|left, right| left.path == right.path);

    Ok(entries)
}

pub fn find_worktree_by_branch(repo_root: &Path, branch: &str) -> Result<Option<PathBuf>> {
    let entries = list_worktrees(repo_root)?;
    Ok(entries
        .into_iter()
        .find(|entry| entry.branch == branch)
        .map(|entry| entry.path))
}

/// Revalidate, immediately before removal, that the worktree at `path` is
/// still registered for `branch` and that its HEAD still points at `branch`.
///
/// `pre_remove` hooks and concurrent processes can switch the worktree to a
/// different branch, detach its HEAD, or remove it between resolution and
/// destruction. Every such change must abort the removal before any
/// destructive step so unique state is preserved.
pub fn ensure_worktree_attached_to_branch(
    repo_root: &Path,
    path: &Path,
    branch: &str,
) -> Result<()> {
    let current = find_worktree_by_branch(repo_root, branch)?;
    ensure!(
        current.as_deref() == Some(path),
        "worktree {} is no longer attached to branch {branch} (found {current:?}); \
         refusing to remove because a hook or concurrent action changed it",
        path.display()
    );

    let repo = Repository::open(path)
        .with_context(|| format!("failed to open worktree repository at {}", path.display()))?;
    let head = repo
        .head()
        .with_context(|| format!("failed to read HEAD of worktree at {}", path.display()))?;
    ensure!(
        head.is_branch() && head.shorthand() == Some(branch),
        "worktree {} HEAD no longer points at branch {branch}; \
         refusing to remove because a hook or concurrent action changed it",
        path.display()
    );

    Ok(())
}

pub fn worktree_stats(worktree_path: &Path) -> Result<WorktreeStats> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["status", "--porcelain=v2", "--branch"])
        .output()
        .with_context(|| {
            format!(
                "failed to read git status for worktree {}",
                worktree_path.display()
            )
        })?;

    ensure!(
        output.status.success(),
        "git status failed for worktree {}",
        worktree_path.display()
    );

    let text = String::from_utf8_lossy(&output.stdout);
    let mut ahead = 0;
    let mut behind = 0;
    let mut uncommitted = 0;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# branch.ab +") {
            let mut parts = rest.split(" -");
            if let Some(ahead_part) = parts.next() {
                ahead = ahead_part.trim().parse::<u32>().unwrap_or(0);
            }
            if let Some(behind_part) = parts.next() {
                behind = behind_part.trim().parse::<u32>().unwrap_or(0);
            }
            continue;
        }

        if !line.starts_with('#') && !line.trim().is_empty() {
            uncommitted += 1;
        }
    }

    Ok(WorktreeStats {
        ahead,
        behind,
        uncommitted,
    })
}

pub fn worktree_add(repo_root: &Path, branch: &str, path: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "add", "-b"])
        .arg(branch)
        .arg("--")
        .arg(path)
        .output()
        .with_context(|| {
            format!(
                "failed to execute git worktree add for branch {branch} at {}",
                path.display()
            )
        })?;

    ensure!(
        output.status.success(),
        "git worktree add failed for branch {branch}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

pub fn worktree_remove(repo_root: &Path, path: &Path, force: bool) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "remove"])
        .args(force.then_some("--force"))
        .arg("--")
        .arg(path)
        .output()
        .with_context(|| {
            format!(
                "failed to execute git worktree remove for {}",
                path.display()
            )
        })?;

    ensure!(
        output.status.success(),
        "git worktree remove failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

pub fn branch_delete(repo_root: &Path, branch: &str, force: bool) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["branch", flag])
        .arg("--")
        .arg(branch)
        .output()
        .with_context(|| format!("failed to execute git branch {flag} {branch}"))?;

    ensure!(
        output.status.success(),
        "git branch {flag} failed for {branch}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

pub fn resolve_worktree_path(template: &str, repo_root: &Path, branch: &str) -> Result<PathBuf> {
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            anyhow!(
                "failed to derive repository name from {}",
                repo_root.display()
            )
        })?;

    // Defense in depth: callers validate the branch name, but never render an
    // unvalidated name into a filesystem path.
    validate_branch_name(branch)?;

    let rendered = template
        .replace("{repo}", repo_name)
        .replace("{branch}", branch);

    let raw_path = PathBuf::from(rendered);
    let absolute = if raw_path.is_absolute() {
        raw_path
    } else {
        repo_root.join(raw_path)
    };
    let normalized = normalize_absolute_path(&absolute)?;
    let repo_canonical = repo_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize repository root {}",
            repo_root.display()
        )
    })?;
    let physical = canonicalize_deepest_existing(&normalized)?;

    ensure!(
        !physical.starts_with(&repo_canonical),
        "resolved worktree path {} is inside source repository {}",
        physical.display(),
        repo_canonical.display()
    );

    Ok(normalized)
}

/// Validate a user-supplied branch name without rewriting it.
///
/// Valid names are returned unchanged — dots such as `release/1.2.3` are
/// preserved and distinct names never collapse into each other. Names that
/// are not clearly safe are rejected with an explicit error rather than
/// silently reinterpreted: the charset is `[A-Za-z0-9/._-]`, no leading `-`
/// (option injection into git subprocesses), no leading `.` or `/`, no `..`
/// (path traversal), no `//`, no component starting with `.`, no trailing
/// `/`, `.`, or `.lock`, and a 60-character cap.
pub fn validate_branch_name(branch: &str) -> Result<&str> {
    ensure!(!branch.is_empty(), "branch name is empty");

    let invalid: String = branch
        .chars()
        .filter(|c| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.')))
        .collect();
    ensure!(
        invalid.is_empty(),
        "branch name '{branch}' contains characters not allowed in branch names: '{invalid}'"
    );

    ensure!(
        !branch.starts_with('-'),
        "branch name '{branch}' starts with '-' and could be parsed as a git option"
    );
    ensure!(
        !branch.starts_with('.') && !branch.starts_with('/'),
        "branch name '{branch}' must not start with '.' or '/'"
    );
    ensure!(
        !branch.contains(".."),
        "branch name '{branch}' must not contain '..'"
    );
    ensure!(
        !branch.contains("//"),
        "branch name '{branch}' must not contain '//'"
    );
    ensure!(
        !branch
            .split('/')
            .any(|component| component.starts_with('.')),
        "branch name '{branch}' must not contain a component starting with '.'"
    );
    ensure!(
        !branch.ends_with('/') && !branch.ends_with('.') && !branch.ends_with(".lock"),
        "branch name '{branch}' must not end with '/', '.', or '.lock'"
    );
    ensure!(
        branch.len() <= 60,
        "branch name '{branch}' is longer than 60 characters"
    );

    Ok(branch)
}

/// Resolve `path` to its physical form by canonicalizing the deepest
/// existing ancestor and re-appending the not-yet-existing tail.
///
/// Worktree target directories do not exist yet when the path is checked,
/// so a plain `canonicalize` fails; a purely lexical normalization misses
/// symlinked ancestors (for example `/var` -> `/private/var` on macOS or a
/// symlinked checkout). Comparing this physically resolved form against the
/// canonical repository root keeps the in-repo guard consistent across
/// platforms and symlink layouts.
fn canonicalize_deepest_existing(path: &Path) -> Result<PathBuf> {
    let mut pending: Vec<std::ffi::OsString> = Vec::new();
    let mut current = path.to_path_buf();

    loop {
        match current.canonicalize() {
            Ok(resolved) => {
                let mut physical = resolved;
                for component in pending.iter().rev() {
                    physical.push(component);
                }
                return Ok(physical);
            }
            Err(_) => match (current.parent(), current.file_name()) {
                (Some(parent), Some(name)) => {
                    pending.push(name.to_os_string());
                    current = parent.to_path_buf();
                }
                _ => {
                    return Err(anyhow!(
                        "failed to resolve physical path for {}",
                        path.display()
                    ));
                }
            },
        }
    }
}

fn branch_for_repo(repo: &Repository) -> Result<String> {
    let head = repo.head().context("failed to read repository HEAD")?;

    if head.is_branch() {
        let shorthand = head
            .shorthand()
            .ok_or_else(|| anyhow!("failed to derive branch shorthand"))?;
        Ok(shorthand.to_owned())
    } else {
        Ok("HEAD".to_owned())
    }
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    let mut base = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().context("failed to read current directory")?
    };

    for component in path.components() {
        match component {
            Component::RootDir => base.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !base.pop() {
                    return Err(anyhow!("invalid path traversal in {}", path.display()));
                }
            }
            Component::Normal(value) => base.push(value),
            Component::Prefix(prefix) => base.push(prefix.as_os_str()),
        }
    }

    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos();
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!(
                "lucus-git-{label}-{}-{stamp}-{n}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temp directory should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn isolate_git(cmd: &mut Command) {
        cmd.env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Lucus Test")
            .env("GIT_AUTHOR_EMAIL", "lucus-test@example.com")
            .env("GIT_COMMITTER_NAME", "Lucus Test")
            .env("GIT_COMMITTER_EMAIL", "lucus-test@example.com")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_TEMPLATE_DIR");
    }

    fn git(dir: &Path, args: &[&str]) -> std::process::Output {
        let mut cmd = Command::new("git");
        cmd.args(["-C"]).arg(dir).args(args);
        isolate_git(&mut cmd);
        let output = cmd.output().expect("git should run");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn init_repo(label: &str) -> TestDir {
        let dir = TestDir::new(label);
        init_repo_at(dir.path());
        dir
    }

    fn init_repo_at(dir: &Path) {
        let mut cmd = Command::new("git");
        cmd.args(["init", "-b", "main"]).arg(dir);
        isolate_git(&mut cmd);
        let output = cmd.output().expect("git init should run");
        assert!(
            output.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        git(dir, &["config", "user.name", "Lucus Test"]);
        git(dir, &["config", "user.email", "lucus-test@example.com"]);
        git(dir, &["config", "commit.gpgsign", "false"]);
        fs::write(dir.join("README.md"), "hello").expect("write readme");
        git(dir, &["add", "README.md"]);
        git(dir, &["commit", "-m", "init"]);
    }

    fn branch_exists(repo: &Path, branch: &str) -> bool {
        let mut cmd = Command::new("git");
        cmd.args(["-C"])
            .arg(repo)
            .args(["show-ref", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"));
        isolate_git(&mut cmd);
        cmd.status().expect("git show-ref should run").success()
    }

    #[test]
    fn validate_branch_name_accepts_safe_names_unchanged() {
        assert_eq!(
            validate_branch_name("feat/auth").expect("safe"),
            "feat/auth"
        );
        assert_eq!(
            validate_branch_name("feat-auth_1").expect("safe"),
            "feat-auth_1"
        );
        assert_eq!(
            validate_branch_name("a/b-c_d.e").expect("safe"),
            "a/b-c_d.e"
        );
    }

    #[test]
    fn validate_branch_name_rejects_invalid_characters_and_overlength() {
        let space = validate_branch_name("feat/foo bar").expect_err("space");
        assert!(
            space.to_string().contains("characters not allowed"),
            "unexpected error: {space:#}"
        );
        let unicode = validate_branch_name("feat/你好").expect_err("unicode");
        assert!(
            unicode.to_string().contains("characters not allowed"),
            "unexpected error: {unicode:#}"
        );
        let empty = validate_branch_name("").expect_err("empty");
        assert!(
            empty.to_string().contains("branch name is empty"),
            "unexpected error: {empty:#}"
        );
        let long = validate_branch_name(&"a".repeat(61)).expect_err("too long");
        assert!(
            long.to_string().contains("longer than 60"),
            "unexpected error: {long:#}"
        );
        assert_eq!(
            validate_branch_name(&"a".repeat(60)).expect("exactly 60"),
            "a".repeat(60)
        );
    }

    /// Distinct valid names must not collapse into each other through
    /// rewriting: validation is identity-preserving.
    #[test]
    fn validate_branch_name_never_rewrites_distinct_names() {
        let left = validate_branch_name("release/1.2.3").expect("dotted");
        let right = validate_branch_name("release/1.23").expect("dotted");
        assert_ne!(left, right);
        assert_eq!(left, "release/1.2.3");
        assert_eq!(right, "release/1.23");
    }

    #[test]
    fn validate_branch_name_rejects_traversal_and_component_shapes() {
        for name in [
            "../../../etc/passwd",
            "..",
            "a..b",
            "a//b",
            "/abs",
            ".hidden",
            "feat/.env",
            "feat/",
            "v1.0.",
            "feat/branch.lock",
        ] {
            assert!(
                validate_branch_name(name).is_err(),
                "'{name}' must be rejected"
            );
        }
        // sanity: the surrounding legitimate shapes stay valid
        validate_branch_name("v1.0.0").expect("plain dotted version is valid");
        validate_branch_name("release/1.2.3").expect("dotted component is valid");
    }

    /// Regression: valid dots in user-supplied names such as `release/1.2.3`
    /// are preserved; sanitization no longer silently rewrites them to
    /// `release/123`.
    #[test]
    fn branch_validation_preserves_dots_in_user_branch() {
        assert_eq!(
            validate_branch_name("release/1.2.3").expect("dotted name"),
            "release/1.2.3"
        );
        assert_eq!(
            validate_branch_name("v1.0.0").expect("dotted name"),
            "v1.0.0"
        );
    }

    /// Regression: names beginning with `-` are rejected explicitly instead
    /// of surviving validation and being passed to git as an option.
    #[test]
    fn branch_validation_rejects_leading_dashes() {
        let err = validate_branch_name("--force").expect_err("option-like name");
        assert!(
            err.to_string().contains("could be parsed as a git option"),
            "unexpected error: {err:#}"
        );
        let single = validate_branch_name("-n").expect_err("single dash");
        assert!(
            single
                .to_string()
                .contains("could be parsed as a git option"),
            "unexpected error: {single:#}"
        );
    }

    #[test]
    fn branch_delete_removes_dotted_branch_after_dashdash_separator() {
        let repo = init_repo("branch-dotted");
        let wt = repo.path().parent().unwrap().join(format!(
            "lucus-wt-dotted-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        worktree_add(repo.path(), "release/1.2.3", &wt).expect("worktree add dotted");
        assert!(branch_exists(repo.path(), "release/1.2.3"));
        worktree_remove(repo.path(), &wt, false).expect("worktree remove dotted");
        branch_delete(repo.path(), "release/1.2.3", false).expect("branch delete dotted");
        assert!(!branch_exists(repo.path(), "release/1.2.3"));
    }

    #[test]
    fn ensure_worktree_attached_accepts_matching_worktree() {
        let repo = init_repo("ensure-ok");
        let wt = repo.path().parent().unwrap().join(format!(
            "lucus-wt-ensure-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        worktree_add(repo.path(), "feat-auth", &wt).expect("worktree add");
        let path = find_worktree_by_branch(repo.path(), "feat-auth")
            .expect("find")
            .expect("path");
        ensure_worktree_attached_to_branch(repo.path(), &path, "feat-auth")
            .expect("attached worktree should pass revalidation");
        worktree_remove(repo.path(), &wt, false).expect("cleanup");
        branch_delete(repo.path(), "feat-auth", false).expect("cleanup branch");
    }

    #[test]
    fn ensure_worktree_attached_rejects_detached_head() {
        let repo = init_repo("ensure-detached");
        let wt = repo.path().parent().unwrap().join(format!(
            "lucus-wt-detached-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        worktree_add(repo.path(), "feat-detached", &wt).expect("worktree add");
        let path = find_worktree_by_branch(repo.path(), "feat-detached")
            .expect("find")
            .expect("path");
        git(&wt, &["checkout", "--detach"]);

        let err = ensure_worktree_attached_to_branch(repo.path(), &path, "feat-detached")
            .expect_err("detached worktree must be refused");
        assert!(
            err.to_string().contains("no longer attached"),
            "unexpected error: {err:#}"
        );
        // Nothing is destroyed on refusal.
        assert!(wt.exists());
        assert!(branch_exists(repo.path(), "feat-detached"));
        worktree_remove(repo.path(), &wt, true).expect("cleanup");
        branch_delete(repo.path(), "feat-detached", true).expect("cleanup branch");
    }

    #[test]
    fn ensure_worktree_attached_rejects_switched_branch() {
        let repo = init_repo("ensure-switched");
        git(repo.path(), &["branch", "other"]);
        let wt = repo.path().parent().unwrap().join(format!(
            "lucus-wt-switched-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        worktree_add(repo.path(), "feat-switch", &wt).expect("worktree add");
        let path = find_worktree_by_branch(repo.path(), "feat-switch")
            .expect("find")
            .expect("path");
        git(&wt, &["checkout", "other"]);

        let err = ensure_worktree_attached_to_branch(repo.path(), &path, "feat-switch")
            .expect_err("switched worktree must be refused");
        assert!(
            err.to_string().contains("no longer attached"),
            "unexpected error: {err:#}"
        );
        assert!(wt.exists());
        assert!(branch_exists(repo.path(), "feat-switch"));
        worktree_remove(repo.path(), &wt, true).expect("cleanup");
        branch_delete(repo.path(), "feat-switch", true).expect("cleanup branch");
    }

    #[test]
    fn resolve_worktree_path_renders_default_template_outside_repo() {
        let repo = init_repo("resolve-default");
        let path = resolve_worktree_path("../{repo}.{branch}", repo.path(), "feat-auth")
            .expect("default template should resolve");
        let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
        assert_eq!(
            path,
            repo.path()
                .parent()
                .unwrap()
                .join(format!("{repo_name}.feat-auth"))
        );
        assert!(!path.starts_with(repo.path()));
    }

    #[test]
    fn resolve_worktree_path_rejects_invalid_branch_names() {
        let repo = init_repo("resolve-invalid");
        let err = resolve_worktree_path("../{repo}.{branch}", repo.path(), "!!!")
            .expect_err("invalid branch should fail");
        assert!(
            err.to_string().contains("characters not allowed"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn resolve_worktree_path_accepts_absolute_template_outside_repo() {
        let repo = init_repo("resolve-abs");
        let outside = TestDir::new("abs-target-root");
        let template = format!("{}/{{branch}}", outside.path().display());
        let path = resolve_worktree_path(&template, repo.path(), "feat-auth")
            .expect("absolute outside path should resolve");
        assert_eq!(path, outside.path().join("feat-auth"));
    }

    /// Regression: a resolved path that is physically inside the source
    /// repository is rejected, including when the compared forms differ only
    /// through symlinked prefixes (macOS `/var` -> `/private/var`) because
    /// fixtures and checkouts live under symlinked temporary directories.
    #[test]
    fn in_repo_worktree_path_is_rejected_through_symlinked_temp_prefix() {
        let repo = init_repo("resolve-inside");
        let err = resolve_worktree_path(".worktrees/{branch}", repo.path(), "feat-auth")
            .expect_err("in-repo template must be rejected");
        assert!(
            err.to_string().contains("inside source repository"),
            "unexpected error: {err:#}"
        );
    }

    /// Regression: the guard compares physically resolved paths, so an
    /// in-repo template is rejected even when the repository is reached
    /// through a symlinked directory. Synthetic symlink — runs on Linux.
    #[test]
    fn in_repo_worktree_path_through_symlinked_repo_dir_is_rejected() {
        let workspace = TestDir::new("resolve-symlink-repo");
        let real_repo = workspace.path().join("repo-real");
        fs::create_dir_all(&real_repo).expect("real repo dir");
        init_repo_at(&real_repo);
        let alias = workspace.path().join("alias");
        std::os::unix::fs::symlink(&real_repo, &alias).expect("alias symlink");

        let err = resolve_worktree_path(".worktrees/{branch}", &alias, "feat-auth")
            .expect_err("in-repo template through symlink must be rejected");
        assert!(
            err.to_string().contains("inside source repository"),
            "unexpected error: {err:#}"
        );
    }

    /// The outside-repository default is unchanged: resolving through a
    /// symlinked repo directory with the default sibling template still
    /// yields an outside path.
    #[test]
    fn symlinked_repo_dir_default_template_still_resolves_outside() {
        let workspace = TestDir::new("resolve-symlink-outside");
        let real_repo = workspace.path().join("repo-real");
        fs::create_dir_all(&real_repo).expect("real repo dir");
        init_repo_at(&real_repo);
        let alias = workspace.path().join("alias");
        std::os::unix::fs::symlink(&real_repo, &alias).expect("alias symlink");

        let path = resolve_worktree_path("../{repo}.{branch}", &alias, "feat-auth")
            .expect("default template through symlink should resolve outside");
        assert!(
            path.starts_with(workspace.path()),
            "expected sibling of the real repo, got {}",
            path.display()
        );
        assert!(!path.starts_with(&real_repo));
    }

    #[test]
    fn list_worktrees_includes_main_checkout() {
        let repo = init_repo("list-main");
        let entries = list_worktrees(repo.path()).expect("list should succeed");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].branch, "main");
        assert_eq!(
            entries[0].path,
            repo.path().canonicalize().expect("canonicalize repo")
        );
    }

    #[test]
    fn current_branch_reads_head() {
        let repo = init_repo("current");
        assert_eq!(current_branch(repo.path()).expect("branch"), "main");
    }

    #[test]
    fn find_worktree_by_branch_none_for_unknown() {
        let repo = init_repo("find-none");
        assert_eq!(
            find_worktree_by_branch(repo.path(), "no-such").expect("find"),
            None
        );
    }

    #[test]
    fn worktree_add_and_stats_and_remove() {
        let repo = init_repo("wt-lifecycle");
        let wt = repo.path().parent().unwrap().join(format!(
            "lucus-wt-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        worktree_add(repo.path(), "feat-auth", &wt).expect("worktree add");
        let found = find_worktree_by_branch(repo.path(), "feat-auth")
            .expect("find")
            .expect("path");
        assert_eq!(found, wt.canonicalize().expect("canonicalize worktree"));

        let clean = worktree_stats(&wt).expect("stats");
        assert_eq!(clean.uncommitted, 0);

        fs::write(wt.join("dirty.txt"), "x").expect("dirty file");
        let dirty = worktree_stats(&wt).expect("dirty stats");
        assert_eq!(dirty.uncommitted, 1);

        worktree_remove(repo.path(), &wt, true).expect("force remove dirty worktree");
        branch_delete(repo.path(), "feat-auth", false).expect("delete branch");
        assert!(!wt.exists());
        assert!(!branch_exists(repo.path(), "feat-auth"));
    }

    #[test]
    fn worktree_remove_without_force_fails_on_dirty_tree() {
        let repo = init_repo("wt-dirty");
        let wt = repo.path().parent().unwrap().join(format!(
            "lucus-wt-dirty-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        worktree_add(repo.path(), "feat-dirty", &wt).expect("worktree add");
        fs::write(wt.join("dirty.txt"), "x").expect("dirty file");
        let err = worktree_remove(repo.path(), &wt, false).expect_err("dirty remove should fail");
        assert!(
            err.to_string().contains("git worktree remove failed"),
            "unexpected error: {err:#}"
        );
        assert!(wt.exists());
        worktree_remove(repo.path(), &wt, true).expect("cleanup");
        branch_delete(repo.path(), "feat-dirty", false).expect("cleanup branch");
    }

    #[test]
    fn previous_branch_fails_without_checkout_history() {
        let repo = init_repo("prev-none");
        let err = previous_branch(repo.path()).expect_err("no previous branch");
        assert!(
            err.to_string()
                .contains("unable to resolve previous branch"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn worktree_add_fails_when_branch_already_exists() {
        let repo = init_repo("wt-dup");
        let wt = repo.path().parent().unwrap().join(format!(
            "lucus-wt-dup-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        worktree_add(repo.path(), "feat-dup", &wt).expect("first add");
        let wt2 = repo.path().parent().unwrap().join(format!(
            "lucus-wt-dup2-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let err = worktree_add(repo.path(), "feat-dup", &wt2).expect_err("duplicate branch");
        assert!(
            err.to_string().contains("git worktree add failed"),
            "unexpected error: {err:#}"
        );
        worktree_remove(repo.path(), &wt, false).expect("cleanup");
        branch_delete(repo.path(), "feat-dup", false).expect("cleanup branch");
    }

    /// Ordinary branch deletion uses `git branch -d` and refuses unique
    /// commits that are not merged; only `--force` uses `git branch -D`.
    #[test]
    fn branch_delete_without_force_refuses_unmerged_branch() {
        let repo = init_repo("force-delete");
        git(repo.path(), &["checkout", "-b", "unique"]);
        fs::write(repo.path().join("unique.txt"), "only-on-unique").expect("write unique");
        git(repo.path(), &["add", "unique.txt"]);
        git(repo.path(), &["commit", "-m", "unique commit"]);
        let sha = String::from_utf8_lossy(&git(repo.path(), &["rev-parse", "HEAD"]).stdout)
            .trim()
            .to_owned();
        git(repo.path(), &["checkout", "main"]);

        let err = branch_delete(repo.path(), "unique", false)
            .expect_err("unmerged branch delete should fail");
        assert!(
            err.to_string().contains("git branch -d failed"),
            "unexpected error: {err:#}"
        );
        assert!(branch_exists(repo.path(), "unique"));

        let mut cat_cmd = Command::new("git");
        cat_cmd
            .args(["-C"])
            .arg(repo.path())
            .args(["cat-file", "-t", &sha]);
        isolate_git(&mut cat_cmd);
        let cat = cat_cmd.output().expect("cat-file");
        assert!(
            cat.status.success(),
            "unique commit object should still exist until gc"
        );

        branch_delete(repo.path(), "unique", true).expect("force delete unmerged branch");
        assert!(!branch_exists(repo.path(), "unique"));
    }
}
