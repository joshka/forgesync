ALTER TABLE failures ADD COLUMN thread_provider_id TEXT;
ALTER TABLE failures ADD COLUMN thread_number INTEGER CHECK (thread_number IS NULL OR thread_number > 0);
ALTER TABLE failures ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0);

CREATE INDEX failures_thread_family_idx
    ON failures (repository_id, thread_provider_id, family, resolved_at_us);
