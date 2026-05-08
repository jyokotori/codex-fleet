use sqlx::{PgPool, Row};
use tracing;

#[derive(Clone)]
struct DingTalkConfig {
    app_key: String,
    app_secret: String,
    robot_code: String,
}

async fn load_dingtalk_config(db: &PgPool) -> Option<DingTalkConfig> {
    let row = sqlx::query(
        "SELECT config_json, enabled FROM third_party_app_configs WHERE provider = 'dingtalk'",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()?;

    let enabled: bool = row.try_get("enabled").ok()?;
    if !enabled {
        return None;
    }
    let config_json: String = row.try_get("config_json").ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&config_json).ok()?;
    let app_key = parsed
        .get("app_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let app_secret = parsed
        .get("app_secret")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let robot_code = parsed
        .get("robot_code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if app_key.is_empty() || app_secret.is_empty() || robot_code.is_empty() {
        return None;
    }
    Some(DingTalkConfig {
        app_key,
        app_secret,
        robot_code,
    })
}

/// Send notifications for a task status change.
/// Looks up the given notification config IDs, checks if each is enabled
/// and subscribed to the event, then POSTs the payload to the webhook URL.
pub async fn send_task_notification(
    db: &PgPool,
    notification_ids: &[String],
    new_status: &str,
    payload: serde_json::Value,
) {
    if notification_ids.is_empty() {
        return;
    }

    let configs = match sqlx::query(
        r#"SELECT id, "type", config_json, events_json
           FROM notification_configs
           WHERE id = ANY($1) AND enabled = true"#,
    )
    .bind(notification_ids.to_vec())
    .fetch_all(db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("Failed to query notification_configs: {e}");
            return;
        }
    };

    let client = reqwest::Client::new();

    for row in configs {
        let config_id: String = row.get("id");
        let config_type: String = row.get("type");
        let config_json: String = row.get("config_json");
        let events_json: String = row.get("events_json");

        let events: Vec<String> = serde_json::from_str(&events_json).unwrap_or_default();
        if !events.iter().any(|e| e == new_status) {
            continue;
        }

        match config_type.as_str() {
            "webhook" => {
                send_webhook_notification(&client, &config_json, &payload, new_status).await
            }
            "dingtalk" => {
                send_dingtalk_notification(db, &payload, new_status, &config_id).await
            }
            other => tracing::warn!("Unsupported notification type {other} for config {config_id}"),
        }
    }
}

async fn send_webhook_notification(
    client: &reqwest::Client,
    config_json: &str,
    payload: &serde_json::Value,
    new_status: &str,
) {
    let config_data: serde_json::Value = match serde_json::from_str(config_json) {
        Ok(v) => v,
        Err(_) => return,
    };

    let url = match config_data.get("url").and_then(|u| u.as_str()) {
        Some(u) => u.to_string(),
        None => return,
    };

    let mut builder = client.post(&url).json(payload);

    if let Some(headers) = config_data.get("headers").and_then(|h| h.as_object()) {
        for (k, v) in headers {
            if let Some(v_str) = v.as_str() {
                builder = builder.header(k.as_str(), v_str);
            }
        }
    }

    if let Err(e) = builder.send().await {
        tracing::warn!("Webhook notification to {} failed: {e}", url);
    } else {
        tracing::info!(
            "Webhook notification sent to {} for event {}",
            url,
            new_status
        );
    }
}

async fn send_dingtalk_notification(
    db: &PgPool,
    payload: &serde_json::Value,
    new_status: &str,
    config_id: &str,
) {
    let config = match load_dingtalk_config(db).await {
        Some(c) => c,
        None => {
            tracing::warn!(
                "DingTalk notification {config_id} skipped: integration disabled or not configured"
            );
            return;
        }
    };

    let agent_id = match payload
        .get("task")
        .and_then(|task| task.get("agent_id"))
        .and_then(|v| v.as_str())
    {
        Some(agent_id) if !agent_id.is_empty() => agent_id,
        _ => {
            tracing::warn!(
                "DingTalk notification {config_id} skipped: payload has no task.agent_id"
            );
            return;
        }
    };

    let recipient = match sqlx::query(
        r#"SELECT a.name AS agent_name, u.dingtalk_userid
           FROM agents a
           LEFT JOIN users u ON u.id = a.user_id
           WHERE a.id = $1"#,
    )
    .bind(agent_id)
    .fetch_optional(db)
    .await
    {
        Ok(Some(row)) => {
            let dingtalk_userid: Option<String> = row.try_get("dingtalk_userid").ok();
            let agent_name: String = row
                .try_get("agent_name")
                .unwrap_or_else(|_| agent_id.to_string());
            (agent_name, dingtalk_userid.unwrap_or_default())
        }
        Ok(None) => {
            tracing::warn!("DingTalk notification {config_id} skipped: agent {agent_id} not found");
            return;
        }
        Err(e) => {
            tracing::warn!(
                "DingTalk notification {config_id} skipped: failed to query recipient: {e}"
            );
            return;
        }
    };

    let (agent_name, dingtalk_userid) = recipient;
    if dingtalk_userid.trim().is_empty() {
        tracing::info!(
            "DingTalk notification {config_id} skipped: agent {agent_id} has no assigned DingTalk user"
        );
        return;
    }

    let access_token = match get_dingtalk_access_token(&config).await {
        Ok(token) => token,
        Err(e) => {
            tracing::warn!("DingTalk notification {config_id} skipped: failed to get token: {e}");
            return;
        }
    };

    let markdown = build_dingtalk_markdown(payload, new_status, &agent_name, agent_id);
    let title = format!("Codex Fleet {}", display_status(new_status));
    let msg_param = serde_json::json!({
        "title": title,
        "text": markdown,
    })
    .to_string();

    let body = serde_json::json!({
        "robotCode": config.robot_code,
        "userIds": [dingtalk_userid],
        "msgKey": "sampleMarkdown",
        "msgParam": msg_param,
    });

    let result = reqwest::Client::new()
        .post("https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend")
        .header("x-acs-dingtalk-access-token", access_token)
        .json(&body)
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("DingTalk notification {config_id} sent for event {new_status}");
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("DingTalk notification {config_id} failed: {status} {body}");
        }
        Err(e) => tracing::warn!("DingTalk notification {config_id} failed: {e}"),
    }
}

async fn get_dingtalk_access_token(config: &DingTalkConfig) -> anyhow::Result<String> {
    let resp: serde_json::Value = reqwest::Client::new()
        .post("https://api.dingtalk.com/v1.0/oauth2/accessToken")
        .json(&serde_json::json!({
            "appKey": config.app_key,
            "appSecret": config.app_secret,
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    resp.get("accessToken")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("DingTalk accessToken missing in response"))
}

fn build_dingtalk_markdown(
    payload: &serde_json::Value,
    new_status: &str,
    agent_name: &str,
    agent_id: &str,
) -> String {
    let task = payload.get("task").unwrap_or(&serde_json::Value::Null);
    let task_id = task.get("id").and_then(|v| v.as_str()).unwrap_or("-");
    let title = task.get("title").and_then(|v| v.as_str()).unwrap_or("-");
    let result_md = task.get("result_md").and_then(|v| v.as_str()).unwrap_or("");
    let summary = truncate_chars(result_md.trim(), 1200);

    let mut lines = vec![
        format!("### {}", title),
        format!("- 状态：{}", display_status(new_status)),
        format!("- 任务 ID：{}", task_id),
        format!("- Agent：{} ({})", agent_name, agent_id),
    ];

    if !summary.is_empty() {
        lines.push(String::new());
        lines.push("#### 完成结果摘要".to_string());
        lines.push(summary);
    }

    lines.join("\n")
}

fn display_status(status: &str) -> &str {
    match status {
        "agent_in_progress" => "执行中",
        "agent_completed" => "已完成",
        "agent_failed" => "失败",
        _ => status,
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_by_chars() {
        assert_eq!(truncate_chars("abcdef", 3), "abc...");
        assert_eq!(truncate_chars("你好世界", 2), "你好...");
    }

    #[test]
    fn dingtalk_markdown_contains_task_context_and_summary() {
        let payload = serde_json::json!({
            "task": {
                "id": "task-1",
                "agent_id": "agent-1",
                "title": "Fix build",
                "result_md": "Build fixed"
            }
        });
        let markdown = build_dingtalk_markdown(&payload, "agent_completed", "Agent A", "agent-1");
        assert!(markdown.contains("Fix build"));
        assert!(markdown.contains("已完成"));
        assert!(markdown.contains("task-1"));
        assert!(markdown.contains("Agent A (agent-1)"));
        assert!(markdown.contains("Build fixed"));
    }
}
