//! accounts repo

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{new_id, now};
use crate::error::ApiError;
use folkmoot_common::AccountSummary;

fn row_to_account(row: &rusqlite::Row) -> rusqlite::Result<AccountSummary> {
    let created: String = row.get(2)?;
    Ok(AccountSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: parse_dt(&created),
    })
}

pub fn parse_dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

pub fn fmt_dt(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 创建账号；name 冲突返回 409
pub fn create(conn: &Connection, name: &str) -> Result<AccountSummary, ApiError> {
    let id = new_id();
    let created = fmt_dt(&now());
    conn.execute(
        "INSERT INTO accounts (id, name, created_at) VALUES (?1, ?2, ?3)",
        params![id, name, created],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            ApiError::conflict("account name already taken")
        }
        other => ApiError::from(other),
    })?;
    Ok(AccountSummary {
        id,
        name: name.to_string(),
        created_at: parse_dt(&created),
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<AccountSummary>, ApiError> {
    let acc = conn
        .query_row(
            "SELECT id, name, created_at FROM accounts WHERE id = ?1",
            params![id],
            row_to_account,
        )
        .optional()?;
    Ok(acc)
}
