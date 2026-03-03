use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, Result};

#[derive(Serialize, Deserialize, Clone)]
pub struct DockerConfig {
    pub id: String,
    pub name: String,
    pub port_mappings: serde_json::Value,
    pub env_vars: serde_json::Value,
    pub volume_mappings: serde_json::Value,
    pub init_script: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct CreateDockerConfigRequest {
    pub name: String,
    pub port_mappings: Option<serde_json::Value>,
    pub env_vars: Option<serde_json::Value>,
    pub volume_mappings: Option<serde_json::Value>,
    pub init_script: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateDockerConfigRequest {
    pub name: Option<String>,
    pub port_mappings: Option<serde_json::Value>,
    pub env_vars: Option<serde_json::Value>,
    pub volume_mappings: Option<serde_json::Value>,
    pub init_script: Option<String>,
}

pub async fn list_docker_configs(
    State(state): State<AppContext>,
) -> Result<Json<Vec<DockerConfig>>> {
    let rows = sqlx::query!(
        "SELECT id, name, port_mappings, env_vars, volume_mappings, init_script, created_at, updated_at FROM docker_configs ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    let configs = rows
        .into_iter()
        .map(|r| DockerConfig {
            id: r.id,
            name: r.name,
            port_mappings: serde_json::from_str(&r.port_mappings).unwrap_or(serde_json::json!([])),
            env_vars: serde_json::from_str(&r.env_vars).unwrap_or(serde_json::json!([])),
            volume_mappings: serde_json::from_str(&r.volume_mappings)
                .unwrap_or(serde_json::json!([])),
            init_script: r.init_script,
            created_at: r.created_at.to_string(),
            updated_at: r.updated_at.to_string(),
        })
        .collect();

    Ok(Json(configs))
}

pub async fn create_docker_config(
    State(state): State<AppContext>,
    Json(req): Json<CreateDockerConfigRequest>,
) -> Result<Json<DockerConfig>> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let port_mappings = req.port_mappings.unwrap_or(serde_json::json!([]));
    let env_vars = req.env_vars.unwrap_or(serde_json::json!([]));
    let volume_mappings = req.volume_mappings.unwrap_or(serde_json::json!([]));
    let init_script = req.init_script.unwrap_or_default();

    let port_mappings_str = port_mappings.to_string();
    let env_vars_str = env_vars.to_string();
    let volume_mappings_str = volume_mappings.to_string();

    sqlx::query!(
        "INSERT INTO docker_configs (id, name, port_mappings, env_vars, volume_mappings, init_script, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        id, req.name, port_mappings_str, env_vars_str, volume_mappings_str, init_script, now, now
    )
    .execute(&state.db)
    .await?;

    Ok(Json(DockerConfig {
        id,
        name: req.name,
        port_mappings,
        env_vars,
        volume_mappings,
        init_script,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn update_docker_config(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDockerConfigRequest>,
) -> Result<Json<DockerConfig>> {
    let existing = sqlx::query!(
        "SELECT id, name, port_mappings, env_vars, volume_mappings, init_script, created_at, updated_at FROM docker_configs WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("DockerConfig {} not found", id)))?;

    let name = req.name.unwrap_or(existing.name);
    let port_mappings = req.port_mappings.unwrap_or_else(|| {
        serde_json::from_str(&existing.port_mappings).unwrap_or(serde_json::json!([]))
    });
    let env_vars = req.env_vars.unwrap_or_else(|| {
        serde_json::from_str(&existing.env_vars).unwrap_or(serde_json::json!([]))
    });
    let volume_mappings = req.volume_mappings.unwrap_or_else(|| {
        serde_json::from_str(&existing.volume_mappings).unwrap_or(serde_json::json!([]))
    });
    let init_script = req.init_script.unwrap_or(existing.init_script);
    let now = Utc::now();

    let port_mappings_str = port_mappings.to_string();
    let env_vars_str = env_vars.to_string();
    let volume_mappings_str = volume_mappings.to_string();

    sqlx::query!(
        "UPDATE docker_configs SET name=$1, port_mappings=$2, env_vars=$3, volume_mappings=$4, init_script=$5, updated_at=$6 WHERE id=$7",
        name, port_mappings_str, env_vars_str, volume_mappings_str, init_script, now, id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(DockerConfig {
        id,
        name,
        port_mappings,
        env_vars,
        volume_mappings,
        init_script,
        created_at: existing.created_at.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn delete_docker_config(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM docker_configs WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("DockerConfig {} not found", id)));
    }

    Ok(Json(serde_json::json!({"message": "DockerConfig deleted"})))
}
