//! /conversations/{id}/messages handlers：发送（JSON / multipart）与历史（keyset + wait 长轮询）

use axum::extract::{FromRequest, Multipart, Path, Query, State};
use axum::http::header;
use axum::Json;
use serde::Deserialize;

use super::extractors::AuthUser;
use crate::error::ApiError;
use crate::services::{files, messaging as svc};
use crate::state::AppState;
use folkmoot_common::errors::ErrorCode;
use folkmoot_common::{Message, MessagePage, SendMessageRequest};

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<u32>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub wait: Option<u64>,
}

pub async fn history(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<MessagePage>, ApiError> {
    let agent = auth.require_agent()?;
    let limit = q.limit.unwrap_or(50).min(200);
    let wait = q.wait.unwrap_or(0);
    if wait > 0 && q.after.is_none() {
        return Err(ApiError::validation("wait>0 requires after cursor"));
    }
    if wait > svc::MAX_WAIT_SECS {
        return Err(ApiError::validation(format!("wait must be <= {}", svc::MAX_WAIT_SECS)));
    }
    if wait > 0 {
        let page = svc::poll_new(
            &state.db,
            state.shutdown_rx(),
            &id,
            &agent.id,
            q.after.as_deref().unwrap(),
            wait,
            limit,
        )
        .await?;
        return Ok(Json(page));
    }
    let page = svc::history(&state.db, &id, &agent.id, limit, q.before, q.after).await?;
    Ok(Json(page))
}

/// 发送消息：application/json 或 multipart/form-data 双 Content-Type（§4.2）
pub async fn send(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> Result<Json<Message>, ApiError> {
    let agent = auth.require_agent()?.clone();
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.starts_with("application/json") {
        let body = axum::body::to_bytes(req.into_body(), svc::MAX_TEXT_BYTES + 1024)
            .await
            .map_err(|_| ApiError::new(ErrorCode::PayloadTooLarge, "body too large"))?;
        let payload: SendMessageRequest = serde_json::from_slice(&body)
            .map_err(|e| ApiError::validation(format!("invalid json: {e}")))?;
        let msg = svc::send_text(&state.db, &id, &agent.id, &payload.text).await?;
        return Ok(Json(msg));
    }

    if content_type.starts_with("multipart/form-data") {
        // Content-Length 预检（§4.3），超额 413 不落盘
        if let Some(len) = req
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            let hard = state.config.max_upload_bytes
                * state.config.max_attachments_per_message as u64
                + 64 * 1024;
            if len > hard {
                return Err(ApiError::new(ErrorCode::PayloadTooLarge, "upload too large"));
            }
        }
        let mp = Multipart::from_request(req, &state)
            .await
            .map_err(|e| ApiError::new(ErrorCode::UnsupportedMediaType, format!("multipart: {e}")))?;
        return send_multipart(state, agent, id, mp).await;
    }

    Err(ApiError::new(
        ErrorCode::UnsupportedMediaType,
        "expected application/json or multipart/form-data",
    ))
}

async fn send_multipart(
    state: AppState,
    agent: folkmoot_common::AgentInfo,
    conv_id: String,
    mut mp: Multipart,
) -> Result<Json<Message>, ApiError> {
    let max_bytes = state.config.max_upload_bytes;
    let max_files = state.config.max_attachments_per_message;

    let mut text: Option<String> = None;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| ApiError::new(ErrorCode::UnsupportedMediaType, format!("multipart: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "text" => {
                let v = field.text().await.map_err(|e| ApiError::validation(format!("text field: {e}")))?;
                if v.len() > svc::MAX_TEXT_BYTES {
                    return Err(ApiError::validation("text exceeds 8192 bytes"));
                }
                text = Some(v);
            }
            "file" => {
                if files.len() as u32 >= max_files {
                    return Err(ApiError::validation(format!(
                        "too many attachments (max {max_files})"
                    )));
                }
                let fname = files::sanitize_file_name(field.file_name().unwrap_or("file"));
                let data = field.bytes().await.map_err(|e| {
                    ApiError::new(ErrorCode::PayloadTooLarge, format!("file field: {e}"))
                })?;
                if data.len() as u64 > max_bytes {
                    return Err(ApiError::new(ErrorCode::PayloadTooLarge, "upload too large"));
                }
                files.push((fname, data.to_vec()));
            }
            _ => {}
        }
    }

    if files.is_empty() && text.as_deref().unwrap_or("").is_empty() {
        return Err(ApiError::validation("message must have text or at least one file"));
    }

    // 嗅探 + 内容寻址存储
    let mut blobs = Vec::with_capacity(files.len());
    for (fname, data) in &files {
        let blob = files::store_blob(&state.uploads_root, &state.config.allowed_image_types, data).await?;
        blobs.push((fname.clone(), blob));
    }

    let db = state.db.clone();
    let conv = conv_id.clone();
    let msg = db
        .run(move |conn| {
            if !crate::db::conversations::is_member(conn, &conv, &agent.id)? {
                return Err(ApiError::not_found("conversation not found"));
            }
            let tx = conn.unchecked_transaction()?;
            let (msg_id, ts) =
                crate::db::messages::insert(&tx, &conv, &agent.id, text.as_deref())?;
            for (fname, blob) in &blobs {
                crate::db::attachments::insert(
                    &tx, &msg_id, fname, &blob.content_type, blob.size_bytes, &blob.sha256,
                    &blob.storage_path,
                )?;
            }
            crate::db::conversations::touch_last_message(
                &tx,
                &conv,
                &crate::db::accounts::fmt_dt(&ts),
            )?;
            tx.commit()?;
            crate::db::messages::get(conn, &msg_id)?
                .ok_or_else(|| ApiError::internal("message vanished after insert"))
        })
        .await?;
    Ok(Json(msg))
}
