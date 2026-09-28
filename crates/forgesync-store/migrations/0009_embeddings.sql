CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL CHECK (length(trim(endpoint)) > 0),
    model TEXT NOT NULL CHECK (length(trim(model)) > 0),
    document_hash TEXT NOT NULL CHECK (
        length(document_hash) = 64 AND document_hash NOT GLOB '*[^0-9a-f]*'
    ),
    chunk_index INTEGER NOT NULL CHECK (chunk_index >= 0),
    chunk_count INTEGER NOT NULL CHECK (chunk_count > chunk_index),
    chunk_hash TEXT NOT NULL CHECK (
        length(chunk_hash) = 64 AND chunk_hash NOT GLOB '*[^0-9a-f]*'
    ),
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    vector_le BLOB NOT NULL CHECK (
        typeof(vector_le) = 'blob' AND length(vector_le) = dimensions * 4
    ),
    created_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    UNIQUE (document_id, endpoint, model, document_hash, chunk_index)
);

CREATE INDEX embeddings_current_lookup
    ON embeddings (document_id, endpoint, model, document_hash, chunk_count, chunk_index);
