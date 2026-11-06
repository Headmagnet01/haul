-- Migration 2: the Collector (plan 3).
CREATE TABLE collector_batches (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source      TEXT NOT NULL,
    label       TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE collector_links (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id    INTEGER NOT NULL REFERENCES collector_batches(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'checking',
    probe_json  TEXT,
    message     TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX collector_links_batch ON collector_links(batch_id);

-- The Queue package a started collector package became, so starting it in two goes keeps one package.
CREATE TABLE collector_groups (
    batch_id    INTEGER NOT NULL REFERENCES collector_batches(id) ON DELETE CASCADE,
    group_key   TEXT NOT NULL,
    package_id  INTEGER NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
    PRIMARY KEY (batch_id, group_key)
);
