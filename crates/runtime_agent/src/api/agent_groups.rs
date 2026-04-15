use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared_kernel::{AppContext, Result};

#[derive(Serialize)]
pub struct AgentGroup {
    pub id: String,
    pub name: String,
    pub agent_ids: Vec<String>,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateAgentGroupRequest {
    pub name: String,
    #[serde(default)]
    pub agent_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateAgentGroupRequest {
    pub name: Option<String>,
    pub agent_ids: Option<Vec<String>>,
}

pub async fn list_agent_groups(
    State(state): State<AppContext>,
) -> Result<Json<Vec<AgentGroup>>> {
    let rows = sqlx::query!(
        "SELECT id, name, created_at FROM agent_groups ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    let mut groups = Vec::with_capacity(rows.len());
    for r in rows {
        let members = sqlx::query_scalar!(
            "SELECT agent_id FROM agent_group_members WHERE group_id = $1",
            r.id
        )
        .fetch_all(&state.db)
        .await?;

        groups.push(AgentGroup {
            id: r.id,
            name: r.name,
            agent_ids: members,
            created_at: r.created_at.to_string(),
        });
    }

    Ok(Json(groups))
}

pub async fn create_agent_group(
    State(state): State<AppContext>,
    Json(req): Json<CreateAgentGroupRequest>,
) -> Result<(StatusCode, Json<AgentGroup>)> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    sqlx::query!(
        "INSERT INTO agent_groups (id, name, created_at) VALUES ($1, $2, $3)",
        id,
        req.name,
        now
    )
    .execute(&state.db)
    .await?;

    for agent_id in &req.agent_ids {
        sqlx::query!(
            "INSERT INTO agent_group_members (group_id, agent_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            id,
            agent_id
        )
        .execute(&state.db)
        .await?;
    }

    Ok((
        StatusCode::CREATED,
        Json(AgentGroup {
            id,
            name: req.name,
            agent_ids: req.agent_ids,
            created_at: now.to_string(),
        }),
    ))
}

pub async fn update_agent_group(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentGroupRequest>,
) -> Result<Json<AgentGroup>> {
    let row = sqlx::query!(
        "SELECT id, name, created_at FROM agent_groups WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| shared_kernel::AppError::NotFound("Agent group not found".into()))?;

    let name = req.name.unwrap_or(row.name);

    sqlx::query!(
        "UPDATE agent_groups SET name = $1 WHERE id = $2",
        name,
        id
    )
    .execute(&state.db)
    .await?;

    let agent_ids = if let Some(ids) = req.agent_ids {
        sqlx::query!("DELETE FROM agent_group_members WHERE group_id = $1", id)
            .execute(&state.db)
            .await?;
        for agent_id in &ids {
            sqlx::query!(
                "INSERT INTO agent_group_members (group_id, agent_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                id,
                agent_id
            )
            .execute(&state.db)
            .await?;
        }
        ids
    } else {
        sqlx::query_scalar!(
            "SELECT agent_id FROM agent_group_members WHERE group_id = $1",
            id
        )
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(AgentGroup {
        id,
        name,
        agent_ids,
        created_at: row.created_at.to_string(),
    }))
}

pub async fn delete_agent_group(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    sqlx::query!("DELETE FROM agent_groups WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
