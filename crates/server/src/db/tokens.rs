//! tokens repo：明文绝不落库，只存 sha256 hex（§8.1）。

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::accounts::{fmt_dt, parse_dt};
use super::{new_id, now};
use crate::error::ApiError;
use folkmoot_common::TokenInfo;

pub const MAX_ACTIVE_TOKENS: u32 = 5;
/// last_used_at 节流窗口（秒）
const LAST_USED_THROTTLE_SECS: i64 = 60;

pub struct TokenRow {
    pub id: String,
    pub account_id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_user_agent: Option<String>,
}

fn row_to_token(row: &rusqlite::Row) -> rusqlite::Result<TokenRow> {
    let created: String = row.get(3)?;
    let last_used: Option<String> = row.get(4)?;
    Ok(TokenRow {
        id: row.get(0)?,
        account_id: row.get(1)?,
        label: row.get(2)?,
        created_at: parse_dt(&created),
        last_used_at: last_used.as_deref().map(parse_dt),
        last_user_agent: row.get(5)?,
    })
}

const TOKEN_COLS: &str = "id, account_id, label, created_at, last_used_at, last_user_agent";

pub fn insert(conn: &Connection, account_id: &str, label: &str, token_hash: &str) -> Result<String, ApiError> {
    let id = new_id();
    conn.execute(
        "INSERT INTO tokens (id, account_id, label, token_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, account_id, label, token_hash, fmt_dt(&now())],
    )?;
    Ok(id)
}

pub fn active_count(conn: &Connection, account_id: &str) -> Result<u32, ApiError> {
    let n: u32 = conn.query_row(
        "SELECT COUNT(*) FROM tokens WHERE account_id = ?1",
        params![account_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// 按 hash 解析 token → row（鉴权主路径，一次索引查找）
pub fn find_by_hash(conn: &Connection, token_hash: &str) -> Result<Option<TokenRow>, ApiError> {
    let q = format!("SELECT {TOKEN_COLS} FROM tokens WHERE token_hash = ?1");
    Ok(conn
        .query_row(&q, params![token_hash], row_to_token)
        .optional()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<TokenRow>, ApiError> {
    let q = format!("SELECT {TOKEN_COLS} FROM tokens WHERE id = ?1");
    Ok(conn.query_row(&q, params![id], row_to_token).optional()?)
}

pub fn list_for_account(conn: &Connection, account_id: &str, current_id: &str) -> Result<Vec<TokenInfo>, ApiError> {
    let q = format!("SELECT {TOKEN_COLS} FROM tokens WHERE account_id = ?1 ORDER BY created_at ASC");
    let mut stmt = conn.prepare(&q)?;
    let rows = stmt
        .query_map(params![account_id], row_to_token)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|t| TokenInfo {
            current: t.id == current_id,
            id: t.id,
            label: t.label,
            created_at: t.created_at,
            last_used_at: t.last_used_at,
            last_user_agent: t.last_user_agent,
        })
        .collect())
}

/// 节流更新 last_used_at / last_user_agent（距上次 >60s 才写，§8.13）
pub fn touch(conn: &Connection, id: &str, user_agent: Option<&str>) -> Result<(), ApiError> {
    let ua = user_agent.map(|s| s.chars().take(256).collect::<String>());
    conn.execute(
        "UPDATE tokens SET last_used_at = ?1, last_user_agent = COALESCE(?3, last_user_agent)
         WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at < ?4)",
        params![
            fmt_dt(&now()),
            id,
            ua,
            fmt_dt(&(now() - chrono::Duration::seconds(LAST_USED_THROTTLE_SECS)))
        ],
    )?;
    Ok(())
}

/// 吊销 token；级联删 session 由 FK ON DELETE CASCADE 完成，返回级联数
pub fn revoke(conn: &Connection, account_id: &str, token_id: &str) -> Result<Option<u64>, ApiError> {
    let cascaded: u64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE created_by_token_id = ?1",
        params![token_id],
        |r| r.get(0),
    )?;
    let n = conn.execute(
        "DELETE FROM tokens WHERE id = ?1 AND account_id = ?2",
        params![token_id, account_id],
    )?;
    Ok((n > 0).then_some(cascaded))
}
