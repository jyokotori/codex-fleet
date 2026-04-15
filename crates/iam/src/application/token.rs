use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use shared_kernel::{AppError, AuthContext, Result};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
    pub token_type: String,
}

#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    format!("{:x}", digest)
}

fn encode_token(claims: &Claims, secret: &str) -> Result<String> {
    let header = serde_json::json!({"alg":"HS256","typ":"JWT"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let payload_b64 = URL_SAFE_NO_PAD
        .encode(serde_json::to_string(claims).map_err(|e| AppError::Internal(e.to_string()))?);

    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    mac.update(signing_input.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_b64 = URL_SAFE_NO_PAD.encode(signature);

    Ok(format!("{}.{}", signing_input, signature_b64))
}

fn decode_token(token: &str, secret: &str) -> Result<Claims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(AppError::Unauthorized);
    }

    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let signature = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| AppError::Unauthorized)?;

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| AppError::Unauthorized)?;
    mac.update(signing_input.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| AppError::Unauthorized)?;

    let payload = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| AppError::Unauthorized)?;

    serde_json::from_slice::<Claims>(&payload).map_err(|_| AppError::Unauthorized)
}

pub fn decode_access_token(token: &str, secret: &str) -> Result<Claims> {
    let claims = decode_token(token, secret)?;
    if claims.token_type != "access" {
        return Err(AppError::Unauthorized);
    }
    Ok(claims)
}

pub fn decode_refresh_token(token: &str, secret: &str) -> Result<Claims> {
    let claims = decode_token(token, secret)?;
    if claims.token_type != "refresh" {
        return Err(AppError::Unauthorized);
    }
    Ok(claims)
}

pub fn issue_tokens(
    user_id: &str,
    secret: &str,
    access_minutes: i64,
    refresh_days: i64,
) -> Result<TokenPair> {
    let now = Utc::now().timestamp();
    let access_exp = now + access_minutes * 60;
    let refresh_exp = now + refresh_days * 24 * 3600;

    let access_claims = Claims {
        sub: user_id.to_string(),
        exp: access_exp,
        iat: now,
        jti: Uuid::new_v4().to_string(),
        token_type: "access".into(),
    };

    let refresh_claims = Claims {
        sub: user_id.to_string(),
        exp: refresh_exp,
        iat: now,
        jti: Uuid::new_v4().to_string(),
        token_type: "refresh".into(),
    };

    Ok(TokenPair {
        access_token: encode_token(&access_claims, secret)?,
        refresh_token: encode_token(&refresh_claims, secret)?,
        expires_in: access_minutes * 60,
    })
}

pub async fn fetch_auth_context(db: &sqlx::PgPool, user_id: &str) -> Result<AuthContext> {
    let user_row =
        sqlx::query("SELECT id, username, display_name, email, status FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?
            .ok_or(AppError::Unauthorized)?;

    let roles_rows = sqlx::query(
        r#"SELECT r.code FROM roles r
           INNER JOIN user_roles ur ON ur.role_id = r.id
           WHERE ur.user_id = $1"#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let roles: Vec<String> = roles_rows
        .into_iter()
        .map(|r| r.get::<String, _>("code"))
        .collect();

    let perms_rows = sqlx::query(
        r#"SELECT DISTINCT p.code FROM permissions p
           INNER JOIN role_permissions rp ON rp.permission_id = p.id
           INNER JOIN user_roles ur ON ur.role_id = rp.role_id
           WHERE ur.user_id = $1"#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let permissions: Vec<String> = perms_rows
        .into_iter()
        .map(|r| r.get::<String, _>("code"))
        .collect();

    Ok(AuthContext {
        user_id: user_row.get("id"),
        username: user_row.get("username"),
        display_name: user_row.get("display_name"),
        email: user_row.get("email"),
        status: user_row.get("status"),
        roles,
        permissions,
    })
}
