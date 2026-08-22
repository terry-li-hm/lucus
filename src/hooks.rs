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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "lucus-hooks-{label}-{}-{stamp}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp directory should be created");
        path
    }

    fn sample_ctx(dir: &std::path::Path) -> HookContext {
        HookContext {
            branch: "feat/auth".to_owned(),
            worktree_path: dir.join("worktree"),
            repo_root: dir.join("repo"),
            agent: Some("codex".to_owned()),
            task: Some("build".to_owned()),
        }
    }

    #[test]
    fn run_blocking_empty_commands_is_ok() {
        let dir = temp_dir("empty");
        let ctx = sample_ctx(&dir);
        run_blocking(&[], &ctx).expect("no commands should succeed");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_blocking_injects_env_vars_without_rewriting_the_command() {
        let dir = temp_dir("env");
        let ctx = sample_ctx(&dir);
        let marker = dir.join("env.txt");
        let command = format!(
            "printf '%s\\n%s\\n%s\\n%s\\n%s' \"$LUCUS_BRANCH\" \"$LUCUS_WORKTREE_PATH\" \"$LUCUS_REPO\" \"$LUCUS_AGENT\" \"$LUCUS_TASK\" > '{}'",
            marker.display()
        );
        run_blocking(&[command], &ctx).expect("hook should succeed");
        let body = fs::read_to_string(&marker).expect("marker should exist");
        let lines: Vec<_> = body.lines().collect();
        assert_eq!(
            lines,
            [
                "feat/auth",
                ctx.worktree_path.to_str().unwrap(),
                ctx.repo_root.to_str().unwrap(),
                "codex",
                "build",
            ]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_blocking_does_not_interpolate_metacharacters_from_the_branch_name() {
        let dir = temp_dir("meta");
        let mut ctx = sample_ctx(&dir);
        ctx.branch = "$(echo pwned)".to_owned();
        let marker = dir.join("branch.txt");
        let command = format!("printf '%s' \"$LUCUS_BRANCH\" > '{}'", marker.display());
        run_blocking(&[command], &ctx).expect("hook should succeed");
        assert_eq!(
            fs::read_to_string(&marker).expect("marker should exist"),
            "$(echo pwned)"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_blocking_reports_failed_status() {
        let dir = temp_dir("fail");
        let ctx = sample_ctx(&dir);
        let err = run_blocking(&["false".to_owned()], &ctx).expect_err("false should fail");
        assert!(
            err.to_string().contains("hook command failed"),
            "unexpected error: {err:#}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_background_spawns_without_waiting_for_failure() {
        let dir = temp_dir("bg");
        let ctx = sample_ctx(&dir);
        run_background(&["false".to_owned()], &ctx).expect("spawn of false should succeed");
        let _ = fs::remove_dir_all(&dir);
    }
}
