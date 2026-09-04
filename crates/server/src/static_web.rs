//! 静态文件伺服：web_dist 磁盘伺服 + SPA fallback + index.html no-cache（§6.5）。
//! embed-web feature（rust-embed）在 M6 接入；此处先落地 disk 模式。

use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};

/// 尝试伺服 web_dist 下的静态资源；未命中回退 index.html（SPA history 路由）
pub async fn spa_fallback(State(state): State<crate::state::AppState>, uri: Uri) -> Response {
    let Some(dist) = state.config.web_dist.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let req_path = uri.path().trim_start_matches('/');
    // 防路径穿越：拒绝对 .. 的请求
    if req_path.split('/').any(|p| p == "..") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let candidate = dist.join(if req_path.is_empty() { "index.html" } else { req_path });
    match tokio::fs::metadata(&candidate).await {
        Ok(meta) if meta.is_file() => {
            match tokio::fs::read(&candidate).await {
                Ok(bytes) => serve_file(&candidate, bytes),
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
        _ => {
            let index = dist.join("index.html");
            match tokio::fs::read(&index).await {
                Ok(bytes) => (
                    [(header::CACHE_CONTROL, "no-cache")],
                    Html(String::from_utf8_lossy(&bytes).into_owned()),
                )
                    .into_response(),
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

fn serve_file(path: &std::path::Path, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // vite 产物带 hash 文件名 → 长缓存；其余短缓存
    let cache = if path
        .file_name()
        .map(|n| n.to_string_lossy().contains('-'))
        .unwrap_or(false)
        && path.to_string_lossy().contains("assets")
    {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        bytes,
    )
        .into_response()
}
