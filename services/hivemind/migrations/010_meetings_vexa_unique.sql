-- Post-meeting ingestion can fire more than once per meeting (six run_all_tasks
-- trigger sites in meeting-api: bot-exit callback, kill paths, timeouts), and
-- ingest was a blind INSERT — meeting 25 landed twice, 112s apart, doubling its
-- weight in search. Dedupe existing rows (keep the oldest), then make the
-- vexa linkage unique so ingestion becomes idempotent at the DB level.

DELETE FROM meetings m
USING meetings keeper
WHERE m.vexa_meeting_id IS NOT NULL
  AND keeper.workspace_id = m.workspace_id
  AND keeper.vexa_meeting_id = m.vexa_meeting_id
  AND (keeper.created_at, keeper.id) < (m.created_at, m.id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_meetings_workspace_vexa_meeting
    ON meetings (workspace_id, vexa_meeting_id)
    WHERE vexa_meeting_id IS NOT NULL;
