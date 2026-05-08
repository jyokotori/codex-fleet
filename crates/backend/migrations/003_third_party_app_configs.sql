-- Platform-level third-party app integration credentials
-- (e.g. DingTalk, Slack). One row per provider; admin-managed via UI.
CREATE TABLE third_party_app_configs (
    provider     TEXT PRIMARY KEY,
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO third_party_app_configs(provider, config_json, enabled)
VALUES ('dingtalk', '{}', FALSE)
ON CONFLICT (provider) DO NOTHING;
