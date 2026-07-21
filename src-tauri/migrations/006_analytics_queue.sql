CREATE TABLE analytics_queue (
    id              INTEGER PRIMARY KEY,
    distinct_id     TEXT NOT NULL,
    event           TEXT NOT NULL,
    properties_json TEXT NOT NULL DEFAULT '{}',
    created_at      INTEGER NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT
);

CREATE INDEX idx_analytics_queue_created
    ON analytics_queue (created_at ASC, id ASC);
