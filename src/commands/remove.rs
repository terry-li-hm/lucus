use anyhow::{Result, anyhow};

use crate::config::Config;
use crate::git;
use crate::hooks::{self, HookContext};

pub fn run(config: &Config, branch: &str, force: bool) -> Result<()> {
    let repo_root = git::repo_root_from_cwd()?;
    let path = match git::find_worktree_by_branch(&repo_root, branch)? {
        Some(path) => path,
        None => {
            // Recovery: the working directory may have been lost out of band.
            // Delete the branch first with the same merged-state protection as
            // the normal path, and prune the stale registration only after that
            // succeeds so a refused removal stays retryable. Hooks are skipped:
            // there is no worktree left for them to act on.
            let stale = git::stale_worktrees(&repo_root)?;
            if let Some(matched) = stale
                .iter()
                .find(|stale| stale.branch.as_deref() == Some(branch))
            {
                // git refuses to delete a branch still used by a registered
                // worktree, so prune the (already dead) registration first.
                // The branch delete below keeps its merged-state protection.
                git::prune_stale_worktrees(&repo_root)?;
                eprintln!(
                    "pruned stale worktree registration '{}' for branch {branch}; the working directory was already missing",
                    matched.name
                );
            } else if force && git::branch_exists(&repo_root, branch)? {
                // Only under explicit --force: finish a deletion the user
                // already attempted (e.g. a refused recovery pruned only the
                // registration). Without force we refuse, so detached or
                // moved worktrees keep every refusal of the normal path.
                eprintln!("no worktree registered for branch {branch}; deleting the branch only");
            } else {
                return Err(anyhow!("no worktree found for branch {branch}"));
            }
            git::branch_delete(&repo_root, branch, force)?;
            eprintln!("deleted branch {branch}");
            return Ok(());
        }
    };

    let hook_context = HookContext {
        branch: branch.to_owned(),
        worktree_path: path.clone(),
        repo_root: repo_root.clone(),
        agent: None,
        task: None,
    };

    hooks::run_blocking(&config.hooks.pre_remove, &hook_context)?;

    // `pre_remove` hooks and concurrent processes can change the worktree;
    // revalidate ownership before any destructive step.
    git::ensure_worktree_attached_to_branch(&repo_root, &path, branch)?;

    git::worktree_remove(&repo_root, &path, force)?;
    git::branch_delete(&repo_root, branch, force)?;
    hooks::run_blocking(&config.hooks.post_remove, &hook_context)?;

    eprintln!(
        "removed worktree {} and deleted branch {}",
        path.display(),
        branch
    );
    Ok(())
}
