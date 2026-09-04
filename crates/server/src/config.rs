//! server 配置：TOML 文件 + env 覆盖（§2.2 config.rs）。

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub max_upload_bytes: u64,
    pub max_attachments_per_message: u32,
    pub allow_registration: bool,
    pub allowed_image_types: Vec<String>,
    pub cors_origins: Vec<String>,
    pub secure_cookies: bool,
    pub web_dist: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:7420".parse().unwrap(),
            data_dir: PathBuf::from("data"),
            max_upload_bytes: 20 * 1024 * 1024,
            max_attachments_per_message: 9,
            allow_registration: true,
            allowed_image_types: ["png", "jpeg", "gif", "webp"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            cors_origins: vec![],
            secure_cookies: false,
            web_dist: None,
        }
    }
}

impl Config {
    /// 加载顺序：内置默认 < config 文件（--config 或 ./folkmoot.toml）< env 覆盖
    pub fn load(path: Option<&str>) -> Result<Self> {
        let mut cfg = match path {
            Some(p) => {
                let text = std::fs::read_to_string(p)
                    .with_context(|| format!("read config file {p}"))?;
                toml::from_str(&text).with_context(|| format!("parse config file {p}"))?
            }
            None => Self::default(),
        };
        if let Ok(v) = std::env::var("FOLKMOOT_BIND") {
            cfg.bind = v.parse().context("FOLKMOOT_BIND")?;
        }
        if let Ok(v) = std::env::var("FOLKMOOT_DATA_DIR") {
            cfg.data_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("FOLKMOOT_ALLOW_REGISTRATION") {
            cfg.allow_registration = matches!(v.as_str(), "1" | "true" | "yes");
        }
        Ok(cfg)
    }
}
