//! attachments repo（元数据；blob 在文件系统，§7）

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::accounts::{fmt_dt, parse_dt};
use super::{new_id, now};
use crate::error::ApiError;
use folkmoot_common::AttachmentInfo;

pub struct AttachmentRow {
    pub info: AttachmentInfo,
    pub message_id: String,
    pub storage_path: String,
    pub created_at: DateTime<Utc>,
}

pub fn insert(
    conn: &Connection,
    message_id: &str,
    file_name: &str,
    content_type: &str,
    size_bytes: u64,
    sha256: &str,
    storage_path: &str,
) -> Result<String, ApiError> {
    let id = new_id();
    conn.execute(
        "INSERT INTO attachments (id, message_id, file_name, content_type, size_bytes, sha256, storage_path, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, message_id, file_name, content_type, size_bytes as i64, sha256, storage_path, fmt_dt(&now())],
    )?;
    Ok(id)
}

pub fn list_for_message(conn: &Connection, message_id: &str) -> Result<Vec<AttachmentInfo>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, content_type, size_bytes, sha256 FROM attachments WHERE message_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![message_id], |r| {
            Ok(AttachmentInfo {
                id: r.get(0)?,
                file_name: r.get(1)?,
                content_type: r.get(2)?,
                size_bytes: r.get::<_, i64>(3)? as u64,
                sha256: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<AttachmentRow>, ApiError> {
    Ok(conn
        .query_row(
            "SELECT id, message_id, file_name, content_type, size_bytes, sha256, storage_path, created_at
             FROM attachments WHERE id = ?1",
            params![id],
            |r| {
                let created: String = r.get(7)?;
                Ok(AttachmentRow {
                    info: AttachmentInfo {
                        id: r.get(0)?,
                        file_name: r.get(2)?,
                        content_type: r.get(3)?,
                        size_bytes: r.get::<_, i64>(4)? as u64,
                        sha256: r.get(5)?,
                    },
                    message_id: r.get(1)?,
                    storage_path: r.get(6)?,
                    created_at: parse_dt(&created),
                })
            },
        )
        .optional()?)
}

/// 附件所属会话 id（成员资格校验用）
pub fn conversation_of(conn: &Connection, attachment_id: &str) -> Result<Option<String>, ApiError> {
    Ok(conn
        .query_row(
            "SELECT m.conversation_id FROM attachments a JOIN messages m ON m.id = a.message_id WHERE a.id = ?1",
            params![attachment_id],
            |r| r.get(0),
        )
        .optional()?)
}
