-- Full rename of "company" -> "workspace" across the schema, to match the
-- application-level concept: a user can belong to (and switch between)
-- several workspaces, one per project, rather than exactly one company.
--
-- BREAKING: any hivemind binary still running the pre-rename code (which
-- queries `companies`/`company_id` directly) will start failing every
-- request the moment this migration runs. This must be deployed together
-- with the matching code change, not rolled out ahead of it.

ALTER TABLE companies RENAME TO workspaces;
ALTER INDEX idx_companies_slug RENAME TO idx_workspaces_slug;
ALTER TABLE workspaces RENAME CONSTRAINT companies_tier_check TO workspaces_tier_check;

ALTER TABLE company_members RENAME TO workspace_members;
ALTER TABLE workspace_members RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_members_company RENAME TO idx_workspace_members_workspace;

ALTER TABLE company_invites RENAME TO workspace_invites;
ALTER TABLE workspace_invites RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_invites_company RENAME TO idx_workspace_invites_workspace;

ALTER TABLE company_config RENAME TO workspace_config;
ALTER TABLE workspace_config RENAME COLUMN company_id TO workspace_id;

ALTER TABLE meetings RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_meetings_company_date RENAME TO idx_meetings_workspace_date;

ALTER TABLE auth_tokens RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_auth_tokens_company RENAME TO idx_auth_tokens_workspace;

ALTER TABLE sessions RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_sessions_company_user_updated RENAME TO idx_sessions_workspace_user_updated;

ALTER TABLE knowledge_documents RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_knowledge_docs_company RENAME TO idx_knowledge_docs_workspace;

ALTER TABLE company_api_keys RENAME TO workspace_api_keys;
ALTER TABLE workspace_api_keys RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_company_api_keys_company RENAME TO idx_workspace_api_keys_workspace;
ALTER INDEX idx_company_api_keys_prefix RENAME TO idx_workspace_api_keys_prefix;

ALTER TABLE coding_sessions RENAME COLUMN company_id TO workspace_id;
ALTER INDEX idx_coding_sessions_company RENAME TO idx_coding_sessions_workspace;
