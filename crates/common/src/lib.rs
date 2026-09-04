//! folkmoot 共享类型：REST API 的 DTO 与统一错误体。
//! server 与 CLI/web codegen 的唯一事实源。

pub mod errors;
pub mod protocol;

pub use errors::{ApiErrorBody, ErrorCode};
pub use protocol::*;
