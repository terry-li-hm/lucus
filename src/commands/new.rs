use anyhow::{Result, ensure};

use crate::config::Config;
use crate::files;
use crate::git;
use crate::hooks::{self, HookContext};

pub fn run(config: &Config, branch: &str) -> Result<()> {
    let sanitized_branch = git::sanitize_branch(branch);
    ensure!(
        !sanitized_branch.is_empty(),
        "branch name is empty after sanitization"
    );

    let repo_root = git::repo_root_from_cwd()?;

    if git::find_worktree_by_branch(&repo_root, &sanitized_branch)?.is_some() {
        anyhow::bail!("worktree already exists for branch {sanitized_branch}");
    }

    let target_path = git::resolve_worktree_path(
        &config.worktree.path_template,
        &repo_root,
        &sanitized_branch,
    )?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    git::worktree_add(&repo_root, &sanitized_branch, &target_path)?;
    files::copy_env_files(&repo_root, &target_path, &config.files.copy)?;
    files::ensure_gitignore(&repo_root, &target_path)?;

    let hook_context = HookContext {
        branch: sanitized_branch,
        worktree_path: target_path.clone(),
        repo_root,
        agent: None,
        task: None,
    };

    hooks::run_blocking(&config.hooks.post_create, &hook_context)?;
    hooks::run_background(&config.hooks.post_create_bg, &hook_context)?;

    println!("{}", target_path.display());
    Ok(())
}
