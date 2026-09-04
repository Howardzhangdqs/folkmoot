//! 文件服务：magic bytes 嗅探、内容寻址写入、流式读（§7）。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::ApiError;
use folkmoot_common::errors::ErrorCode;

/// magic bytes 嗅探（§7 表），不信任客户端声明
pub fn sniff_image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        Some("png")
    } else if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        Some("jpeg")
    } else if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.len() >= 2 && &bytes[..2] == b"BM" {
        Some("bmp")
    } else {
        None
    }
}

pub fn mime_of(kind: &str) -> &'static str {
    match kind {
        "png" => "image/png",
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// sha256 → 'ab/cd/<sha256>' 相对路径（§7 两级分桶）
pub fn storage_path_for(sha256: &str) -> String {
    format!("{}/{}/{}", &sha256[0..2], &sha256[2..4], sha256)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    super::auth::hex_lower(&h.finalize())
}

pub struct StoredBlob {
    pub sha256: String,
    pub storage_path: String,
    pub content_type: String,
    pub size_bytes: u64,
}

/// 内容寻址写入：先落 tmp 再 rename 至终路径；同哈希已存在则丢弃 tmp（天然去重，§7）
pub async fn store_blob(
    uploads_root: &Path,
    allowed_types: &[String],
    bytes: &[u8],
) -> Result<StoredBlob, ApiError> {
    let kind = sniff_image_type(bytes).ok_or_else(|| {
        ApiError::new(
            ErrorCode::UnsupportedMediaType,
            "unrecognized or unsupported image format",
        )
    })?;
    if !allowed_types.iter().any(|t| t == kind) {
        return Err(ApiError::new(
            ErrorCode::UnsupportedMediaType,
            format!("image type {kind} not allowed"),
        ));
    }
    let sha256 = sha256_hex(bytes);
    let rel = storage_path_for(&sha256);
    let final_path = uploads_root.join(&rel);

    if final_path.exists() {
        return Ok(StoredBlob {
            sha256,
            storage_path: rel,
            content_type: mime_of(kind).to_string(),
            size_bytes: bytes.len() as u64,
        });
    }

    let tmp_dir = uploads_root.join("tmp");
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .map_err(|e| ApiError::internal(format!("create tmp dir: {e}")))?;
    let tmp_path = tmp_dir.join(uuid::Uuid::now_v7().to_string());
    let write_result = async {
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        f.write_all(bytes).await?;
        f.flush().await?;
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // rename 已存在则去重（并发同哈希写入）
        if final_path.exists() {
            tokio::fs::remove_file(&tmp_path).await?;
        } else {
            tokio::fs::rename(&tmp_path, &final_path).await?;
        }
        Ok::<(), std::io::Error>(())
    }
    .await;
    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(ApiError::internal(format!("store blob: {e}")));
    }
    Ok(StoredBlob {
        sha256,
        storage_path: rel,
        content_type: mime_of(kind).to_string(),
        size_bytes: bytes.len() as u64,
    })
}

/// 绝对路径解析：storage_path 仅由服务端 sha256 生成（§8.3）
pub fn absolute_path(uploads_root: &Path, storage_path: &str) -> Result<PathBuf, ApiError> {
    // 防御性校验：必须是 ab/cd/<64hex> 形态
    let parts: Vec<&str> = storage_path.split('/').collect();
    let valid = parts.len() == 3
        && parts[0].len() == 2
        && parts[1].len() == 2
        && parts[2].len() == 64
        && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()));
    if !valid {
        return Err(ApiError::internal("invalid storage path"));
    }
    Ok(uploads_root.join(storage_path))
}

/// Content-Disposition filename sanitize：basename + 剥离路径分隔符（§4.4/§8.3）
pub fn sanitize_file_name(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("file")
        .trim_matches(|c: char| c.is_control() || c == '"');
    if base.is_empty() {
        "file".to_string()
    } else {
        base.chars().take(128).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_magic_bytes() {
        assert_eq!(sniff_image_type(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0]), Some("png"));
        assert_eq!(sniff_image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpeg"));
        assert_eq!(sniff_image_type(b"GIF89a..."), Some("gif"));
        assert_eq!(sniff_image_type(b"RIFF\x00\x00\x00\x00WEBP"), Some("webp"));
        assert_eq!(sniff_image_type(b"BM...."), Some("bmp"));
        assert_eq!(sniff_image_type(b"<svg>"), None);
        assert_eq!(sniff_image_type(&[0x89]), None); // 截断
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name("a/b\\c.png"), "c.png");
        assert_eq!(sanitize_file_name(""), "file");
    }
}
