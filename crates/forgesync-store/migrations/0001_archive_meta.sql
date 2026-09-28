CREATE TABLE archive_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    archive_id TEXT NOT NULL UNIQUE,
    format_id TEXT NOT NULL,
    created_at_us INTEGER NOT NULL
);
