//! 账号注册、token 批量签发/补发/吊销、session 登录/登出（§4.2/§8）。

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::db::{accounts, agents, sessions, tokens, Db};
use crate::error::ApiError;
use folkmoot_common::errors::ErrorCode;
use folkmoot_common::{
    AccountSummary, IssuedToken, LoginResponse, RegisterResponse, RevokeTokenResponse, TokenInfo,
};

/// sha256 hex（token 与 session secret 共用此哈希模式）
pub fn hash_secret(secret: &str) -> String {
    let mut h = Sha256::new();
    h.update(secret.as_bytes());
    hex_lower(&h.finalize())
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 生成 opaque token：fm1_ + base64url(32B 随机)（§8.1）
pub fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::fill(&mut buf);
    format!(
        "fm1_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
    )
}

/// 生成 session secret（无前缀，仅存于 cookie）
pub fn generate_session_secret() -> String {
    let mut buf = [0u8; 32];
    rand::fill(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub async fn register(db: &Db, name: &str, token_count: u32) -> Result<RegisterResponse, ApiError> {
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err(ApiError::validation("name must be 1..=64 chars"));
    }
    if !(1..=tokens::MAX_ACTIVE_TOKENS).contains(&token_count) {
        return Err(ApiError::validation(format!(
            "token_count must be 1..={}",
            tokens::MAX_ACTIVE_TOKENS
        )));
    }
    db.run(move |conn| {
        let account = accounts::create(conn, &name)?;
        let mut issued = Vec::with_capacity(token_count as usize);
        for i in 0..token_count {
            let token = generate_token();
            let hash = hash_secret(&token);
            let label = if i == 0 { "default".to_string() } else { String::new() };
            let id = tokens::insert(conn, &account.id, &label, &hash)?;
            issued.push(IssuedToken { id, label, token });
        }
        Ok(RegisterResponse {
            account,
            tokens: issued,
        })
    })
    .await
}

pub async fn issue_token(
    db: &Db,
    account: &AccountSummary,
    label: String,
) -> Result<IssuedToken, ApiError> {
    let account_id = account.id.clone();
    db.run(move |conn| {
        if tokens::active_count(conn, &account_id)? >= tokens::MAX_ACTIVE_TOKENS {
            return Err(ApiError::conflict(format!(
                "active token limit reached ({})",
                tokens::MAX_ACTIVE_TOKENS
            )));
        }
        let token = generate_token();
        let hash = hash_secret(&token);
        let id = tokens::insert(conn, &account_id, &label, &hash)?;
        Ok(IssuedToken { id, label, token })
    })
    .await
}

pub async fn revoke_token(
    db: &Db,
    account: &AccountSummary,
    token_id: &str,
) -> Result<RevokeTokenResponse, ApiError> {
    let account_id = account.id.clone();
    let token_id = token_id.to_string();
    db.run(move |conn| {
        match tokens::revoke(conn, &account_id, &token_id)? {
            Some(cascaded) => Ok(RevokeTokenResponse {
                revoked: true,
                revoked_sessions: cascaded,
            }),
            None => Err(ApiError::not_found("token not found")),
        }
    })
    .await
}

pub async fn list_tokens(
    db: &Db,
    account: &AccountSummary,
    current_token_id: &str,
) -> Result<Vec<TokenInfo>, ApiError> {
    let account_id = account.id.clone();
    let current = current_token_id.to_string();
    db.run(move |conn| tokens::list_for_account(conn, &account_id, &current))
        .await
}

/// token 换发 session（web 登录）
pub async fn login(db: &Db, account: &AccountSummary, token_id: &str) -> Result<(String, LoginResponse), ApiError> {
    let account_id = account.id.clone();
    let token_id = token_id.to_string();
    let secret = generate_session_secret();
    let secret2 = secret.clone();
    let resp = db
        .run(move |conn| {
            sessions::create(conn, &account_id, &token_id, &secret2)?;
            let keys = agents::keys_for_account(conn, &account_id)?;
            let acc = accounts::get(conn, &account_id)?
                .ok_or_else(|| ApiError::new(ErrorCode::Internal, "account vanished"))?;
            Ok(LoginResponse {
                account: acc,
                agent_keys: keys,
            })
        })
        .await?;
    Ok((secret, resp))
}

pub async fn logout(db: &Db, session_id: Option<String>) -> Result<(), ApiError> {
    db.run(move |conn| {
        if let Some(id) = session_id {
            sessions::delete(conn, &id)?;
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_format() {
        let t = generate_token();
        assert!(t.starts_with("fm1_"));
        assert_eq!(t.len(), 4 + 43);
        assert_eq!(hash_secret(&t).len(), 64);
    }
}
