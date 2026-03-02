use anyhow::Result;
use owo_colors::OwoColorize;

use crate::git;
use crate::output::OutputFormat;

#[derive(Clone, Debug)]
struct Row {
    branch: String,
    path: String,
    ahead: u32,
    behind: u32,
    uncommitted: u32,
}

pub fn run(format: OutputFormat) -> Result<()> {
    let repo_root = git::repo_root_from_cwd()?;
    let entries = git::list_worktrees(&repo_root)?;

    let mut rows = Vec::with_capacity(entries.len());
    for entry in entries {
        let stats = git::worktree_stats(&entry.path)?;
        rows.push(Row {
            branch: entry.branch,
            path: entry.path.display().to_string(),
            ahead: stats.ahead,
            behind: stats.behind,
            uncommitted: stats.uncommitted,
        });
    }

    match format {
        OutputFormat::Human => print_human(&rows),
        OutputFormat::Ndjson => print_ndjson(&rows),
    }

    Ok(())
}

fn print_human(rows: &[Row]) {
    let branch_width = rows
        .iter()
        .map(|row| row.branch.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let path_width = rows
        .iter()
        .map(|row| row.path.len())
        .max()
        .unwrap_or(4)
        .max(4);

    println!(
        "{:<branch_width$}  {:<path_width$}  {:>5}  {:>6}  {:>11}",
        "BRANCH".bold(),
        "PATH".bold(),
        "AHEAD".bold(),
        "BEHIND".bold(),
        "UNCOMMITTED".bold(),
        branch_width = branch_width,
        path_width = path_width,
    );

    for row in rows {
        println!(
            "{:<branch_width$}  {:<path_width$}  {:>5}  {:>6}  {:>11}",
            row.branch.cyan(),
            row.path,
            row.ahead,
            row.behind,
            row.uncommitted,
            branch_width = branch_width,
            path_width = path_width,
        );
    }
}

fn print_ndjson(rows: &[Row]) {
    for row in rows {
        println!(
            "{{\"branch\":\"{}\",\"path\":\"{}\",\"ahead\":{},\"behind\":{},\"uncommitted\":{}}}",
            escape_json(&row.branch),
            escape_json(&row.path),
            row.ahead,
            row.behind,
            row.uncommitted,
        );
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
