CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    recipe TEXT NOT NULL CHECK (recipe IN ('original_body', 'discussion_enriched')),
    recipe_version INTEGER NOT NULL CHECK (recipe_version > 0),
    source_identity_json TEXT NOT NULL,
    content_hash TEXT NOT NULL CHECK (
        length(content_hash) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*'
    ),
    title TEXT NOT NULL,
    text TEXT NOT NULL,
    dedupe_text TEXT NOT NULL,
    source_updated_at_us INTEGER NOT NULL,
    built_at_us INTEGER NOT NULL,
    UNIQUE (thread_id, recipe)
);

CREATE INDEX documents_content_hash_idx ON documents (content_hash);
