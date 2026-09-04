//! PRAGMA user_version 迁移 runner（§9.2）。schema 变更直接改初始 migration。

const SCHEMA_V1: &str = include_str!("../../migrations/0001_init.sql");

pub fn migrate(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    Ok(())
}
