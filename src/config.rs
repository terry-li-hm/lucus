use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub worktree: WorktreeConfig,
    #[serde(default)]
    pub hooks: HookConfig,
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
