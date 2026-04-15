mod db;
mod embed;

use axum::{middleware, routing::get, Router};
use shared_kernel::{AppConfig, AppContext, AgentStatusCache};
use std::net::SocketAddr;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok(); // load .env if present (ignored in production)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend=info,tower_http=info".parse().unwrap()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(tracing_subscriber::fmt::time::SystemTime)
                .with_target(true),
        )
        .init();

    let config = AppConfig::from_env();
    let db = db::create_pool(&config).await?;

    let state = AppContext {
        db,
        config: config.clone(),
        provision_channels: Arc::new(Mutex::new(HashMap::new())),
        task_channels: Arc::new(Mutex::new(HashMap::new())),
        task_abort_signals: Arc::new(Mutex::new(HashMap::new())),
        agent_status_cache: AgentStatusCache::new(Duration::from_secs(10)),
        agent_dispatch_locks: Arc::new(RwLock::new(HashMap::new())),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Public routes
    let public_routes = iam::public_router();

    // Protected API routes
    let protected_api = Router::new()
        .merge(iam::protected_router())
        .merge(config_center::router())
        .merge(runtime_agent::router())
        .merge(notification_center::router())
        // Config templates (read-only, embedded)
        .route("/api/config-templates/{*path}", get(embed::get_template))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            iam::auth_middleware,
        ));

    let ws_routes = runtime_agent::ws_router().layer(middleware::from_fn_with_state(
        state.clone(),
        iam::auth_middleware,
    ));

    // External API routes (header-secret auth)
    let external_api = iam::external_router().layer(middleware::from_fn_with_state(
        state.clone(),
        iam::external_api_auth,
    ));

    // Static file fallback for SPA
    let static_route = Router::new().fallback(embed::static_handler);

    // Start the work-item scheduler
    tokio::spawn(runtime_agent::scheduler::run_scheduler(state.clone()));

    let app = Router::new()
        .merge(public_routes)
        .merge(runtime_agent::webhook_router())
        .merge(protected_api)
        .merge(external_api)
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
