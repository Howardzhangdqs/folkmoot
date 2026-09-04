//! CLI 配置：~/.config/folkmoot/config.toml + aliases.toml（§5.1）
//! 优先级：flag > env > file > 默认

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server_url: Option<String>,
    pub token: Option<String>,
    pub default_agent: Option<String>,
    pub extra_tokens: Vec<String>,
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    pub limit: u32,
    pub wait_secs: u64,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            limit: 50,
            wait_secs: 25,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("folkmoot")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn aliases_path() -> PathBuf {
    config_dir().join("aliases.toml")
}

impl Config {
    pub fn load(path: Option<&PathBuf>) -> Result<Self> {
        let p = path.cloned().unwrap_or_else(config_path);
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        Ok(toml::from_str(&text).with_context(|| format!("parse {}", p.display()))?)
    }

    pub fn save(&self) -> Result<()> {
        let p = config_path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&p, text)?;
        // token 落盘必须 0600（§5.1）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn resolve_server(&self, flag: Option<&str>) -> String {
        flag.map(str::to_string)
            .or_else(|| std::env::var("FOLKMOOT_SERVER").ok())
            .or_else(|| self.server_url.clone())
            .unwrap_or_else(|| "http://127.0.0.1:7420".into())
    }

    pub fn resolve_token(&self, flag: Option<&str>) -> Option<String> {
        flag.map(str::to_string)
            .or_else(|| std::env::var("FOLKMOOT_TOKEN").ok())
            .or_else(|| self.token.clone())
    }

    pub fn resolve_agent(&self, flag: Option<&str>) -> Option<String> {
        flag.map(str::to_string)
            .or_else(|| std::env::var("FOLKMOOT_AGENT").ok())
            .or_else(|| self.default_agent.clone())
    }
}

// ---------- aliases.toml ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Aliases {
    pub conversations: std::collections::BTreeMap<String, String>,
}

impl Aliases {
    pub fn load() -> Self {
        let p = aliases_path();
        std::fs::read_to_string(&p)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let p = aliases_path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
