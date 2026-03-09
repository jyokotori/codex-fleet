mod api;
mod application;
mod domain;
mod infrastructure;

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
    Router,
};
use chrono::Utc;

use application::token::{decode_access_token, fetch_auth_context};
use shared_kernel::{AppContext, AppError};

pub use api::{
    admin_users::admin_router,
    auth::auth_router,
    external::{external_api_auth, external_router},
};

pub fn public_router() -> Router<AppContext> {
    auth_router()
}

pub fn protected_router() -> Router<AppContext> {
    Router::new()
        .nest("/api/auth", api::auth::protected_auth_router())
        .nest("/api", api::auth::me_router())
        .nest("/api/admin", api::admin_users::router())
}

pub async fn auth_middleware(
    State(state): State<AppContext>,
    mut request: Request,
    next: Next,
) -> std::result::Result<Response, AppError> {
    let token = api::auth::extract_token(request.headers())
        .or_else(|| {
            request.uri().query().and_then(|query| {
                query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("token=").map(|v| v.to_string()))
            })
        })
        .ok_or(AppError::Unauthorized)?;

    let claims = decode_access_token(&token, &state.config.jwt_secret)?;
    let now = Utc::now().timestamp();
    if claims.exp < now {
        return Err(AppError::Unauthorized);
    }

    let auth = fetch_auth_context(&state.db, &claims.sub).await?;
    if auth.status != "active" {
        return Err(AppError::Forbidden("User is disabled".into()));
    }

    request.extensions_mut().insert(auth);
    Ok(next.run(request).await)
}
