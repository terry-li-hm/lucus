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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ENV_LOCK;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos();
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!(
                "lucus-init-{label}-{}-{stamp}-{n}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temp directory should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn with_home<R>(home: &Path, f: impl FnOnce() -> R) -> R {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old_home = std::env::var_os("HOME");
        // SAFETY: ENV_LOCK serializes environment mutation for these tests.
        unsafe {
            std::env::set_var("HOME", home);
        }
        let result = f();
        unsafe {
            match old_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
        }
        result
    }

    #[test]
    fn rc_path_uses_home_for_zsh_and_bash() {
        let home = TestDir::new("rc");
        with_home(home.path(), || {
            assert_eq!(rc_path("zsh").unwrap(), home.path().join(".zshrc"));
            assert_eq!(rc_path("bash").unwrap(), home.path().join(".bashrc"));
        });
    }

    #[test]
    fn rc_path_rejects_unsupported_and_empty_shell() {
        let home = TestDir::new("bad-shell");
        with_home(home.path(), || {
            let fish = rc_path("fish").expect_err("fish should fail");
            assert!(fish.to_string().contains("unsupported shell"));
            let empty = rc_path("").expect_err("empty should fail");
            assert!(empty.to_string().contains("unsupported shell"));
            let zsh_upper = rc_path("Zsh").expect_err("case-sensitive");
            assert!(zsh_upper.to_string().contains("unsupported shell"));
        });
    }

    #[test]
    fn rc_path_requires_home() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old_home = std::env::var_os("HOME");
        unsafe {
            std::env::remove_var("HOME");
        }
        let err = rc_path("zsh").expect_err("HOME should be required");
        assert!(err.to_string().contains("HOME is not set"));
        unsafe {
            match old_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn wrapper_block_quotes_query_arguments() {
        let zsh = wrapper_block("zsh").unwrap();
        assert!(zsh.contains(START_MARKER));
        assert!(zsh.contains(END_MARKER));
        assert!(zsh.contains(r#"command lucus query "${@:2}""#));
        let bash = wrapper_block("bash").unwrap();
        assert_eq!(zsh, bash);
    }

    #[test]
    fn run_installs_wrapper_and_skips_the_second_time() {
        let home = TestDir::new("install");
        with_home(home.path(), || {
            run("zsh").expect("first install");
            let rc = fs::read_to_string(home.path().join(".zshrc")).expect("zshrc");
            assert!(rc.contains("lucus() {"));
            assert!(rc.contains(START_MARKER));
            run("zsh").expect("second install should skip");
            let rc2 = fs::read_to_string(home.path().join(".zshrc")).expect("zshrc");
            assert_eq!(rc, rc2);
        });
    }

    #[test]
    fn run_appends_after_existing_rc_without_newline() {
        let home = TestDir::new("append");
        fs::write(home.path().join(".bashrc"), "export FOO=1").expect("write bashrc");
        with_home(home.path(), || {
            run("bash").expect("install");
            let rc = fs::read_to_string(home.path().join(".bashrc")).expect("bashrc");
            assert!(rc.starts_with("export FOO=1\n# >>> lucus init >>>\n"));
        });
    }

    #[test]
    fn run_skips_when_unmarked_lucus_function_exists() {
        let home = TestDir::new("skip-unmarked");
        fs::write(home.path().join(".zshrc"), "lucus() { echo hi; }\n").expect("write zshrc");
        with_home(home.path(), || {
            run("zsh").expect("should skip");
            let rc = fs::read_to_string(home.path().join(".zshrc")).expect("zshrc");
            assert_eq!(rc, "lucus() { echo hi; }\n");
        });
    }

    #[test]
    fn run_rejects_unsupported_shell_before_writing() {
        let home = TestDir::new("reject");
        with_home(home.path(), || {
            let err = run("fish").expect_err("fish should fail");
            assert!(err.to_string().contains("unsupported shell"));
            assert!(!home.path().join(".zshrc").exists());
            assert!(!home.path().join(".bashrc").exists());
        });
    }
}
