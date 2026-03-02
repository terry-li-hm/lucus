use anyhow::{Result, anyhow};
use std::str::FromStr;

use crate::config::Config;
use crate::git;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchRef {
    Previous,
    DefaultBranch,
    Current,
    Named(String),
}

impl FromStr for BranchRef {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "-" => Ok(Self::Previous),
            "^" => Ok(Self::DefaultBranch),
            "@" => Ok(Self::Current),
            "" => Err(anyhow!("branch reference cannot be empty")),
            _ => Ok(Self::Named(value.to_owned())),
        }
    }
}

pub fn run(config: &Config, branch_ref: &BranchRef) -> Result<()> {
    let path = resolve_path(config, branch_ref)?;
    println!("{}", path.display());
    Ok(())
}

pub fn resolve_path(config: &Config, branch_ref: &BranchRef) -> Result<std::path::PathBuf> {
    let repo_root = git::repo_root_from_cwd()?;

    let branch = match branch_ref {
        BranchRef::Previous => git::previous_branch(&repo_root)?,
        BranchRef::DefaultBranch => config.worktree.default_branch.clone(),
        BranchRef::Current => git::current_branch(&repo_root)?,
        BranchRef::Named(name) => name.clone(),
    };

    git::find_worktree_by_branch(&repo_root, &branch)?
        .ok_or_else(|| anyhow!("no worktree found for branch {branch}"))
}
