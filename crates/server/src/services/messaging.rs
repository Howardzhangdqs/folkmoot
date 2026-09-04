//! 会话/消息业务规则：DM 规范化、成员资格、发消息事务、长轮询（§4.2）。

use std::time::Duration;

use crate::db::{agents, conversations, messages, Db};
use crate::error::ApiError;
use folkmoot_common::{
    ConversationSummary, CreateConversationRequest, Message, MessagePage,
};

pub const MAX_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_WAIT_SECS: u64 = 30;

/// 创建会话（DM 幂等：已存在返回 (existing, false)；group 新建）
pub async fn create_conversation(
    db: &Db,
    account_id: &str,
    creator_agent_id: &str,
    req: CreateConversationRequest,
) -> Result<(ConversationSummary, bool), ApiError> {
    let account_id = account_id.to_string();
    let creator = creator_agent_id.to_string();
    db.run(move |conn| match req {
        CreateConversationRequest::Dm { peer } => {
            let peer_agent = agents::resolve_ref(conn, &account_id, &peer)?
                .ok_or_else(|| ApiError::not_found("peer agent not found"))?;
            if peer_agent.id == creator {
                return Err(ApiError::validation("self-DM is not allowed"));
            }
            let key = folkmoot_common::dm_key(&creator, &peer_agent.id);
            if let Some(existing) = conversations::find_dm(conn, &key)? {
                let ms = conversations::members(conn, &existing.id)?;
                return Ok((
                    ConversationSummary {
                        id: existing.id,
                        kind: existing.kind,
                        title: existing.title,
                        created_by: existing.created_by,
                        created_at: existing.created_at,
                        last_message_at: existing.last_message_at,
                        members: ms,
                    },
                    false,
                ));
            }
            let conv = conversations::create(
                conn,
                folkmoot_common::ConversationKind::Dm,
                None,
                Some(&key),
                &creator,
                &[creator.clone(), peer_agent.id],
            )?;
            let ms = conversations::members(conn, &conv.id)?;
            Ok((
                ConversationSummary {
                    id: conv.id,
                    kind: conv.kind,
                    title: conv.title,
                    created_by: conv.created_by,
                    created_at: conv.created_at,
                    last_message_at: conv.last_message_at,
                    members: ms,
                },
                true,
            ))
        }
        CreateConversationRequest::Group { title, members } => {
            let mut member_ids = vec![creator.clone()];
            for m in members {
                let a = agents::resolve_ref(conn, &account_id, &m)?
                    .ok_or_else(|| ApiError::not_found(format!("agent not found: {m}")))?;
                if !member_ids.contains(&a.id) {
                    member_ids.push(a.id);
                }
            }
            let conv = conversations::create(
                conn,
                folkmoot_common::ConversationKind::Group,
                title.as_deref(),
                None,
                &creator,
                &member_ids,
            )?;
            let ms = conversations::members(conn, &conv.id)?;
            Ok((
                ConversationSummary {
                    id: conv.id,
                    kind: conv.kind,
                    title: conv.title,
                    created_by: conv.created_by,
                    created_at: conv.created_at,
                    last_message_at: conv.last_message_at,
                    members: ms,
                },
                true,
            ))
        }
    })
    .await
}

/// 校验成员资格：非成员/不存在统一 404（避免会话存在性泄露，§8.2）
pub async fn require_membership(db: &Db, conv_id: &str, agent_id: &str) -> Result<(), ApiError> {
    let conv_id = conv_id.to_string();
    let agent_id = agent_id.to_string();
    db.run(move |conn| {
        if conversations::is_member(conn, &conv_id, &agent_id)? {
            Ok(())
        } else {
            Err(ApiError::not_found("conversation not found"))
        }
    })
    .await
}

/// 发文本消息（事务：insert + touch last_message_at）
pub async fn send_text(
    db: &Db,
    conv_id: &str,
    sender_id: &str,
    text: &str,
) -> Result<Message, ApiError> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(ApiError::validation("text must be 1..=8192 bytes"));
    }
    let conv_id = conv_id.to_string();
    let sender_id = sender_id.to_string();
    let text = text.to_string();
    db.run(move |conn| {
        if !conversations::is_member(conn, &conv_id, &sender_id)? {
            return Err(ApiError::not_found("conversation not found"));
        }
        let tx = conn.unchecked_transaction()?;
        let (id, ts) = messages::insert(&tx, &conv_id, &sender_id, Some(&text))?;
        conversations::touch_last_message(&tx, &conv_id, &crate::db::accounts::fmt_dt(&ts))?;
        tx.commit()?;
        messages::get(conn, &id)?
            .ok_or_else(|| ApiError::internal("message vanished after insert"))
    })
    .await
}

/// 读历史（keyset 分页）
pub async fn history(
    db: &Db,
    conv_id: &str,
    agent_id: &str,
    limit: u32,
    before: Option<String>,
    after: Option<String>,
) -> Result<MessagePage, ApiError> {
    let conv_id = conv_id.to_string();
    let agent_id = agent_id.to_string();
    db.run(move |conn| {
        if !conversations::is_member(conn, &conv_id, &agent_id)? {
            return Err(ApiError::not_found("conversation not found"));
        }
        let (items, next_cursor) =
            messages::list(conn, &conv_id, limit, before.as_deref(), after.as_deref())?;
        Ok(MessagePage { items, next_cursor })
    })
    .await
}

/// 长轮询取新：短读事务 + sleep 循环，绝不持锁睡眠（§4.2/R8）
pub async fn poll_new(
    db: &Db,
    state_shutdown: tokio::sync::watch::Receiver<bool>,
    conv_id: &str,
    agent_id: &str,
    after: &str,
    wait_secs: u64,
    limit: u32,
) -> Result<MessagePage, ApiError> {
    let wait = wait_secs.min(MAX_WAIT_SECS);
    let deadline = std::time::Instant::now() + Duration::from_secs(wait);
    let mut shutdown = state_shutdown;
    loop {
        let page = history(db, conv_id, agent_id, limit, None, Some(after.to_string())).await?;
        if !page.items.is_empty() {
            return Ok(page);
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return Ok(MessagePage {
                items: vec![],
                next_cursor: None,
            });
        }
        let remaining = deadline - now;
        let tick = Duration::from_secs(1).min(remaining);
        // shutdown 信号可中断在途等待（≤2s 退出）
        if tokio::time::timeout(tick, shutdown.changed()).await.is_ok() {
            return Ok(MessagePage {
                items: vec![],
                next_cursor: None,
            });
        }
    }
}
