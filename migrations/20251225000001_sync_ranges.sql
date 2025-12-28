CREATE TABLE IF NOT EXISTS sync_ranges (
    channel_id INTEGER NOT NULL,
    start_id INTEGER NOT NULL,
    end_id INTEGER NOT NULL,
    PRIMARY KEY (channel_id, start_id)
);