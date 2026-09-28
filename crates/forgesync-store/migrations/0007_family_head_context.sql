CREATE TABLE thread_family_head_contexts (
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    family TEXT NOT NULL CHECK (family IN ('reviews', 'review_threads')),
    head_sha TEXT NOT NULL CHECK (length(head_sha) IN (40, 64)),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    PRIMARY KEY (thread_id, family),
    FOREIGN KEY (thread_id, family)
        REFERENCES family_coverage(thread_id, family) ON DELETE CASCADE
);
