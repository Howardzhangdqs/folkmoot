//! sessions repo：web 登录换发，7d 滑动续期，上限 20，惰性清理（§3.2/§6.4）。

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::accounts::{fmt_dt, parse_dt};
use super::{new_id, now};
use crate::error::ApiError;

pub const SESSION_TTL_DAYS: i64 = 7;
pub const SESSION_RENEW_BELOW_DAYS: i64 = 3; // 剩余 <3.5d 顺延（取整 3 天 + 12h 判定见 touch）
pub const MAX_SESSIONS_PER_ACCOUNT: u32 = 20;

pub struct SessionRow {
    pub id: String,
    pub account_id: String,
    pub created_by_token_id: String,
    pub expires_at: DateTime<Utc>,
}

/// 新建 session 并惰性清理过期行 + 上限逐出最旧；返回 (session_id, secret)
pub fn create(conn: &Connection, account_id: &str, token_id: &str, secret: &str) -> Result<String, ApiError> {
    let now = now();
    // 惰性清理过期
    conn.execute(
        "DELETE FROM sessions WHERE expires_at < ?1",
        params![fmt_dt(&now)],
    )?;
    // 上限：超出逐出最旧
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE account_id = ?1",
        params![account_id],
        |r| r.get(0),
    )?;
    if count >= MAX_SESSIONS_PER_ACCOUNT {
        conn.execute(
            "DELETE FROM sessions WHERE id IN (
               SELECT id FROM sessions WHERE account_id = ?1 ORDER BY created_at ASC
               LIMIT ?2)",
            params![account_id, (count - MAX_SESSIONS_PER_ACCOUNT + 1) as i64],
        )?;
    }
    let id = new_id();
    let hash = super::super::services::auth::hash_secret(secret);
    conn.execute(
        "INSERT INTO sessions (id, account_id, session_hash, created_by_token_id, created_at, expires_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?5)",
        params![
            id,
            account_id,
            hash,
            token_id,
            fmt_dt(&now),
            fmt_dt(&(now + Duration::days(SESSION_TTL_DAYS)))
        ],
    )?;
    Ok(id)
}

/// 按 secret hash 查找未过期 session
pub fn find_by_secret(conn: &Connection, secret: &str) -> Result<Option<SessionRow>, ApiError> {
    let hash = super::super::services::auth::hash_secret(secret);
    let row = conn
        .query_row(
            "SELECT id, account_id, created_by_token_id, expires_at FROM sessions
             WHERE session_hash = ?1 AND expires_at >= ?2",
            params![hash, fmt_dt(&now())],
            |r| {
                let expires: String = r.get(3)?;
                Ok(SessionRow {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    created_by_token_id: r.get(2)?,
                    expires_at: parse_dt(&expires),
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// 滑动续期：剩余 <3.5d 时顺延 7d；同时节流更新 last_seen_at。返回是否续期（需重发 cookie）
pub fn touch(conn: &Connection, session: &SessionRow) -> Result<bool, ApiError> {
    let now = now();
    conn.execute(
        "UPDATE sessions SET last_seen_at = ?1 WHERE id = ?2 AND last_seen_at < ?3",
        params![fmt_dt(&now), session.id, fmt_dt(&(now - Duration::seconds(60)))],
    )?;
    let renew = session.expires_at - now < Duration::hours(84); // 3.5d
    if renew {
        conn.execute(
            "UPDATE sessions SET expires_at = ?1 WHERE id = ?2",
            params![fmt_dt(&(now + Duration::days(SESSION_TTL_DAYS))), session.id],
        )?;
    }
    Ok(renew)
}

pub fn delete(conn: &Connection, id: &str) -> Result<(), ApiError> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
    Ok(())
}
