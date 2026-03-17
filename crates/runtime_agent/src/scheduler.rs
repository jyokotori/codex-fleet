use std::time::Duration;

use sqlx::Row;
use shared_kernel::AppContext;

use crate::api::agents::{get_agent_with_credentials, sync_agent_status_with_creds};
use crate::api::tasks::dispatch_task_for_agent;

pub async fn run_scheduler(state: AppContext) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        if let Err(e) = tick(&state).await {
            tracing::error!("Scheduler tick error: {e}");
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
