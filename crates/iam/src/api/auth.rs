use axum::{
    extract::{Extension, State},
    http::HeaderMap,
    routing::{get, post, put},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use shared_kernel::{AppContext, AppError, AuthContext, Result};

use crate::application::{
    audit::write_audit_log,
    password::{hash_password, verify_password},
    token::{decode_refresh_token, fetch_auth_context, hash_token, issue_tokens},
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub status: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: LoginUser,
}

pub fn auth_router() -> Router<AppContext> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/refresh", post(refresh))
}

pub fn protected_auth_router() -> Router<AppContext> {
    Router::new().route("/logout", post(logout))
}

pub fn me_router() -> Router<AppContext> {
    Router::new()
        .route("/me", get(me))
        .route("/me/password", put(change_my_password))
        .route("/users", get(list_users_simple))
}

pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub async fn login(
    State(state): State<AppContext>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(AppError::BadRequest(
            "Username and password are required".into(),
        ));
    }

    let user_opt = sqlx::query(
        "SELECT id, username, display_name, password_hash, status, failed_attempts, locked_until FROM users WHERE username = $1",
    )
    .bind(req.username.trim())
    .fetch_optional(&state.db)
    .await?;

    let user = if let Some(u) = user_opt {
        u
    } else {
        write_audit_log(
            &state.db,
            None,
            "auth.login_failed",
            None,
            serde_json::json!({"username": req.username, "reason": "user_not_found"}),
        )
        .await;
        return Err(AppError::Unauthorized);
    };

    let user_id: String = user.get("id");
    let status: String = user.get("status");
    let failed_attempts: i32 = user.get("failed_attempts");
    let locked_until: Option<chrono::DateTime<Utc>> = user.get("locked_until");
    let password_hash: String = user.get("password_hash");

    if status != "active" {
        return Err(AppError::Forbidden("User is disabled".into()));
    }

    if let Some(locked_until) = locked_until {
        if locked_until > Utc::now() {
            return Err(AppError::Forbidden("User is temporarily locked".into()));
        }
    }

    if !verify_password(&req.password, &password_hash) {
        let next_failures = failed_attempts + 1;
        let lock_at = if next_failures >= state.config.max_login_failures {
            Some(Utc::now() + Duration::minutes(state.config.lock_minutes))
        } else {
            None
        };

        sqlx::query("UPDATE users SET failed_attempts = $1, locked_until = $2, updated_at = NOW() WHERE id = $3")
            .bind(next_failures)
            .bind(lock_at)
            .bind(&user_id)
            .execute(&state.db)
            .await?;

        write_audit_log(
            &state.db,
            Some(&user_id),
            "auth.login_failed",
            Some(&user_id),
            serde_json::json!({"reason": "invalid_password", "failed_attempts": next_failures}),
        )
        .await;

        return Err(AppError::Unauthorized);
    }

    sqlx::query(
        "UPDATE users SET failed_attempts = 0, locked_until = NULL, last_login_at = NOW(), updated_at = NOW() WHERE id = $1",
    )
    .bind(&user_id)
    .execute(&state.db)
    .await?;

    let pair = issue_tokens(
        &user_id,
        &state.config.jwt_secret,
        state.config.access_token_minutes,
        state.config.refresh_token_days,
    )?;
    let refresh_claims = decode_refresh_token(&pair.refresh_token, &state.config.jwt_secret)?;

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, jti, expires_at, revoked, created_at) VALUES ($1, $2, $3, $4, $5, false, NOW())",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&user_id)
    .bind(hash_token(&pair.refresh_token))
    .bind(refresh_claims.jti)
    .bind(chrono::DateTime::<Utc>::from_timestamp(refresh_claims.exp, 0).ok_or_else(|| AppError::Internal("Invalid exp".into()))?)
    .execute(&state.db)
    .await?;

    let auth = fetch_auth_context(&state.db, &user_id).await?;

    write_audit_log(
        &state.db,
        Some(&user_id),
        "auth.login_succeeded",
        Some(&user_id),
        serde_json::json!({}),
    )
    .await;

    Ok(Json(LoginResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_in: pair.expires_in,
        user: LoginUser {
            id: auth.user_id,
            username: auth.username,
            display_name: auth.display_name,
            status: auth.status,
            roles: auth.roles,
        },
    }))
}

pub async fn refresh(
    State(state): State<AppContext>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<LoginResponse>> {
    let claims = decode_refresh_token(&req.refresh_token, &state.config.jwt_secret)?;
    if claims.exp < Utc::now().timestamp() {
        return Err(AppError::Unauthorized);
    }

    let token_hash = hash_token(&req.refresh_token);
    let token_row = sqlx::query(
        "SELECT id, user_id, revoked, expires_at FROM refresh_tokens WHERE token_hash = $1 AND jti = $2",
    )
    .bind(token_hash)
    .bind(&claims.jti)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let revoked: bool = token_row.get("revoked");
    let expires_at: chrono::DateTime<Utc> = token_row.get("expires_at");
    if revoked || expires_at < Utc::now() {
        return Err(AppError::Unauthorized);
    }

    let user_id: String = token_row.get("user_id");

    sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE id = $1")
        .bind(token_row.get::<String, _>("id"))
        .execute(&state.db)
        .await?;

    let auth = fetch_auth_context(&state.db, &user_id).await?;
    if auth.status != "active" {
        return Err(AppError::Forbidden("User is disabled".into()));
    }

    let pair = issue_tokens(
        &user_id,
        &state.config.jwt_secret,
        state.config.access_token_minutes,
        state.config.refresh_token_days,
    )?;
    let refresh_claims = decode_refresh_token(&pair.refresh_token, &state.config.jwt_secret)?;

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, jti, expires_at, revoked, created_at) VALUES ($1, $2, $3, $4, $5, false, NOW())",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&user_id)
    .bind(hash_token(&pair.refresh_token))
    .bind(refresh_claims.jti)
    .bind(chrono::DateTime::<Utc>::from_timestamp(refresh_claims.exp, 0).ok_or_else(|| AppError::Internal("Invalid exp".into()))?)
    .execute(&state.db)
    .await?;

    Ok(Json(LoginResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_in: pair.expires_in,
        user: LoginUser {
            id: auth.user_id,
            username: auth.username,
            display_name: auth.display_name,
            status: auth.status,
            roles: auth.roles,
        },
    }))
}

pub async fn logout(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE user_id = $1")
        .bind(&auth.user_id)
        .execute(&state.db)
        .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        "auth.logout",
        Some(&auth.user_id),
        serde_json::json!({}),
    )
    .await;

    Ok(Json(serde_json::json!({"message": "Logged out"})))
}

pub async fn me(Extension(auth): Extension<AuthContext>) -> Result<Json<LoginUser>> {
    Ok(Json(LoginUser {
        id: auth.user_id,
        username: auth.username,
        display_name: auth.display_name,
        status: auth.status,
        roles: auth.roles,
    }))
}

pub async fn change_my_password(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "New password must be at least 8 characters".into(),
        ));
    }

    let row = sqlx::query("SELECT password_hash FROM users WHERE id = $1")
        .bind(&auth.user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    let old_hash: String = row.get("password_hash");
    if !verify_password(&req.old_password, &old_hash) {
        return Err(AppError::Forbidden("Old password is incorrect".into()));
    }

    let new_hash = hash_password(&req.new_password)?;
    sqlx::query("UPDATE users SET password_hash = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_hash)
        .bind(&auth.user_id)
        .execute(&state.db)
        .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        "user.change_own_password",
        Some(&auth.user_id),
        serde_json::json!({}),
    )
    .await;

    Ok(Json(serde_json::json!({"message": "Password updated"})))
}

#[derive(Debug, Serialize)]
pub struct SimpleUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
}

pub async fn list_users_simple(
    State(state): State<AppContext>,
) -> Result<Json<Vec<SimpleUser>>> {
    let rows = sqlx::query(
        "SELECT id, username, display_name FROM users WHERE status = 'active' ORDER BY display_name ASC",
    )
    .fetch_all(&state.db)
    .await?;

    let users = rows
        .into_iter()
        .map(|r| SimpleUser {
            id: r.get("id"),
            username: r.get("username"),
            display_name: r.get("display_name"),
        })
        .collect();

    Ok(Json(users))
}
