-- Per-user API access tokens. Holders authenticate via
-- `Authorization: Bearer <token>` and act as the bound user.
--
-- Only a SHA-256 hash of the token is stored; the raw token is returned
-- exactly once (on generation/regeneration). A non-sensitive preview
-- (first-6…last-4 chars) is kept for UI display so users can recognize
-- which token is active without exposing it.
CREATE TABLE api_access_tokens (
    user_id       TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    token_hash    TEXT NOT NULL UNIQUE,
    token_preview TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_access_tokens_hash ON api_access_tokens(token_hash);
