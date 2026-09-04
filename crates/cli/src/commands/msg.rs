//! send / history / upload / download / listen / chat

use std::io::Write as _;

use anyhow::Result;

use super::{resolve_conversation, Ctx};
use crate::client::CliError;
use crate::display;
use folkmoot_common::{Message, MessagePage, SendMessageRequest};

pub async fn send(ctx: &Ctx, conv: &str, message: &str) -> Result<(), CliError> {
    ctx.require_token()?;
    ctx.print_agent_line();
    let id = resolve_conversation(ctx, conv).await?;
    let msg: Message = ctx
        .client
        .post_json(
            &format!("/conversations/{id}/messages"),
            &SendMessageRequest { text: message.to_string() },
        )
        .await?;
    if ctx.json {
        ctx.print_json(&msg);
    } else {
        println!("sent {}", msg.id);
    }
    Ok(())
}

pub async fn history(
    ctx: &Ctx,
    conv: &str,
    limit: u32,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<(), CliError> {
    ctx.require_token()?;
    let id = resolve_conversation(ctx, conv).await?;
    let mut path = format!("/conversations/{id}/messages?limit={limit}");
    if let Some(b) = before {
        path.push_str(&format!("&before={b}"));
    }
    if let Some(a) = after {
        path.push_str(&format!("&after={a}"));
    }
    let page: MessagePage = ctx.client.get(&path).await?;
    if ctx.json {
        ctx.print_json(&page);
        return Ok(());
    }
    for m in &page.items {
        println!("{}", display::timeline_line(m));
    }
    if let Some(cur) = &page.next_cursor {
        eprintln!("(more history: --before {cur})");
    }
    Ok(())
}

pub async fn upload(ctx: &Ctx, conv: &str, files: &[String], caption: Option<&str>) -> Result<(), CliError> {
    ctx.require_token()?;
    ctx.print_agent_line();
    let id = resolve_conversation(ctx, conv).await?;
    let mut form = reqwest::multipart::Form::new();
    if let Some(t) = caption {
        form = form.text("text", t.to_string());
    }
    for f in files {
        let data = std::fs::read(f).map_err(|e| CliError {
            status: None,
            code: None,
            message: format!("read {f}: {e}"),
        })?;
        let name = std::path::Path::new(f)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        form = form.part(
            "file",
            reqwest::multipart::Part::bytes(data).file_name(name),
        );
    }
    let msg: Message = ctx
        .client
        .post_multipart(&format!("/conversations/{id}/messages"), form)
        .await?;
    if ctx.json {
        ctx.print_json(&msg);
    } else {
        for a in &msg.attachments {
            println!("uploaded {} ({} bytes) id={}", a.file_name, a.size_bytes, a.id);
        }
    }
    Ok(())
}

pub async fn download(ctx: &Ctx, attachment_id: &str, output: Option<&str>) -> Result<(), CliError> {
    ctx.require_token()?;
    let meta: folkmoot_common::AttachmentInfo =
        ctx.client.get(&format!("/attachments/{attachment_id}")).await?;
    let out_path = output
        .map(str::to_string)
        .unwrap_or_else(|| meta.file_name.clone());
    let file = tokio::fs::File::create(&out_path).await.map_err(|e| CliError {
        status: None,
        code: None,
        message: format!("create {out_path}: {e}"),
    })?;
    ctx.client
        .download(&format!("/attachments/{attachment_id}/download"), file)
        .await?;
    if ctx.json {
        ctx.print_json(&serde_json::json!({"saved": out_path, "sha256": meta.sha256}));
    } else {
        println!("saved to {out_path} (sha256:{}…)", &meta.sha256[..8]);
    }
    Ok(())
}

/// 阻塞等待新消息：默认持续监听；--once 等到一批即退出（§5.2）
/// 返回 Ok(true) = 拿到消息；Ok(false) = --once 超时（退出码 21）
pub async fn listen(
    ctx: &Ctx,
    conv: &str,
    once: bool,
    timeout_secs: u64,
    after: Option<&str>,
    interval_secs: u64,
) -> Result<bool, CliError> {
    ctx.require_token()?;
    let id = resolve_conversation(ctx, conv).await?;
    // 起点：当前最新消息（可 --after 覆盖），不做历史回填
    let mut cursor = match after {
        Some(a) => a.to_string(),
        None => {
            let page: MessagePage = ctx
                .client
                .get(&format!("/conversations/{id}/messages?limit=1"))
                .await?;
            page.items.last().map(|m| m.id.clone()).unwrap_or_default()
        }
    };
    let wait = interval_secs.min(25).max(1);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if once && remaining.is_zero() {
            return Ok(false);
        }
        // --once 时单次 wait 不超过剩余 deadline，保证按时以 21 退出
        let wait = if once {
            wait.min(remaining.as_secs().max(1))
        } else {
            wait
        };
        let path = if cursor.is_empty() {
            format!("/conversations/{id}/messages?limit=50")
        } else {
            format!("/conversations/{id}/messages?after={cursor}&wait={wait}&limit=50")
        };
        let page: MessagePage = match ctx.client.get_timeout(&path, wait + 10).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("listen: {e}; retrying in 2s");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        if page.items.is_empty() {
            if once && std::time::Instant::now() >= deadline {
                return Ok(false);
            }
            if cursor.is_empty() {
                // 首轮无消息：以最新一条为起点继续等
                let latest: MessagePage = ctx
                    .client
                    .get(&format!("/conversations/{id}/messages?limit=1"))
                    .await?;
                cursor = latest.items.last().map(|m| m.id.clone()).unwrap_or_default();
            }
            continue;
        }
        for m in &page.items {
            if ctx.json {
                println!("{}", serde_json::to_string(m).unwrap()); // NDJSON
            } else {
                println!("{}", display::timeline_line(m));
            }
            cursor = m.id.clone();
        }
        std::io::stdout().flush().ok();
        if once {
            return Ok(true);
        }
    }
}

/// 交互式 chat：初始历史 + 长轮询取新 + stdin 逐行发送；/upload /members /quit
pub async fn chat(ctx: &Ctx, conv: &str, interval_secs: u64) -> Result<(), CliError> {
    ctx.require_token()?;
    ctx.print_agent_line();
    let id = resolve_conversation(ctx, conv).await?;
    eprintln!("entering chat {conv} ({}); /upload <file>  /members  /quit", &id[..8]);

    // 初始拉一页历史
    let page: MessagePage = ctx
        .client
        .get(&format!("/conversations/{id}/messages?limit={}", ctx.config.defaults.limit))
        .await?;
    for m in &page.items {
        println!("{}", display::timeline_line(m));
    }
    let mut cursor = page.items.last().map(|m| m.id.clone()).unwrap_or_default();

    // 轮询任务
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Message>(64);
    {
        let client = ctx.client.clone();
        let json = ctx.json;
        let id2 = id.clone();
        let wait = interval_secs.min(25).max(1);
        tokio::spawn(async move {
            loop {
                let path = if cursor.is_empty() {
                    format!("/conversations/{id2}/messages?limit=50")
                } else {
                    format!("/conversations/{id2}/messages?after={cursor}&wait={wait}&limit=50")
                };
                match client.get_timeout::<MessagePage>(&path, wait + 10).await {
                    Ok(page) => {
                        for m in page.items {
                            cursor = m.id.clone();
                            if tx.send(m).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
                let _ = json;
            }
        });
    }

    // stdin 行任务
    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(16);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        loop {
            let mut buf = String::new();
            match stdin.read_line(&mut buf) {
                Ok(0) => return, // EOF
                Ok(_) => {
                    if line_tx.blocking_send(buf.trim_end().to_string()).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    });

    loop {
        tokio::select! {
            Some(m) = rx.recv() => {
                if ctx.json {
                    println!("{}", serde_json::to_string(&m).unwrap());
                } else {
                    println!("{}", display::timeline_line(&m));
                }
                std::io::stdout().flush().ok();
            }
            line = line_rx.recv() => {
                let Some(line) = line else { break }; // stdin EOF
                if line.is_empty() { continue; }
                match line.as_str() {
                    "/quit" => break,
                    "/members" => {
                        if let Ok(c) = ctx.client.get::<folkmoot_common::ConversationSummary>(&format!("/conversations/{id}")).await {
                            println!("members: {}", c.members.iter()
                                .map(|m| m.agent_key.clone().unwrap_or_else(|| m.agent_id[..8].into()))
                                .collect::<Vec<_>>().join(", "));
                        }
                    }
                    _ if line.starts_with("/upload ") => {
                        let path = line.trim_start_matches("/upload ").trim().to_string();
                        if let Err(e) = upload(ctx, conv, &[path], None).await {
                            eprintln!("upload failed: {e}");
                        }
                    }
                    text => {
                        let req = SendMessageRequest { text: text.to_string() };
                        if let Err(e) = ctx.client.post_json::<_, Message>(&format!("/conversations/{id}/messages"), &req).await {
                            eprintln!("send failed: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
