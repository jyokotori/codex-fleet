use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
};

use axum::{
    extract::{Extension, Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tokio::sync::RwLock;
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, AuthContext, Result};

#[derive(Debug, Clone)]
struct DingTalkCredentials {
    app_key: String,
    app_secret: String,
}

async fn load_dingtalk_credentials(db: &PgPool) -> Result<DingTalkCredentials> {
    let row = sqlx::query!(
        "SELECT config_json, enabled FROM third_party_app_configs WHERE provider = 'dingtalk'"
    )
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::BadRequest("DingTalk integration is not configured".into()))?;

    if !row.enabled {
        return Err(AppError::BadRequest("DingTalk integration is disabled".into()));
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&row.config_json).unwrap_or(serde_json::json!({}));
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
    if app_key.is_empty() || app_secret.is_empty() {
        return Err(AppError::BadRequest(
            "DingTalk app key and secret are not configured".into(),
        ));
    }
    Ok(DingTalkCredentials { app_key, app_secret })
}

use crate::application::{audit::write_audit_log, password::hash_password};

use super::admin_users::require_permission;

static SYNC_JOBS: LazyLock<Arc<RwLock<HashMap<String, DingTalkSyncJob>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[derive(Debug, Deserialize)]
pub struct StartDingTalkSyncRequest {
    pub default_password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DingTalkSyncJob {
    pub id: String,
    pub status: String,
    pub processed: usize,
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DingTalkAccessTokenResponse {
    errcode: Option<i64>,
    errmsg: Option<String>,
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DingTalkResponse<T> {
    errcode: Option<i64>,
    errmsg: Option<String>,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct DingTalkDepartment {
    dept_id: i64,
}

#[derive(Debug, Deserialize)]
struct DingTalkUserIdList {
    #[serde(default)]
    userid_list: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DingTalkUserDetail {
    userid: String,
    name: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    mobile: String,
}

#[derive(Default)]
struct SyncCounters {
    processed: usize,
    created: usize,
    updated: usize,
    skipped: usize,
    errors: Vec<String>,
}

enum SyncAction {
    Created,
    Updated,
}

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/dingtalk/users/sync", post(start_sync))
        .route("/dingtalk/users/sync/{id}", get(get_sync_job))
}

async fn start_sync(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<StartDingTalkSyncRequest>,
) -> Result<Json<DingTalkSyncJob>> {
    require_permission(&auth, "user:create")?;

    if req.default_password.len() < 8 {
        return Err(AppError::BadRequest(
            "Default password must be at least 8 characters".into(),
        ));
    }
    let credentials = load_dingtalk_credentials(&state.db).await?;

    let job = DingTalkSyncJob {
        id: Uuid::new_v4().to_string(),
        status: "running".into(),
        processed: 0,
        created: 0,
        updated: 0,
        skipped: 0,
        errors: Vec::new(),
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
    };

    {
        let mut jobs = SYNC_JOBS.write().await;
        if jobs.values().any(|job| job.status == "running") {
            return Err(AppError::Conflict(
                "A DingTalk user sync job is already running".into(),
            ));
        }
        jobs.insert(job.id.clone(), job.clone());
    }

    let job_id = job.id.clone();
    let actor_user_id = auth.user_id.clone();
    tokio::spawn(async move {
        let counters = run_sync_job(&state, &credentials, &req.default_password).await;
        finish_job(&job_id, counters).await;
        write_audit_log(
            &state.db,
            Some(&actor_user_id),
            "dingtalk.users.sync",
            None,
            serde_json::json!({"job_id": job_id}),
        )
        .await;
    });

    Ok(Json(job))
}

async fn get_sync_job(
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<DingTalkSyncJob>> {
    require_permission(&auth, "user:list")?;

    let jobs = SYNC_JOBS.read().await;
    let job = jobs
        .get(&id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("DingTalk sync job {} not found", id)))?;

    Ok(Json(job))
}

async fn run_sync_job(
    state: &AppContext,
    credentials: &DingTalkCredentials,
    default_password: &str,
) -> SyncCounters {
    let mut counters = SyncCounters::default();

    let token = match get_access_token(&credentials.app_key, &credentials.app_secret).await {
        Ok(token) => token,
        Err(e) => {
            counters
                .errors
                .push(format!("Failed to get DingTalk access token: {e}"));
            return counters;
        }
    };

    let userids = match fetch_all_userids(&token).await {
        Ok(userids) => userids,
        Err(e) => {
            counters
                .errors
                .push(format!("Failed to fetch DingTalk users: {e}"));
            return counters;
        }
    };

    let password_hash = match hash_password(default_password) {
        Ok(hash) => hash,
        Err(e) => {
            counters
                .errors
                .push(format!("Failed to hash default password: {e}"));
            return counters;
        }
    };

    for userid in userids {
        counters.processed += 1;
        match fetch_user_detail(&token, &userid).await {
            Ok(user) => match upsert_user_from_dingtalk(&state.db, &user, &password_hash).await {
                Ok(SyncAction::Created) => counters.created += 1,
                Ok(SyncAction::Updated) => counters.updated += 1,
                Err(e) => {
                    counters.skipped += 1;
                    let msg = format!("{}: {}", user.userid, display_sync_error(e));
                    tracing::warn!(target: "dingtalk_sync", "{}", msg);
                    counters.errors.push(msg);
                }
            },
            Err(e) => {
                counters.skipped += 1;
                let msg = format!("{}: failed to fetch user detail: {e}", userid);
                tracing::warn!(target: "dingtalk_sync", "{}", msg);
                counters.errors.push(msg);
            }
        }

        update_running_job_snapshot(&counters).await;
    }

    counters
}

async fn update_running_job_snapshot(counters: &SyncCounters) {
    let mut jobs = SYNC_JOBS.write().await;
    if let Some(job) = jobs.values_mut().find(|job| job.status == "running") {
        job.processed = counters.processed;
        job.created = counters.created;
        job.updated = counters.updated;
        job.skipped = counters.skipped;
        job.errors = counters.errors.clone();
    }
}

async fn finish_job(job_id: &str, counters: SyncCounters) {
    let mut jobs = SYNC_JOBS.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        job.status = if counters.errors.is_empty() {
            "completed".into()
        } else {
            "completed_with_errors".into()
        };
        job.processed = counters.processed;
        job.created = counters.created;
        job.updated = counters.updated;
        job.skipped = counters.skipped;
        job.errors = counters.errors;
        job.completed_at = Some(Utc::now().to_rfc3339());
    }
}

async fn get_access_token(app_key: &str, app_secret: &str) -> anyhow::Result<String> {
    let resp: DingTalkAccessTokenResponse = reqwest::Client::new()
        .get("https://oapi.dingtalk.com/gettoken")
        .query(&[("appkey", app_key), ("appsecret", app_secret)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if resp.errcode.unwrap_or(0) != 0 {
        return Err(anyhow::anyhow!(
            "{}",
            resp.errmsg
                .unwrap_or_else(|| "DingTalk gettoken failed".into())
        ));
    }

    resp.access_token
        .ok_or_else(|| anyhow::anyhow!("DingTalk access token missing"))
}

async fn fetch_all_userids(access_token: &str) -> anyhow::Result<Vec<String>> {
    let mut seen_depts = HashSet::new();
    let mut pending = vec![1_i64];
    let mut userids = HashSet::new();

    while let Some(dept_id) = pending.pop() {
        if !seen_depts.insert(dept_id) {
            continue;
        }

        for userid in fetch_department_userids(access_token, dept_id).await? {
            userids.insert(userid);
        }

        for dept in fetch_sub_departments(access_token, dept_id).await? {
            pending.push(dept.dept_id);
        }
    }

    let mut out: Vec<String> = userids.into_iter().collect();
    out.sort();
    Ok(out)
}

async fn fetch_sub_departments(
    access_token: &str,
    dept_id: i64,
) -> anyhow::Result<Vec<DingTalkDepartment>> {
    let resp: DingTalkResponse<Vec<DingTalkDepartment>> = reqwest::Client::new()
        .post(format!(
            "https://oapi.dingtalk.com/topapi/v2/department/listsub?access_token={}",
            access_token
        ))
        .json(&serde_json::json!({ "dept_id": dept_id }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    unwrap_dingtalk_result(resp, "department/listsub")
}

async fn fetch_department_userids(access_token: &str, dept_id: i64) -> anyhow::Result<Vec<String>> {
    let resp: DingTalkResponse<DingTalkUserIdList> = reqwest::Client::new()
        .post(format!(
            "https://oapi.dingtalk.com/topapi/user/listid?access_token={}",
            access_token
        ))
        .json(&serde_json::json!({ "dept_id": dept_id }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(unwrap_dingtalk_result(resp, "user/listid")?.userid_list)
}

async fn fetch_user_detail(access_token: &str, userid: &str) -> anyhow::Result<DingTalkUserDetail> {
    let resp: DingTalkResponse<DingTalkUserDetail> = reqwest::Client::new()
        .post(format!(
            "https://oapi.dingtalk.com/topapi/v2/user/get?access_token={}",
            access_token
        ))
        .json(&serde_json::json!({ "userid": userid }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    unwrap_dingtalk_result(resp, "user/get")
}

fn unwrap_dingtalk_result<T>(resp: DingTalkResponse<T>, api: &str) -> anyhow::Result<T> {
    if resp.errcode.unwrap_or(0) != 0 {
        return Err(anyhow::anyhow!(
            "{}",
            resp.errmsg
                .unwrap_or_else(|| format!("DingTalk API {api} failed"))
        ));
    }
    resp.result
        .ok_or_else(|| anyhow::anyhow!("DingTalk API {api} returned no result"))
}

async fn upsert_user_from_dingtalk(
    db: &PgPool,
    user: &DingTalkUserDetail,
    password_hash: &str,
) -> anyhow::Result<SyncAction> {
    let email = user.email.trim();
    if email.is_empty() {
        return Err(anyhow::anyhow!("email is empty"));
    }

    let matched = sqlx::query("SELECT id FROM users WHERE lower(email) = lower($1)")
        .bind(email)
        .fetch_all(db)
        .await?;

    match matched.len() {
        0 => create_user_from_dingtalk(db, user, email, password_hash).await,
        1 => {
            let user_id: String = matched[0].get("id");
            update_user_from_dingtalk(db, &user_id, user, email).await
        }
        _ => Err(anyhow::anyhow!(
            "email {} matches multiple existing users",
            email
        )),
    }
}

async fn create_user_from_dingtalk(
    db: &PgPool,
    user: &DingTalkUserDetail,
    email: &str,
    password_hash: &str,
) -> anyhow::Result<SyncAction> {
    ensure_dingtalk_userid_available(db, &user.userid, None).await?;

    let username = next_available_username(db, email_username_prefix(email)).await?;
    let user_id = Uuid::new_v4().to_string();
    let mut tx = db.begin().await?;

    sqlx::query(
        r#"INSERT INTO users
           (id, username, display_name, email, mobile, dingtalk_userid, password_hash, status, failed_attempts, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', 0, NOW(), NOW())"#,
    )
    .bind(&user_id)
    .bind(username)
    .bind(user.name.trim())
    .bind(email)
    .bind(user.mobile.trim())
    .bind(user.userid.trim())
    .bind(password_hash)
    .execute(&mut *tx)
    .await?;

    let member_role = sqlx::query("SELECT id FROM roles WHERE code = 'member'")
        .fetch_one(&mut *tx)
        .await?;
    let role_id: String = member_role.get("id");

    sqlx::query("INSERT INTO user_roles (user_id, role_id, created_at) VALUES ($1, $2, NOW())")
        .bind(&user_id)
        .bind(role_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(SyncAction::Created)
}

async fn update_user_from_dingtalk(
    db: &PgPool,
    user_id: &str,
    user: &DingTalkUserDetail,
    email: &str,
) -> anyhow::Result<SyncAction> {
    ensure_dingtalk_userid_available(db, &user.userid, Some(user_id)).await?;

    sqlx::query(
        r#"UPDATE users
           SET display_name = $1, email = $2, mobile = $3, dingtalk_userid = $4, updated_at = NOW()
           WHERE id = $5"#,
    )
    .bind(user.name.trim())
    .bind(email)
    .bind(user.mobile.trim())
    .bind(user.userid.trim())
    .bind(user_id)
    .execute(db)
    .await?;

    Ok(SyncAction::Updated)
}

async fn ensure_dingtalk_userid_available(
    db: &PgPool,
    dingtalk_userid: &str,
    allowed_user_id: Option<&str>,
) -> anyhow::Result<()> {
    if dingtalk_userid.trim().is_empty() {
        return Ok(());
    }

    let existing = sqlx::query("SELECT id FROM users WHERE dingtalk_userid = $1")
        .bind(dingtalk_userid.trim())
        .fetch_optional(db)
        .await?;

    if let Some(row) = existing {
        let existing_id: String = row.get("id");
        if allowed_user_id != Some(existing_id.as_str()) {
            return Err(anyhow::anyhow!(
                "dingtalk_userid {} is already linked to another user",
                dingtalk_userid
            ));
        }
    }

    Ok(())
}

async fn next_available_username(db: &PgPool, base: String) -> anyhow::Result<String> {
    let mut suffix = 1;
    loop {
        let candidate = if suffix == 1 {
            base.clone()
        } else {
            format!("{}_{}", base, suffix)
        };
        let exists = sqlx::query("SELECT id FROM users WHERE username = $1")
            .bind(&candidate)
            .fetch_optional(db)
            .await?
            .is_some();
        if !exists {
            return Ok(candidate);
        }
        suffix += 1;
    }
}

fn email_username_prefix(email: &str) -> String {
    let raw_prefix = email.split('@').next().unwrap_or("user").trim();
    let sanitized: String = raw_prefix
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "user".into()
    } else {
        sanitized
    }
}

fn display_sync_error(err: anyhow::Error) -> String {
    let text = err.to_string();
    if text.chars().count() > 300 {
        format!("{}...", text.chars().take(300).collect::<String>())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_username_from_email_prefix() {
        assert_eq!(email_username_prefix("alice@example.com"), "alice");
        assert_eq!(email_username_prefix("a b@example.com"), "a_b");
        assert_eq!(email_username_prefix("@example.com"), "user");
    }
}
