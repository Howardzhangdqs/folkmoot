//! /attachments handlers：元数据 + 流式下载（§4.4）

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::extractors::AuthUser;
use crate::error::ApiError;
use crate::services::files;
use crate::state::AppState;
use folkmoot_common::AttachmentInfo;

/// 成员资格校验 + 取附件行
async fn load_for_member(
    state: &AppState,
    attachment_id: &str,
    agent_id: &str,
) -> Result<crate::db::attachments::AttachmentRow, ApiError> {
    let att_id = attachment_id.to_string();
    let agent_id = agent_id.to_string();
    state
        .db
        .run(move |conn| {
            let conv_id = crate::db::attachments::conversation_of(conn, &att_id)?
                .ok_or_else(|| ApiError::not_found("attachment not found"))?;
            if !crate::db::conversations::is_member(conn, &conv_id, &agent_id)? {
                return Err(ApiError::not_found("attachment not found"));
            }
            crate::db::attachments::get(conn, &att_id)?
                .ok_or_else(|| ApiError::not_found("attachment not found"))
        })
        .await
}

pub async fn meta(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<AttachmentInfo>, ApiError> {
    let agent = auth.require_agent()?;
    let row = load_for_member(&state, &id, &agent.id).await?;
    Ok(Json(row.info))
}

pub async fn download(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let agent = auth.require_agent()?;
    let row = load_for_member(&state, &id, &agent.id).await?;
    let path = files::absolute_path(&state.uploads_root, &row.storage_path)?;

    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::not_found("attachment blob missing"))?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    // Content-Disposition: ascii fallback + RFC 5987 UTF-8 原名（§4.4）
    let fname = files::sanitize_file_name(&row.info.file_name);
    let ascii_fallback: String = fname
        .chars()
        .map(|c| if c.is_ascii() && c != '"' { c } else { '_' })
        .collect();
    let encoded = urlencoded(&fname);
    let disposition = format!(
        "attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{encoded}"
    );

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, row.info.content_type.clone()),
            (
                header::CONTENT_DISPOSITION,
                disposition,
            ),
            (header::CONTENT_LENGTH, row.info.size_bytes.to_string()),
        ],
        body,
    )
        .into_response())
}

fn urlencoded(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
