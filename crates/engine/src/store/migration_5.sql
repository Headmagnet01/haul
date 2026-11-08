-- Plan 5: request headers move to a DPAPI blob; paired browser extensions.
ALTER TABLE downloads ADD COLUMN headers_blob BLOB;
UPDATE downloads SET headers_json = NULL;
CREATE TABLE paired_clients (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    browser     TEXT NOT NULL,
    origin      TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_seen   TEXT
);
