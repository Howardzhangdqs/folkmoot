//! 鉴权：双通道（Bearer token / folkmoot_session cookie）+ CSRF 规则（§4.1/§6.4）。
//! 实现为 axum middleware（from_fn_with_state），身份解析结果经 request extensions 传递；
//! 滑动续期需在响应上重发 Set-Cookie，故 middleware 比 extractor 更贴合。

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::db::{accounts, agents, sessions, tokens};
use crate::error::ApiError;
use crate::services::auth as auth_service;
use crate::state::AppState;
use folkmoot_common::errors::{ApiErrorBody, ErrorCode};
use folkmoot_common::{AccountSummary, AgentInfo};

pub const SESSION_COOKIE: &str = "folkmoot_session";

/// 已鉴权身份（注入 request extensions）
#[derive(Debug, Clone)]
pub struct Auth {
    pub account: AccountSummary,
    /// Bearer = token id；cookie = session 的 created_by_token_id
    pub token_id: String,
    pub session_id: Option<String>,
    pub via_cookie: bool,
    /// 请求携带 X-Agent-Key 时 upsert 出的当前 agent
    pub agent: Option<AgentInfo>,
}

impl Auth {
    /// 需要 agent 身份的 handler 调用
    pub fn require_agent(&self) -> Result<&AgentInfo, ApiError> {
        self.agent.as_ref().ok_or_else(|| {
            ApiError::validation("missing X-Agent-Key header")
        })
    }
}

/// 从 request extensions 取 Auth（handler 侧）
pub fn auth_from(parts: &axum::http::request::Parts) -> Result<Auth, ApiError> {
    parts
        .extensions
        .get::<Auth>()
        .cloned()
        .ok_or_else(|| ApiError::internal("auth extension missing"))
}

/// handler extractor：取 middleware 注入的 Auth
pub struct AuthUser(pub Auth);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Auth>()
            .cloned()
            .map(AuthUser)
            .ok_or_else(|| ApiError::unauthorized("missing credentials"))
    }
}

fn parse_bearer(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    v.strip_prefix("Bearer ").map(|s| s.trim().to_string())
}

pub fn parse_session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            return Some(v.to_string());
        }
    }
    None
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(256).collect())
}

pub fn build_session_cookie(secret: &str, max_age_secs: u64, secure: bool) -> String {
    let mut c = format!(
        "{SESSION_COOKIE}={secret}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age_secs}"
    );
    if secure {
        c.push_str("; Secure");
    }
    c
}

pub fn clear_session_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

fn err_response(code: ErrorCode, msg: &str) -> Response {
    let status = StatusCode::from_u16(code.http_status()).unwrap();
    (status, Json(ApiErrorBody::new(code, msg))).into_response()
}

/// 鉴权 middleware：挂于 /api/v1 受保护路由
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let headers = req.headers().clone();
    let method = req.method().clone();
    let agent_key_hdr = headers
        .get("x-agent-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let bearer = parse_bearer(&headers);
    let cookie_secret = parse_session_cookie(&headers);

    if bearer.is_none() && cookie_secret.is_none() {
        return err_response(ErrorCode::Unauthorized, "missing credentials");
    }

    // CSRF：cookie 通道的变更类请求必须携带 X-Agent-Key（§4.1/§6.4）
    let via_cookie = bearer.is_none();
    let is_safe = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);
    if via_cookie && !is_safe && agent_key_hdr.is_none() {
        return err_response(
            ErrorCode::CsrfRejected,
            "cookie-channel mutations require X-Agent-Key",
        );
    }

    let ua = user_agent(&headers);
    let result = state
        .db
        .run(move |conn| {
            let (account, token_id, session_id, renewed): (AccountSummary, String, Option<String>, bool) =
                if let Some(token) = bearer {
                    let row = tokens::find_by_hash(conn, &auth_service::hash_secret(&token))?
                        .ok_or_else(|| ApiError::unauthorized("invalid token"))?;
                    tokens::touch(conn, &row.id, ua.as_deref())?;
                    let acc = accounts::get(conn, &row.account_id)?
                        .ok_or_else(|| ApiError::unauthorized("account not found"))?;
                    (acc, row.id, None, false)
                } else {
                    let secret = cookie_secret.unwrap();
                    let sess = sessions::find_by_secret(conn, &secret)?
                        .ok_or_else(|| ApiError::unauthorized("invalid or expired session"))?;
                    let renewed = sessions::touch(conn, &sess)?;
                    let acc = accounts::get(conn, &sess.account_id)?
                        .ok_or_else(|| ApiError::unauthorized("account not found"))?;
                    (acc, sess.created_by_token_id, Some(sess.id), renewed)
                };

            let agent = match agent_key_hdr {
                Some(raw) => {
                    let key = folkmoot_common::normalize_agent_key(&raw)
                        .map_err(ApiError::validation)?;
                    Some(agents::upsert(conn, &account.id, &key)?)
                }
                None => None,
            };
            Ok((account, token_id, session_id, agent, renewed))
        })
        .await;

    let (account, token_id, session_id, agent, renewed) = match result {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };

    req.extensions_mut().insert(Auth {
        account,
        token_id,
        session_id,
        via_cookie,
        agent,
    });

    let mut resp = next.run(req).await;
    // 滑动续期：重发 cookie（§6.4）
    if renewed {
        if let Some(secret) = parse_session_cookie(&headers) {
            resp.headers_mut().insert(
                header::SET_COOKIE,
                build_session_cookie(
                    &secret,
                    (sessions::SESSION_TTL_DAYS * 86400) as u64,
                    state.config.secure_cookies,
                )
                .parse()
                .unwrap(),
            );
        }
    }
    resp
}
