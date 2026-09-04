//! messages repo：keyset 分页（(created_at, id) 复合游标，§3.4）

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::accounts::{fmt_dt, parse_dt};
use super::{new_id, now};
use crate::error::ApiError;
use folkmoot_common::{AttachmentInfo, Message};

pub struct MessageRow {
    pub id: String,
    pub created_at: String, // RFC3339 原文，keyset 比较用
}

/// 插入消息（调用方负责事务与 conversations.last_message_at 更新）
pub fn insert(
    conn: &Connection,
    conv_id: &str,
    sender_id: &str,
    text: Option<&str>,
) -> Result<(String, DateTime<Utc>), ApiError> {
    let id = new_id();
    let ts = now();
    conn.execute(
        "INSERT INTO messages (id, conversation_id, sender_id, text, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, conv_id, sender_id, text, fmt_dt(&ts)],
    )?;
    Ok((id, ts))
}

/// 取消息的 (created_at, id) 供 keyset 比较
pub fn position(conn: &Connection, id: &str) -> Result<Option<MessageRow>, ApiError> {
    Ok(conn
        .query_row(
            "SELECT id, created_at FROM messages WHERE id = ?1",
            params![id],
            |r| Ok(MessageRow { id: r.get(0)?, created_at: r.get(1)? }),
        )
        .optional()?)
}

fn hydrate(conn: &Connection, rows: Vec<(String, String, String, String, Option<String>, String)>) -> Result<Vec<Message>, ApiError> {
    let mut out = Vec::with_capacity(rows.len());
    for (id, conv, sender, sender_key, text, created) in rows {
        let atts = super::attachments::list_for_message(conn, &id)?;
        out.push(Message {
            id,
            conversation_id: conv,
            sender_id: sender,
            sender_key: Some(sender_key),
            text,
            attachments: atts,
            created_at: parse_dt(&created),
        });
    }
    Ok(out)
}

const SELECT: &str = "SELECT m.id, m.conversation_id, m.sender_id, a.agent_key, m.text, m.created_at
    FROM messages m JOIN agents a ON a.id = m.sender_id";

/// keyset 分页：before（向更旧）/ after（取新），升序返回 + next_cursor
pub fn list(
    conn: &Connection,
    conv_id: &str,
    limit: u32,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<(Vec<Message>, Option<String>), ApiError> {
    let mut sql = format!("{SELECT} WHERE m.conversation_id = ?1");
    let mut args: Vec<String> = vec![conv_id.to_string()];

    if let Some(b) = before {
        let pos = position(conn, b)?
            .ok_or_else(|| ApiError::not_found("before cursor message not found"))?;
        sql.push_str(" AND (m.created_at < ?2 OR (m.created_at = ?2 AND m.id < ?3))");
        args.push(pos.created_at.clone());
        args.push(pos.id.clone());
    }
    if let Some(a) = after {
        let pos = position(conn, a)?
            .ok_or_else(|| ApiError::not_found("after cursor message not found"))?;
        let idx = args.len() + 1;
        sql.push_str(&format!(
            " AND (m.created_at > ?{idx} OR (m.created_at = ?{idx} AND m.id > ?{}))",
            idx + 1
        ));
        args.push(pos.created_at.clone());
        args.push(pos.id.clone());
    }

    // 默认（无游标）= 最新一页：倒序取再反转；before = 向更旧翻页；after = 取新增量
    let fetch = limit + 1;
    let descending = after.is_none();
    let idx = args.len() + 1;
    let order = if descending { "DESC" } else { "ASC" };
    let q = format!("{sql} ORDER BY m.created_at {order}, m.id {order} LIMIT ?{idx}");
    args.push(fetch.to_string());
    let mut stmt = conn.prepare(&q)?;
    let mut rows: Vec<(String, String, String, String, Option<String>, String)> = stmt
        .query_map(rusqlite::params_from_iter(args), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if descending {
        rows.reverse();
    }

    let has_more = rows.len() as u32 > limit;
    // 多余一条用于探测 has_more：descending 模式下最旧的在头部，ascending 在尾部
    let rows = if has_more {
        if descending {
            rows[rows.len() - limit as usize..].to_vec()
        } else {
            rows[..limit as usize].to_vec()
        }
    } else {
        rows
    };
    // next_cursor = 本页最旧一条（before/默认页继续向更旧）；after 增量模式为 None
    let next_cursor = if after.is_none() && has_more {
        rows.first().map(|r| r.0.clone())
    } else {
        None
    };
    Ok((hydrate(conn, rows)?, next_cursor))
}

/// 按 id 取单条消息（含附件与 sender_key）
pub fn get(conn: &Connection, id: &str) -> Result<Option<Message>, ApiError> {
    let row: Option<(String, String, String, String, Option<String>, String)> = conn
        .query_row(
            &format!("{SELECT} WHERE m.id = ?1"),
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?;
    match row {
        Some(r) => Ok(hydrate(conn, vec![r])?.pop()),
        None => Ok(None),
    }
}

/// 会话内最新消息 id（listen 起点用）
pub fn latest_id(conn: &Connection, conv_id: &str) -> Result<Option<String>, ApiError> {
    Ok(conn
        .query_row(
            "SELECT id FROM messages WHERE conversation_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
            params![conv_id],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn attachments_for(conn: &Connection, message_id: &str) -> Result<Vec<AttachmentInfo>, ApiError> {
    super::attachments::list_for_message(conn, message_id)
}

#[cfg(test)]
mod tests {
    //! 分页语义：默认=最新一页、before 向更旧、after 取新增量（§3.4）
    use crate::db::{agents, conversations, messages, Db};
    use folkmoot_common::ConversationKind;

    fn fixture(n: u32) -> (rusqlite::Connection, String, String) {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::migrate(&conn).unwrap();
        let acc = crate::db::accounts::create(&conn, "t").unwrap();
        let a1 = agents::upsert(&conn, &acc.id, "a1").unwrap();
        let a2 = agents::upsert(&conn, &acc.id, "a2").unwrap();
        let conv = conversations::create(&conn, ConversationKind::Group, None, None, &a1.id, &[a1.id.clone(), a2.id.clone()]).unwrap();
        for i in 0..n {
            let (id, ts) = messages::insert(&conn, &conv.id, &a1.id, Some(&format!("m{i}"))).unwrap();
            // 保证 created_at 严格递增（毫秒精度下同毫秒内靠 id tie-break，测试强制拉开）
            conn.execute("UPDATE messages SET created_at = ?1 WHERE id = ?2", rusqlite::params![
                crate::db::accounts::fmt_dt(&(chrono::DateTime::from_timestamp_millis(1_700_000_000_000 + i as i64 * 1000).unwrap())),
                id,
            ]).unwrap();
            let _ = ts;
        }
        (conn, conv.id, a1.id)
    }

    #[test]
    fn default_page_is_latest() {
        let (conn, conv, _a) = fixture(5);
        let (items, next) = messages::list(&conn, &conv, 2, None, None).unwrap();
        assert_eq!(items.iter().map(|m| m.text.clone().unwrap()).collect::<Vec<_>>(), ["m3", "m4"]);
        assert!(next.is_some());
        // 继续向更旧翻页
        let (older, next2) = messages::list(&conn, &conv, 2, next.as_deref(), None).unwrap();
        assert_eq!(older.iter().map(|m| m.text.clone().unwrap()).collect::<Vec<_>>(), ["m1", "m2"]);
        let (oldest, next3) = messages::list(&conn, &conv, 2, next2.as_deref(), None).unwrap();
        assert_eq!(oldest.len(), 1);
        assert!(next3.is_none());
    }

    #[test]
    fn after_returns_newer_only() {
        let (conn, conv, _a) = fixture(3);
        let (items, _) = messages::list(&conn, &conv, 1, None, None).unwrap();
        let latest = &items[0];
        let (newer, next) = messages::list(&conn, &conv, 50, None, Some(&latest.id)).unwrap();
        assert!(newer.is_empty());
        assert!(next.is_none());
        // after 第一条 → 其余两条
        let (all, _) = messages::list(&conn, &conv, 50, None, None).unwrap();
        let (rest, _) = messages::list(&conn, &conv, 50, None, Some(&all[0].id)).unwrap();
        assert_eq!(rest.len(), 2);
    }
}
