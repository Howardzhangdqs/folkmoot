//! /auth/* handlers：register / login / logout / tokens

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::extractors::{
    auth_from, build_session_cookie, clear_session_cookie, parse_session_cookie, AuthUser,
    SESSION_COOKIE,
};
use crate::db::sessions;
use crate::error::ApiError;
use crate::services::auth as svc;
use crate::state::AppState;
use folkmoot_common::{
    IssueTokenRequest, IssuedToken, RegisterRequest, RegisterResponse, RevokeTokenResponse,
    TokenInfo,
};

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Response, ApiError> {
    if !state.config.allow_registration {
        return Err(ApiError::forbidden("registration is disabled"));
    }
    let resp = svc::register(&state.db, &req.name, req.token_count.unwrap_or(3)).await?;
    Ok((StatusCode::CREATED, Json(resp)).into_response())
}

/// token 换发 session（web 登录）。该请求本身用 Bearer。
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
        .ok_or_else(|| ApiError::unauthorized("login requires Bearer token"))?;

    let state2 = state.clone();
    let (account, token_id) = state
        .db
        .run(move |conn| {
            let row = crate::db::tokens::find_by_hash(conn, &svc::hash_secret(&token))?
                .ok_or_else(|| ApiError::unauthorized("invalid token"))?;
            crate::db::tokens::touch(conn, &row.id, None)?;
            let acc = crate::db::accounts::get(conn, &row.account_id)?
                .ok_or_else(|| ApiError::unauthorized("account not found"))?;
            Ok((acc, row.id))
        })
        .await?;

    let (secret, resp) = svc::login(&state2.db, &account, &token_id).await?;
    let cookie = build_session_cookie(
        &secret,
        (sessions::SESSION_TTL_DAYS * 86400) as u64,
        state.config.secure_cookies,
    );
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(resp),
    )
        .into_response())
}

/// 登出：删除当前 session 行 + 过期 Set-Cookie；幂等 200
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if headers.contains_key(header::AUTHORIZATION) && parse_session_cookie(&headers).is_none() {
        return Err(ApiError::validation("logout is a cookie-channel operation"));
    }
    let session_id = if let Some(secret) = parse_session_cookie(&headers) {
        state
            .db
            .run(move |conn| {
                Ok(sessions::find_by_secret(conn, &secret)?.map(|s| s.id))
            })
            .await?
    } else {
        None
    };
    svc::logout(&state.db, session_id).await?;
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, clear_session_cookie())],
        Json(serde_json::json!({"ok": true})),
    )
        .into_response())
}

pub async fn list_tokens(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<TokenInfo>>, ApiError> {
    let list = svc::list_tokens(&state.db, &auth.account, &auth.token_id).await?;
    Ok(Json(list))
}

pub async fn issue_token(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<IssueTokenRequest>,
) -> Result<(StatusCode, Json<IssuedToken>), ApiError> {
    let issued = svc::issue_token(&state.db, &auth.account, req.label).await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<RevokeTokenResponse>, ApiError> {
    let resp = svc::revoke_token(&state.db, &auth.account, &id).await?;
    Ok(Json(resp))
}
