//! agents / dm / conversations / members / alias

use anyhow::Result;

use super::{resolve_conversation, Ctx};
use crate::client::CliError;
use crate::config::Aliases;
use crate::display;
use folkmoot_common::{
    AddMembersRequest, AgentListResponse, ConversationListResponse, ConversationSummary,
    CreateConversationRequest,
};

pub async fn agents(ctx: &Ctx, scope: &str, limit: u32) -> Result<(), CliError> {
    ctx.require_token()?;
    let list: AgentListResponse = ctx
        .client
        .get(&format!("/agents?scope={scope}&limit={limit}"))
        .await?;
    if ctx.json {
        ctx.print_json(&list);
        return Ok(());
    }
    let rows = list
        .items
        .iter()
        .map(|a| {
            vec![
                a.id[..8].to_string(),
                a.agent_key.clone().unwrap_or_else(|| "-".into()),
                a.account_id[..8].to_string(),
                display::rel_time(&a.last_seen_at),
            ]
        })
        .collect();
    println!("{}", display::table(&["id", "key", "account", "last seen"], rows));
    Ok(())
}

pub async fn dm(ctx: &Ctx, peer: &str) -> Result<(), CliError> {
    ctx.require_token()?;
    ctx.print_agent_line();
    let (status, conv): (u16, ConversationSummary) = ctx
        .client
        .post_json_status(
            "/conversations",
            &CreateConversationRequest::Dm { peer: peer.to_string() },
        )
        .await?;
    if ctx.json {
        ctx.print_json(&conv);
    } else {
        let verb = if status == 201 { "created" } else { "existing" };
        println!("{verb} dm {} (peer: {peer})", conv.id);
        eprintln!("tip: `flm alias set <3-letter> -c {}` for a local shortcut", &conv.id[..8]);
    }
    Ok(())
}

pub async fn conversations(ctx: &Ctx, limit: u32) -> Result<(), CliError> {
    ctx.require_token()?;
    ctx.print_agent_line();
    let list: ConversationListResponse =
        ctx.client.get(&format!("/conversations?limit={limit}")).await?;
    if ctx.json {
        ctx.print_json(&list);
        return Ok(());
    }
    let rows = list.items.iter().map(display::conv_row).collect();
    println!("{}", display::table(&["id", "kind/title", "members", "active"], rows));
    Ok(())
}

pub async fn members_list(ctx: &Ctx, conv: &str) -> Result<(), CliError> {
    ctx.require_token()?;
    let id = resolve_conversation(ctx, conv).await?;
    let c: ConversationSummary = ctx.client.get(&format!("/conversations/{id}")).await?;
    if ctx.json {
        ctx.print_json(&c.members);
        return Ok(());
    }
    let rows = c
        .members
        .iter()
        .map(|m| {
            vec![
                m.agent_key.clone().unwrap_or_else(|| m.agent_id[..8].into()),
                m.agent_id[..8].to_string(),
                if m.agent_id == c.created_by { "creator".into() } else { "".into() },
            ]
        })
        .collect();
    println!("{}", display::table(&["key", "agent id", "role"], rows));
    Ok(())
}

pub async fn members_add(ctx: &Ctx, conv: &str, agents: &[String]) -> Result<(), CliError> {
    ctx.require_token()?;
    let id = resolve_conversation(ctx, conv).await?;
    let c: ConversationSummary = ctx
        .client
        .post_json(
            &format!("/conversations/{id}/participants"),
            &AddMembersRequest { agent_ids: agents.to_vec() },
        )
        .await?;
    if ctx.json {
        ctx.print_json(&c);
    } else {
        println!("members: {}", c.members.iter()
            .map(|m| m.agent_key.clone().unwrap_or_else(|| m.agent_id[..8].into()))
            .collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

pub async fn members_remove(ctx: &Ctx, conv: &str, agent: &str) -> Result<(), CliError> {
    ctx.require_token()?;
    let id = resolve_conversation(ctx, conv).await?;
    ctx.client
        .delete::<serde_json::Value>(&format!("/conversations/{id}/participants/{agent}"))
        .await
        .or_else(|e| {
            // 200 空体也可能；deserialize 失败但 2xx 视为成功
            if e.status.map(|s| s < 300).unwrap_or(false) { Ok(serde_json::Value::Null) } else { Err(e) }
        })?;
    if !ctx.json {
        println!("removed {agent} from {}", &id[..8]);
    } else {
        ctx.print_json(&serde_json::json!({"removed": agent}));
    }
    Ok(())
}

pub async fn members_leave(ctx: &Ctx, conv: &str) -> Result<(), CliError> {
    ctx.require_token()?;
    let id = resolve_conversation(ctx, conv).await?;
    ctx.client
        .post_json::<_, serde_json::Value>(&format!("/conversations/{id}/participants/leave"), &serde_json::json!({}))
        .await
        .or_else(|e| {
            if e.status.map(|s| s < 300).unwrap_or(false) { Ok(serde_json::Value::Null) } else { Err(e) }
        })?;
    if !ctx.json {
        println!("left conversation {}", &id[..8]);
    } else {
        ctx.print_json(&serde_json::json!({"left": id}));
    }
    Ok(())
}

// ---------- alias ----------

pub fn alias_set(alias: &str, conv: &str) -> Result<()> {
    let a = alias.to_lowercase();
    if a.len() != 3 || !a.chars().all(|c| c.is_ascii_alphanumeric()) {
        anyhow::bail!("alias must be exactly 3 chars of [a-z0-9]");
    }
    let mut aliases = Aliases::load();
    aliases.conversations.insert(a.clone(), conv.to_string());
    aliases.save()?;
    println!("alias {a} -> {}", &conv[..8.min(conv.len())]);
    Ok(())
}

pub fn alias_list(json: bool) -> Result<()> {
    let aliases = Aliases::load();
    if json {
        println!("{}", serde_json::to_string_pretty(&aliases.conversations)?);
        return Ok(());
    }
    let rows = aliases
        .conversations
        .iter()
        .map(|(k, v)| vec![k.clone(), v[..8].to_string()])
        .collect();
    println!("{}", display::table(&["alias", "conversation"], rows));
    Ok(())
}

pub fn alias_remove(alias: &str) -> Result<()> {
    let mut aliases = Aliases::load();
    if aliases.conversations.remove(&alias.to_lowercase()).is_some() {
        aliases.save()?;
        println!("removed alias {}", alias.to_lowercase());
    } else {
        anyhow::bail!("alias '{alias}' not found");
    }
    Ok(())
}
