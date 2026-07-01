-- Per-user Vexa credential link, provisioned lazily on first bot-management call.
-- Replaces the single shared vexa_admin_token for user-facing Vexa gateway calls,
-- so one leaked/misconfigured credential no longer has blast radius across every user.
ALTER TABLE users ADD COLUMN IF NOT EXISTS vexa_user_id INTEGER;
ALTER TABLE users ADD COLUMN IF NOT EXISTS vexa_token_id INTEGER;
ALTER TABLE users ADD COLUMN IF NOT EXISTS vexa_token TEXT;
