use std::collections::HashMap;

use axum::{
    extract::{Extension, Path, State},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, AuthContext, Result};

use crate::application::{audit::write_audit_log, password::hash_password};

#[derive(Debug, Serialize)]
pub struct AdminUserItem {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub status: String,
    pub failed_attempts: i32,
    pub locked_until: Option<String>,
    pub roles: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub fn admin_router() -> Router<AppContext> {
    Router::new().nest("/users", router())
}

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users", post(create_user))
        .route("/users/{id}/reset-password", post(reset_password))
        .route("/users/{id}/status", patch(update_status))
        .route("/users/{id}/unlock", post(unlock_user))
}

fn require_permission(auth: &AuthContext, permission: &str) -> Result<()> {
    if auth.has_role("admin") || auth.has_permission(permission) {
        return Ok(());
    }
    Err(AppError::Forbidden(format!(
        "Missing permission: {}",
        permission
    )))
}

pub async fn list_users(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<AdminUserItem>>> {
    require_permission(&auth, "user:list")?;

    let rows = sqlx::query(
        "SELECT id, username, display_name, status, failed_attempts, locked_until, created_at FROM users ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let role_rows = sqlx::query(
        r#"SELECT ur.user_id, r.code FROM user_roles ur
           INNER JOIN roles r ON r.id = ur.role_id"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut roles_by_user: HashMap<String, Vec<String>> = HashMap::new();
    for r in role_rows {
        let user_id: String = r.get("user_id");
        let role_code: String = r.get("code");
        roles_by_user.entry(user_id).or_default().push(role_code);
    }

    let users = rows
        .into_iter()
        .map(|r| {
            let user_id: String = r.get("id");
            AdminUserItem {
                id: user_id.clone(),
                username: r.get("username"),
                display_name: r.get("display_name"),
                status: r.get("status"),
                failed_attempts: r.get("failed_attempts"),
                locked_until: r
                    .get::<Option<chrono::DateTime<Utc>>, _>("locked_until")
                    .map(|t| t.to_rfc3339()),
                roles: roles_by_user.remove(&user_id).unwrap_or_default(),
                created_at: r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            }
        })
        .collect();

    Ok(Json(users))
}

pub async fn create_user(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<AdminUserItem>> {
    require_permission(&auth, "user:create")?;

    if req.username.trim().is_empty() || req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Username cannot be empty and password must be at least 8 characters".into(),
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

    let role_codes = req.roles.unwrap_or_else(|| vec!["member".to_string()]);
    let mut applied_roles = Vec::new();
    for role_code in role_codes {
        let role = sqlx::query("SELECT id FROM roles WHERE code = $1")
            .bind(&role_code)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("Unknown role: {}", role_code)))?;

        let role_id: String = role.get("id");
        sqlx::query("INSERT INTO user_roles (user_id, role_id, created_at) VALUES ($1, $2, NOW())")
            .bind(&user_id)
            .bind(role_id)
            .execute(&state.db)
            .await?;

        applied_roles.push(role_code);
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        "user.create",
        Some(&user_id),
        serde_json::json!({"roles": applied_roles}),
    )
    .await;

    Ok(Json(AdminUserItem {
        id: user_id,
        username: req.username.trim().to_string(),
        display_name: req.display_name.trim().to_string(),
        status: "active".into(),
        failed_attempts: 0,
        locked_until: None,
        roles: applied_roles,
        created_at: Utc::now().to_rfc3339(),
    }))
}

pub async fn reset_password(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<Json<serde_json::Value>> {
    require_permission(&auth, "user:reset_password")?;

    if req.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }

    let exists = sqlx::query("SELECT id FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!("User {} not found", user_id)));
    }

    let new_hash = hash_password(&req.new_password)?;
    sqlx::query("UPDATE users SET password_hash = $1, failed_attempts = 0, locked_until = NULL, updated_at = NOW() WHERE id = $2")
        .bind(new_hash)
        .bind(&user_id)
        .execute(&state.db)
        .await?;

    sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE user_id = $1")
        .bind(&user_id)
        .execute(&state.db)
        .await?;

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        "user.reset_password",
        Some(&user_id),
        serde_json::json!({}),
    )
    .await;

    Ok(Json(serde_json::json!({"message": "Password reset"})))
}

pub async fn update_status(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<serde_json::Value>> {
    require_permission(&auth, "user:change_status")?;

    if req.status != "active" && req.status != "disabled" {
        return Err(AppError::BadRequest(
            "status must be active or disabled".into(),
        ));
    }

    let result = sqlx::query("UPDATE users SET status = $1, updated_at = NOW() WHERE id = $2")
        .bind(&req.status)
        .bind(&user_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("User {} not found", user_id)));
    }

    if req.status == "disabled" {
        sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE user_id = $1")
            .bind(&user_id)
            .execute(&state.db)
            .await?;
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        "user.change_status",
        Some(&user_id),
        serde_json::json!({"status": req.status}),
    )
    .await;

    Ok(Json(serde_json::json!({"message": "Status updated"})))
}

pub async fn unlock_user(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_permission(&auth, "user:unlock")?;

    let result = sqlx::query(
        "UPDATE users SET failed_attempts = 0, locked_until = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(&user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("User {} not found", user_id)));
    }

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        "user.unlock",
        Some(&user_id),
        serde_json::json!({}),
    )
    .await;

    Ok(Json(serde_json::json!({"message": "User unlocked"})))
}
