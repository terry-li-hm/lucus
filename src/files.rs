use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn copy_env_files(repo_root: &Path, target_path: &Path, patterns: &[String]) -> Result<()> {
    let sources = env_sources(repo_root, patterns)?;
    if sources.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(target_path).with_context(|| {
        format!(
            "failed to create target directory {}",
            target_path.display()
        )
    })?;

    for source in sources {
        if !source.is_file() {
            continue;
        }

        let Some(file_name) = source.file_name() else {
            continue;
        };
        let destination = target_path.join(file_name);

        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to copy env file {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }

    Ok(())
}

pub fn ensure_gitignore(repo_root: &Path, target_path: &Path) -> Result<()> {
    let repo_root = repo_root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize repo root {}", repo_root.display()))?;
    let target_path = if target_path.exists() {
        target_path.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize target path {}",
                target_path.display()
            )
        })?
    } else {
        normalize_path(target_path, &repo_root)
    };

    if !target_path.starts_with(&repo_root) {
        return Ok(());
    }

    let Some(parent_dir) = target_path.parent() else {
        return Ok(());
    };
    let Ok(relative_parent) = parent_dir.strip_prefix(&repo_root) else {
        return Ok(());
    };
    if relative_parent.as_os_str().is_empty() {
        return Ok(());
    }

    let entry = format!("{}/", path_to_unix(relative_parent));
    let gitignore_path = repo_root.join(".gitignore");
    let existing = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)
            .with_context(|| format!("failed to read {}", gitignore_path.display()))?
    } else {
        String::new()
    };

    if has_gitignore_entry(&existing, &entry) {
        return Ok(());
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&entry);
    updated.push('\n');

    fs::write(&gitignore_path, updated)
        .with_context(|| format!("failed to write {}", gitignore_path.display()))?;

    Ok(())
}

/// Resolve `files.copy` entries to copyable sources.
///
/// The list is an explicit allowlist of regular files inside the repository.
/// An empty list copies nothing: there is no implicit `.env*` discovery.
/// Every entry must be repository-relative and is rejected when it is
/// absolute, contains `..`, names a directory, is a symlink, or resolves
/// (canonically, through any intermediate symlink) outside the repository.
fn env_sources(repo_root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }

    let repo_canonical = repo_root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize repo root {}", repo_root.display()))?;

    let mut sources = Vec::new();
    for pattern in patterns {
        let relative = Path::new(pattern);
        ensure!(
            !relative.is_absolute(),
            "files.copy entry '{pattern}' must be relative to the repository root"
        );
        ensure!(
            !relative
                .components()
                .any(|c| matches!(c, Component::ParentDir)),
            "files.copy entry '{pattern}' must not contain '..'"
        );

        let candidate = repo_root.join(relative);
        match fs::symlink_metadata(&candidate) {
            // Missing entries are skipped; the allowlist may list
            // machine-specific optional files.
            Err(_) => continue,
            Ok(metadata) => {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "files.copy entry '{pattern}' is a symlink; refusing to copy through symlinks"
                );
                ensure!(
                    !metadata.is_dir(),
                    "files.copy entry '{pattern}' is a directory; only regular files can be copied"
                );
            }
        }

        let canonical = candidate
            .canonicalize()
            .with_context(|| format!("failed to canonicalize files.copy entry '{pattern}'"))?;
        ensure!(
            canonical.starts_with(&repo_canonical),
            "files.copy entry '{pattern}' resolves outside the repository"
        );

        sources.push(candidate);
    }
    Ok(sources)
}

fn normalize_path(path: &Path, base: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };

    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
        }
    }

    normalized
}

fn path_to_unix(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn has_gitignore_entry(contents: &str, entry: &str) -> bool {
    let wanted = normalize_gitignore_entry(entry);
    contents
        .lines()
        .map(normalize_gitignore_entry)
        .any(|line| line == wanted)
}

fn normalize_gitignore_entry(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{copy_env_files, ensure_gitignore};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

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
            path.push(format!("lucus-{label}-{}-{stamp}", process::id()));
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

    #[test]
    fn copy_env_files_copies_only_explicitly_listed_files() {
        let root = TestDir::new("repo");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");

        fs::write(root.path().join(".env"), "ROOT=1").expect("should write .env");
        fs::write(root.path().join(".env.local"), "LOCAL=1").expect("should write .env.local");
        fs::write(root.path().join("README.md"), "ignore").expect("should write readme");

        copy_env_files(
            root.path(),
            &target,
            &[".env".to_owned(), ".env.local".to_owned()],
        )
        .expect("copy should succeed");

        assert_eq!(
            fs::read_to_string(target.join(".env")).expect("copied .env should exist"),
            "ROOT=1"
        );
        assert_eq!(
            fs::read_to_string(target.join(".env.local")).expect("copied .env.local should exist"),
            "LOCAL=1"
        );
        assert!(
            !target.join("README.md").exists(),
            "files outside the allowlist should not be copied"
        );
    }

    #[test]
    fn copy_env_files_pattern_mode_skips_missing_files() {
        let root = TestDir::new("repo-pattern");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        fs::write(root.path().join(".env"), "ROOT=1").expect("should write .env");

        let patterns = vec![".env".to_owned(), ".env.missing".to_owned()];
        copy_env_files(root.path(), &target, &patterns).expect("copy should succeed");

        assert!(
            target.join(".env").exists(),
            "configured env file should copy"
        );
        assert!(
            !target.join(".env.missing").exists(),
            "missing files should be skipped silently"
        );
    }

    #[test]
    fn ensure_gitignore_adds_parent_entry_once() {
        let root = TestDir::new("repo-gitignore");
        let target = root.path().join("worktrees").join("feat-abc");
        fs::create_dir_all(&target).expect("target path should be created");

        ensure_gitignore(root.path(), &target).expect("first update should succeed");
        ensure_gitignore(root.path(), &target).expect("second update should be idempotent");

        let gitignore =
            fs::read_to_string(root.path().join(".gitignore")).expect("gitignore should exist");
        assert_eq!(
            gitignore
                .lines()
                .filter(|line| *line == "worktrees/")
                .count(),
            1,
            "worktree parent should be added only once"
        );
    }

    #[test]
    fn ensure_gitignore_noop_when_target_outside_repo() {
        let root = TestDir::new("repo-outside");
        let outside = TestDir::new("outside");
        let target = outside.path().join("worktrees").join("feat-xyz");
        fs::create_dir_all(&target).expect("outside target should be created");

        ensure_gitignore(root.path(), &target).expect("call should succeed");

        assert!(
            !root.path().join(".gitignore").exists(),
            "gitignore should not be created for outside targets"
        );
    }

    #[test]
    fn copy_env_files_creates_missing_target_dir_for_explicit_entries() {
        let root = TestDir::new("repo-mkdir");
        fs::write(root.path().join(".env"), "ROOT=1").expect("should write .env");
        let target = root.path().join("missing").join("worktree");
        copy_env_files(root.path(), &target, &[".env".to_owned()]).expect("copy should succeed");
        assert!(target.is_dir());
        assert!(target.join(".env").exists());
    }

    #[test]
    fn copy_env_files_rejects_directory_entry() {
        let root = TestDir::new("repo-env-dir");
        let target = root.path().join("worktree");
        fs::create_dir_all(root.path().join("conf")).expect("directory entry should exist");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        let err = copy_env_files(root.path(), &target, &["conf".to_owned()])
            .expect_err("directory entries should be rejected");
        assert!(
            err.to_string().contains("is a directory"),
            "unexpected error: {err:#}"
        );
        assert!(
            fs::read_dir(&target)
                .expect("target readable")
                .next()
                .is_none(),
            "rejected entries must not be copied"
        );
    }

    #[test]
    fn copy_env_files_empty_repo_copies_nothing() {
        let root = TestDir::new("repo-empty");
        let target = root.path().join("worktree");
        copy_env_files(root.path(), &target, &[]).expect("copy should succeed");
        assert!(
            !target.exists(),
            "empty allowlist should have no filesystem side effects"
        );
    }

    #[test]
    fn copy_env_files_unicode_entry_is_copied() {
        let root = TestDir::new("repo-unicode");
        let target = root.path().join("worktree");
        fs::write(root.path().join(".env.ünicode"), "OK=1").expect("should write unicode env");
        copy_env_files(root.path(), &target, &[".env.ünicode".to_owned()])
            .expect("copy should succeed");
        assert_eq!(
            fs::read_to_string(target.join(".env.ünicode")).expect("copied file should exist"),
            "OK=1"
        );
    }

    #[test]
    fn ensure_gitignore_appends_after_file_without_trailing_newline() {
        let root = TestDir::new("repo-gitignore-nl");
        let target = root.path().join("worktrees").join("feat-abc");
        fs::create_dir_all(&target).expect("target path should be created");
        fs::write(root.path().join(".gitignore"), "target").expect("should write gitignore");

        ensure_gitignore(root.path(), &target).expect("update should succeed");

        let gitignore =
            fs::read_to_string(root.path().join(".gitignore")).expect("gitignore should exist");
        assert_eq!(gitignore, "target\nworktrees/\n");
    }

    /// Regression: with an empty `files.copy` list, creation copies no files
    /// and never auto-discovers `.env*` (including `.envrc`), so credentials
    /// stay in the source checkout unless explicitly configured.
    #[test]
    fn empty_copy_list_copies_nothing_and_never_discovers_env_files() {
        let root = TestDir::new("repo-autodiscover-regression");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        fs::write(root.path().join(".env"), "FAKECREDENTIAL=1").expect("should write .env");
        fs::write(root.path().join(".envrc"), "FAKECREDENTIAL=2").expect("should write .envrc");

        copy_env_files(root.path(), &target, &[]).expect("copy should succeed");

        assert!(
            !target.join(".env").exists(),
            ".env must not be copied implicitly"
        );
        assert!(
            !target.join(".envrc").exists(),
            ".envrc must not be copied implicitly"
        );
        assert!(fs::read_dir(&target).unwrap().next().is_none());
    }

    /// Regression: `files.copy` entries are repository-relative only;
    /// absolute paths are rejected instead of replacing the repo root.
    #[test]
    fn absolute_copy_pattern_is_rejected() {
        let root = TestDir::new("repo-abs-pattern");
        let outside = TestDir::new("outside-abs-pattern");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        let leaked = outside.path().join("leaked.env");
        fs::write(&leaked, "FAKECREDENTIAL=1").expect("should write outside file");

        let err = copy_env_files(
            root.path(),
            &target,
            &[leaked.to_string_lossy().into_owned()],
        )
        .expect_err("absolute entries should be rejected");

        assert!(
            err.to_string().contains("relative to the repository root"),
            "unexpected error: {err:#}"
        );
        assert!(!target.join("leaked.env").exists());
    }

    /// Regression: `../` components in a copy pattern are rejected, so they
    /// cannot reach files outside the repository.
    #[test]
    fn parent_traversal_copy_pattern_is_rejected() {
        let workspace = TestDir::new("workspace-parent-pattern");
        let repo = workspace.path().join("repo");
        fs::create_dir_all(&repo).expect("repo should exist");
        let target = repo.join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        fs::write(workspace.path().join("secret.env"), "FAKECREDENTIAL=1")
            .expect("should write sibling secret");

        let err = copy_env_files(&repo, &target, &["../secret.env".to_owned()])
            .expect_err("parent traversal should be rejected");

        assert!(
            err.to_string().contains("must not contain '..'"),
            "unexpected error: {err:#}"
        );
        assert!(!target.join("secret.env").exists());
    }

    /// Regression: copy sources that are symlinks are refused, including
    /// symlinks pointing outside the repository.
    #[test]
    fn symlink_copy_pattern_is_rejected() {
        let root = TestDir::new("repo-symlink");
        let outside = TestDir::new("outside-symlink");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        let leaked = outside.path().join("leaked.env");
        fs::write(&leaked, "FAKECREDENTIAL=1").expect("should write outside file");
        std::os::unix::fs::symlink(&leaked, root.path().join(".env"))
            .expect("symlink should exist");

        let err = copy_env_files(root.path(), &target, &[".env".to_owned()])
            .expect_err("symlink entries should be rejected");

        assert!(
            err.to_string().contains("is a symlink"),
            "unexpected error: {err:#}"
        );
        assert!(!target.join(".env").exists());
    }

    /// Regression: an entry is rejected when its canonical path escapes the
    /// repository through an intermediate symlinked directory, even though
    /// the final component is a regular file.
    #[test]
    fn copy_pattern_escaping_repo_through_intermediate_symlink_is_rejected() {
        let root = TestDir::new("repo-inter-symlink");
        let outside = TestDir::new("outside-inter-symlink");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        let leaked = outside.path().join("leaked.env");
        fs::write(&leaked, "FAKECREDENTIAL=1").expect("should write outside file");
        std::os::unix::fs::symlink(outside.path(), root.path().join("link"))
            .expect("symlinked directory should exist");

        let err = copy_env_files(root.path(), &target, &["link/leaked.env".to_owned()])
            .expect_err("escaping entries should be rejected");

        assert!(
            err.to_string().contains("resolves outside the repository"),
            "unexpected error: {err:#}"
        );
        assert!(!target.join("leaked.env").exists());
    }
}
