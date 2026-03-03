mod api;
mod application;
mod domain;
mod infrastructure;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use shared_kernel::AppContext;

pub use api::notifications::send_notification;

pub fn router() -> Router<AppContext> {
    Router::new()
        .route(
            "/api/notifications",
            get(api::notifications::list_notifications),
        )
        .route(
            "/api/notifications",
            post(api::notifications::create_notification),
        )
        .route(
            "/api/notifications/{id}",
            put(api::notifications::update_notification),
        )
        .route(
            "/api/notifications/{id}",
            delete(api::notifications::delete_notification),
        )
}
