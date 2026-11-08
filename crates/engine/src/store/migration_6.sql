-- Plan 6a: when a download completed (UTC).
ALTER TABLE downloads ADD COLUMN finished_at TEXT;
