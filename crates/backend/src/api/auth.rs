use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    crypto::Crypto,
    error::{AppError, Result},
    AppState,
};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
    pub display_name: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>> {
    let user = sqlx::query!(
        "SELECT id, username, display_name, password_encrypted FROM users WHERE username = $1",
        req.username
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let crypto = Crypto::new(&state.config.master_key);
    let decrypted = crypto
        .decrypt(&user.password_encrypted)
        .map_err(|_| AppError::Unauthorized)?;

    if decrypted != req.password {
        return Err(AppError::Unauthorized);
    }

    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::days(30);

    let user_id = user.id.clone();
    sqlx::query!(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)",
        token,
        user_id,
        expires_at
    )
    .execute(&state.db)
    .await?;

    Ok(Json(LoginResponse {
        token,
        user_id: user.id,
        username: user.username,
        display_name: user.display_name,
    }))
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.username.trim().is_empty() || req.password.len() < 3 {
        return Err(AppError::BadRequest(
            "Username cannot be empty and password must be at least 3 characters".into(),
        ));
    }

    let existing = sqlx::query!("SELECT id FROM users WHERE username = $1", req.username)
        .fetch_optional(&state.db)
        .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Username already exists".into()));
    }

    let crypto = Crypto::new(&state.config.master_key);
    let password_encrypted = crypto
        .encrypt(&req.password)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let id = Uuid::new_v4().to_string();
    sqlx::query!(
        "INSERT INTO users (id, username, display_name, password_encrypted) VALUES ($1, $2, $3, $4)",
        id,
        req.username,
        req.display_name,
        password_encrypted
    )
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({"message": "User registered successfully"})))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let token = extract_token(&headers).ok_or(AppError::Unauthorized)?;
    sqlx::query!("DELETE FROM sessions WHERE id = $1", token)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({"message": "Logged out"})))
}

pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Middleware to validate session tokens
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> std::result::Result<Response, AppError> {
    let token = extract_token(request.headers()).ok_or(AppError::Unauthorized)?;

    let now = Utc::now();
    let session = sqlx::query!(
        "SELECT user_id FROM sessions WHERE id = $1 AND expires_at > $2",
        token,
        now
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let _ = session.user_id; // validate session exists

    Ok(next.run(request).await)
}
