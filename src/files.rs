use anyhow::{Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn copy_env_files(repo_root: &Path, target_path: &Path, patterns: &[String]) -> Result<()> {
    let sources = env_sources(repo_root, patterns)?;
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

fn env_sources(repo_root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
    if patterns.is_empty() {
        let mut discovered = Vec::new();
        for entry in fs::read_dir(repo_root)
            .with_context(|| format!("failed to read directory {}", repo_root.display()))?
        {
            let entry =
                entry.with_context(|| format!("failed to inspect {}", repo_root.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to read type for {}", entry.path().display()))?;
            if !file_type.is_file() {
                continue;
            }

            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with(".env") {
                discovered.push(entry.path());
            }
        }

        discovered.sort();
        return Ok(discovered);
    }

    Ok(patterns
        .iter()
        .map(|pattern| repo_root.join(pattern))
        .collect())
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
    fn copy_env_files_auto_discovers_root_env_files() {
        let root = TestDir::new("repo");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");

        fs::write(root.path().join(".env"), "ROOT=1").expect("should write .env");
        fs::write(root.path().join(".env.local"), "LOCAL=1").expect("should write .env.local");
        fs::write(root.path().join("README.md"), "ignore").expect("should write readme");

        copy_env_files(root.path(), &target, &[]).expect("copy should succeed");

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
            "non-env files should not be copied"
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
    fn copy_env_files_creates_missing_target_dir() {
        let root = TestDir::new("repo-mkdir");
        let target = root.path().join("missing").join("worktree");
        copy_env_files(root.path(), &target, &[]).expect("copy should succeed");
        assert!(target.is_dir());
    }

    #[test]
    fn copy_env_files_skips_directories_named_env() {
        let root = TestDir::new("repo-env-dir");
        let target = root.path().join("worktree");
        fs::create_dir_all(root.path().join(".env")).expect("dir .env should exist");
        copy_env_files(root.path(), &target, &[".env".to_owned()]).expect("copy should succeed");
        assert!(
            !target.join(".env").exists(),
            "directory sources should be skipped"
        );
    }

    #[test]
    fn copy_env_files_empty_repo_copies_nothing() {
        let root = TestDir::new("repo-empty");
        let target = root.path().join("worktree");
        copy_env_files(root.path(), &target, &[]).expect("copy should succeed");
        let entries: Vec<_> = fs::read_dir(&target)
            .expect("target should exist")
            .collect();
        assert!(entries.is_empty(), "no env files should be copied");
    }

    #[test]
    fn copy_env_files_unicode_env_suffix_is_copied() {
        let root = TestDir::new("repo-unicode");
        let target = root.path().join("worktree");
        fs::write(root.path().join(".env.ünicode"), "OK=1").expect("should write unicode env");
        copy_env_files(root.path(), &target, &[]).expect("copy should succeed");
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

    /// Expected: with an empty `files.copy` list, creation copies no files and
    /// never auto-discovers `.env*` (including `.envrc`), so credentials stay
    /// in the source checkout.
    /// Observed: empty patterns scan the repo root and copy every filename
    /// that starts with `.env`, including `.envrc`.
    #[test]
    fn documents_defect_copy_env_files_auto_discovers_when_unconfigured() {
        let root = TestDir::new("repo-autodiscover-defect");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        fs::write(root.path().join(".env"), "FAKECREDENTIAL=1").expect("should write .env");
        fs::write(root.path().join(".envrc"), "FAKECREDENTIAL=2").expect("should write .envrc");

        copy_env_files(root.path(), &target, &[]).expect("copy should succeed");

        assert_eq!(
            fs::read_to_string(target.join(".env")).expect("copied .env should exist"),
            "FAKECREDENTIAL=1"
        );
        assert_eq!(
            fs::read_to_string(target.join(".envrc")).expect("copied .envrc should exist"),
            "FAKECREDENTIAL=2"
        );
    }

    /// Expected: `files.copy` entries are resolved relative to the repo root
    /// and rejected if they escape it.
    /// Observed: an absolute pattern replaces the repo root via `Path::join`
    /// and the file is copied into the worktree.
    #[test]
    fn documents_defect_copy_env_files_absolute_pattern_escapes_repo() {
        let root = TestDir::new("repo-abs-pattern");
        let outside = TestDir::new("outside-abs-pattern");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        let leaked = outside.path().join("leaked.env");
        fs::write(&leaked, "FAKECREDENTIAL=1").expect("should write outside file");

        copy_env_files(
            root.path(),
            &target,
            &[leaked.to_string_lossy().into_owned()],
        )
        .expect("copy currently succeeds");

        assert_eq!(
            fs::read_to_string(target.join("leaked.env")).expect("escaped file should be copied"),
            "FAKECREDENTIAL=1"
        );
    }

    /// Expected: `../` components in a copy pattern cannot reach files outside
    /// the repository.
    /// Observed: `repo_root.join("../secret.env")` is copied unchanged.
    #[test]
    fn documents_defect_copy_env_files_parent_pattern_escapes_repo() {
        let workspace = TestDir::new("workspace-parent-pattern");
        let repo = workspace.path().join("repo");
        fs::create_dir_all(&repo).expect("repo should exist");
        let target = repo.join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        fs::write(workspace.path().join("secret.env"), "FAKECREDENTIAL=1")
            .expect("should write sibling secret");

        copy_env_files(&repo, &target, &["../secret.env".to_owned()])
            .expect("copy currently succeeds");

        assert_eq!(
            fs::read_to_string(target.join("secret.env")).expect("escaped file should be copied"),
            "FAKECREDENTIAL=1"
        );
    }

    /// Expected: copy sources that are symlinks are refused, especially when
    /// they point outside the repository.
    /// Observed: pattern mode uses `Path::is_file()`, which follows the
    /// symlink and copies the target's contents.
    #[test]
    fn documents_defect_copy_env_files_follows_symlink_in_pattern_mode() {
        let root = TestDir::new("repo-symlink");
        let outside = TestDir::new("outside-symlink");
        let target = root.path().join("worktree");
        fs::create_dir_all(&target).expect("worktree dir should exist");
        let leaked = outside.path().join("leaked.env");
        fs::write(&leaked, "FAKECREDENTIAL=1").expect("should write outside file");
        std::os::unix::fs::symlink(&leaked, root.path().join(".env"))
            .expect("symlink should exist");

        copy_env_files(root.path(), &target, &[".env".to_owned()])
            .expect("copy currently succeeds");

        assert_eq!(
            fs::read_to_string(target.join(".env")).expect("symlink target should be copied"),
            "FAKECREDENTIAL=1"
        );
    }
}
