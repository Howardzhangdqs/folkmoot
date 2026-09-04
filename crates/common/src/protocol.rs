//! REST API DTO（§3/§4）。成功响应直接返回资源 DTO，不包 envelope。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ---------- auth / accounts ----------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub name: String,
    /// 批量签发 token 数，1..=5，默认 3
    #[serde(default)]
    pub token_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterResponse {
    pub account: AccountSummary,
    /// 明文 token 仅此一次返回
    pub tokens: Vec<IssuedToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountSummary {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IssuedToken {
    pub id: String,
    pub label: String,
    /// 明文 token（fm1_...），仅签发响应可见
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenInfo {
    pub id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_user_agent: Option<String>,
    /// 是否为本次请求鉴权所用的 token（cookie 通道 = 当前 session 来源 token）
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IssueTokenRequest {
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevokeTokenResponse {
    pub revoked: bool,
    /// 级联吊销的 session 数
    pub revoked_sessions: u64,
}

/// 登录响应 / GET /me 无 key 形态：账号摘要 + 既有 agent key 列表
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub account: AccountSummary,
    pub agent_keys: Vec<String>,
}

// ---------- agents ----------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgentInfo {
    pub id: String,
    /// 本账号可见；scope=all 时跨账号为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_key: Option<String>,
    pub account_id: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// GET /me 带 X-Agent-Key 的形态
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MeResponse {
    pub account: AccountSummary,
    pub agent: AgentInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgentListResponse {
    pub items: Vec<AgentInfo>,
    pub total: u64,
}

// ---------- conversations ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConversationKind {
    Dm,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CreateConversationRequest {
    Dm { peer: String },
    Group {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        members: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ParticipantInfo {
    pub agent_id: String,
    pub agent_key: Option<String>,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConversationSummary {
    pub id: String,
    pub kind: ConversationKind,
    pub title: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
    pub members: Vec<ParticipantInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConversationListResponse {
    pub items: Vec<ConversationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddMembersRequest {
    pub agent_ids: Vec<String>,
}

// ---------- messages ----------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SendMessageRequest {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AttachmentInfo {
    pub id: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub sender_key: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<AttachmentInfo>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MessagePage {
    pub items: Vec<Message>,
    /// 本页最旧一条的 id；null 表示到底
    pub next_cursor: Option<String>,
}

// ---------- misc ----------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// 归一化 agent key：转小写，校验 charset `^[a-z0-9][a-z0-9-]{0,31}$`
pub fn normalize_agent_key(raw: &str) -> Result<String, &'static str> {
    let key = raw.trim().to_lowercase();
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return Err("agent key must be 1..=32 chars");
    }
    if !bytes[0].is_ascii_alphanumeric() {
        return Err("agent key must start with [a-z0-9]");
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
    {
        return Err("agent key charset: [a-z0-9-]");
    }
    Ok(key)
}

/// DM 规范化键：min(a,b):max(b)
pub fn dm_key(a: &str, b: &str) -> String {
    if a < b {
        format!("{a}:{b}")
    } else {
        format!("{b}:{a}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_key_rules() {
        assert_eq!(normalize_agent_key("Ops-1").unwrap(), "ops-1");
        assert!(normalize_agent_key("").is_err());
        assert!(normalize_agent_key("-abc").is_err());
        assert!(normalize_agent_key("a b").is_err());
        assert!(normalize_agent_key(&"a".repeat(33)).is_err());
        assert!(normalize_agent_key("x7k2q9zz").is_ok());
    }

    #[test]
    fn dm_key_is_order_independent() {
        assert_eq!(dm_key("b", "a"), dm_key("a", "b"));
        assert_eq!(dm_key("a", "b"), "a:b");
    }

    #[test]
    fn dto_serde_roundtrip() {
        let m = Message {
            id: "0198abcd".into(),
            conversation_id: "c1".into(),
            sender_id: "s1".into(),
            sender_key: Some("ops".into()),
            text: Some("hi".into()),
            attachments: vec![],
            created_at: Utc::now(),
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, m.id);
    }
}
