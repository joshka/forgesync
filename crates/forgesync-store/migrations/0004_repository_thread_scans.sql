CREATE TABLE repository_thread_scans (
    repository_id INTEGER PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    status TEXT NOT NULL CHECK (status IN ('in_progress', 'incomplete', 'complete')),
    started_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    next_page_url TEXT,
    pages_completed INTEGER NOT NULL DEFAULT 0 CHECK (pages_completed >= 0),
    threads_seen INTEGER NOT NULL DEFAULT 0 CHECK (threads_seen >= 0),
    failure_json TEXT
);
