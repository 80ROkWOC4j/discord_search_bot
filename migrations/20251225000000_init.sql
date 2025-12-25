-- Config Table
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Messages Table
CREATE TABLE IF NOT EXISTS messages (
    message_id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL,
    guild_id INTEGER NOT NULL,
    author_id INTEGER NOT NULL,
    author_name TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_messages_channel_created ON messages (channel_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_guild ON messages (guild_id);

-- FTS Table (unicode61)
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    tokenize='unicode61'
);

-- Triggers to sync messages -> messages_fts
CREATE TRIGGER IF NOT EXISTS ai_messages AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (new.message_id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS ad_messages AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.message_id;
END;

CREATE TRIGGER IF NOT EXISTS au_messages AFTER UPDATE ON messages BEGIN
    UPDATE messages_fts SET content = new.content WHERE rowid = old.message_id;
END;
