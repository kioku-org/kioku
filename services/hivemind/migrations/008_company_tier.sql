-- Subscription tier per company. Free tier is capped at 1 member (enforced
-- in application code, see AuthService::register_member and invite::create),
-- which means free-tier knowledge/meetings — already scoped by company_id —
-- are effectively user-scoped for free. Pro and Teams unlock the existing
-- company_invites/company_members sharing mechanism; the only structural
-- difference between them is how seats get added (self-serve invites vs.
-- bundled provisioning), not the data model.
ALTER TABLE companies ADD COLUMN IF NOT EXISTS tier VARCHAR(16) NOT NULL DEFAULT 'free';
ALTER TABLE companies ADD CONSTRAINT companies_tier_check CHECK (tier IN ('free', 'pro', 'teams'));
