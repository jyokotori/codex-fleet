use std::time::Duration;

use sqlx::Row;
use shared_kernel::AppContext;

use crate::api::agents::{get_agent_with_credentials, sync_agent_status_with_creds};
use crate::api::tasks::dispatch_task_for_agent;
use crate::infrastructure::plane_client::PlaneClient;

pub async fn run_scheduler(state: AppContext) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        if let Err(e) = tick(&state).await {
            tracing::error!("Scheduler tick error: {e}");
        }
        if let Err(e) = plane_tick(&state).await {
            tracing::error!("Plane scheduler tick error: {e}");
        }
    }
}

/// Row returned by the single scheduler query.
struct DispatchCandidate {
    agent_id: String,
    work_item_id: String,
    title: String,
    description: String,
    notification_ids: String,
    assigned_user_id: Option<String>,
    assigned_username: String,
}

async fn tick(state: &AppContext) -> anyhow::Result<()> {
    // Single JOIN query: one work_item per idle running agent, highest priority first.
    // DISTINCT ON (a.id) ensures DB-level dedup — exactly 1 row per agent.
    let rows = sqlx::query(
        r#"SELECT DISTINCT ON (a.id)
                  a.id AS agent_id,
                  wi.id AS work_item_id,
                  wi.title,
                  wi.description,
                  wi.notification_ids,
                  wi.assigned_user_id,
                  wi.assigned_username
           FROM agents a
           INNER JOIN work_items wi ON wi.assigned_agent_id = a.id AND wi.status = 'waiting'
           WHERE a.status = 'running'
             AND NOT EXISTS (
               SELECT 1 FROM work_items wi2
               WHERE wi2.assigned_agent_id = a.id AND wi2.status IN ('agent_in_progress','agent_completed')
             )
             AND NOT EXISTS (
               SELECT 1 FROM tasks t
               WHERE t.agent_id = a.id AND t.status = 'agent_in_progress'
             )
           ORDER BY a.id,
                    CASE wi.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 END,
                    wi.created_at"#,
    )
    .fetch_all(&state.db)
    .await?;

    let candidates: Vec<DispatchCandidate> = rows
        .into_iter()
        .map(|r| DispatchCandidate {
            agent_id: r.get("agent_id"),
            work_item_id: r.get("work_item_id"),
            title: r.get("title"),
            description: r.get("description"),
            notification_ids: r.get("notification_ids"),
            assigned_user_id: r.get("assigned_user_id"),
            assigned_username: r.get("assigned_username"),
        })
        .collect();

    for candidate in candidates {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = dispatch_one(&state, candidate).await {
                tracing::warn!("Scheduler dispatch error: {e}");
            }
        });
    }

    Ok(())
}

async fn revert_work_item(db: &sqlx::PgPool, work_item_id: &str) {
    let _ = sqlx::query("UPDATE work_items SET status = 'waiting', updated_at = NOW() WHERE id = $1")
        .bind(work_item_id)
        .execute(db)
        .await;
}

async fn dispatch_one(state: &AppContext, c: DispatchCandidate) -> anyhow::Result<()> {
    // Per-agent mutex: skip if already dispatching for this agent
    let lock = state.agent_lock(&c.agent_id).await;
    let guard = match lock.try_lock() {
        Ok(g) => g,
        Err(_) => {
            tracing::debug!("Scheduler: agent {} already dispatching, skipping", c.agent_id);
            return Ok(());
        }
    };

    // Atomic claim: UPDATE ... WHERE status = 'waiting' with rows_affected check
    let claim = sqlx::query(
        "UPDATE work_items SET status = 'agent_in_progress', updated_at = NOW() WHERE id = $1 AND status = 'waiting'",
    )
    .bind(&c.work_item_id)
    .execute(&state.db)
    .await?;

    if claim.rows_affected() != 1 {
        drop(guard);
        return Ok(());
    }

    // Fetch credentials and sync agent status
    let (creds, agent_info) = match get_agent_with_credentials(state, &c.agent_id).await {
        Ok(v) => v,
        Err(e) => {
            revert_work_item(&state.db, &c.work_item_id).await;
            drop(guard);
            return Err(e.into());
        }
    };

    let synced_status = match sync_agent_status_with_creds(state, &c.agent_id, &creds, &agent_info).await {
        Ok(s) => s,
        Err(e) => {
            revert_work_item(&state.db, &c.work_item_id).await;
            drop(guard);
            return Err(e.into());
        }
    };

    if synced_status != "running" {
        revert_work_item(&state.db, &c.work_item_id).await;
        drop(guard);
        tracing::info!(
            "Scheduler: agent {} is {}, reverting work item {}",
            c.agent_id,
            synced_status,
            c.work_item_id
        );
        return Ok(());
    }

    let notif_ids: Vec<String> =
        serde_json::from_str(&c.notification_ids).unwrap_or_default();

    match dispatch_task_for_agent(
        state,
        &c.agent_id,
        &c.title,
        &c.description,
        Some(c.work_item_id.clone()),
        notif_ids,
        c.assigned_user_id.clone(),
        c.assigned_username.clone(),
    )
    .await
    {
        Ok(task) => {
            sqlx::query("UPDATE work_items SET execution_id = $1, updated_at = NOW() WHERE id = $2")
                .bind(&task.id)
                .bind(&c.work_item_id)
                .execute(&state.db)
                .await?;
            tracing::info!(
                "Scheduler dispatched work item {} to agent {}",
                c.work_item_id,
                c.agent_id
            );
        }
        Err(e) => {
            revert_work_item(&state.db, &c.work_item_id).await;
            tracing::warn!(
                "Scheduler failed to dispatch work item {} to agent {}: {e}",
                c.work_item_id,
                c.agent_id
            );
        }
    }

    drop(guard);
    Ok(())
}

// ── Plane Task Scheduler ──

struct PlanePendingTask {
    id: String,
    workspace_id: String,
    plane_issue_id: String,
    plane_project_id: String,
    assignee_email: String,
    base_url: String,
    workspace_slug: String,
    api_key: String,
}

async fn plane_tick(state: &AppContext) -> anyhow::Result<()> {
    // Only pick up pending tasks whose workspace is currently enabled.
    let rows = sqlx::query(
        r#"SELECT pt.id, pt.workspace_id, pt.plane_issue_id, pt.plane_project_id, pt.assignee_email,
                  pw.base_url, pw.workspace_slug, pw.api_key
           FROM plane_tasks pt
           INNER JOIN plane_workspaces pw ON pw.id = pt.workspace_id AND pw.enabled = true
           WHERE pt.status = 'pending'
           ORDER BY pt.created_at ASC"#,
    )
    .fetch_all(&state.db)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    let tasks: Vec<PlanePendingTask> = rows
        .into_iter()
        .map(|r| PlanePendingTask {
            id: r.get("id"),
            workspace_id: r.get("workspace_id"),
            plane_issue_id: r.get("plane_issue_id"),
            plane_project_id: r.get("plane_project_id"),
            assignee_email: r.get("assignee_email"),
            base_url: r.get("base_url"),
            workspace_slug: r.get("workspace_slug"),
            api_key: r.get("api_key"),
        })
        .collect();

    for task in tasks {
        let state = state.clone();
        tokio::spawn(async move {
            let client = PlaneClient::new(&task.base_url, &task.workspace_slug, &task.api_key);
            if let Err(e) = plane_dispatch_one(&state, &client, task).await {
                tracing::warn!("Plane scheduler dispatch error: {e}");
            }
        });
    }

    Ok(())
}

async fn plane_dispatch_one(
    state: &AppContext,
    client: &PlaneClient,
    task: PlanePendingTask,
) -> anyhow::Result<()> {
    // Find binding for this (workspace, project)
    let binding = sqlx::query(
        "SELECT agent_group_id FROM plane_bindings WHERE workspace_id = $1 AND plane_project_id = $2 AND enabled = true LIMIT 1",
    )
    .bind(&task.workspace_id)
    .bind(&task.plane_project_id)
    .fetch_optional(&state.db)
    .await?;

    let group_id: String = match binding {
        Some(r) => r.get("agent_group_id"),
        None => {
            tracing::warn!(
                "Plane scheduler: no binding for project {}, skipping plane_task {}",
                task.plane_project_id,
                task.id
            );
            return Ok(());
        }
    };

    // Find idle agent in the group, optionally filtered by assignee email
    let agent_row = if task.assignee_email.is_empty() {
        // Match any idle agent in the group
        sqlx::query(
            r#"SELECT a.id AS agent_id
               FROM agents a
               INNER JOIN agent_group_members agm ON agm.agent_id = a.id AND agm.group_id = $1
               WHERE a.status = 'running'
                 AND NOT EXISTS (
                   SELECT 1 FROM tasks t WHERE t.agent_id = a.id AND t.status = 'agent_in_progress'
                 )
               LIMIT 1"#,
        )
        .bind(&group_id)
        .fetch_optional(&state.db)
        .await?
    } else {
        // Match idle agent owned by user with matching email
        sqlx::query(
            r#"SELECT a.id AS agent_id
               FROM agents a
               INNER JOIN agent_group_members agm ON agm.agent_id = a.id AND agm.group_id = $1
               INNER JOIN users u ON u.id = a.user_id AND u.email = $2
               WHERE a.status = 'running'
                 AND NOT EXISTS (
                   SELECT 1 FROM tasks t WHERE t.agent_id = a.id AND t.status = 'agent_in_progress'
                 )
               LIMIT 1"#,
        )
        .bind(&group_id)
        .bind(&task.assignee_email)
        .fetch_optional(&state.db)
        .await?
    };

    let agent_id: String = match agent_row {
        Some(r) => r.get("agent_id"),
        None => return Ok(()), // No idle agent, will retry next tick
    };

    // ★ Query Plane for current issue state before dispatching
    let current_state = match client.get_issue_state_name(&task.plane_project_id, &task.plane_issue_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Plane scheduler: failed to get issue state for {}: {e}", task.plane_issue_id);
            return Ok(()); // Retry next tick
        }
    };

    if current_state != "Todo" {
        tracing::info!(
            "Plane scheduler: issue {} is now '{}' (not Todo), cancelling plane_task {}",
            task.plane_issue_id,
            current_state,
            task.id
        );
        sqlx::query("UPDATE plane_tasks SET status = 'cancelled', updated_at = NOW() WHERE id = $1")
            .bind(&task.id)
            .execute(&state.db)
            .await?;
        return Ok(());
    }

    // Fetch latest issue data from Plane
    let (title, description) = match client.get_issue(&task.plane_project_id, &task.plane_issue_id).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("Plane scheduler: failed to fetch issue {}: {e}", task.plane_issue_id);
            return Ok(());
        }
    };

    // Verify agent is still running
    let (creds, agent_info) = match get_agent_with_credentials(state, &agent_id).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let synced_status = match sync_agent_status_with_creds(state, &agent_id, &creds, &agent_info).await {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    if synced_status != "running" {
        return Ok(());
    }

    // Dispatch task
    match dispatch_task_for_agent(
        state,
        &agent_id,
        &title,
        &description,
        None, // no codex-fleet work_item
        vec![],
        None,
        String::new(),
    )
    .await
    {
        Ok(dispatched_task) => {
            // Update plane_task record
            sqlx::query(
                "UPDATE plane_tasks SET status = 'dispatched', agent_id = $1, task_id = $2, updated_at = NOW() WHERE id = $3",
            )
            .bind(&agent_id)
            .bind(&dispatched_task.id)
            .bind(&task.id)
            .execute(&state.db)
            .await?;

            // Set Plane issue state to "In Progress" (unconditional — actually working)
            if let Err(e) = client.update_issue_state(&task.plane_project_id, &task.plane_issue_id, "In Progress").await {
                tracing::warn!("Plane scheduler: failed to set In Progress for {}: {e}", task.plane_issue_id);
            }

            // Add dispatch comment
            let agent_name = sqlx::query_scalar::<_, String>("SELECT name FROM agents WHERE id = $1")
                .bind(&agent_id)
                .fetch_optional(&state.db)
                .await?
                .unwrap_or_else(|| agent_id.clone());

            let comment = format!("<p>Task dispatched to agent <strong>{}</strong></p>", agent_name);
            if let Err(e) = client.add_comment(&task.plane_project_id, &task.plane_issue_id, &comment).await {
                tracing::warn!("Plane scheduler: failed to add comment for {}: {e}", task.plane_issue_id);
            }

            tracing::info!(
                "Plane scheduler: dispatched issue {} to agent {} (plane_task={})",
                task.plane_issue_id,
                agent_id,
                task.id
            );
        }
        Err(e) => {
            tracing::warn!(
                "Plane scheduler: failed to dispatch issue {} to agent {}: {e}",
                task.plane_issue_id,
                agent_id
            );
        }
    }

    Ok(())
}
