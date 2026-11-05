-- Migration 1: the plan-1 subset of the spec's data model.
CREATE TABLE packages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    source      TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE downloads (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id       INTEGER REFERENCES packages(id) ON DELETE SET NULL,
    url              TEXT NOT NULL,
    headers_json     TEXT,
    state            TEXT NOT NULL DEFAULT 'queued',
    priority         INTEGER NOT NULL DEFAULT 1,
    position         INTEGER NOT NULL,
    connections      INTEGER NOT NULL DEFAULT 8,
    speed_limit      INTEGER,
    target_dir       TEXT NOT NULL,
    file_name        TEXT,
    target_path      TEXT,
    error            TEXT,
    -- probe results (final_url IS NULL means "not probed yet")
    final_url        TEXT,
    size             INTEGER,
    etag             TEXT,
    last_modified    TEXT,
    resumable        INTEGER,
    probe_file_name  TEXT,
    content_type     TEXT,
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX downloads_order ON downloads(priority, position, id);

CREATE TABLE segments (
    download_id  INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    start        INTEGER NOT NULL,
    pos          INTEGER NOT NULL,
    "end"        INTEGER NOT NULL,
    PRIMARY KEY (download_id, start)
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);
