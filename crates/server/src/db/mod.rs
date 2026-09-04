//! db 层：单连接 Mutex<Connection> + spawn_blocking（§9.2 终审方案）。
//! 每连接统一 PRAGMA：WAL / synchronous=NORMAL / foreign_keys=ON / busy_timeout=5000。

pub mod accounts;
pub mod agents;
pub mod attachments;
pub mod conversations;
pub mod messages;
pub mod migrations;
pub mod sessions;
pub mod tokens;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};

use crate::error::ApiError;
use folkmoot_common::errors::ErrorCode;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("create data dir")?;
        }
        let conn = rusqlite::Connection::open(path)
            .with_context(|| format!("open sqlite at {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        migrations::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 内存数据库（测试用）
    pub fn in_memory() -> Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn lock(&self) -> MutexGuard<'_, rusqlite::Connection> {
        self.conn.lock().expect("db mutex poisoned")
    }

    /// 在 blocking 线程池上执行同步 DB 闭包
    pub async fn run<F, T>(&self, f: F) -> Result<T, ApiError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, ApiError> + Send + 'static,
        T: Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.lock();
            f(&conn)
        })
        .await
        .map_err(|e| ApiError::new(ErrorCode::Internal, format!("db task join: {e}")))?
    }
}

pub fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
