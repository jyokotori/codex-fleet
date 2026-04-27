use std::collections::HashMap;
use std::time::Duration;

use shared_kernel::AppContext;
use sqlx::Row;
use tracing::warn;

use crate::api::agents::{get_agent_with_credentials, sync_agent_status_with_creds};
use crate::api::tasks::dispatch_task_for_agent;
use crate::infrastructure::plane_client::PlaneClient;

pub async fn run_scheduler(state: AppContext) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        if let Err(e) = plane_tick(&state).await {
            tracing::error!("Plane scheduler tick error: {e}");
        }
    }
}

// ── Plane Task Scheduler ──

struct PlanePending {
    plane_task_id: String,
    #[allow(dead_code)]
    workspace_id: String,
    plane_issue_id: String,
    plane_project_id: String,
    assignee_email: String,
    base_url: String,
    workspace_slug: String,
    api_key: String,
    binding_id: String,
    #[allow(dead_code)]
    agent_group_id: String,
    accept_state_id: String,
    in_progress_state_id: String,
    completion_state_id: String,
    matching_count: i64,
    idle_agent_id: Option<String>,
}

async fn plane_tick(state: &AppContext) -> anyhow::Result<()> {
    let rows = sqlx::query(
        r#"SELECT pt.id AS plane_task_id,
                  pt.workspace_id, pt.plane_issue_id, pt.plane_project_id, pt.assignee_email,
                  pw.base_url, pw.workspace_slug, pw.api_key,
                  pb.id AS binding_id, pb.agent_group_id,
                  pb.accept_state_id, pb.in_progress_state_id, pb.completion_state_id,
                  m.matching_count,
                  i.idle_agent_id
           FROM plane_tasks pt
           JOIN plane_workspaces pw
             ON pw.id = pt.workspace_id AND pw.enabled = TRUE
           JOIN plane_bindings pb
             ON pb.workspace_id = pt.workspace_id
            AND pb.plane_project_id = pt.plane_project_id
            AND pb.enabled = TRUE
           LEFT JOIN LATERAL (
               SELECT COUNT(*)::bigint AS matching_count
               FROM agents a
               JOIN agent_group_members agm ON agm.agent_id = a.id AND agm.group_id = pb.agent_group_id
               JOIN users u ON u.id = a.user_id AND u.email = pt.assignee_email
           ) m ON TRUE
           LEFT JOIN LATERAL (
               SELECT a.id AS idle_agent_id
               FROM agents a
               JOIN agent_group_members agm ON agm.agent_id = a.id AND agm.group_id = pb.agent_group_id
               JOIN users u ON u.id = a.user_id AND u.email = pt.assignee_email
               WHERE a.status = 'running'
                 AND NOT EXISTS (
                     SELECT 1 FROM tasks t WHERE t.agent_id = a.id AND t.status = 'agent_in_progress'
                 )
               ORDER BY a.id LIMIT 1
           ) i ON TRUE
           WHERE pt.status = 'pending'
           ORDER BY pt.created_at ASC"#,
    )
    .fetch_all(&state.db)
    .await?;

    if rows.is_empty() {
        // Sweep orphans: pending tasks whose binding/workspace was deleted/disabled.
        sweep_orphans(state).await;
        return Ok(());
    }

    let pending: Vec<PlanePending> = rows
        .into_iter()
        .map(|r| PlanePending {
            plane_task_id: r.get("plane_task_id"),
            workspace_id: r.get("workspace_id"),
            plane_issue_id: r.get("plane_issue_id"),
            plane_project_id: r.get("plane_project_id"),
            assignee_email: r.get("assignee_email"),
            base_url: r.get("base_url"),
            workspace_slug: r.get("workspace_slug"),
            api_key: r.get("api_key"),
            binding_id: r.get("binding_id"),
            agent_group_id: r.get("agent_group_id"),
            accept_state_id: r.get("accept_state_id"),
            in_progress_state_id: r.get("in_progress_state_id"),
            completion_state_id: r.get("completion_state_id"),
            matching_count: r.try_get::<i64, _>("matching_count").unwrap_or(0),
            idle_agent_id: r.try_get("idle_agent_id").ok(),
        })
        .collect();

    for p in pending {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_pending(&state, p).await {
                warn!("Plane scheduler handle error: {e}");
            }
        });
    }

    sweep_orphans(state).await;
    Ok(())
}

async fn sweep_orphans(state: &AppContext) {
    // Pending tasks where the binding no longer exists (or is disabled / workspace disabled).
    // We only mark the row as 'rejected'; we cannot transition Plane state without binding.
    let _ = sqlx::query(
        r#"UPDATE plane_tasks pt
           SET status = 'rejected', updated_at = NOW()
           WHERE pt.status = 'pending'
             AND NOT EXISTS (
                 SELECT 1 FROM plane_bindings pb
                 JOIN plane_workspaces pw ON pw.id = pb.workspace_id
                 WHERE pb.workspace_id = pt.workspace_id
                   AND pb.plane_project_id = pt.plane_project_id
                   AND pb.enabled = TRUE
                   AND pw.enabled = TRUE
             )"#,
    )
    .execute(&state.db)
    .await;
}

async fn handle_pending(state: &AppContext, p: PlanePending) -> anyhow::Result<()> {
    let client = PlaneClient::new(&p.base_url, &p.workspace_slug, &p.api_key);

    // No matching agent at all — comment + transition to completion + reject.
    if p.matching_count == 0 {
        let comment = format!(
            "<p>No agent in the bound group is owned by the assignee email \
             <code>{}</code>; cannot dispatch.</p>",
            html_escape::encode_text(&p.assignee_email)
        );
        let _ = client
            .add_comment(&p.plane_project_id, &p.plane_issue_id, &comment)
            .await;
        let _ = client
            .update_issue_state_by_id(&p.plane_project_id, &p.plane_issue_id, &p.completion_state_id)
            .await;
        sqlx::query("UPDATE plane_tasks SET status = 'rejected', updated_at = NOW() WHERE id = $1")
            .bind(&p.plane_task_id)
            .execute(&state.db)
            .await?;
        return Ok(());
    }

    let agent_id = match p.idle_agent_id.as_deref() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return Ok(()), // Busy — wait next tick
    };

    // Acquire agent dispatch lock to avoid TOCTOU with concurrent dispatchers.
    let lock = state.agent_lock(&agent_id).await;
    let _guard = lock.lock().await;

    // Plane recheck — transient errors retry; data mismatches cancel the task.
    let snap = match client
        .get_issue_full(&p.plane_project_id, &p.plane_issue_id)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            warn!(
                "Plane scheduler: get_issue_full failed for {}: {e}; retrying next tick",
                p.plane_issue_id
            );
            return Ok(());
        }
    };

    if snap.state_id != p.accept_state_id {
        sqlx::query("UPDATE plane_tasks SET status = 'cancelled', updated_at = NOW() WHERE id = $1")
            .bind(&p.plane_task_id)
            .execute(&state.db)
            .await?;
        return Ok(());
    }

    // Recheck label intersection
    let bound_labels: Vec<(String, String, i32)> = sqlx::query(
        r#"SELECT label_id, cli_type, priority FROM plane_binding_labels
           WHERE binding_id = $1 ORDER BY priority ASC"#,
    )
    .bind(&p.binding_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|r| {
        (
            r.get::<String, _>("label_id"),
            r.get::<String, _>("cli_type"),
            r.get::<i32, _>("priority"),
        )
    })
    .collect();

    let issue_label_set: std::collections::HashSet<&String> = snap.label_ids.iter().collect();
    let matched_bound: Vec<&(String, String, i32)> = bound_labels
        .iter()
        .filter(|(lid, _, _)| issue_label_set.contains(lid))
        .collect();

    if matched_bound.is_empty() {
        sqlx::query("UPDATE plane_tasks SET status = 'cancelled', updated_at = NOW() WHERE id = $1")
            .bind(&p.plane_task_id)
            .execute(&state.db)
            .await?;
        return Ok(());
    }

    // Recheck assignee email is still on the issue
    let mut assignee_emails: Vec<String> = Vec::new();
    for uid in &snap.assignee_user_ids {
        if let Ok(em) = client.member_email(uid).await {
            if !em.is_empty() {
                assignee_emails.push(em);
            }
        }
    }
    if !assignee_emails.iter().any(|e| e == &p.assignee_email) {
        sqlx::query("UPDATE plane_tasks SET status = 'cancelled', updated_at = NOW() WHERE id = $1")
            .bind(&p.plane_task_id)
            .execute(&state.db)
            .await?;
        return Ok(());
    }

    // Pick a CLI: scan matched bound labels by priority, find first whose cli_type
    // is in the agent's installed cli_inits.
    let agent_clis: std::collections::HashSet<String> = sqlx::query_scalar!(
        "SELECT cli_type FROM agent_cli_inits WHERE agent_id = $1",
        agent_id
    )
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .collect();

    let picked_cli = matched_bound
        .iter()
        .find(|(_, cli, _)| agent_clis.contains(cli))
        .map(|(_, cli, _)| cli.clone());

    let picked_cli = match picked_cli {
        Some(c) => c,
        None => {
            let comment = "<p>None of the issue's bound labels map to a CLI installed on the assigned agent.</p>";
            let _ = client
                .add_comment(&p.plane_project_id, &p.plane_issue_id, comment)
                .await;
            let _ = client
                .update_issue_state_by_id(
                    &p.plane_project_id,
                    &p.plane_issue_id,
                    &p.completion_state_id,
                )
                .await;
            sqlx::query("UPDATE plane_tasks SET status = 'rejected', updated_at = NOW() WHERE id = $1")
                .bind(&p.plane_task_id)
                .execute(&state.db)
                .await?;
            return Ok(());
        }
    };

    // Verify agent is still running before dispatch
    let (creds, agent_info) = match get_agent_with_credentials(state, &agent_id).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let synced = match sync_agent_status_with_creds(state, &agent_id, &creds, &agent_info).await {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    if synced != "running" {
        return Ok(());
    }

    // Dispatch — pass the picked CLI in metadata via tags.
    let mut meta: HashMap<String, String> = HashMap::new();
    meta.insert("cli_type".into(), picked_cli.clone());

    match dispatch_task_for_agent(
        state,
        &agent_id,
        &snap.title,
        &snap.description,
        vec![],
        None,
        String::new(),
    )
    .await
    {
        Ok(dispatched) => {
            sqlx::query(
                "UPDATE plane_tasks SET status = 'dispatched', agent_id = $1, task_id = $2, updated_at = NOW() WHERE id = $3",
            )
            .bind(&agent_id)
            .bind(&dispatched.id)
            .bind(&p.plane_task_id)
            .execute(&state.db)
            .await?;

            if let Err(e) = client
                .update_issue_state_by_id(
                    &p.plane_project_id,
                    &p.plane_issue_id,
                    &p.in_progress_state_id,
                )
                .await
            {
                warn!(
                    "Plane scheduler: failed to set in_progress state for {}: {e}",
                    p.plane_issue_id
                );
            }

            let agent_name = sqlx::query_scalar::<_, String>("SELECT name FROM agents WHERE id = $1")
                .bind(&agent_id)
                .fetch_optional(&state.db)
                .await?
                .unwrap_or_else(|| agent_id.clone());
            let comment = format!(
                "<p>Dispatched to agent <strong>{}</strong> via <code>{}</code></p>",
                html_escape::encode_text(&agent_name),
                html_escape::encode_text(&picked_cli),
            );
            let _ = client
                .add_comment(&p.plane_project_id, &p.plane_issue_id, &comment)
                .await;
        }
        Err(e) => {
            warn!(
                "Plane scheduler: failed to dispatch issue {} to agent {}: {e}",
                p.plane_issue_id, agent_id
            );
        }
    }

    Ok(())
}
