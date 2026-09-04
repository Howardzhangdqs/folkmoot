//! /me 与 /agents handlers

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use super::extractors::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;
use folkmoot_common::{AgentListResponse, LoginResponse, MeResponse};

pub async fn me(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(agent) = &auth.agent {
        return Ok(Json(serde_json::to_value(MeResponse {
            account: auth.account.clone(),
            agent: agent.clone(),
        })
        .unwrap()));
    }
    // 无 key 形态：账号摘要 + 既有 agent key 列表
    let account_id = auth.account.id.clone();
    let keys = state
        .db
        .run(move |conn| crate::db::agents::keys_for_account(conn, &account_id))
        .await?;
    Ok(Json(serde_json::to_value(LoginResponse {
        account: auth.account.clone(),
        agent_keys: keys,
    })
    .unwrap()))
}

#[derive(Debug, Deserialize)]
pub struct AgentsQuery {
    pub scope: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn list_agents(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Query(q): Query<AgentsQuery>,
) -> Result<Json<AgentListResponse>, ApiError> {
    let scope_all = q.scope.as_deref() == Some("all");
    let account_id = auth.account.id.clone();
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let (items, total) = state
        .db
        .run(move |conn| crate::db::agents::list(conn, &account_id, scope_all, limit, offset))
        .await?;
    Ok(Json(AgentListResponse { items, total }))
}
