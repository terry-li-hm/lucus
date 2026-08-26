use anyhow::Result;
use std::path::Path;

use crate::config::Config;
use crate::files;
use crate::git;
use crate::hooks::{self, HookContext};

pub fn run(config: &Config, branch: &str) -> Result<()> {
    let branch = git::validate_branch_name(branch)?;

    let repo_root = git::repo_root_from_cwd()?;

    if git::find_worktree_by_branch(&repo_root, branch)?.is_some() {
        anyhow::bail!("worktree already exists for branch {branch}");
    }

    let target_path =
        git::resolve_worktree_path(&config.worktree.path_template, &repo_root, branch)?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let base_sha = git::rev_parse(&repo_root, "HEAD")?;
    let copied_names: Vec<String> = config
        .files
        .copy
        .iter()
        .filter_map(|entry| {
            Path::new(entry)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .collect();

    git::worktree_add(&repo_root, branch, &target_path)?;

    if let Err(err) = finalize_creation(config, branch, &repo_root, &target_path) {
        return Err(handle_creation_failure(
            &repo_root,
            &target_path,
            branch,
            &base_sha,
            &copied_names,
            err,
        ));
    }

    println!("{}", target_path.display());
    Ok(())
}

/// Everything that runs after `git worktree add` and can fail while the
/// worktree and branch already exist.
fn finalize_creation(
    config: &Config,
    branch: &str,
    repo_root: &Path,
    target_path: &Path,
) -> Result<()> {
    files::copy_env_files(repo_root, target_path, &config.files.copy)?;
    files::ensure_gitignore(repo_root, target_path)?;

    let hook_context = HookContext {
        branch: branch.to_owned(),
        worktree_path: target_path.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        agent: None,
        task: None,
    };

    hooks::run_blocking(&config.hooks.post_create, &hook_context)?;
    hooks::run_background(&config.hooks.post_create_bg, &hook_context)?;
    Ok(())
}

/// On a creation failure, roll back only when deterministic checks prove the
/// just-created worktree and branch hold no unique state; otherwise preserve
/// everything and report exact recovery steps.
fn handle_creation_failure(
    repo_root: &Path,
    target_path: &Path,
    branch: &str,
    base_sha: &str,
    copied: &[String],
    err: anyhow::Error,
) -> anyhow::Error {
    let eligible =
        git::creation_is_rollback_eligible(repo_root, target_path, branch, base_sha, copied)
            .unwrap_or(false);

    if eligible {
        match git::rollback_clean_creation(repo_root, target_path, branch, copied) {
            Ok(()) => err.context(
                "creation failed; rolled back the freshly created clean worktree and branch",
            ),
            Err(rollback_err) => err.context(format!(
                "creation failed and rollback could not complete: {rollback_err:#}; \
                 worktree {} and branch {branch} are preserved — inspect the worktree, \
                 then recover with: lucus remove {branch}",
                target_path.display()
            )),
        }
    } else {
        err.context(format!(
            "creation failed; worktree {} and branch {branch} are preserved because they \
             may contain unique state — inspect the worktree, then recover with: \
             lucus remove {branch}",
            target_path.display()
        ))
    }
}
