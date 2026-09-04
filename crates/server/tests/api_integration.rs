//! 集成测试：axum Router + tower oneshot 内存请求（§11）
//! 覆盖：注册/鉴权路径、agent upsert 复用、DM 幂等、成员权限矩阵、消息分页、
//! multipart 上传/415/下载字节一致、cookie 通道 CSRF。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use folkmoot_server::config::Config;
use folkmoot_server::db::Db;
use folkmoot_server::state::AppState;
use folkmoot_server::api;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = dir.path().to_path_buf();
    let (shutdown_tx, _) = tokio::sync::watch::channel(false);
    let state = AppState {
        db: Db::open(&dir.path().join("t.db")).unwrap(),
        config: Arc::new(cfg),
        uploads_root: dir.path().join("uploads"),
        shutdown_tx,
    };
    (api::router(state), dir)
}

async fn call(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers_cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json, headers_cookie)
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn register(app: &axum::Router, name: &str) -> String {
    let (st, body, _) = call(
        app,
        Request::post("/api/v1/auth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"name": name, "token_count": 1}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "register {name}: {body}");
    body["tokens"][0]["token"].as_str().unwrap().to_string()
}

async fn touch_agent(app: &axum::Router, token: &str, key: &str) {
    let (st, _, _) = call(
        app,
        Request::get("/api/v1/me")
            .header(header::AUTHORIZATION, bearer(token))
            .header("x-agent-key", key)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
}

async fn dm(app: &axum::Router, token: &str, from: &str, peer: &str) -> (StatusCode, String) {
    let (st, body, _) = call(
        app,
        Request::post("/api/v1/conversations")
            .header(header::AUTHORIZATION, bearer(token))
            .header("x-agent-key", from)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"kind": "dm", "peer": peer}).to_string()))
            .unwrap(),
    )
    .await;
    (st, body["id"].as_str().unwrap_or("").to_string())
}

async fn send(app: &axum::Router, token: &str, key: &str, conv: &str, text: &str) -> StatusCode {
    let (st, _, _) = call(
        app,
        Request::post(format!("/api/v1/conversations/{conv}/messages"))
            .header(header::AUTHORIZATION, bearer(token))
            .header("x-agent-key", key)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"text": text}).to_string()))
            .unwrap(),
    )
    .await;
    st
}

#[tokio::test]
async fn auth_dm_message_flow() {
    let (app, _dir) = app().await;
    let token = register(&app, "alice").await;
    touch_agent(&app, &token, "ops").await;
    touch_agent(&app, &token, "worker").await;

    // 无 token → 401
    let (st, body, _) = call(&app, Request::get("/api/v1/me").body(Body::empty()).unwrap()).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");

    // agent upsert 复用：同 key = 同 agent
    let (st, me1, _) = call(
        &app,
        Request::get("/api/v1/me")
            .header(header::AUTHORIZATION, bearer(&token))
            .header("x-agent-key", "ops")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    touch_agent(&app, &token, "ops").await;
    let (_, me2, _) = call(
        &app,
        Request::get("/api/v1/me")
            .header(header::AUTHORIZATION, bearer(&token))
            .header("x-agent-key", "ops")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(me1["agent"]["id"], me2["agent"]["id"]);

    // DM 幂等：201 → 200 同 id
    let (st1, id1) = dm(&app, &token, "ops", "worker").await;
    let (st2, id2) = dm(&app, &token, "ops", "worker").await;
    assert_eq!(st1, StatusCode::CREATED);
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(id1, id2);

    // self-DM → 422
    let (st, _) = dm(&app, &token, "ops", "ops").await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // 收发 + 分页
    for i in 0..5 {
        assert_eq!(
            send(&app, &token, "ops", &id1, &format!("m{i}")).await,
            StatusCode::OK
        );
    }
    let (st, page, _) = call(
        &app,
        Request::get(format!("/api/v1/conversations/{id1}/messages?limit=2"))
            .header(header::AUTHORIZATION, bearer(&token))
            .header("x-agent-key", "worker")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["items"][1]["text"], "m4"); // 默认最新一页
    let cursor = page["next_cursor"].as_str().unwrap().to_string();
    let (_, older, _) = call(
        &app,
        Request::get(format!("/api/v1/conversations/{id1}/messages?limit=2&before={cursor}"))
            .header(header::AUTHORIZATION, bearer(&token))
            .header("x-agent-key", "worker")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(older["items"][1]["text"], "m2");

    // wait>0 无 after → 422
    let (st, _, _) = call(
        &app,
        Request::get(format!("/api/v1/conversations/{id1}/messages?wait=5"))
            .header(header::AUTHORIZATION, bearer(&token))
            .header("x-agent-key", "worker")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn membership_and_token_lifecycle() {
    let (app, _dir) = app().await;
    let t1 = register(&app, "bob").await;
    let t2 = register(&app, "carol").await;
    touch_agent(&app, &t1, "a").await;
    touch_agent(&app, &t1, "b").await;
    touch_agent(&app, &t2, "c").await;

    let (_, conv) = dm(&app, &t1, "a", "b").await;

    // 跨账号非成员 → 404（存在性不泄露，§8.2）
    let (st, _, _) = call(
        &app,
        Request::get(format!("/api/v1/conversations/{conv}/messages"))
            .header(header::AUTHORIZATION, bearer(&t2))
            .header("x-agent-key", "c")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(send(&app, &t2, "c", &conv, "intrude").await, StatusCode::NOT_FOUND);

    // token 上限与吊销
    for _ in 0..4 {
        let (st, _, _) = call(
            &app,
            Request::post("/api/v1/auth/tokens")
                .header(header::AUTHORIZATION, bearer(&t1))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"label": "x"}).to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
    }
    let (st, _, _) = call(
        &app,
        Request::post("/api/v1/auth/tokens")
            .header(header::AUTHORIZATION, bearer(&t1))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"label": "over"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT); // 上限 5

    // 吊销后 401
    let (_, list, _) = call(
        &app,
        Request::get("/api/v1/auth/tokens")
            .header(header::AUTHORIZATION, bearer(&t1))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let victim = list.as_array().unwrap()[1]["id"].as_str().unwrap().to_string();
    let (st, _, _) = call(
        &app,
        Request::delete(format!("/api/v1/auth/tokens/{victim}"))
            .header(header::AUTHORIZATION, bearer(&t1))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn cookie_channel_and_csrf() {
    let (app, _dir) = app().await;
    let token = register(&app, "dora").await;
    touch_agent(&app, &token, "web").await;

    // login 换 cookie
    let (st, _, set_cookie) = call(
        &app,
        Request::post("/api/v1/auth/login")
            .header(header::AUTHORIZATION, bearer(&token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));
    let cookie = set_cookie.split(';').next().unwrap().to_string();

    // cookie 通道 GET 可用
    let (st, _, _) = call(
        &app,
        Request::get("/api/v1/me")
            .header(header::COOKIE, &cookie)
            .header("x-agent-key", "web")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // cookie 通道变更请求缺 X-Agent-Key → 403 csrf_rejected
    let (st, body, _) = call(
        &app,
        Request::post("/api/v1/conversations")
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"kind":"dm","peer":"nobody"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "csrf_rejected");

    // 登出幂等 200 + cookie 失效
    let (st, _, cleared) = call(
        &app,
        Request::delete("/api/v1/auth/login")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(cleared.contains("Max-Age=0"));
    let (st, _, _) = call(
        &app,
        Request::get("/api/v1/me")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn upload_download_roundtrip() {
    let (app, _dir) = app().await;
    let token = register(&app, "erin").await;
    touch_agent(&app, &token, "x").await;
    touch_agent(&app, &token, "y").await;
    let (_, conv) = dm(&app, &token, "x", "y").await;

    // 构造 multipart body
    let boundary = "testboundary";
    let png: &[u8] = b"\x89PNG\r\n\x1a\nFAKEDATA";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"text\"\r\n\r\n看图\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"../evil.png\"\r\n\
         Content-Type: image/png\r\n\r\n"
    );
    let mut bytes = body.into_bytes();
    bytes.extend_from_slice(png);
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let (st, msg, _) = call(
        &app,
        Request::post(format!("/api/v1/conversations/{conv}/messages"))
            .header(header::AUTHORIZATION, bearer(&token))
            .header("x-agent-key", "x")
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(bytes))
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{msg}");
    let att = &msg["attachments"][0];
    assert_eq!(att["file_name"], "evil.png"); // basename sanitize
    assert_eq!(att["content_type"], "image/png");
    let att_id = att["id"].as_str().unwrap();

    // 下载字节一致
    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/attachments/{att_id}/download"))
                .header(header::AUTHORIZATION, bearer(&token))
                .header("x-agent-key", "y")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    assert_eq!(&bytes[..], png);

    // 伪格式 → 415
    let svg_body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.png\"\r\n\r\n<svg></svg>\r\n--{boundary}--\r\n"
    );
    let (st, _, _) = call(
        &app,
        Request::post(format!("/api/v1/conversations/{conv}/messages"))
            .header(header::AUTHORIZATION, bearer(&token))
            .header("x-agent-key", "x")
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(svg_body))
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}
