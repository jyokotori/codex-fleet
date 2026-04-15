use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

/// Client for Plane API operations (state updates, comments).
#[derive(Clone)]
pub struct PlaneClient {
    base_url: String,
    workspace_slug: String,
    api_key: String,
    http: reqwest::Client,
    /// Cache: project_id → (state_name → state_id)
    states_cache: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
}

impl PlaneClient {
    pub fn new(base_url: &str, workspace_slug: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            workspace_slug: workspace_slug.to_string(),
            api_key: api_key.to_string(),
            http: reqwest::Client::new(),
            states_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[allow(dead_code)]
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.api_key.is_empty()
    }

    /// Fetch all states for a project, returns name→id mapping.
    async fn fetch_states(&self, project_id: &str) -> anyhow::Result<HashMap<String, String>> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/states/",
            self.base_url, self.workspace_slug, project_id
        );
        let resp = self.http.get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        let mut map = HashMap::new();
        if let Some(results) = body["results"].as_array() {
            for s in results {
                if let (Some(name), Some(id)) = (s["name"].as_str(), s["id"].as_str()) {
                    map.insert(name.to_string(), id.to_string());
                }
            }
        }
        Ok(map)
    }

    /// Get states mapping for a project (cached).
    pub async fn get_states(&self, project_id: &str) -> anyhow::Result<HashMap<String, String>> {
        // Check cache first
        {
            let cache = self.states_cache.read().await;
            if let Some(states) = cache.get(project_id) {
                return Ok(states.clone());
            }
        }
        // Fetch and cache
        let states = self.fetch_states(project_id).await?;
        {
            let mut cache = self.states_cache.write().await;
            cache.insert(project_id.to_string(), states.clone());
        }
        Ok(states)
    }

    /// Get current state name of an issue from Plane API.
    pub async fn get_issue_state_name(&self, project_id: &str, issue_id: &str) -> anyhow::Result<String> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/issues/{}/",
            self.base_url, self.workspace_slug, project_id, issue_id
        );
        let resp = self.http.get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        // The issue response has state as an ID, we need to map it
        let state_id = body["state"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing state field"))?;

        let states = self.get_states(project_id).await?;
        // Reverse lookup: find name by id
        for (name, id) in &states {
            if id == state_id {
                return Ok(name.clone());
            }
        }
        anyhow::bail!("unknown state id: {state_id}")
    }

    /// Get latest issue data (title + description) from Plane API.
    pub async fn get_issue(&self, project_id: &str, issue_id: &str) -> anyhow::Result<(String, String)> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/issues/{}/",
            self.base_url, self.workspace_slug, project_id, issue_id
        );
        let resp = self.http.get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        let title = body["name"].as_str().unwrap_or_default().to_string();
        let description = body["description_stripped"].as_str().unwrap_or_default().to_string();
        Ok((title, description))
    }

    /// Update issue state by state name. Returns Ok(true) if updated, Ok(false) if state name not found.
    pub async fn update_issue_state(&self, project_id: &str, issue_id: &str, state_name: &str) -> anyhow::Result<bool> {
        let states = self.get_states(project_id).await?;
        let state_id = match states.get(state_name) {
            Some(id) => id.clone(),
            None => {
                // Invalidate cache and retry
                {
                    let mut cache = self.states_cache.write().await;
                    cache.remove(project_id);
                }
                let states = self.get_states(project_id).await?;
                match states.get(state_name) {
                    Some(id) => id.clone(),
                    None => {
                        warn!("Plane: state '{state_name}' not found in project {project_id}");
                        return Ok(false);
                    }
                }
            }
        };

        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/issues/{}/",
            self.base_url, self.workspace_slug, project_id, issue_id
        );
        self.http.patch(&url)
            .header("x-api-key", &self.api_key)
            .json(&serde_json::json!({"state": state_id}))
            .send()
            .await?;

        Ok(true)
    }

    /// Add a comment to an issue.
    pub async fn add_comment(&self, project_id: &str, issue_id: &str, comment_html: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/v1/workspaces/{}/projects/{}/issues/{}/comments/",
            self.base_url, self.workspace_slug, project_id, issue_id
        );
        self.http.post(&url)
            .header("x-api-key", &self.api_key)
            .json(&serde_json::json!({
                "comment_html": comment_html
            }))
            .send()
            .await?;

        Ok(())
    }
}
