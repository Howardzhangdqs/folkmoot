//! agents repo：X-Agent-Key 即时 upsert（§3.2）。

use rusqlite::{params, Connection, OptionalExtension};

use super::accounts::{fmt_dt, parse_dt};
use super::{new_id, now};
use crate::error::ApiError;
use folkmoot_common::AgentInfo;

fn row_to_agent(row: &rusqlite::Row) -> rusqlite::Result<AgentInfo> {
    let created: String = row.get(4)?;
    let last_seen: String = row.get(5)?;
    Ok(AgentInfo {
        id: row.get(0)?,
        agent_key: Some(row.get(1)?),
        account_id: row.get(2)?,
        display_name: row.get(3)?,
        created_at: parse_dt(&created),
        last_seen_at: parse_dt(&last_seen),
    })
}

const COLS: &str = "id, agent_key, account_id, display_name, created_at, last_seen_at";

/// 账号内按 key upsert：不存在即创建；存在则节流刷新 last_seen_at。同 key = 同一 agent。
pub fn upsert(conn: &Connection, account_id: &str, key: &str) -> Result<AgentInfo, ApiError> {
    let existing: Option<AgentInfo> = conn
        .query_row(
            &format!("SELECT {COLS} FROM agents WHERE account_id = ?1 AND agent_key = ?2"),
            params![account_id, key],
            row_to_agent,
        )
        .optional()?;
    if let Some(agent) = existing {
        let now = now();
        conn.execute(
            "UPDATE agents SET last_seen_at = ?1 WHERE id = ?2 AND last_seen_at < ?3",
            params![fmt_dt(&now), agent.id, fmt_dt(&(now - chrono::Duration::seconds(60)))],
        )?;
        return Ok(agent);
    }
    let id = new_id();
    let ts = fmt_dt(&now());
    conn.execute(
        "INSERT INTO agents (id, account_id, agent_key, created_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, account_id, key, ts],
    )?;
    Ok(AgentInfo {
        id,
        agent_key: Some(key.to_string()),
        account_id: account_id.to_string(),
        display_name: None,
        created_at: parse_dt(&ts),
        last_seen_at: parse_dt(&ts),
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<AgentInfo>, ApiError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM agents WHERE id = ?1"),
            params![id],
            row_to_agent,
        )
        .optional()?)
}

/// scope=account：本账号 agent；scope=all：全量（跨账号 DM 寻址）
pub fn list(
    conn: &Connection,
    account_id: &str,
    scope_all: bool,
    limit: u32,
    offset: u32,
) -> Result<(Vec<AgentInfo>, u64), ApiError> {
    let (where_sql, count_args, query_args): (&str, Vec<String>, Vec<String>) = if scope_all {
        ("", vec![], vec![])
    } else {
        ("WHERE account_id = ?1", vec![account_id.to_string()], vec![account_id.to_string()])
    };
    let total: u64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM agents {where_sql}"),
        rusqlite::params_from_iter(count_args),
        |r| r.get(0),
    )?;
    let li = query_args.len() + 1;
    let q = format!(
        "SELECT {COLS} FROM agents {where_sql} ORDER BY created_at ASC LIMIT ?{li} OFFSET ?{}",
        li + 1
    );
    let mut stmt = conn.prepare(&q)?;
    let mut args = query_args;
    args.push(limit.to_string());
    args.push(offset.to_string());
    let items = stmt
        .query_map(rusqlite::params_from_iter(args), row_to_agent)?
        .collect::<Result<Vec<_>, _>>()?;
    // scope=all 时隐藏跨账号的 agent_key
    let items = items
        .into_iter()
        .map(|mut a| {
            if scope_all && a.account_id != account_id {
                a.agent_key = None;
            }
            a
        })
        .collect();
    Ok((items, total))
}

pub fn keys_for_account(conn: &Connection, account_id: &str) -> Result<Vec<String>, ApiError> {
    let mut stmt = conn.prepare("SELECT agent_key FROM agents WHERE account_id = ?1 ORDER BY created_at ASC")?;
    let keys = stmt
        .query_map(params![account_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(keys)
}

/// 解析 agent 引用：本账号可用 key，跨账号用 UUID
pub fn resolve_ref(conn: &Connection, account_id: &str, reference: &str) -> Result<Option<AgentInfo>, ApiError> {
    if let Some(a) = get(conn, reference)? {
        return Ok(Some(a));
    }
    let key = folkmoot_common::normalize_agent_key(reference)
        .map_err(ApiError::validation)?;
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM agents WHERE account_id = ?1 AND agent_key = ?2"),
            params![account_id, key],
            row_to_agent,
        )
        .optional()?)
}
