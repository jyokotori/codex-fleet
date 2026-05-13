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

use application::{
    api_token::find_user_id_by_token,
    token::{decode_access_token, fetch_auth_context},
};
use shared_kernel::{AppContext, AppError};

pub use api::{admin_users::admin_router, auth::auth_router};

pub fn public_router() -> Router<AppContext> {
    auth_router()
}

pub fn protected_router() -> Router<AppContext> {
    Router::new()
        .nest("/api/auth", api::auth::protected_auth_router())
        .nest("/api", api::auth::me_router())
        .nest("/api/me", api::api_tokens::router())
        .nest(
            "/api/admin",
            Router::new()
                .merge(api::admin_users::router())
                .merge(api::dingtalk::router()),
        )
}

pub async fn auth_middleware(
    State(state): State<AppContext>,
    mut request: Request,
    next: Next,
) -> std::result::Result<Response, AppError> {
    // API access tokens are only accepted via the Authorization header.
    // The query-string fallback is reserved for short-lived JWT access tokens
    // (e.g. WebSocket upgrades), to avoid long-lived tokens leaking into
    // proxy logs / browser history / Referer headers.
    let header_token = api::auth::extract_token(request.headers());
    let query_token = request.uri().query().and_then(|query| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix("token=").map(|v| v.to_string()))
    });

    let user_id = if let Some(token) = header_token {
        match decode_access_token(&token, &state.config.jwt_secret) {
            Ok(claims) => {
                if claims.exp < Utc::now().timestamp() {
                    return Err(AppError::Unauthorized);
                }
                claims.sub
            }
            Err(_) => find_user_id_by_token(&state.db, &token)
                .await?
                .ok_or(AppError::Unauthorized)?,
        }
    } else if let Some(token) = query_token {
        let claims = decode_access_token(&token, &state.config.jwt_secret)?;
        if claims.exp < Utc::now().timestamp() {
            return Err(AppError::Unauthorized);
        }
        claims.sub
    } else {
        return Err(AppError::Unauthorized);
    };

    let auth = fetch_auth_context(&state.db, &user_id).await?;
    if auth.status != "active" {
        return Err(AppError::Forbidden("User is disabled".into()));
    }

    request.extensions_mut().insert(auth);
    Ok(next.run(request).await)
}
