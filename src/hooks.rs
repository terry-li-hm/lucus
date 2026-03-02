use anyhow::{Context, Result, anyhow, ensure};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct HookContext {
    pub branch: String,
    pub worktree_path: PathBuf,
    pub repo_root: PathBuf,
    pub agent: Option<String>,
    pub task: Option<String>,
}

pub fn run_blocking(commands: &[String], ctx: &HookContext) -> Result<()> {
    for command in commands {
        run_one(command, ctx, false)?;
    }
    Ok(())
}

pub fn run_background(commands: &[String], ctx: &HookContext) -> Result<()> {
    for command in commands {
        run_one(command, ctx, true)?;
    }
    Ok(())
}

fn run_one(command: &str, ctx: &HookContext, background: bool) -> Result<()> {
    let worktree_path = path_to_env_value(&ctx.worktree_path)?;
    let repo_root = path_to_env_value(&ctx.repo_root)?;

    let mut process = Command::new("sh");
    process
        .arg("-c")
        .arg(command)
        .env("LUCUS_BRANCH", &ctx.branch)
        .env("LUCUS_WORKTREE_PATH", worktree_path)
        .env("LUCUS_REPO", repo_root)
        .env("LUCUS_AGENT", ctx.agent.as_deref().unwrap_or(""))
        .env("LUCUS_TASK", ctx.task.as_deref().unwrap_or(""));

    if background {
        process
            .spawn()
            .with_context(|| format!("failed to spawn background hook command: {command}"))?;
    } else {
        let status = process
            .status()
            .with_context(|| format!("failed to execute hook command: {command}"))?;

        ensure!(
            status.success(),
            "hook command failed with status {status}: {command}"
        );
    }
    Ok(())
}

fn path_to_env_value(path: &Path) -> Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}
