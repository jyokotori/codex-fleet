mod api;
mod application;
mod domain;
mod infrastructure;
pub mod scheduler;
mod ssh;
mod ws;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use shared_kernel::AppContext;

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/api/servers", get(api::servers::list_servers))
        .route("/api/servers", post(api::servers::create_server))
        .route("/api/servers/{id}", put(api::servers::update_server))
        .route("/api/servers/{id}", delete(api::servers::delete_server))
        .route(
            "/api/servers/{id}/test",
            post(api::servers::test_server_connection),
        )
        .route("/api/agents", get(api::agents::list_agents))
        .route("/api/agents/sync-status", post(api::agents::sync_status))
        .route("/api/agents", post(api::agents::create_agent))
        .route("/api/agents/{id}", get(api::agents::get_agent).put(api::agents::update_agent).delete(api::agents::delete_agent))
        .route("/api/agents/{id}/start", post(api::agents::start_agent))
        .route("/api/agents/{id}/stop", post(api::agents::stop_agent))
        .route("/api/agents/{id}/restart", post(api::agents::restart_agent))
        .route(
            "/api/agents/{id}/terminal-command",
            get(api::agents::terminal_command),
        )
        .route(
            "/api/agents/{id}/resume-command",
            get(api::agents::resume_command),
        )
        .route(
            "/api/agents/{id}/check-resume-process",
            get(api::agents::check_resume_process),
        )
        .route("/api/agents/{id}/clone", post(api::agents::clone_agent))
        .route("/api/agents/{id}/tasks", post(api::tasks::create_task))
        .route("/api/agents/{id}/tasks", get(api::tasks::list_tasks))
        .route("/api/tasks/{id}", get(api::tasks::get_task))
        .route("/api/tasks/{id}/abort", post(api::tasks::abort_task))
        .route(
            "/api/agent-groups",
            get(api::agent_groups::list_agent_groups).post(api::agent_groups::create_agent_group),
        )
        .route(
            "/api/agent-groups/{id}",
            put(api::agent_groups::update_agent_group).delete(api::agent_groups::delete_agent_group),
        )
        .route(
            "/api/plane/workspaces",
            get(api::plane::list_workspaces).post(api::plane::create_workspace),
        )
        .route(
            "/api/plane/workspaces/{id}",
            put(api::plane::update_workspace).delete(api::plane::delete_workspace),
        )
        .route(
            "/api/plane/workspaces/{id}/toggle",
            post(api::plane::toggle_workspace),
        )
        .route(
            "/api/plane/workspaces/{id}/projects",
            get(api::plane::list_workspace_projects),
        )
        .route(
            "/api/plane/workspaces/{id}/projects/{pid}/states",
            get(api::plane::list_project_states),
        )
        .route(
            "/api/plane/workspaces/{id}/projects/{pid}/labels",
            get(api::plane::list_project_labels),
        )
        .route("/api/clis", get(api::plane::list_clis))
        .route(
            "/api/plane/workspaces/{id}/bindings",
            get(api::plane::list_workspace_bindings).post(api::plane::create_workspace_binding),
        )
        .route(
            "/api/plane/bindings/{id}",
            put(api::plane::update_plane_binding).delete(api::plane::delete_plane_binding),
        )
        .route(
            "/api/plane/bindings/{id}/toggle",
            post(api::plane::toggle_plane_binding),
        )
        .route(
            "/api/plane/tasks",
            get(api::plane::list_plane_tasks),
        )
}

/// Public webhook routes (no auth required — called by external services like Plane).
pub fn webhook_router() -> Router<AppContext> {
    Router::new().route(
        "/api/webhooks/plane/{workspace_id}",
        post(api::webhooks::plane_webhook),
    )
}

pub fn ws_router() -> Router<AppContext> {
    Router::new()
        .route(
            "/ws/agents/{id}/terminal",
            get(ws::terminal::ws_terminal_handler),
        )
        .route(
            "/ws/agents/{id}/provision",
            get(ws::provision::ws_provision_handler),
        )
        .route(
            "/ws/tasks/{id}/logs",
            get(ws::task_logs::ws_task_logs_handler),
        )
}
