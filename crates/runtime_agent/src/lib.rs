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
        .route("/api/agents", post(api::agents::create_agent))
        .route("/api/agents/{id}", put(api::agents::update_agent))
        .route("/api/agents/{id}", delete(api::agents::delete_agent))
        .route("/api/agents/{id}/start", post(api::agents::start_agent))
        .route("/api/agents/{id}/stop", post(api::agents::stop_agent))
        .route("/api/agents/{id}/restart", post(api::agents::restart_agent))
        .route("/api/agents/{id}/resume", post(api::agents::resume_agent))
        .route(
            "/api/agents/{id}/terminal-command",
            get(api::agents::terminal_command),
        )
        .route(
            "/api/agents/{id}/resume-command",
            get(api::agents::resume_command),
        )
        .route(
            "/api/agents/{id}/clone",
            post(api::agents::clone_agent),
        )
        .route("/api/agents/{id}/tasks", post(api::tasks::create_task))
        .route("/api/agents/{id}/tasks", get(api::tasks::list_tasks))
        .route("/api/tasks/{id}", get(api::tasks::get_task))
        .route("/api/projects", get(api::requirements::list_projects))
        .route("/api/projects", post(api::requirements::create_project))
        .route("/api/projects/{id}", get(api::requirements::get_project))
        .route("/api/projects/{id}", put(api::requirements::update_project))
        .route("/api/projects/{id}", delete(api::requirements::delete_project))
        .route("/api/projects/{id}/work-items", get(api::requirements::list_work_items))
        .route("/api/projects/{id}/work-items", post(api::requirements::create_work_item))
        .route("/api/work-items/by-execution/{execution_id}", get(api::requirements::get_work_item_by_execution))
        .route("/api/work-items/{id}", get(api::requirements::get_work_item))
        .route("/api/work-items/{id}", put(api::requirements::update_work_item))
        .route("/api/work-items/{id}", delete(api::requirements::delete_work_item))
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
