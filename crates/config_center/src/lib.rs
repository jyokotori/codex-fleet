mod api;
pub mod application;
mod domain;
mod infrastructure;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use shared_kernel::AppContext;

pub use api::third_party_app_configs::dingtalk_enabled;

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/api/configs", get(api::configs::list_configs))
        .route("/api/configs", post(api::configs::create_config))
        .route("/api/configs/{id}", put(api::configs::update_config))
        .route("/api/configs/{id}", delete(api::configs::delete_config))
        .route(
            "/api/codex-configs",
            get(api::codex_configs::list_codex_configs),
        )
        .route(
            "/api/codex-configs",
            post(api::codex_configs::create_codex_config),
        )
        .route(
            "/api/codex-configs/{id}",
            put(api::codex_configs::update_codex_config),
        )
        .route(
            "/api/codex-configs/{id}",
            delete(api::codex_configs::delete_codex_config),
        )
        .route(
            "/api/claude-configs",
            get(api::claude_configs::list_claude_configs),
        )
        .route(
            "/api/claude-configs",
            post(api::claude_configs::create_claude_config),
        )
        .route(
            "/api/claude-configs/{id}",
            put(api::claude_configs::update_claude_config),
        )
        .route(
            "/api/claude-configs/{id}",
            delete(api::claude_configs::delete_claude_config),
        )
        .route(
            "/api/docker-configs",
            get(api::docker_configs::list_docker_configs),
        )
        .route(
            "/api/docker-configs",
            post(api::docker_configs::create_docker_config),
        )
        .route(
            "/api/docker-configs/{id}",
            put(api::docker_configs::update_docker_config),
        )
        .route(
            "/api/docker-configs/{id}",
            delete(api::docker_configs::delete_docker_config),
        )
        .route(
            "/api/admin/integrations",
            get(api::third_party_app_configs::list_integrations),
        )
        .route(
            "/api/admin/integrations/{provider}",
            get(api::third_party_app_configs::get_integration),
        )
        .route(
            "/api/admin/integrations/{provider}",
            put(api::third_party_app_configs::upsert_integration),
        )
        .route(
            "/api/integrations/status",
            get(api::third_party_app_configs::integrations_status),
        )
}
