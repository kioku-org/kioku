CREATE TABLE IF NOT EXISTS coding_sessions (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    decisions JSONB NOT NULL DEFAULT '[]',
    tags JSONB NOT NULL DEFAULT '[]',
    date BIGINT NOT NULL,
    created_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_coding_sessions_company ON coding_sessions(company_id);
CREATE INDEX IF NOT EXISTS idx_coding_sessions_date ON coding_sessions(company_id, date DESC);

-- Allow knowledge_chunks to reference coding sessions
ALTER TABLE knowledge_chunks ADD COLUMN IF NOT EXISTS session_id UUID REFERENCES coding_sessions(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_chunks_session ON knowledge_chunks(session_id);
