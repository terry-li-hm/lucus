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

pub fn branch_delete(repo_root: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["branch", "-D", branch])
        .output()
        .with_context(|| format!("failed to execute git branch -D {branch}"))?;

    ensure!(
        output.status.success(),
        "git branch -D failed for {branch}: {}",
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

    let safe_branch = sanitize_branch(branch);
    ensure!(
        !safe_branch.is_empty(),
        "branch name is empty after sanitization"
    );

    let rendered = template
        .replace("{repo}", repo_name)
        .replace("{branch}", &safe_branch);

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

    ensure!(
        !normalized.starts_with(&repo_canonical),
        "resolved worktree path {} is inside source repository {}",
        normalized.display(),
        repo_canonical.display()
    );

    Ok(normalized)
}

pub fn sanitize_branch(branch: &str) -> String {
    branch
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '/' || *c == '-' || *c == '_')
        .take(60)
        .collect::<String>()
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
        let mut cmd = Command::new("git");
        cmd.args(["init", "-b", "main"]).arg(dir.path());
        isolate_git(&mut cmd);
        let output = cmd.output().expect("git init should run");
        assert!(
            output.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        git(dir.path(), &["config", "user.name", "Lucus Test"]);
        git(
            dir.path(),
            &["config", "user.email", "lucus-test@example.com"],
        );
        git(dir.path(), &["config", "commit.gpgsign", "false"]);
        fs::write(dir.path().join("README.md"), "hello").expect("write readme");
        git(dir.path(), &["add", "README.md"]);
        git(dir.path(), &["commit", "-m", "init"]);
        dir
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
    fn sanitize_branch_keeps_safe_characters() {
        assert_eq!(sanitize_branch("feat/auth"), "feat/auth");
        assert_eq!(sanitize_branch("feat-auth_1"), "feat-auth_1");
        assert_eq!(sanitize_branch(""), "");
    }

    #[test]
    fn sanitize_branch_strips_spaces_and_unicode_and_caps_length() {
        assert_eq!(sanitize_branch("feat/foo bar"), "feat/foobar");
        assert_eq!(sanitize_branch("feat/你好"), "feat/");
        assert_eq!(sanitize_branch("!!!"), "");
        let long = "a".repeat(80);
        assert_eq!(sanitize_branch(&long).len(), 60);
    }

    #[test]
    fn sanitize_branch_strips_dots_from_path_traversal_attempts() {
        assert_eq!(sanitize_branch("../../../etc/passwd"), "///etc/passwd");
    }

    /// Expected: a user-supplied branch such as `release/1.2.3` is created
    /// unchanged; sanitization is only for path substitution / generated names.
    /// Observed: `.` is stripped, so `release/1.2.3` becomes `release/123`.
    #[test]
    fn documents_defect_sanitize_strips_dots_from_user_branch() {
        assert_eq!(sanitize_branch("release/1.2.3"), "release/123");
        assert_eq!(sanitize_branch("v1.0.0"), "v100");
    }

    /// Expected: leading dashes are stripped or the git subprocess is given
    /// `--` so the branch cannot be parsed as a flag.
    /// Observed: `--force` survives sanitization and would be passed as `-b
    /// --force`.
    #[test]
    fn documents_defect_sanitize_keeps_leading_dashes() {
        assert_eq!(sanitize_branch("--force"), "--force");
        assert_eq!(sanitize_branch("-n"), "-n");
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
    fn resolve_worktree_path_rejects_empty_sanitized_branch() {
        let repo = init_repo("resolve-empty");
        let err = resolve_worktree_path("../{repo}.{branch}", repo.path(), "!!!")
            .expect_err("empty sanitized branch should fail");
        assert!(
            err.to_string().contains("empty after sanitization"),
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

    /// Expected: in-repo templates such as `.worktrees/{branch}` are allowed
    /// so `ensure_gitignore` can ignore the parent directory.
    /// Observed: any resolved path inside the source repository is rejected.
    #[test]
    fn documents_defect_in_repo_worktree_path_rejected() {
        let repo = init_repo("resolve-inside");
        let err = resolve_worktree_path(".worktrees/{branch}", repo.path(), "feat-auth")
            .expect_err("in-repo template currently fails");
        assert!(
            err.to_string().contains("inside source repository"),
            "unexpected error: {err:#}"
        );
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
        branch_delete(repo.path(), "feat-auth").expect("delete branch");
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
        branch_delete(repo.path(), "feat-dirty").expect("cleanup branch");
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
        branch_delete(repo.path(), "feat-dup").expect("cleanup branch");
    }

    /// Expected: ordinary branch deletion uses `git branch -d` and refuses
    /// unique commits that are not merged.
    /// Observed: `branch_delete` always runs `git branch -D`, so an unmerged
    /// branch is removed and its unique commits become unreachable.
    #[test]
    fn documents_defect_branch_delete_force_drops_unmerged_commits() {
        let repo = init_repo("force-delete");
        git(repo.path(), &["checkout", "-b", "unique"]);
        fs::write(repo.path().join("unique.txt"), "only-on-unique").expect("write unique");
        git(repo.path(), &["add", "unique.txt"]);
        git(repo.path(), &["commit", "-m", "unique commit"]);
        let sha = String::from_utf8_lossy(&git(repo.path(), &["rev-parse", "HEAD"]).stdout)
            .trim()
            .to_owned();
        git(repo.path(), &["checkout", "main"]);

        branch_delete(repo.path(), "unique").expect("force delete currently succeeds");
        assert!(!branch_exists(repo.path(), "unique"));

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
    }
}
