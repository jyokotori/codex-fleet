use chrono::Utc;

pub async fn write_audit_log(
    db: &sqlx::PgPool,
    actor_user_id: Option<&str>,
    action: &str,
    target_user_id: Option<&str>,
    metadata: serde_json::Value,
) {
    let _ = sqlx::query(
        "INSERT INTO audit_logs (id, actor_user_id, action, target_user_id, metadata_json, created_at) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(actor_user_id)
    .bind(action)
    .bind(target_user_id)
    .bind(metadata.to_string())
    .bind(Utc::now())
    .execute(db)
    .await;
}
