//! axum State（§2.2）。

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;
use crate::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub uploads_root: PathBuf,
    /// 优雅退出广播：长轮询循环据此中断（≤2s）
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl AppState {
    pub fn shutdown_rx(&self) -> tokio::sync::watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    pub fn is_shutting_down(&self) -> bool {
        *self.shutdown_tx.borrow()
    }
}
