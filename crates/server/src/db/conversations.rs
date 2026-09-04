//! conversations / participants repo

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::accounts::{fmt_dt, parse_dt};
use super::{new_id, now};
use crate::error::ApiError;
use folkmoot_common::{ConversationKind, ConversationSummary, ParticipantInfo};

pub struct ConversationRow {
    pub id: String,
    pub kind: ConversationKind,
    pub title: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
}

fn row_to_conv(row: &rusqlite::Row) -> rusqlite::Result<ConversationRow> {
    let kind: String = row.get(1)?;
    let created: String = row.get(4)?;
    let last_msg: String = row.get(5)?;
    Ok(ConversationRow {
        id: row.get(0)?,
        kind: if kind == "dm" {
            ConversationKind::Dm
        } else {
            ConversationKind::Group
        },
        title: row.get(2)?,
        created_by: row.get(3)?,
        created_at: parse_dt(&created),
        last_message_at: parse_dt(&last_msg),
    })
}

const COLS: &str = "id, kind, title, created_by, created_at, last_message_at";

/// 创建会话并批量写入 participants（调用方负责事务）
pub fn create(
    conn: &Connection,
    kind: ConversationKind,
    title: Option<&str>,
    dm_key: Option<&str>,
    created_by: &str,
    member_ids: &[String],
) -> Result<ConversationRow, ApiError> {
    let id = new_id();
    let ts = fmt_dt(&now());
    let kind_str = match kind {
        ConversationKind::Dm => "dm",
        ConversationKind::Group => "group",
    };
    conn.execute(
        "INSERT INTO conversations (id, kind, title, dm_key, created_by, created_at, last_message_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, kind_str, title, dm_key, created_by, ts],
    )?;
    for m in member_ids {
        add_participant(conn, &id, m)?;
    }
    Ok(get(conn, &id)?.expect("just inserted"))
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<ConversationRow>, ApiError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM conversations WHERE id = ?1"),
            params![id],
            row_to_conv,
        )
        .optional()?)
}

pub fn find_dm(conn: &Connection, dm_key: &str) -> Result<Option<ConversationRow>, ApiError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM conversations WHERE dm_key = ?1"),
            params![dm_key],
            row_to_conv,
        )
        .optional()?)
}

/// 幂等添加成员（重复忽略）
pub fn add_participant(conn: &Connection, conv_id: &str, agent_id: &str) -> Result<(), ApiError> {
    conn.execute(
        "INSERT OR IGNORE INTO participants (conversation_id, agent_id, joined_at) VALUES (?1, ?2, ?3)",
        params![conv_id, agent_id, fmt_dt(&now())],
    )?;
    Ok(())
}

pub fn remove_participant(conn: &Connection, conv_id: &str, agent_id: &str) -> Result<bool, ApiError> {
    let n = conn.execute(
        "DELETE FROM participants WHERE conversation_id = ?1 AND agent_id = ?2",
        params![conv_id, agent_id],
    )?;
    Ok(n > 0)
}

pub fn is_member(conn: &Connection, conv_id: &str, agent_id: &str) -> Result<bool, ApiError> {
    let n: u32 = conn.query_row(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ?1 AND agent_id = ?2",
        params![conv_id, agent_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

pub fn members(conn: &Connection, conv_id: &str) -> Result<Vec<ParticipantInfo>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT p.agent_id, a.agent_key, a.account_id, p.joined_at
         FROM participants p JOIN agents a ON a.id = p.agent_id
         WHERE p.conversation_id = ?1 ORDER BY p.joined_at ASC",
    )?;
    let rows = stmt
        .query_map(params![conv_id], |r| {
            let joined: String = r.get(3)?;
            Ok(ParticipantInfo {
                agent_id: r.get(0)?,
                agent_key: Some(r.get(1)?),
                joined_at: parse_dt(&joined),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 我的会话列表，按 last_message_at DESC
pub fn list_for_agent(
    conn: &Connection,
    agent_id: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<ConversationSummary>, ApiError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT {COLS} FROM conversations c
             JOIN participants p ON p.conversation_id = c.id
             WHERE p.agent_id = ?1
             ORDER BY c.last_message_at DESC LIMIT ?2 OFFSET ?3"
        ),
    )?;
    let convs = stmt
        .query_map(params![agent_id, limit, offset], row_to_conv)?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(convs.len());
    for c in convs {
        let ms = members(conn, &c.id)?;
        out.push(ConversationSummary {
            id: c.id,
            kind: c.kind,
            title: c.title,
            created_by: c.created_by,
            created_at: c.created_at,
            last_message_at: c.last_message_at,
            members: ms,
        });
    }
    Ok(out)
}

pub fn touch_last_message(conn: &Connection, conv_id: &str, at: &str) -> Result<(), ApiError> {
    conn.execute(
        "UPDATE conversations SET last_message_at = ?1 WHERE id = ?2",
        params![at, conv_id],
    )?;
    Ok(())
}
