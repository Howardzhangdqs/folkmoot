//! /conversations handlers：创建（DM 幂等 / group）、列表、详情、成员增删、自退

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::extractors::AuthUser;
use crate::error::ApiError;
use crate::services::messaging as svc;
use crate::state::AppState;
use folkmoot_common::{
    AddMembersRequest, ConversationListResponse, ConversationSummary, CreateConversationRequest,
};

pub async fn create(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<CreateConversationRequest>,
) -> Result<(StatusCode, Json<ConversationSummary>), ApiError> {
    let agent = auth.require_agent()?;
    let (conv, created) =
        svc::create_conversation(&state.db, &auth.account.id, &agent.id, req).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(conv)))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<ConversationListResponse>, ApiError> {
    let agent = auth.require_agent()?;
    let agent_id = agent.id.clone();
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let items = state
        .db
        .run(move |conn| crate::db::conversations::list_for_agent(conn, &agent_id, limit, offset))
        .await?;
    Ok(Json(ConversationListResponse { items }))
}

pub async fn detail(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ConversationSummary>, ApiError> {
    let agent = auth.require_agent()?;
    let agent_id = agent.id.clone();
    let conv = state
        .db
        .run(move |conn| {
            if !crate::db::conversations::is_member(conn, &id, &agent_id)? {
                return Err(ApiError::not_found("conversation not found"));
            }
            let c = crate::db::conversations::get(conn, &id)?
                .ok_or_else(|| ApiError::not_found("conversation not found"))?;
            let ms = crate::db::conversations::members(conn, &c.id)?;
            Ok(ConversationSummary {
                id: c.id,
                kind: c.kind,
                title: c.title,
                created_by: c.created_by,
                created_at: c.created_at,
                last_message_at: c.last_message_at,
                members: ms,
            })
        })
        .await?;
    Ok(Json(conv))
}

pub async fn add_members(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<AddMembersRequest>,
) -> Result<Json<ConversationSummary>, ApiError> {
    let agent = auth.require_agent()?;
    let account_id = auth.account.id.clone();
    let agent_id = agent.id.clone();
    let conv = state
        .db
        .run(move |conn| {
            let c = crate::db::conversations::get(conn, &id)?
                .filter(|_| crate::db::conversations::is_member(conn, &id, &agent_id).unwrap_or(false))
                .ok_or_else(|| ApiError::not_found("conversation not found"))?;
            if c.kind != folkmoot_common::ConversationKind::Group {
                return Err(ApiError::validation("members can only be added to group conversations"));
            }
            if c.created_by != agent_id {
                return Err(ApiError::forbidden("only the creator can add members"));
            }
            for m in &req.agent_ids {
                let a = crate::db::agents::resolve_ref(conn, &account_id, m)?
                    .ok_or_else(|| ApiError::not_found(format!("agent not found: {m}")))?;
                crate::db::conversations::add_participant(conn, &id, &a.id)?; // 幂等
            }
            let ms = crate::db::conversations::members(conn, &id)?;
            Ok(ConversationSummary {
                id: c.id,
                kind: c.kind,
                title: c.title,
                created_by: c.created_by,
                created_at: c.created_at,
                last_message_at: c.last_message_at,
                members: ms,
            })
        })
        .await?;
    Ok(Json(conv))
}

pub async fn remove_member(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path((id, target)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let agent = auth.require_agent()?;
    let account_id = auth.account.id.clone();
    let agent_id = agent.id.clone();
    state
        .db
        .run(move |conn| {
            let c = crate::db::conversations::get(conn, &id)?
                .filter(|_| crate::db::conversations::is_member(conn, &id, &agent_id).unwrap_or(false))
                .ok_or_else(|| ApiError::not_found("conversation not found"))?;
            if c.kind != folkmoot_common::ConversationKind::Group {
                return Err(ApiError::validation("members can only be removed from group conversations"));
            }
            if c.created_by != agent_id {
                return Err(ApiError::forbidden("only the creator can remove members"));
            }
            let target_agent = crate::db::agents::resolve_ref(conn, &account_id, &target)?
                .or(crate::db::agents::get(conn, &target)?)
                .ok_or_else(|| ApiError::not_found("agent not found"))?;
            if target_agent.id == c.created_by {
                return Err(ApiError::validation("cannot remove the creator"));
            }
            if !crate::db::conversations::remove_participant(conn, &id, &target_agent.id)? {
                return Err(ApiError::not_found("member not found"));
            }
            Ok(())
        })
        .await?;
    Ok(StatusCode::OK)
}

pub async fn leave(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let agent = auth.require_agent()?;
    let agent_id = agent.id.clone();
    state
        .db
        .run(move |conn| {
            let c = crate::db::conversations::get(conn, &id)?
                .ok_or_else(|| ApiError::not_found("conversation not found"))?;
            if c.kind != folkmoot_common::ConversationKind::Group {
                return Err(ApiError::validation("cannot leave a DM"));
            }
            if c.created_by == agent_id {
                return Err(ApiError::validation(
                    "creator cannot leave (ownership must not be orphaned)",
                ));
            }
            if !crate::db::conversations::remove_participant(conn, &id, &agent_id)? {
                return Err(ApiError::not_found("not a member"));
            }
            Ok(())
        })
        .await?;
    Ok(StatusCode::OK)
}
