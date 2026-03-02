mod commands;
mod config;
mod files;
mod git;
mod hooks;
mod output;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

use crate::commands::query::BranchRef;

#[derive(Debug, Parser)]
#[command(name = "lucus", version, about = "Git worktree manager")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a worktree for a branch.
    New { branch: String },

    /// List worktrees with git status details.
    List,

    /// Remove a worktree and delete its branch.
    Remove {
        branch: String,
        #[arg(long, help = "Remove even if the worktree has uncommitted changes")]
        force: bool,
    },

    /// Print a worktree path for shell wrappers.
    Query {
        #[arg(allow_hyphen_values = true)]
        branch: BranchRef,
    },

    /// Alias for query.
    Switch {
        #[arg(allow_hyphen_values = true)]
        branch: BranchRef,
    },

    /// Install shell wrapper to rc file.
    Init { shell: String },
}

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("lucus: {err:#}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::List => {
            let format = output::detect_format(cli.json);
            commands::list::run(format)
        }
        command => {
            let config = config::load()?;
            match command {
                Command::New { branch } => commands::new::run(&config, &branch),
                Command::Remove { branch, force } => commands::remove::run(&config, &branch, force),
                Command::Query { branch } => commands::query::run(&config, &branch),
                Command::Switch { branch } => commands::switch::run(&config, &branch),
                Command::Init { shell } => commands::init::run(&shell),
                Command::List => unreachable!(),
            }
        }
    }
}
