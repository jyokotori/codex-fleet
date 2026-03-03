use axum::{
    extract::{Path, State},
    Json,
};
use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ssh::client::{ensure_ssh_key, read_public_key, SshClientPool};
use shared_kernel::{AppContext, AppError, Result};

#[derive(Serialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub os_type: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    pub ip: String,
    pub port: Option<i64>,
    pub username: String,
    /// Optional: only needed if passwordless SSH is not yet configured.
    /// Used once to install the backend's public key; never stored.
    pub password: Option<String>,
    pub os_type: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateServerRequest {
    pub name: Option<String>,
    pub ip: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub os_type: Option<String>,
}

pub async fn list_servers(State(state): State<AppContext>) -> Result<Json<Vec<Server>>> {
    let rows = sqlx::query!(
        "SELECT id, name, ip, port, username, auth_type, os_type, status, created_at FROM servers ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    let servers = rows
        .into_iter()
        .map(|r| Server {
            id: r.id,
            name: r.name,
            ip: r.ip,
            port: r.port,
            username: r.username,
            auth_type: r.auth_type,
            os_type: r.os_type,
            status: r.status,
            created_at: r.created_at.to_string(),
        })
        .collect();

    Ok(Json(servers))
}

pub async fn create_server(
    State(state): State<AppContext>,
    Json(req): Json<CreateServerRequest>,
) -> Result<Json<Server>> {
    let port = req.port.unwrap_or(22) as u16;

    // Ensure the backend has an SSH key pair
    let key_path = ensure_ssh_key()
        .await
        .map_err(|e| AppError::Internal(format!("Cannot initialize SSH key: {}", e)))?;

    // Try passwordless auth first
    let passwordless_ok =
        SshClientPool::connect_passwordless(&req.ip, port, &req.username, &key_path)
            .await
            .is_ok();

    if !passwordless_ok {
        // Passwordless failed — need password to install the public key
        let password = req.password.as_deref().ok_or_else(|| {
            AppError::BadRequest(
                "Passwordless SSH is not configured on this server. \
                 Please provide the login password so we can auto-configure SSH key access."
                    .into(),
            )
        })?;

        // Connect via password auth
        let client = SshClientPool::connect_with_password(&req.ip, port, &req.username, password)
            .await
            .map_err(|e| {
                let msg = e.to_string().to_lowercase();
                if msg.contains("authentication")
                    || msg.contains("permission denied")
                    || msg.contains("incorrect")
                {
                    AppError::BadRequest(format!("Password authentication failed: {}", e))
                } else {
                    AppError::Ssh(format!("Cannot connect to server: {}", e))
                }
            })?;

        // Read the backend's public key and install it on the remote server.
        // Use base64 encoding to safely transfer the key without shell escaping issues.
        let pub_key = read_public_key(&key_path)
            .map_err(|e| AppError::Internal(format!("Cannot read SSH public key: {}", e)))?;

        let encoded = base64::engine::general_purpose::STANDARD.encode(pub_key.as_bytes());
        // Decode the key and append with a guaranteed leading newline so it never
        // gets merged onto the previous line if authorized_keys has no trailing newline.
        // Also skip adding if the exact key is already present (idempotent).
        let install_cmd = format!(
            "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
             touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && \
             KEY=$(echo '{encoded}' | base64 -d) && \
             grep -qF \"$KEY\" ~/.ssh/authorized_keys || printf '\\n%s\\n' \"$KEY\" >> ~/.ssh/authorized_keys && \
             echo 'ok'"
        );

        client
            .execute(&install_cmd)
            .await
            .map_err(|e| AppError::Ssh(format!("Failed to install SSH key: {}", e)))?;

        // Verify passwordless now works
        SshClientPool::connect_passwordless(&req.ip, port, &req.username, &key_path)
            .await
            .map_err(|e| {
                AppError::Ssh(format!(
                    "SSH key was installed but passwordless login still failed: {}",
                    e
                ))
            })?;
    }

    // Save server (always passwordless, password never stored)
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_string();
    let port_i64 = port as i64;
    let os_type = req.os_type.clone().unwrap_or_else(|| "linux".into());

    sqlx::query(
        "INSERT INTO servers (id, name, ip, port, username, auth_type, os_type, status) \
         VALUES ($1, $2, $3, $4, $5, 'passwordless', $6, 'online')",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.ip)
    .bind(port_i64)
    .bind(&req.username)
    .bind(&os_type)
    .execute(&state.db)
    .await?;

    Ok(Json(Server {
        id,
        name: req.name,
        ip: req.ip,
        port: port_i64,
        username: req.username,
        auth_type: "passwordless".into(),
        os_type,
        status: "online".into(),
        created_at: now,
    }))
}

pub async fn update_server(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateServerRequest>,
) -> Result<Json<Server>> {
    let existing = sqlx::query!(
        "SELECT id, name, ip, port, username, auth_type, os_type, status, created_at FROM servers WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Server {} not found", id)))?;

    let name = req.name.unwrap_or(existing.name);
    let ip = req.ip.unwrap_or(existing.ip);
    let port = req.port.unwrap_or(existing.port);
    let username = req.username.unwrap_or(existing.username);
    let os_type = req.os_type.unwrap_or(existing.os_type);

    sqlx::query("UPDATE servers SET name=$1, ip=$2, port=$3, username=$4, os_type=$5 WHERE id=$6")
        .bind(&name)
        .bind(&ip)
        .bind(port)
        .bind(&username)
        .bind(&os_type)
        .bind(&id)
        .execute(&state.db)
        .await?;

    Ok(Json(Server {
        id,
        name,
        ip,
        port,
        username,
        auth_type: existing.auth_type,
        os_type,
        status: existing.status,
        created_at: existing.created_at.to_string(),
    }))
}

pub async fn delete_server(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM servers WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Server {} not found", id)));
    }

    Ok(Json(serde_json::json!({"message": "Server deleted"})))
}

pub async fn test_server_connection(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct ServerRow {
        ip: String,
        port: i64,
        username: String,
    }

    let server =
        sqlx::query_as::<_, ServerRow>("SELECT ip, port, username FROM servers WHERE id = $1")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Server {} not found", id)))?;

    let key_path = ensure_ssh_key()
        .await
        .map_err(|e| AppError::Internal(format!("Cannot initialize SSH key: {}", e)))?;

    let result = SshClientPool::connect_passwordless(
        &server.ip,
        server.port as u16,
        &server.username,
        &key_path,
    )
    .await;

    match result {
        Ok(client) => {
            let output = client
                .execute("echo 'connection-ok' && uname -a")
                .await
                .unwrap_or_default();
            sqlx::query!("UPDATE servers SET status = 'online' WHERE id = $1", id)
                .execute(&state.db)
                .await?;
            Ok(Json(serde_json::json!({
                "status": "online",
                "message": "Connection successful",
                "output": output
            })))
        }
        Err(e) => {
            sqlx::query!("UPDATE servers SET status = 'offline' WHERE id = $1", id)
                .execute(&state.db)
                .await?;
            Err(AppError::Ssh(e.to_string()))
        }
    }
}
