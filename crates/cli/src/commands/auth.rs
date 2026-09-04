//! register / whoami / tokens

use anyhow::Result;

use super::Ctx;
use crate::client::CliError;
use crate::display;
use folkmoot_common::{
    IssueTokenRequest, IssuedToken, LoginResponse, MeResponse, RegisterRequest, RegisterResponse,
    TokenInfo,
};

pub async fn register(ctx: &mut Ctx, name: &str, token_count: u32) -> Result<(), CliError> {
    let (status, resp): (u16, RegisterResponse) = ctx
        .client
        .post_json_status(
            "/auth/register",
            &RegisterRequest {
                name: name.to_string(),
                token_count: Some(token_count),
            },
        )
        .await?;
    let _ = status;

    // 自动写入配置：首个 token 为主，其余入 extra_tokens（§5.1）
    if let Some(first) = resp.tokens.first() {
        ctx.config.token = Some(first.token.clone());
        ctx.config.server_url = Some(ctx.client.base.clone());
        ctx.config.extra_tokens = resp.tokens[1..].iter().map(|t| t.token.clone()).collect();
        if let Err(e) = ctx.config.save() {
            eprintln!("warning: could not save config: {e}");
        }
    }

    if ctx.json {
        ctx.print_json(&resp);
    } else {
        println!("registered account: {} ({})", resp.account.name, resp.account.id);
        println!("tokens (shown once, saved to config):");
        for t in &resp.tokens {
            let label = if t.label.is_empty() { "-" } else { &t.label };
            println!("  {label}: {}", t.token);
        }
    }
    Ok(())
}

pub async fn whoami(ctx: &Ctx) -> Result<(), CliError> {
    ctx.require_token()?;
    ctx.print_agent_line();
    let v: serde_json::Value = ctx.client.get("/me").await?;
    if ctx.json {
        ctx.print_json(&v);
        return Ok(());
    }
    if v.get("agent").is_some() {
        let me: MeResponse = serde_json::from_value(v).unwrap();
        println!("account: {} ({})", me.account.name, me.account.id);
        println!(
            "agent:   {} ({})",
            me.agent.agent_key.unwrap_or_default(),
            me.agent.id
        );
    } else {
        let me: LoginResponse = serde_json::from_value(v).unwrap();
        println!("account: {} ({})", me.account.name, me.account.id);
        println!("agents:  {}", me.agent_keys.join(", "));
    }
    Ok(())
}

pub async fn tokens_list(ctx: &Ctx) -> Result<(), CliError> {
    ctx.require_token()?;
    let list: Vec<TokenInfo> = ctx.client.get("/auth/tokens").await?;
    if ctx.json {
        ctx.print_json(&list);
        return Ok(());
    }
    let rows = list
        .iter()
        .map(|t| {
            vec![
                format!("{}{}", &t.id[..8], if t.current { " *" } else { "" }),
                if t.label.is_empty() { "-".into() } else { t.label.clone() },
                display::rel_time(&t.created_at),
                t.last_used_at
                    .as_ref()
                    .map(display::rel_time)
                    .unwrap_or_else(|| "never".into()),
                t.last_user_agent
                    .clone()
                    .map(|u| u.chars().take(40).collect())
                    .unwrap_or_else(|| "-".into()),
            ]
        })
        .collect();
    println!("{}", display::table(&["id", "label", "created", "last used", "user agent"], rows));
    Ok(())
}

pub async fn tokens_issue(ctx: &Ctx, label: &str) -> Result<(), CliError> {
    ctx.require_token()?;
    let issued: IssuedToken = ctx
        .client
        .post_json("/auth/tokens", &IssueTokenRequest { label: label.to_string() })
        .await?;
    if ctx.json {
        ctx.print_json(&issued);
    } else {
        println!("issued token (shown once): {}", issued.token);
        eprintln!("note: not written to config; update config.toml or use --token to switch");
    }
    Ok(())
}

pub async fn tokens_revoke(ctx: &Ctx, id: &str, force: bool) -> Result<(), CliError> {
    ctx.require_token()?;
    // 字面量 current 指 config 在用 token
    let target = if id == "current" {
        let list: Vec<TokenInfo> = ctx.client.get("/auth/tokens").await?;
        let cur = list.iter().find(|t| t.current).ok_or_else(|| CliError {
            status: None,
            code: None,
            message: "no current token".into(),
        })?;
        if !force {
            return Err(CliError {
                status: None,
                code: None,
                message: "revoking the current token requires --force (subsequent requests will 401; clean up config.toml)".into(),
            });
        }
        cur.id.clone()
    } else {
        // 唯一前缀解析
        let list: Vec<TokenInfo> = ctx.client.get("/auth/tokens").await?;
        let matches: Vec<_> = list.iter().filter(|t| t.id.starts_with(id)).collect();
        match matches.len() {
            1 => matches[0].id.clone(),
            0 => return Err(CliError { status: Some(404), code: None, message: format!("token '{id}' not found") }),
            _ => return Err(CliError { status: None, code: None, message: format!("'{id}' is ambiguous") }),
        }
    };
    let resp: folkmoot_common::RevokeTokenResponse =
        ctx.client.delete(&format!("/auth/tokens/{target}")).await?;
    if ctx.json {
        ctx.print_json(&resp);
    } else {
        println!(
            "revoked token {} (cascaded {} session(s))",
            &target[..8],
            resp.revoked_sessions
        );
    }
    Ok(())
}
