-- Plan 6b: a browser capture waiting in the Collector keeps its request headers, DPAPI-sealed.
ALTER TABLE collector_links ADD COLUMN headers_blob BLOB;
