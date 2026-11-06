-- Migration 3: archive sets group across intakes. The Queue package a set became, by its
-- group key ("rar:movie"), so a part copied after its siblings were started joins them.
CREATE TABLE collector_archives (
    group_key   TEXT PRIMARY KEY,
    package_id  INTEGER NOT NULL REFERENCES packages(id) ON DELETE CASCADE
);
