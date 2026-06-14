-- Long-lived API keys for CLI/programmatic auth (distinct from member_api_keys which store provider keys)
CREATE TABLE IF NOT EXISTS company_api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(128) NOT NULL DEFAULT 'default',
    key_prefix VARCHAR(12) NOT NULL,
    key_hash TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    last_used_at BIGINT
);
CREATE INDEX IF NOT EXISTS idx_company_api_keys_prefix ON company_api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_company_api_keys_company ON company_api_keys(company_id);