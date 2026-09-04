//! folkmoot-server：启动入口（§2.2 main.rs）

use std::sync::Arc;

use anyhow::{Context, Result};
use folkmoot_server::state::AppState;
use folkmoot_server::{api, config, db};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // 参数：--config <path> | --print-openapi
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--print-openapi") {
        println!("{}", api::openapi::openapi_string());
        return Ok(());
    }
    let config_path = args
        .windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| w[1].clone());

    let config = Arc::new(config::Config::load(config_path.as_deref())?);
    std::fs::create_dir_all(&config.data_dir).context("create data dir")?;
    let uploads_root = config.data_dir.join("uploads");
    std::fs::create_dir_all(&uploads_root).context("create uploads dir")?;

    let db_path = config.data_dir.join("folkmoot.db");
    let db = db::Db::open(&db_path)?;

    let (shutdown_tx, _) = tokio::sync::watch::channel(false);
    let state = AppState {
        db,
        config: config.clone(),
        uploads_root,
        shutdown_tx: shutdown_tx.clone(),
    };

    let mut app = api::router(state.clone());

    // CORS：默认空（不发送 CORS 头），配置即允许（§8.6）
    if !config.cors_origins.is_empty() {
        let origins: Vec<axum::http::HeaderValue> = config
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        app = app.layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::DELETE,
                ])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderName::from_static("x-agent-key"),
                ])
                .allow_credentials(true),
        );
    }
    // 静态伺服 fallback 已在 api::router 内装配
    let app = app.layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("bind {}", config.bind))?;
    tracing::info!(bind = %config.bind, "folkmoot-server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("shutdown signal received, draining long polls");
            let _ = shutdown_tx.send(true);
            // 给在途长轮询 ≤2s 中断窗口
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
