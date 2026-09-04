//! 薄 reqwest 封装：base_url + Bearer + X-Agent-Key；错误映射到退出码（§5.4）

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;

use folkmoot_common::errors::ApiErrorBody;

#[derive(Debug)]
pub struct CliError {
    pub status: Option<u16>,
    pub code: Option<String>,
    pub message: String,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.code {
            Some(c) => write!(f, "{c}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}
impl std::error::Error for CliError {}

impl CliError {
    /// 本地可操作错误（退出码 12，§5.4）
    pub fn local(message: impl Into<String>) -> Self {
        Self {
            status: Some(422),
            code: None,
            message: message.into(),
        }
    }
}

/// §5.4 退出码契约
pub fn exit_code(err: &CliError) -> i32 {
    match err.status {
        Some(401) | Some(403) => 10,
        Some(404) => 11,
        Some(409) | Some(413) | Some(415) | Some(422) => 12,
        Some(s) if s >= 500 => 20,
        Some(_) => 1,
        None => 20, // 网络失败 / 超时
    }
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    pub base: String,
    token: Option<String>,
    agent: Option<String>,
}

impl Client {
    pub fn new(base: String, token: Option<String>, agent: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_string(),
            token,
            agent,
        }
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn req(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut r = self.http.request(method, format!("{}/api/v1{path}", self.base));
        if let Some(t) = &self.token {
            r = r.bearer_auth(t);
        }
        if let Some(a) = &self.agent {
            r = r.header("X-Agent-Key", a);
        }
        r
    }

    async fn handle<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, CliError> {
        let status = resp.status().as_u16();
        if resp.status().is_success() {
            let text = resp.text().await.map_err(|e| CliError {
                status: None,
                code: None,
                message: format!("read response: {e}"),
            })?;
            // 空体 2xx 按 null 处理（对 Value/Option 友好）
            let text = if text.trim().is_empty() { "null".to_string() } else { text };
            return serde_json::from_str::<T>(&text).map_err(|e| CliError {
                status: None,
                code: None,
                message: format!("decode response: {e}"),
            });
        }
        let body = resp.text().await.unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<ApiErrorBody>(&body) {
            return Err(CliError {
                status: Some(status),
                code: Some(serde_json::to_string(&parsed.error.code).unwrap().trim_matches('"').to_string()),
                message: parsed.error.message,
            });
        }
        Err(CliError {
            status: Some(status),
            code: None,
            message: format!("http {status}: {body}"),
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, CliError> {
        let resp = self.req(reqwest::Method::GET, path).send().await.map_err(net_err)?;
        Self::handle(resp).await
    }

    /// 带超时的 GET（长轮询用：wait + 10s，§5.2）
    pub async fn get_timeout<T: DeserializeOwned>(&self, path: &str, timeout_secs: u64) -> Result<T, CliError> {
        let resp = self
            .req(reqwest::Method::GET, path)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(net_err)?;
        Self::handle(resp).await
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T, CliError> {
        let resp = self.req(reqwest::Method::POST, path).json(body).send().await.map_err(net_err)?;
        Self::handle(resp).await
    }

    /// 需要区分 200/201 时返回 (status, body)
    pub async fn post_json_status<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<(u16, T), CliError> {
        let resp = self.req(reqwest::Method::POST, path).json(body).send().await.map_err(net_err)?;
        let status = resp.status().as_u16();
        let v = Self::handle::<T>(resp).await?;
        Ok((status, v))
    }

    pub async fn post_multipart<T: DeserializeOwned>(&self, path: &str, form: reqwest::multipart::Form) -> Result<T, CliError> {
        let resp = self.req(reqwest::Method::POST, path).multipart(form).send().await.map_err(net_err)?;
        Self::handle(resp).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, CliError> {
        let resp = self.req(reqwest::Method::DELETE, path).send().await.map_err(net_err)?;
        Self::handle(resp).await
    }

    /// 流式下载到 writer
    pub async fn download(&self, path: &str, mut out: impl tokio::io::AsyncWrite + Unpin + Send) -> Result<(), CliError> {
        use tokio::io::AsyncWriteExt;
        let resp = self.req(reqwest::Method::GET, path).send().await.map_err(net_err)?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            let code = serde_json::from_str::<ApiErrorBody>(&body).ok()
                .map(|e| serde_json::to_string(&e.error.code).unwrap().trim_matches('"').to_string());
            let msg = serde_json::from_str::<ApiErrorBody>(&body).ok().map(|e| e.error.message)
                .unwrap_or(body);
            return Err(CliError { status: Some(status), code, message: msg });
        }
        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(net_err)?;
            out.write_all(&chunk).await.map_err(|e| CliError { status: None, code: None, message: e.to_string() })?;
        }
        out.flush().await.ok();
        Ok(())
    }
}

fn net_err(e: reqwest::Error) -> CliError {
    CliError {
        status: None,
        code: None,
        message: format!("network: {e}"),
    }
}
