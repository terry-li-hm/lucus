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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_list_without_json() {
        let cli = Cli::try_parse_from(["lucus", "list"]).unwrap();
        assert!(!cli.json);
        assert!(matches!(cli.command, Command::List));
    }

    #[test]
    fn parses_global_json_before_and_after_subcommand() {
        let before = Cli::try_parse_from(["lucus", "--json", "list"]).unwrap();
        assert!(before.json);
        assert!(matches!(before.command, Command::List));
        let after = Cli::try_parse_from(["lucus", "list", "--json"]).unwrap();
        assert!(after.json);
    }

    #[test]
    fn parses_new_branch() {
        let cli = Cli::try_parse_from(["lucus", "new", "feat/auth"]).unwrap();
        match cli.command {
            Command::New { branch } => assert_eq!(branch, "feat/auth"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_remove_force_flag_in_both_positions() {
        let plain = Cli::try_parse_from(["lucus", "remove", "feat/auth"]).unwrap();
        match plain.command {
            Command::Remove { branch, force } => {
                assert_eq!(branch, "feat/auth");
                assert!(!force);
            }
            other => panic!("unexpected {other:?}"),
        }

        let after = Cli::try_parse_from(["lucus", "remove", "feat/auth", "--force"]).unwrap();
        match after.command {
            Command::Remove { force, .. } => assert!(force),
            other => panic!("unexpected {other:?}"),
        }

        let before = Cli::try_parse_from(["lucus", "remove", "--force", "feat/auth"]).unwrap();
        match before.command {
            Command::Remove { branch, force } => {
                assert_eq!(branch, "feat/auth");
                assert!(force);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_query_symbolic_refs_including_hyphen() {
        let hyphen = Cli::try_parse_from(["lucus", "query", "-"]).unwrap();
        match hyphen.command {
            Command::Query { branch } => assert_eq!(branch, BranchRef::Previous),
            other => panic!("unexpected {other:?}"),
        }
        let caret = Cli::try_parse_from(["lucus", "query", "^"]).unwrap();
        match caret.command {
            Command::Query { branch } => assert_eq!(branch, BranchRef::DefaultBranch),
            other => panic!("unexpected {other:?}"),
        }
        let at = Cli::try_parse_from(["lucus", "switch", "@"]).unwrap();
        match at.command {
            Command::Switch { branch } => assert_eq!(branch, BranchRef::Current),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_init_shell() {
        let cli = Cli::try_parse_from(["lucus", "init", "zsh"]).unwrap();
        match cli.command {
            Command::Init { shell } => assert_eq!(shell, "zsh"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_subcommand_and_missing_args() {
        assert!(Cli::try_parse_from(["lucus"]).is_err());
        assert!(Cli::try_parse_from(["lucus", "new"]).is_err());
        assert!(Cli::try_parse_from(["lucus", "remove"]).is_err());
        assert!(Cli::try_parse_from(["lucus", "query"]).is_err());
        assert!(Cli::try_parse_from(["lucus", "init"]).is_err());
        assert!(Cli::try_parse_from(["lucus", "unknown"]).is_err());
    }

    #[test]
    fn help_and_version_are_clap_display_errors() {
        let help = Cli::try_parse_from(["lucus", "--help"]).unwrap_err();
        assert_eq!(help.kind(), clap::error::ErrorKind::DisplayHelp);
        let version = Cli::try_parse_from(["lucus", "--version"]).unwrap_err();
        assert_eq!(version.kind(), clap::error::ErrorKind::DisplayVersion);
    }
}
