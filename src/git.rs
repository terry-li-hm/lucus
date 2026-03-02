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
