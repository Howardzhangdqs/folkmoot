//! 子命令实现

pub mod auth;
pub mod conv;
pub mod link;
pub mod msg;

use anyhow::Result;

use crate::client::{CliError, Client};
use crate::config::{Aliases, Config};

pub struct Ctx {
    pub config: Config,
    pub client: Client,
    pub json: bool,
    /// 未指定 -a 时随机生成的 agent id（人类模式需打印）
    pub generated_agent: Option<String>,
}

impl Ctx {
    pub fn require_token(&self) -> Result<(), CliError> {
        if self.client.has_token() {
            Ok(())
        } else {
            Err(CliError {
                status: None,
                code: None,
                message: "no token configured; run `folkmoot register` or pass --token".into(),
            })
        }
    }

    pub fn print_json<T: serde::Serialize>(&self, v: &T) {
        println!("{}", serde_json::to_string_pretty(v).unwrap());
    }

    pub fn print_agent_line(&self) {
        if !self.json {
            if let Some(id) = &self.generated_agent {
                println!("agent id: {id}");
            }
        }
    }
}

/// 生成随机 8 位 agent id（[a-z0-9]，§5.1）
pub fn random_agent_key() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..8)
        .map(|_| {
            let i = (rand::random::<u8>() as usize) % CHARSET.len();
            CHARSET[i] as char
        })
        .collect()
}

/// 会话引用解析：三字母别名 > 完整 id > 唯一 id 前缀（§5.1）
pub async fn resolve_conversation(ctx: &Ctx, reference: &str) -> Result<String, CliError> {
    // 1. 别名
    let aliases = Aliases::load();
    if let Some(id) = aliases.conversations.get(&reference.to_lowercase()) {
        return Ok(id.clone());
    }
    // 2. 完整 id
    if reference.len() == 36 && reference.chars().filter(|c| *c == '-').count() == 4 {
        return Ok(reference.to_string());
    }
    // 3. 唯一前缀（拉会话列表本地解析）
    let list: folkmoot_common::ConversationListResponse =
        ctx.client.get("/conversations?limit=200").await?;
    let matches: Vec<_> = list
        .items
        .iter()
        .filter(|c| c.id.starts_with(reference))
        .collect();
    match matches.len() {
        1 => Ok(matches[0].id.clone()),
        0 => Err(CliError::local(format!("no conversation matches '{reference}'"))),
        _ => Err(CliError::local(format!(
            "'{reference}' is ambiguous ({} matches)",
            matches.len()
        ))),
    }
}
