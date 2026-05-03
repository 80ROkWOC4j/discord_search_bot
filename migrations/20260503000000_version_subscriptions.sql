CREATE TABLE IF NOT EXISTS version_subscriptions (
    user_id INTEGER PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_notified_version TEXT
);
