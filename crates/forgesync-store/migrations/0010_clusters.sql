CREATE TABLE cluster_runs (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL CHECK (length(trim(endpoint)) > 0),
    model TEXT NOT NULL CHECK (length(trim(model)) > 0),
    recipe TEXT NOT NULL CHECK (recipe IN ('original_body', 'discussion_enriched')),
    recipe_version INTEGER NOT NULL CHECK (recipe_version > 0),
    status TEXT NOT NULL CHECK (status IN ('complete', 'partial')),
    eligible_threads INTEGER NOT NULL CHECK (eligible_threads >= 0),
    vector_threads INTEGER NOT NULL CHECK (vector_threads >= 0),
    candidate_edges INTEGER NOT NULL CHECK (candidate_edges >= 0),
    cluster_count INTEGER NOT NULL CHECK (cluster_count >= 0),
    member_count INTEGER NOT NULL CHECK (member_count >= 0),
    started_at_us INTEGER NOT NULL,
    finished_at_us INTEGER NOT NULL
);

CREATE TABLE clusters (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    stable_key TEXT NOT NULL CHECK (length(trim(stable_key)) > 0),
    status TEXT NOT NULL CHECK (status IN ('active', 'retired')),
    representative_thread_id INTEGER REFERENCES threads(id) ON DELETE SET NULL,
    canonical_thread_id INTEGER REFERENCES threads(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    last_run_id INTEGER REFERENCES cluster_runs(id) ON DELETE SET NULL,
    retired_at_us INTEGER,
    dismissed_at_us INTEGER,
    dismissal_reason TEXT NOT NULL DEFAULT '',
    created_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    UNIQUE (repository_id, stable_key),
    CHECK ((status = 'retired' AND retired_at_us IS NOT NULL) OR
           (status = 'active' AND retired_at_us IS NULL))
);

CREATE TABLE cluster_memberships (
    cluster_id INTEGER NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('active', 'excluded', 'removed')),
    score_to_representative REAL CHECK (
        score_to_representative IS NULL OR
        (score_to_representative >= -1.0 AND score_to_representative <= 1.0)
    ),
    first_seen_run_id INTEGER NOT NULL REFERENCES cluster_runs(id),
    last_seen_run_id INTEGER NOT NULL REFERENCES cluster_runs(id),
    created_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    PRIMARY KEY (cluster_id, thread_id)
);

CREATE INDEX cluster_memberships_thread_state
    ON cluster_memberships (thread_id, state, cluster_id);

CREATE TABLE cluster_member_decisions (
    cluster_id INTEGER NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
    thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    excluded INTEGER NOT NULL CHECK (excluded IN (0, 1)),
    reason TEXT NOT NULL DEFAULT '',
    updated_at_us INTEGER NOT NULL,
    PRIMARY KEY (cluster_id, thread_id)
);

CREATE TABLE cluster_events (
    id INTEGER PRIMARY KEY,
    cluster_id INTEGER NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
    run_id INTEGER REFERENCES cluster_runs(id) ON DELETE SET NULL,
    event_type TEXT NOT NULL CHECK (
        event_type IN ('generated', 'dismissed', 'restored', 'member_excluded',
                       'member_included', 'canonical_set')
    ),
    thread_id INTEGER REFERENCES threads(id) ON DELETE SET NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at_us INTEGER NOT NULL
);

CREATE INDEX cluster_groups_repository_status
    ON clusters (repository_id, status, dismissed_at_us, id);
