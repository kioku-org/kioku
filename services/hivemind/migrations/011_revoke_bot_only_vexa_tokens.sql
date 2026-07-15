-- Cached per-user Vexa tokens were minted with scope=bot only (006-era
-- provisioning), but the gateway requires the tx scope for /transcripts and
-- /meetings — so CLI --follow and transcript reads 403'd with a cached key.
-- Clear the cached tokens; resolve_vexa_api_key re-provisions on next use
-- with scopes=bot,tx. vexa_user_id is kept (find-or-create is by email and
-- stable either way).
UPDATE users SET vexa_token = NULL, vexa_token_id = NULL WHERE vexa_token IS NOT NULL;
