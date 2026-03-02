use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;

const START_MARKER: &str = "# >>> lucus init >>>";
const END_MARKER: &str = "# <<< lucus init <<<";

pub fn run(shell: &str) -> Result<()> {
    let rc_file = rc_path(shell)?;
    let block = wrapper_block(shell)?;

    let existing = std::fs::read_to_string(&rc_file).unwrap_or_default();
    if existing.contains(START_MARKER)
        || existing.contains("lucus() {")
        || existing.contains("function lucus")
    {
        eprintln!(
            "lucus shell function already exists in {}, skipping",
            rc_file.display()
        );
        return Ok(());
    }

    let mut updated = existing;
    if !updated.ends_with('\n') && !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&block);

    std::fs::write(&rc_file, updated)
        .with_context(|| format!("failed to write {}", rc_file.display()))?;

    eprintln!("installed lucus shell wrapper in {}", rc_file.display());
    Ok(())
}

fn rc_path(shell: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    let path = match shell {
        "zsh" => PathBuf::from(home).join(".zshrc"),
        "bash" => PathBuf::from(home).join(".bashrc"),
        _ => return Err(anyhow!("unsupported shell '{shell}', expected zsh or bash")),
    };
    Ok(path)
}

fn wrapper_block(shell: &str) -> Result<String> {
    let body = match shell {
        "zsh" => {
            r#"lucus() {
  if [[ "$1" == "switch" || "$1" == "new" ]]; then
    local dir
    dir=$(command lucus query "${@:2}" 2>/dev/null)
    if [[ -n "$dir" ]]; then
      cd "$dir" || return 1
    else
      command lucus "$@"
    fi
  else
    command lucus "$@"
  fi
}
"#
        }
        "bash" => {
            r#"lucus() {
  if [[ "$1" == "switch" || "$1" == "new" ]]; then
    local dir
    dir=$(command lucus query "${@:2}" 2>/dev/null)
    if [[ -n "$dir" ]]; then
      cd "$dir" || return 1
    else
      command lucus "$@"
    fi
  else
    command lucus "$@"
  fi
}
"#
        }
        _ => return Err(anyhow!("unsupported shell '{shell}', expected zsh or bash")),
    };

    Ok(format!("{START_MARKER}\n{body}{END_MARKER}\n"))
}
