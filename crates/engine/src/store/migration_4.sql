-- Migration 4: post-processing (plan 4). Checksums on downloads, one run per package or lone
-- download, DPAPI-encrypted archive passwords and the user's sort rules.
ALTER TABLE downloads ADD COLUMN expected_hash TEXT;
ALTER TABLE downloads ADD COLUMN server_digest TEXT;
ALTER TABLE downloads ADD COLUMN verify_retries INTEGER NOT NULL DEFAULT 0;

CREATE TABLE postproc_runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id  INTEGER UNIQUE REFERENCES packages(id) ON DELETE CASCADE,
    download_id INTEGER UNIQUE REFERENCES downloads(id) ON DELETE CASCADE,
    state       TEXT NOT NULL,
    steps_json  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE passwords (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    position  INTEGER NOT NULL,
    blob      BLOB NOT NULL
);

CREATE TABLE rules (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    position     INTEGER NOT NULL,
    enabled      INTEGER NOT NULL,
    name         TEXT NOT NULL,
    match_json   TEXT NOT NULL,
    action_json  TEXT NOT NULL
);
