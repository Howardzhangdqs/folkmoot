//! 路由装配（§4.2 端点全集）+ /healthz + openapi.json

pub mod agents;
pub mod attachments;
pub mod auth;
pub mod conversations;
pub mod extractors;
pub mod messages;
pub mod openapi;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use crate::state::AppState;
use folkmoot_common::HealthResponse;

async fn healthz(State(state): State<AppState>) -> Result<Json<HealthResponse>, crate::error::ApiError> {
    // 轻量 DB 探活
    state
        .db
        .run(|conn| {
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?;
            Ok(())
        })
        .await?;
    Ok(Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }))
}

use axum::extract::State;

/// API 404 始终为 JSON（不被 SPA fallback 吞掉，§4.2 注）
async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(folkmoot_common::errors::ApiErrorBody::new(
            folkmoot_common::errors::ErrorCode::NotFound,
            "route not found",
        )),
    )
}

pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/login", delete(auth::logout))
        .route("/openapi.json", get(openapi::openapi_json));

    let protected = Router::new()
        .route("/auth/tokens", get(auth::list_tokens).post(auth::issue_token))
        .route("/auth/tokens/{id}", delete(auth::revoke_token))
        .route("/me", get(agents::me))
        .route("/agents", get(agents::list_agents))
        .route(
            "/conversations",
            post(conversations::create).get(conversations::list),
        )
        .route("/conversations/{id}", get(conversations::detail))
        .route(
            "/conversations/{id}/participants",
            post(conversations::add_members),
        )
        .route(
            "/conversations/{id}/participants/leave",
            post(conversations::leave),
        )
        .route(
            "/conversations/{id}/participants/{agent_id}",
            delete(conversations::remove_member),
        )
        .route(
            "/conversations/{id}/messages",
            get(messages::history).post(messages::send),
        )
        .route("/attachments/{id}", get(attachments::meta))
        .route(
            "/attachments/{id}/download",
            get(attachments::download),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            extractors::auth_middleware,
        ));

    let api = public.merge(protected).fallback(api_not_found);

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .nest("/api/v1", api);

    // 静态伺服（web_dist 配置时；/api/v1 内未匹配仍返回 JSON 404）
    if state.config.web_dist.is_some() {
        app = app.fallback(crate::static_web::spa_fallback);
    }

    app.with_state(state)
}
