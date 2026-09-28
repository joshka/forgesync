CREATE TABLE runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_run_id INTEGER REFERENCES runs(id),
    status TEXT NOT NULL CHECK (status IN ('in_progress', 'complete', 'partial', 'failed', 'interrupted', 'deferred')),
    started_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    finished_at_us INTEGER,
    scope_json TEXT NOT NULL,
    outcome_json TEXT
);

CREATE INDEX runs_started_idx ON runs (started_at_us DESC, id DESC);

CREATE TABLE jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    family TEXT NOT NULL CHECK (family IN ('threads', 'comments', 'pull_request_metadata', 'reviews', 'review_threads')),
    scope_key TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'complete', 'failed', 'deferred', 'interrupted')),
    started_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    pages_completed INTEGER NOT NULL DEFAULT 0 CHECK (pages_completed >= 0),
    items_committed INTEGER NOT NULL DEFAULT 0 CHECK (items_committed >= 0),
    failure_json TEXT,
    UNIQUE (run_id, repository_id, family, scope_key)
);

CREATE INDEX jobs_run_idx ON jobs (run_id, id);

CREATE TABLE failures (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_id INTEGER REFERENCES jobs(id) ON DELETE CASCADE,
    repository_id INTEGER REFERENCES repositories(id) ON DELETE CASCADE,
    family TEXT CHECK (family IN ('threads', 'comments', 'pull_request_metadata', 'reviews', 'review_threads')),
    target_key TEXT NOT NULL,
    scope_key TEXT NOT NULL DEFAULT '',
    failure_json TEXT NOT NULL,
    created_at_us INTEGER NOT NULL,
    resolved_at_us INTEGER,
    retry_run_id INTEGER REFERENCES runs(id)
);

CREATE INDEX failures_unresolved_idx ON failures (repository_id, family, resolved_at_us);

CREATE TABLE archive_lease (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    owner_id TEXT,
    fencing_token INTEGER NOT NULL CHECK (fencing_token >= 0),
    expires_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL
);

CREATE TABLE repository_checkpoints (
    repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    checkpoint TEXT NOT NULL CHECK (checkpoint IN ('closed_sweep')),
    watermark_us INTEGER NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    updated_at_us INTEGER NOT NULL,
    PRIMARY KEY (repository_id, checkpoint)
);
