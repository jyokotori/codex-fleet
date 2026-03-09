use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, Result};

use crate::application::password::hash_password;

#[derive(Debug, Deserialize)]
pub struct ExternalCreateUserRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct ExternalCreateUserResponse {
    pub id: String,
}

pub fn external_router() -> Router<AppContext> {
    Router::new().route("/api/external/users", post(create_user))
}

/// Middleware: validate the configured header + secret.
pub async fn external_api_auth(
    State(state): State<AppContext>,
    request: Request,
    next: Next,
) -> std::result::Result<Response, AppError> {
    let secret = &state.config.external_api_secret;
    if secret.is_empty() {
        return Err(AppError::Forbidden(
            "External API is not enabled (EXTERNAL_API_SECRET not set)".into(),
        ));
    }

    let header_name = &state.config.external_api_header;
    let provided = request
        .headers()
        .get(header_name.as_str())
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if provided != secret.as_str() {
        return Err(AppError::Unauthorized);
    }

    Ok(next.run(request).await)
}

async fn create_user(
    State(state): State<AppContext>,
    Json(req): Json<ExternalCreateUserRequest>,
) -> Result<Json<ExternalCreateUserResponse>> {
    if req.username.trim().is_empty() {
        return Err(AppError::BadRequest("Username cannot be empty".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }

    let exists = sqlx::query("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim())
        .fetch_optional(&state.db)
        .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Username already exists".into()));
    }

    let user_id = Uuid::new_v4().to_string();
    let password_hash = hash_password(&req.password)?;

    sqlx::query(
        "INSERT INTO users (id, username, display_name, password_hash, status, failed_attempts, created_at, updated_at) VALUES ($1, $2, $3, $4, 'active', 0, NOW(), NOW())",
    )
    .bind(&user_id)
    .bind(req.username.trim())
    .bind(req.display_name.trim())
    .bind(password_hash)
    .execute(&state.db)
    .await?;

    // Assign default "member" role
    let role = sqlx::query("SELECT id FROM roles WHERE code = 'member'")
        .fetch_optional(&state.db)
        .await?;
    if let Some(role_row) = role {
        let role_id: String = sqlx::Row::get(&role_row, "id");
        sqlx::query("INSERT INTO user_roles (user_id, role_id, created_at) VALUES ($1, $2, NOW())")
            .bind(&user_id)
            .bind(role_id)
            .execute(&state.db)
            .await?;
    }

    // Audit log
    crate::application::audit::write_audit_log(
        &state.db,
        None,
        "external.user.create",
        Some(&user_id),
        serde_json::json!({"username": req.username.trim()}),
    )
    .await;

    Ok(Json(ExternalCreateUserResponse { id: user_id }))
}
