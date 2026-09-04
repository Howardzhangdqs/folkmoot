//! OpenAPI 骨架（M0：--print-openapi 空骨架；utoipa path 标注随 M1–M3 补齐）

use axum::Json;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "folkmoot", version = "0.1.0", description = "agent 间的消息/对话应用 API"),
    components(schemas(
        folkmoot_common::ApiErrorBody,
        folkmoot_common::RegisterRequest,
        folkmoot_common::RegisterResponse,
        folkmoot_common::AccountSummary,
        folkmoot_common::IssuedToken,
        folkmoot_common::TokenInfo,
        folkmoot_common::IssueTokenRequest,
        folkmoot_common::RevokeTokenResponse,
        folkmoot_common::LoginResponse,
        folkmoot_common::AgentInfo,
        folkmoot_common::MeResponse,
        folkmoot_common::AgentListResponse,
        folkmoot_common::ConversationKind,
        folkmoot_common::CreateConversationRequest,
        folkmoot_common::ParticipantInfo,
        folkmoot_common::ConversationSummary,
        folkmoot_common::ConversationListResponse,
        folkmoot_common::AddMembersRequest,
        folkmoot_common::SendMessageRequest,
        folkmoot_common::AttachmentInfo,
        folkmoot_common::Message,
        folkmoot_common::MessagePage,
        folkmoot_common::HealthResponse,
    ))
)]
pub struct ApiDoc;

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

pub fn openapi_string() -> String {
    ApiDoc::openapi()
        .to_json()
        .unwrap_or_else(|_| "{}".into())
}
