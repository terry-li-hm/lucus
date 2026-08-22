use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub worktree: WorktreeConfig,
    #[serde(default)]
    pub hooks: HookConfig,
    #[serde(default)]
    pub files: FilesConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorktreeConfig {
    #[serde(default = "default_path_template")]
    pub path_template: String,
    #[serde(default = "default_default_branch")]
    pub default_branch: String,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        Self {
            path_template: default_path_template(),
            default_branch: default_default_branch(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub post_create: Vec<String>,
    #[serde(default)]
    pub post_create_bg: Vec<String>,
    #[serde(default)]
    pub pre_remove: Vec<String>,
    #[serde(default)]
    pub post_remove: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FilesConfig {
    #[serde(default)]
    pub copy: Vec<String>,
}

fn default_path_template() -> String {
    "../{repo}.{branch}".to_owned()
}

fn default_default_branch() -> String {
    "main".to_owned()
}

pub fn load() -> Result<Config> {
    let dirs =
        ProjectDirs::from("", "", "lucus").context("cannot determine lucus config directory")?;
    let path = dirs.config_dir().join("config.toml");

    if !path.exists() {
        return Ok(Config::default());
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file at {}", path.display()))?;

    let config: Config = toml::from_str(&raw)
        .with_context(|| format!("failed to parse config file at {}", path.display()))?;

    Ok(config)
}

#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
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
                "lucus-config-{label}-{}-{stamp}-{n}",
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

    fn write_user_config(home: &Path, xdg: &Path, body: &str) {
        let linux = xdg.join("lucus");
        let macos = home
            .join("Library")
            .join("Application Support")
            .join("lucus");
        fs::create_dir_all(&linux).expect("linux config dir");
        fs::create_dir_all(&macos).expect("macos config dir");
        fs::write(linux.join("config.toml"), body).expect("write linux config");
        fs::write(macos.join("config.toml"), body).expect("write macos config");
    }

    fn with_config_env<R>(home: &Path, xdg: &Path, f: impl FnOnce() -> R) -> R {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old_home = std::env::var_os("HOME");
        let old_xdg = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: ENV_LOCK serializes environment mutation for these tests.
        unsafe {
            std::env::set_var("HOME", home);
            std::env::set_var("XDG_CONFIG_HOME", xdg);
        }
        let result = f();
        unsafe {
            match old_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match old_xdg {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
        result
    }

    #[test]
    fn default_config_matches_documented_template() {
        let config = Config::default();
        assert_eq!(config.worktree.path_template, "../{repo}.{branch}");
        assert_eq!(config.worktree.default_branch, "main");
        assert!(config.files.copy.is_empty());
        assert!(config.hooks.post_create.is_empty());
        assert!(config.hooks.post_create_bg.is_empty());
        assert!(config.hooks.pre_remove.is_empty());
        assert!(config.hooks.post_remove.is_empty());
    }

    #[test]
    fn empty_toml_uses_struct_defaults() {
        let config: Config = toml::from_str("").expect("empty toml should parse");
        assert_eq!(config.worktree.path_template, "../{repo}.{branch}");
        assert_eq!(config.worktree.default_branch, "main");
    }

    #[test]
    fn partial_toml_fills_remaining_defaults() {
        let config: Config = toml::from_str("[worktree]\ndefault_branch = \"master\"\n")
            .expect("partial toml should parse");
        assert_eq!(config.worktree.default_branch, "master");
        assert_eq!(config.worktree.path_template, "../{repo}.{branch}");
        assert!(config.files.copy.is_empty());
    }

    #[test]
    fn full_toml_parses_hooks_and_copy_list() {
        let raw = r#"
[worktree]
path_template = ".worktrees/{branch}"
default_branch = "develop"

[hooks]
post_create = ["echo create"]
post_create_bg = ["true"]
pre_remove = ["echo pre"]
post_remove = ["echo post"]

[files]
copy = [".env", ".env.local"]
"#;
        let config: Config = toml::from_str(raw).expect("full toml should parse");
        assert_eq!(config.worktree.path_template, ".worktrees/{branch}");
        assert_eq!(config.worktree.default_branch, "develop");
        assert_eq!(config.hooks.post_create, vec!["echo create".to_owned()]);
        assert_eq!(
            config.files.copy,
            vec![".env".to_owned(), ".env.local".to_owned()]
        );
    }

    #[test]
    fn malformed_toml_fails_to_parse() {
        assert!(toml::from_str::<Config>("[worktree\n").is_err());
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let home = TestDir::new("home-missing");
        let xdg = home.path().join("xdg-config");
        fs::create_dir_all(&xdg).expect("xdg dir");
        with_config_env(home.path(), &xdg, || {
            let config = load().expect("missing config should default");
            assert_eq!(config.worktree.default_branch, "main");
            assert!(config.files.copy.is_empty());
        });
    }

    #[test]
    fn load_reads_valid_config_file() {
        let home = TestDir::new("home-valid");
        let xdg = home.path().join("xdg-config");
        write_user_config(
            home.path(),
            &xdg,
            "[worktree]\ndefault_branch = \"develop\"\n",
        );
        with_config_env(home.path(), &xdg, || {
            let config = load().expect("config should load");
            assert_eq!(config.worktree.default_branch, "develop");
        });
    }

    #[test]
    fn load_malformed_config_file_errors() {
        let home = TestDir::new("home-bad");
        let xdg = home.path().join("xdg-config");
        write_user_config(home.path(), &xdg, "[worktree\n");
        with_config_env(home.path(), &xdg, || {
            let err = load().expect_err("malformed config should fail");
            assert!(
                err.to_string().contains("failed to parse config file"),
                "unexpected error: {err:#}"
            );
        });
    }
}
