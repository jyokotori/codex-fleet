use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    AppState,
};

#[derive(Serialize)]
pub struct CompanyConfig {
    pub id: String,
    pub name: String,
    pub category: String,
    pub cli_type: String,
    pub file_type: Option<String>,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct CreateConfigRequest {
    pub name: String,
    pub category: Option<String>,
    pub cli_type: String,
    pub file_type: Option<String>,
    pub content: String,
}

#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub name: Option<String>,
    pub cli_type: Option<String>,
    pub file_type: Option<String>,
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct ListConfigsQuery {
    pub category: Option<String>,
    pub cli_type: Option<String>,
}

pub async fn list_configs(
    State(state): State<AppState>,
    Query(query): Query<ListConfigsQuery>,
) -> Result<Json<Vec<CompanyConfig>>> {
    let rows = sqlx::query!(
        "SELECT id, name, category, cli_type, file_type, content, created_at, updated_at FROM company_configs ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    let configs = rows
        .into_iter()
        .filter(|r| {
            query.category.as_deref().map_or(true, |c| r.category == c)
                && query.cli_type.as_deref().map_or(true, |ct| r.cli_type == ct)
        })
        .map(|r| CompanyConfig {
            id: r.id,
            name: r.name,
            category: r.category,
            cli_type: r.cli_type,
            file_type: r.file_type,
            content: r.content,
            created_at: r.created_at.to_string(),
            updated_at: r.updated_at.to_string(),
        })
        .collect();

    Ok(Json(configs))
}

pub async fn create_config(
    State(state): State<AppState>,
    Json(req): Json<CreateConfigRequest>,
) -> Result<Json<CompanyConfig>> {
    let category = req.category.unwrap_or_else(|| "config_file".to_string());

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    sqlx::query!(
        "INSERT INTO company_configs (id, name, category, cli_type, file_type, content, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        id, req.name, category, req.cli_type, req.file_type, req.content, now, now
    )
    .execute(&state.db)
    .await?;

    Ok(Json(CompanyConfig {
        id,
        name: req.name,
        category,
        cli_type: req.cli_type,
        file_type: req.file_type,
        content: req.content,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn update_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<CompanyConfig>> {
    let existing = sqlx::query!(
        "SELECT id, name, category, cli_type, file_type, content, created_at, updated_at FROM company_configs WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Config {} not found", id)))?;

    let name = req.name.unwrap_or(existing.name);
    let cli_type = req.cli_type.unwrap_or(existing.cli_type);
    let file_type = req.file_type.or(existing.file_type);
    let content = req.content.unwrap_or(existing.content);
    let now = Utc::now();

    sqlx::query!(
        "UPDATE company_configs SET name=?, cli_type=?, file_type=?, content=?, updated_at=? WHERE id=?",
        name, cli_type, file_type, content, now, id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(CompanyConfig {
        id,
        name,
        category: existing.category,
        cli_type,
        file_type,
        content,
        created_at: existing.created_at.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn delete_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM company_configs WHERE id = ?", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Config {} not found", id)));
    }

    Ok(Json(serde_json::json!({"message": "Config deleted"})))
}
