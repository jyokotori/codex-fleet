mod api;
mod config;
mod crypto;
mod db;
mod embed;
mod error;
mod ssh;
mod ws;

use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api::auth::auth_middleware;
use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend=info,tower_http=info".parse().unwrap()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    let db = db::create_pool(&config).await?;

    let state = AppState {
        db,
        config: config.clone(),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/api/auth/login", post(api::auth::login))
        .route("/api/auth/register", post(api::auth::register));

    // Protected API routes
    let protected_api = Router::new()
        .route("/api/auth/logout", post(api::auth::logout))
        // Config templates (read-only, embedded)
        .route("/api/config-templates/{*path}", get(embed::get_template))
        // Company configs
        .route("/api/configs", get(api::configs::list_configs))
        .route("/api/configs", post(api::configs::create_config))
        .route("/api/configs/{id}", put(api::configs::update_config))
        .route("/api/configs/{id}", delete(api::configs::delete_config))
        // Servers
        .route("/api/servers", get(api::servers::list_servers))
        .route("/api/servers", post(api::servers::create_server))
        .route("/api/servers/{id}", put(api::servers::update_server))
        .route("/api/servers/{id}", delete(api::servers::delete_server))
        .route("/api/servers/{id}/test", post(api::servers::test_server_connection))
        // Agents
        .route("/api/agents", get(api::agents::list_agents))
        .route("/api/agents", post(api::agents::create_agent))
        .route("/api/agents/{id}", put(api::agents::update_agent))
        .route("/api/agents/{id}", delete(api::agents::delete_agent))
        .route("/api/agents/{id}/start", post(api::agents::start_agent))
        .route("/api/agents/{id}/stop", post(api::agents::stop_agent))
        .route("/api/agents/{id}/resume", post(api::agents::resume_agent))
        .route("/api/agents/{id}/terminal-command", get(api::agents::terminal_command))
        // Tasks
        .route("/api/agents/{id}/tasks", post(api::tasks::create_task))
        .route("/api/agents/{id}/tasks", get(api::tasks::list_tasks))
        .route("/api/tasks/{id}", get(api::tasks::get_task))
        // Codex configs
        .route("/api/codex-configs", get(api::codex_configs::list_codex_configs))
        .route("/api/codex-configs", post(api::codex_configs::create_codex_config))
        .route("/api/codex-configs/{id}", put(api::codex_configs::update_codex_config))
        .route("/api/codex-configs/{id}", delete(api::codex_configs::delete_codex_config))
        // Docker configs
        .route("/api/docker-configs", get(api::docker_configs::list_docker_configs))
        .route("/api/docker-configs", post(api::docker_configs::create_docker_config))
        .route("/api/docker-configs/{id}", put(api::docker_configs::update_docker_config))
        .route("/api/docker-configs/{id}", delete(api::docker_configs::delete_docker_config))
        // Notifications
        .route("/api/notifications", get(api::notifications::list_notifications))
        .route("/api/notifications", post(api::notifications::create_notification))
        .route("/api/notifications/{id}", put(api::notifications::update_notification))
        .route("/api/notifications/{id}", delete(api::notifications::delete_notification))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // WebSocket routes (auth checked inside handler)
    let ws_routes = Router::new()
        .route("/ws/agents/{id}/logs", get(ws::logs::ws_logs_handler))
        .route("/ws/agents/{id}/terminal", get(ws::terminal::ws_terminal_handler))
        .route("/ws/agents/{id}/provision", get(ws::provision::ws_provision_handler));

    // Static file fallback for SPA
    let static_route = Router::new().fallback(embed::static_handler);

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_api)
        .merge(ws_routes)
        .merge(static_route)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
