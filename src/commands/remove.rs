use anyhow::{Result, anyhow};

use crate::config::Config;
use crate::git;
use crate::hooks::{self, HookContext};

pub fn run(config: &Config, branch: &str) -> Result<()> {
    let repo_root = git::repo_root_from_cwd()?;
    let path = git::find_worktree_by_branch(&repo_root, branch)?
        .ok_or_else(|| anyhow!("no worktree found for branch {branch}"))?;

    let hook_context = HookContext {
        branch: branch.to_owned(),
        worktree_path: path.clone(),
        repo_root: repo_root.clone(),
        agent: None,
        task: None,
    };

    hooks::run_blocking(&config.hooks.pre_remove, &hook_context)?;
    git::worktree_remove(&repo_root, &path)?;
    git::branch_delete(&repo_root, branch)?;
    hooks::run_blocking(&config.hooks.post_remove, &hook_context)?;

    eprintln!(
        "removed worktree {} and deleted branch {}",
        path.display(),
        branch
    );
    Ok(())
}
