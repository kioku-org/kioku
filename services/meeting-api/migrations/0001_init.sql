-- Mirrors the tables the Python meeting-api (SQLAlchemy models.py) already created in
-- production via create_all(). IF NOT EXISTS everywhere: this must be a no-op against the
-- existing "vexa" schema, not a fresh-schema migration.

CREATE TABLE IF NOT EXISTS meetings (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL,
    platform VARCHAR(100) NOT NULL,
    platform_specific_id VARCHAR(255),
    status VARCHAR(50) NOT NULL DEFAULT 'requested',
    bot_container_id VARCHAR(255),
    start_time TIMESTAMP,
    end_time TIMESTAMP,
    data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMP DEFAULT now(),
    updated_at TIMESTAMP DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ix_meetings_user_id ON meetings (user_id);
CREATE INDEX IF NOT EXISTS ix_meetings_platform_specific_id ON meetings (platform_specific_id);
CREATE INDEX IF NOT EXISTS ix_meetings_status ON meetings (status);
CREATE INDEX IF NOT EXISTS ix_meetings_created_at ON meetings (created_at);
CREATE INDEX IF NOT EXISTS ix_meeting_user_platform_native_id_created_at
    ON meetings (user_id, platform, platform_specific_id, created_at);
CREATE INDEX IF NOT EXISTS ix_meeting_data_gin ON meetings USING gin (data);

CREATE TABLE IF NOT EXISTS transcriptions (
    id SERIAL PRIMARY KEY,
    meeting_id INTEGER NOT NULL REFERENCES meetings (id),
    start_time DOUBLE PRECISION NOT NULL,
    end_time DOUBLE PRECISION NOT NULL,
    text TEXT NOT NULL,
    speaker VARCHAR(255),
    language VARCHAR(10),
    created_at TIMESTAMP,
    session_uid VARCHAR,
    segment_id VARCHAR
);
CREATE INDEX IF NOT EXISTS ix_transcriptions_meeting_id ON transcriptions (meeting_id);
CREATE INDEX IF NOT EXISTS ix_transcriptions_session_uid ON transcriptions (session_uid);
CREATE INDEX IF NOT EXISTS ix_transcription_meeting_start ON transcriptions (meeting_id, start_time);
CREATE UNIQUE INDEX IF NOT EXISTS ix_transcription_meeting_segment
    ON transcriptions (meeting_id, segment_id) WHERE segment_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS meeting_sessions (
    id SERIAL PRIMARY KEY,
    meeting_id INTEGER NOT NULL REFERENCES meetings (id),
    session_uid VARCHAR NOT NULL,
    session_start_time TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT _meeting_session_uc UNIQUE (meeting_id, session_uid)
);
CREATE INDEX IF NOT EXISTS ix_meeting_sessions_meeting_id ON meeting_sessions (meeting_id);
CREATE INDEX IF NOT EXISTS ix_meeting_sessions_session_uid ON meeting_sessions (session_uid);

CREATE TABLE IF NOT EXISTS recordings (
    id SERIAL PRIMARY KEY,
    meeting_id INTEGER REFERENCES meetings (id),
    user_id INTEGER NOT NULL,
    session_uid VARCHAR,
    source VARCHAR(50) NOT NULL DEFAULT 'bot',
    status VARCHAR(50) NOT NULL DEFAULT 'in_progress',
    error_message TEXT,
    created_at TIMESTAMP DEFAULT now(),
    completed_at TIMESTAMP
);
CREATE INDEX IF NOT EXISTS ix_recordings_meeting_id ON recordings (meeting_id);
CREATE INDEX IF NOT EXISTS ix_recordings_user_id ON recordings (user_id);
CREATE INDEX IF NOT EXISTS ix_recordings_status ON recordings (status);
CREATE INDEX IF NOT EXISTS ix_recording_meeting_session ON recordings (meeting_id, session_uid);
CREATE INDEX IF NOT EXISTS ix_recording_user_created ON recordings (user_id, created_at);

CREATE TABLE IF NOT EXISTS media_files (
    id SERIAL PRIMARY KEY,
    recording_id INTEGER NOT NULL REFERENCES recordings (id),
    type VARCHAR(50) NOT NULL,
    format VARCHAR(20) NOT NULL,
    storage_path VARCHAR(1024) NOT NULL,
    storage_backend VARCHAR(50) NOT NULL DEFAULT 'minio',
    file_size_bytes INTEGER,
    duration_seconds DOUBLE PRECISION,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMP DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ix_media_files_recording_id ON media_files (recording_id);

CREATE TABLE IF NOT EXISTS calendar_events (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL,
    external_event_id TEXT NOT NULL,
    title TEXT,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    meeting_url TEXT,
    platform TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    meeting_id INTEGER REFERENCES meetings (id),
    sync_token TEXT,
    created_at TIMESTAMP DEFAULT now(),
    CONSTRAINT uq_calendar_event_user_ext_id UNIQUE (user_id, external_event_id)
);
CREATE INDEX IF NOT EXISTS ix_calendar_events_user_id ON calendar_events (user_id);
CREATE INDEX IF NOT EXISTS ix_calendar_events_start_time ON calendar_events (start_time);
CREATE INDEX IF NOT EXISTS ix_calendar_events_status ON calendar_events (status);
