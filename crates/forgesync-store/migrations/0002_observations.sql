CREATE TABLE observation_sequence (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    value INTEGER NOT NULL CHECK (value >= 0 AND value < 9223372036854775807),
    last_started_at_us INTEGER NOT NULL
);

INSERT INTO observation_sequence (singleton, value, last_started_at_us) VALUES (1, 0, 0);

CREATE TABLE repositories (
    id INTEGER PRIMARY KEY,
    host TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    full_name TEXT NOT NULL,
    default_branch TEXT,
    updated_at_us INTEGER,
    provider_data_json TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    UNIQUE (host, provider_id)
);

CREATE TABLE threads (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL,
    number INTEGER NOT NULL CHECK (number > 0),
    kind TEXT NOT NULL CHECK (kind IN ('issue', 'pull_request')),
    state TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    html_url TEXT,
    created_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    closed_at_us INTEGER,
    labels_json TEXT NOT NULL,
    assignees_json TEXT NOT NULL,
    provider_data_json TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    source_clock_state TEXT NOT NULL CHECK (source_clock_state IN ('missing', 'valid', 'invalid')),
    source_clock_raw TEXT NOT NULL,
    source_clock_us INTEGER,
    observation_sequence INTEGER NOT NULL CHECK (observation_sequence > 0),
    observed_at_us INTEGER NOT NULL,
    evidence_clock_state TEXT NOT NULL CHECK (evidence_clock_state IN ('missing', 'valid', 'invalid')),
    evidence_clock_raw TEXT NOT NULL,
    evidence_clock_us INTEGER,
    evidence_sequence INTEGER NOT NULL DEFAULT 0 CHECK (evidence_sequence >= 0),
    UNIQUE (repository_id, provider_id),
    UNIQUE (repository_id, number),
    CHECK (
        (source_clock_state = 'valid' AND source_clock_us IS NOT NULL AND source_clock_raw = '') OR
        (source_clock_state = 'missing' AND source_clock_us IS NULL AND source_clock_raw = '') OR
        (source_clock_state = 'invalid' AND source_clock_us IS NULL AND source_clock_raw <> '')
    ),
    CHECK (
        (evidence_clock_state = 'valid' AND evidence_clock_us IS NOT NULL AND evidence_clock_raw = '') OR
        (evidence_clock_state = 'missing' AND evidence_clock_us IS NULL AND evidence_clock_raw = '') OR
        (evidence_clock_state = 'invalid' AND evidence_clock_us IS NULL AND evidence_clock_raw <> '')
    )
);

CREATE TABLE thread_family_reservations (
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    family TEXT NOT NULL CHECK (family IN ('comments', 'pull_request_metadata', 'reviews', 'review_threads')),
    source_clock_state TEXT NOT NULL CHECK (source_clock_state IN ('missing', 'valid', 'invalid')),
    source_clock_raw TEXT NOT NULL,
    source_clock_us INTEGER,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    started_at_us INTEGER NOT NULL,
    request_scope TEXT NOT NULL,
    PRIMARY KEY (thread_id, family),
    CHECK (
        (source_clock_state = 'valid' AND source_clock_us IS NOT NULL AND source_clock_raw = '') OR
        (source_clock_state = 'missing' AND source_clock_us IS NULL AND source_clock_raw = '') OR
        (source_clock_state = 'invalid' AND source_clock_us IS NULL AND source_clock_raw <> '')
    )
);

CREATE TABLE observation_generations (
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    family TEXT NOT NULL CHECK (family IN ('comments', 'pull_request_metadata', 'reviews', 'review_threads')),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    source_clock_state TEXT NOT NULL CHECK (source_clock_state IN ('missing', 'valid', 'invalid')),
    source_clock_raw TEXT NOT NULL,
    source_clock_us INTEGER,
    started_at_us INTEGER NOT NULL,
    request_scope TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'incomplete', 'complete')),
    received_items INTEGER NOT NULL DEFAULT 0 CHECK (received_items >= 0),
    item_count INTEGER NOT NULL DEFAULT 0 CHECK (item_count >= 0),
    PRIMARY KEY (thread_id, family, sequence),
    FOREIGN KEY (thread_id, family)
        REFERENCES thread_family_reservations(thread_id, family) ON DELETE CASCADE,
    CHECK (
        (source_clock_state = 'valid' AND source_clock_us IS NOT NULL AND source_clock_raw = '') OR
        (source_clock_state = 'missing' AND source_clock_us IS NULL AND source_clock_raw = '') OR
        (source_clock_state = 'invalid' AND source_clock_us IS NULL AND source_clock_raw <> '')
    )
);

CREATE TABLE observation_staging_pages (
    thread_id INTEGER NOT NULL,
    family TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    page_index INTEGER NOT NULL CHECK (page_index >= 0),
    payload_json TEXT NOT NULL,
    PRIMARY KEY (thread_id, family, sequence, page_index),
    FOREIGN KEY (thread_id, family, sequence)
        REFERENCES observation_generations(thread_id, family, sequence) ON DELETE CASCADE
);

CREATE TABLE thread_family_membership (
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    family TEXT NOT NULL CHECK (family IN ('comments', 'pull_request_metadata', 'reviews', 'review_threads')),
    provider_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    PRIMARY KEY (thread_id, family, provider_id)
);

CREATE TABLE family_coverage (
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    family TEXT NOT NULL CHECK (family IN ('threads', 'comments', 'pull_request_metadata', 'reviews', 'review_threads')),
    status TEXT NOT NULL CHECK (status IN ('incomplete', 'complete')),
    source_clock_state TEXT NOT NULL CHECK (source_clock_state IN ('missing', 'valid', 'invalid')),
    source_clock_raw TEXT NOT NULL,
    source_clock_us INTEGER,
    observed_at_us INTEGER NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    state_json TEXT NOT NULL,
    PRIMARY KEY (thread_id, family),
    CHECK (
        (source_clock_state = 'valid' AND source_clock_us IS NOT NULL AND source_clock_raw = '') OR
        (source_clock_state = 'missing' AND source_clock_us IS NULL AND source_clock_raw = '') OR
        (source_clock_state = 'invalid' AND source_clock_us IS NULL AND source_clock_raw <> '')
    )
);
