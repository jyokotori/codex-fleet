use axum::{
    extract::{Extension, State},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use shared_kernel::{AppContext, AuthContext, Result};

use crate::application::{
    api_token::{get_token_preview_for_user, token_preview, upsert_token_for_user},
    audit::write_audit_log,
};

#[derive(Debug, Serialize)]
pub struct ApiTokenStatus {
    /// `Some` only on regenerate (one-time reveal). `None` on subsequent GETs.
    pub token: Option<String>,
    /// Masked preview suitable for display (e.g. `L5isGj…aDeo`).
    pub preview: Option<String>,
    pub has_token: bool,
}

pub fn router() -> Router<AppContext> {
    Router::new()
        .route("/api-token", get(get_my_token))
        .route("/api-token/regenerate", post(regenerate_my_token))
}

async fn get_my_token(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<ApiTokenStatus>> {
    let preview = get_token_preview_for_user(&state.db, &auth.user_id).await?;
    Ok(Json(ApiTokenStatus {
        token: None,
        has_token: preview.is_some(),
        preview,
    }))
}

async fn regenerate_my_token(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<ApiTokenStatus>> {
    let token = upsert_token_for_user(&state.db, &auth.user_id).await?;
    let preview = token_preview(&token);

    write_audit_log(
        &state.db,
        Some(&auth.user_id),
        "user.regenerate_api_token",
        Some(&auth.user_id),
        serde_json::json!({}),
    )
    .await;

    Ok(Json(ApiTokenStatus {
        token: Some(token),
        preview: Some(preview),
        has_token: true,
    }))
}
