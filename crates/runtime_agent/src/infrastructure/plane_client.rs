use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Snapshot of an issue's relevant fields, fetched in a single GET.
#[derive(Debug, Clone, Default)]
pub struct IssueSnapshot {
    pub title: String,
    pub description: String,
    pub state_id: String,
    pub label_ids: Vec<String>,
    pub assignee_user_ids: Vec<String>,
}

/// Workspace member info for resolving assignee user_id → email.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorkspaceMember {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
}

/// Client for Plane API operations (state updates, comments).
#[derive(Clone)]
pub struct PlaneClient {
    base_url: String,
    workspace_slug: String,
    api_key: String,
    http: reqwest::Client,
    /// Cache: project_id → (state_name → state_id)
    states_cache: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    /// Cache: project_id → (label_id → label_name)
    labels_cache: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    /// Cache: workspace member user_id → email
    members_cache: Arc<RwLock<Option<HashMap<String, String>>>>,
}

impl PlaneClient {
    pub fn new(base_url: &str, workspace_slug: &str, api_key: &str) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url: base_url.to_string(),
            workspace_slug: workspace_slug.to_string(),
            api_key: api_key.to_string(),
            http,
            states_cache: Arc::new(RwLock::new(HashMap::new())),
            labels_cache: Arc::new(RwLock::new(HashMap::new())),
            members_cache: Arc::new(RwLock::new(None)),
        }
    }

    #[allow(dead_code)]
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.api_key.is_empty()
    }

    /// Invalidate all per-project caches. Call when binding config or remote
    /// project state may have shifted.
    #[allow(dead_code)]
    pub async fn invalidate_caches(&self) {
        self.states_cache.write().await.clear();
        self.labels_cache.write().await.clear();
        *self.members_cache.write().await = None;
    }

    // ───────────────────────── states ─────────────────────────

    async fn fetch_states(&self, project_id: &str) -> anyhow::Result<HashMap<String, String>> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/states/",
            self.base_url, self.workspace_slug, project_id
        );
        let resp = self
            .http
            .get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        let mut map = HashMap::new();
        let arr = body["results"].as_array().or_else(|| body.as_array());
        if let Some(arr) = arr {
            for s in arr {
                if let (Some(name), Some(id)) = (s["name"].as_str(), s["id"].as_str()) {
                    map.insert(name.to_string(), id.to_string());
                }
            }
        }
        Ok(map)
    }

    /// Get states mapping for a project (cached). name → id.
    pub async fn get_states(&self, project_id: &str) -> anyhow::Result<HashMap<String, String>> {
        {
            let cache = self.states_cache.read().await;
            if let Some(states) = cache.get(project_id) {
                return Ok(states.clone());
            }
        }
        let states = self.fetch_states(project_id).await?;
        {
            let mut cache = self.states_cache.write().await;
            cache.insert(project_id.to_string(), states.clone());
        }
        Ok(states)
    }

    // ───────────────────────── labels ─────────────────────────

    async fn fetch_labels(&self, project_id: &str) -> anyhow::Result<HashMap<String, String>> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/labels/",
            self.base_url, self.workspace_slug, project_id
        );
        let resp = self
            .http
            .get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        let mut map = HashMap::new();
        let arr = body["results"].as_array().or_else(|| body.as_array());
        if let Some(arr) = arr {
            for l in arr {
                if let (Some(id), Some(name)) = (l["id"].as_str(), l["name"].as_str()) {
                    map.insert(id.to_string(), name.to_string());
                }
            }
        }
        Ok(map)
    }

    /// Get labels mapping for a project (cached). id → name.
    pub async fn get_labels(&self, project_id: &str) -> anyhow::Result<HashMap<String, String>> {
        {
            let cache = self.labels_cache.read().await;
            if let Some(labels) = cache.get(project_id) {
                return Ok(labels.clone());
            }
        }
        let labels = self.fetch_labels(project_id).await?;
        {
            let mut cache = self.labels_cache.write().await;
            cache.insert(project_id.to_string(), labels.clone());
        }
        Ok(labels)
    }

    // ─────────────────────── members ────────────────────────

    async fn fetch_members(&self) -> anyhow::Result<HashMap<String, String>> {
        // Plane workspace members endpoint shape:
        //   GET /api/v1/workspaces/{slug}/members/
        // Response items contain { member: { id, email, display_name }, ... }
        // or sometimes { id, email, ... } depending on version.
        let url = format!(
            "{}/api/v1/workspaces/{}/members/",
            self.base_url, self.workspace_slug
        );
        let resp = self
            .http
            .get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;
        let body: serde_json::Value = resp.json().await?;
        let arr = body["results"].as_array().or_else(|| body.as_array());
        let mut map = HashMap::new();
        if let Some(arr) = arr {
            for m in arr {
                let (id, email) = if let Some(member) = m.get("member") {
                    (
                        member["id"].as_str().unwrap_or_default(),
                        member["email"].as_str().unwrap_or_default(),
                    )
                } else {
                    (
                        m["id"].as_str().unwrap_or_default(),
                        m["email"].as_str().unwrap_or_default(),
                    )
                };
                if !id.is_empty() {
                    map.insert(id.to_string(), email.to_string());
                }
            }
        }
        Ok(map)
    }

    /// Resolve a workspace member user_id → email (cached for the lifetime of
    /// this client). Returns empty string if not found.
    pub async fn member_email(&self, user_id: &str) -> anyhow::Result<String> {
        {
            let cache = self.members_cache.read().await;
            if let Some(map) = cache.as_ref() {
                return Ok(map.get(user_id).cloned().unwrap_or_default());
            }
        }
        let map = self.fetch_members().await?;
        let email = map.get(user_id).cloned().unwrap_or_default();
        *self.members_cache.write().await = Some(map);
        Ok(email)
    }

    // ─────────────────────── issues ────────────────────────

    /// One-shot fetch returning the relevant issue fields. Plane stores
    /// `state` as an id, `labels` as id list, `assignees` as user_id list.
    pub async fn get_issue_full(
        &self,
        project_id: &str,
        issue_id: &str,
    ) -> anyhow::Result<IssueSnapshot> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/issues/{}/",
            self.base_url, self.workspace_slug, project_id, issue_id
        );
        let resp = self
            .http
            .get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;
        let body: serde_json::Value = resp.json().await?;
        let title = body["name"].as_str().unwrap_or_default().to_string();
        let description = body["description_stripped"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let state_id = body["state"].as_str().unwrap_or_default().to_string();
        let label_ids = body["labels"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let assignee_user_ids = body["assignees"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(IssueSnapshot {
            title,
            description,
            state_id,
            label_ids,
            assignee_user_ids,
        })
    }

    /// Update issue state directly by state_id. Rename-safe (we hold the id).
    pub async fn update_issue_state_by_id(
        &self,
        project_id: &str,
        issue_id: &str,
        state_id: &str,
    ) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/issues/{}/",
            self.base_url, self.workspace_slug, project_id, issue_id
        );
        self.http
            .patch(&url)
            .header("x-api-key", &self.api_key)
            .json(&serde_json::json!({ "state": state_id }))
            .send()
            .await?;
        Ok(())
    }

    /// Add a comment to an issue.
    pub async fn add_comment(
        &self,
        project_id: &str,
        issue_id: &str,
        comment_html: &str,
    ) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/issues/{}/comments/",
            self.base_url, self.workspace_slug, project_id, issue_id
        );
        self.http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .json(&serde_json::json!({ "comment_html": comment_html }))
            .send()
            .await?;
        Ok(())
    }
}
