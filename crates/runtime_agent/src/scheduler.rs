use std::time::Duration;

use shared_kernel::AppContext;

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

async fn tick(state: &AppContext) -> anyhow::Result<()> {
    // Find running agents that are idle (no work_item in agent_in_progress/agent_completed, no running task)
    let idle_agents = sqlx::query_scalar!(
        r#"SELECT a.id AS "id!"
           FROM agents a
           WHERE a.status = 'running'
             AND NOT EXISTS (
               SELECT 1 FROM work_items wi
               WHERE wi.assigned_agent_id = a.id AND wi.status IN ('agent_in_progress','agent_completed')
             )
             AND NOT EXISTS (
               SELECT 1 FROM tasks t
               WHERE t.agent_id = a.id AND t.status = 'agent_in_progress'
             )"#
    )
    .fetch_all(&state.db)
    .await?;

    for agent_id in idle_agents {
        // Find highest priority waiting work item for this agent
        let item = sqlx::query!(
            r#"SELECT id, title, description, notification_ids, assigned_user_id, assigned_username FROM work_items
               WHERE assigned_agent_id = $1 AND status = 'waiting'
               ORDER BY CASE priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 END, created_at
               LIMIT 1"#,
            agent_id
        )
        .fetch_optional(&state.db)
        .await?;

        let item = match item {
            Some(i) => i,
            None => continue,
        };

        // Use a transaction to atomically claim the work item and dispatch
        let mut tx = state.db.begin().await?;

        // Re-verify status = 'waiting' with FOR UPDATE lock
        let still_waiting = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM work_items WHERE id = $1 AND status = 'waiting' FOR UPDATE"#,
            item.id
        )
        .fetch_optional(&mut *tx)
        .await?;

        if still_waiting.is_none() {
            // Another scheduler tick or user action already claimed it
            tx.rollback().await?;
            continue;
        }

        // Dispatch task (this uses the main db pool, not the transaction)
        let notif_ids: Vec<String> =
            serde_json::from_str(&item.notification_ids).unwrap_or_default();
        match dispatch_task_for_agent(
            state,
            &agent_id,
            &item.title,
            &item.description,
            Some(item.id.clone()),
            notif_ids,
            item.assigned_user_id.clone(),
            item.assigned_username.clone(),
        )
        .await
        {
            Ok(task) => {
                // Update work item: status = agent_in_progress, link execution_id
                sqlx::query!(
                    "UPDATE work_items SET status = 'agent_in_progress', execution_id = $1, updated_at = NOW() WHERE id = $2",
                    task.id,
                    item.id
                )
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                tracing::info!(
                    "Scheduler dispatched work item {} to agent {}",
                    item.id,
                    agent_id
                );
            }
            Err(e) => {
                tx.rollback().await?;
                tracing::warn!(
                    "Scheduler failed to dispatch work item {} to agent {}: {e}",
                    item.id,
                    agent_id
                );
            }
        }
    }

    Ok(())
}
