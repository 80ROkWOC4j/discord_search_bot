use poise::serenity_prelude as serenity;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const DB_ENCRYPTION_KEY_PATHS: &[&str] = &[
    "/run/secrets/db_key", // Docker secret mount
    "secrets/db_key",      // Native local run
];

#[derive(sqlx::FromRow, Debug)]
pub struct SearchResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub guild_id: i64,
    #[allow(unused)]
    pub author_id: i64,
    pub author_name: String,
    pub content: String,
    pub created_at: i64,
}

impl SearchResult {
    pub fn link(&self) -> String {
        // guild id 이상하게 넣어도 잘 작동하긴 함
        format!(
            "https://discord.com/channels/{}/{}/{}",
            self.guild_id, self.channel_id, self.message_id
        )
    }

    pub fn from_message(msg: &serenity::Message, guild_id: i64) -> Self {
        Self {
            message_id: msg.id.get() as i64,
            channel_id: msg.channel_id.get() as i64,
            guild_id,
            author_id: msg.author.id.get() as i64,
            author_name: msg.author.name.clone(),
            content: msg.content.clone(),
            created_at: msg.timestamp.timestamp(),
        }
    }
}

pub async fn init_db() -> Result<SqlitePool, sqlx::Error> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://discord_bot.db?mode=rwc".to_string());

    let mut options = SqliteConnectOptions::from_str(&database_url)?.create_if_missing(true);

    if let Some(key) = database_encryption_key()? {
        let encrypted_filename = encrypted_database_filename(options.get_filename());
        options = options.filename(encrypted_filename);
        options = options.pragma("key", sql_string_literal(&key));
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

fn database_encryption_key() -> Result<Option<String>, sqlx::Error> {
    for path in DB_ENCRYPTION_KEY_PATHS.iter().map(Path::new) {
        if path.exists() {
            let key = std::fs::read_to_string(path)?;
            return validate_encryption_key(key).map(Some);
        }
    }

    Ok(None)
}

fn encrypted_database_filename(filename: &Path) -> PathBuf {
    if filename == Path::new(":memory:") {
        return filename.to_path_buf();
    }

    let encrypted_name = match (
        filename.file_stem().and_then(|s| s.to_str()),
        filename.extension().and_then(|s| s.to_str()),
    ) {
        (Some(stem), Some(extension)) => format!("{stem}.sqlcipher.{extension}"),
        (Some(stem), None) => format!("{stem}.sqlcipher"),
        _ => "discord_bot.sqlcipher.db".to_string(),
    };

    filename.with_file_name(encrypted_name)
}

fn validate_encryption_key(key: String) -> Result<String, sqlx::Error> {
    let key = key.trim_end_matches(&['\r', '\n'][..]).to_owned();

    if key.is_empty() {
        return Err(config_error("database encryption key is empty"));
    }

    Ok(key)
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn config_error(message: &'static str) -> sqlx::Error {
    sqlx::Error::Configuration(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    )))
}

#[cfg(test)]
mod encryption_tests {
    use super::encrypted_database_filename;
    use std::path::Path;

    #[test]
    fn encrypted_database_filename_keeps_plaintext_db_separate() {
        assert_eq!(
            encrypted_database_filename(Path::new("/app/data/discord_bot.db")),
            Path::new("/app/data/discord_bot.sqlcipher.db")
        );
    }

    #[test]
    fn encrypted_database_filename_keeps_memory_databases_in_memory() {
        assert_eq!(
            encrypted_database_filename(Path::new(":memory:")),
            Path::new(":memory:")
        );
    }
}

use std::cmp::{max, min};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub start: i64,
    pub end: i64,
}

impl Range {
    pub fn new(start: i64, end: i64) -> Self {
        Self {
            start: min(start, end),
            end: max(start, end),
        }
    }

    pub fn touches(&self, other: &Range) -> bool {
        max(self.start, other.start) <= min(self.end, other.end) + 1
    }

    pub fn contains(&self, other: &Range) -> bool {
        self.start <= other.start && self.end >= other.end
    }

    // 두 range가 연속된 하나의 range로 합칠 수 있는지
    pub fn merge(&self, other: &Range) -> Option<Range> {
        if self.touches(other) {
            Some(Range::new(
                min(self.start, other.start),
                max(self.end, other.end),
            ))
        } else {
            None
        }
    }
}

pub fn merge_ranges(mut ranges: Vec<Range>, new_range: Range) -> Vec<Range> {
    ranges.push(new_range);
    ranges.sort_by_key(|r| r.start);

    let mut merged: Vec<Range> = Vec::new();

    for range in ranges {
        if let Some(last) = merged.last_mut()
            && let Some(new_merged) = last.merge(&range)
        {
            *last = new_merged;
            continue;
        }

        merged.push(range);
    }

    merged
}

pub async fn add_sync_range(
    pool: &SqlitePool,
    channel_id: i64,
    start: i64,
    end: i64,
) -> Result<Range, sqlx::Error> {
    // transaction
    let mut tx = pool.begin().await?;

    let rows = sqlx::query(
        "SELECT start_id, end_id FROM sync_ranges WHERE channel_id = ? ORDER BY start_id",
    )
    .bind(channel_id)
    .fetch_all(&mut *tx)
    .await?;

    let existing_ranges: Vec<Range> = rows
        .iter()
        .map(|r| Range::new(r.try_get("start_id").unwrap(), r.try_get("end_id").unwrap()))
        .collect();

    let new_range = Range::new(start, end);
    let merged_ranges = merge_ranges(existing_ranges, new_range);

    sqlx::query("DELETE FROM sync_ranges WHERE channel_id = ?")
        .bind(channel_id)
        .execute(&mut *tx)
        .await?;

    for r in &merged_ranges {
        sqlx::query("INSERT INTO sync_ranges (channel_id, start_id, end_id) VALUES (?, ?, ?)")
            .bind(channel_id)
            .bind(r.start)
            .bind(r.end)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // 방금 병합 했으니 겹치는거 하나는 있을 것임
    let result_range = merged_ranges
        .into_iter()
        .find(|r| r.contains(&new_range))
        .unwrap_or(new_range);

    Ok(result_range)
}

pub async fn set_channel_caching(
    pool: &SqlitePool,
    channel_id: serenity::ChannelId,
    enabled: bool,
) -> Result<(), sqlx::Error> {
    let key = format!("channel:{}:caching", channel_id);
    let value = if enabled { "true" } else { "false" };

    sqlx::query("INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn is_channel_caching_enabled(
    pool: &SqlitePool,
    channel_id: serenity::ChannelId,
) -> Result<bool, sqlx::Error> {
    let key = format!("channel:{}:caching", channel_id);

    let row = sqlx::query("SELECT value FROM config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        let value: String = row.try_get("value")?;
        Ok(value == "true")
    } else {
        Ok(false)
    }
}

pub async fn set_version_subscription(
    pool: &SqlitePool,
    user_id: u64,
    enabled: bool,
) -> Result<(), sqlx::Error> {
    let enabled_value = if enabled { 1 } else { 0 };
    sqlx::query(
        "INSERT INTO version_subscriptions (user_id, enabled) VALUES (?, ?)
         ON CONFLICT(user_id) DO UPDATE SET enabled = excluded.enabled",
    )
    .bind(user_id as i64)
    .bind(enabled_value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_version_subscribers(pool: &SqlitePool) -> Result<Vec<i64>, sqlx::Error> {
    let rows = sqlx::query("SELECT user_id FROM version_subscriptions WHERE enabled = 1")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(|row| row.try_get("user_id")).collect()
}

pub async fn mark_version_notified(
    pool: &SqlitePool,
    user_id: u64,
    latest_version: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE version_subscriptions SET last_notified_version = ? WHERE user_id = ?")
        .bind(latest_version)
        .bind(user_id as i64)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn should_notify_version(
    pool: &SqlitePool,
    user_id: u64,
    latest_version: &str,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT last_notified_version FROM version_subscriptions WHERE user_id = ? AND enabled = 1")
        .bind(user_id as i64)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => {
            let last: Option<String> = r.try_get("last_notified_version")?;
            Ok(last.as_deref() != Some(latest_version))
        }
        None => Ok(false),
    }
}

pub async fn insert_message(pool: &SqlitePool, msg: &serenity::Message) -> Result<(), sqlx::Error> {
    let guild_id = match msg.guild_id {
        Some(id) => id.get() as i64,
        None => return Ok(()), // Ignore DM messages for now
    };

    sqlx::query(
        r#"
        INSERT OR REPLACE INTO messages (message_id, channel_id, guild_id, author_id, author_name, content, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(msg.id.get() as i64)
    .bind(msg.channel_id.get() as i64)
    .bind(guild_id)
    .bind(msg.author.id.get() as i64)
    .bind(&msg.author.name)
    .bind(&msg.content)
    .bind(msg.timestamp.timestamp())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_messages(
    pool: &SqlitePool,
    msgs: &[serenity::Message],
    guild_id: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    for msg in msgs {
        // let guild_id = match msg.guild_id {
        //     Some(id) => id.get() as i64,
        //     None => continue,
        // };

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO messages (message_id, channel_id, guild_id, author_id, author_name, content, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(msg.id.get() as i64)
        .bind(msg.channel_id.get() as i64)
        .bind(guild_id)
        .bind(msg.author.id.get() as i64)
        .bind(&msg.author.name)
        .bind(&msg.content)
        .bind(msg.timestamp.timestamp())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn update_message(
    pool: &SqlitePool,
    event: &serenity::MessageUpdateEvent,
) -> Result<(), sqlx::Error> {
    if let Some(content) = &event.content {
        sqlx::query("UPDATE messages SET content = ? WHERE message_id = ?")
            .bind(content)
            .bind(event.id.get() as i64)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn delete_message(
    pool: &SqlitePool,
    message_id: serenity::MessageId,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM messages WHERE message_id = ?")
        .bind(message_id.get() as i64)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_messages(
    pool: &SqlitePool,
    message_ids: &[serenity::MessageId],
) -> Result<(), sqlx::Error> {
    for id in message_ids {
        delete_message(pool, *id).await?;
    }
    Ok(())
}

pub async fn delete_channel_messages(
    pool: &SqlitePool,
    channel_id: serenity::ChannelId,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM messages WHERE channel_id = ?")
        .bind(channel_id.get() as i64)
        .execute(pool)
        .await?;
    Ok(())
}

// fts5 search
#[allow(unused)]
pub async fn search_messages_fts(
    pool: &SqlitePool,
    guild_id: i64,
    channel_id: i64,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    // unicode61 supports simple prefix matching with *
    let query_pattern = format!("\"{}\"*", query);

    sqlx::query_as::<_, SearchResult>(
        r#"
        SELECT m.message_id, m.channel_id, m.guild_id, m.author_id, m.author_name, m.content, m.created_at
        FROM messages m
        JOIN messages_fts f ON m.message_id = f.rowid
        WHERE m.guild_id = ?
          AND m.channel_id = ?
          AND messages_fts MATCH ?
        ORDER BY m.created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(guild_id)
    .bind(channel_id)
    .bind(query_pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn search_messages_range(
    pool: &SqlitePool,
    guild_id: i64,
    channel_id: i64,
    query: &str,
    min_id: i64,
    max_id: i64,
    limit: u32,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    let query_pattern = format!("%{}%", query);

    sqlx::query_as::<_, SearchResult>(
        r#"
        SELECT m.message_id, m.channel_id, m.guild_id, m.author_id, m.author_name, m.content, m.created_at
        FROM messages m
        WHERE m.guild_id = ? 
          AND m.channel_id = ? 
          AND m.message_id >= ? 
          AND m.message_id <= ?
          AND content LIKE ?
        ORDER BY m.message_id DESC
        LIMIT ?
        "#,
    )
    .bind(guild_id)
    .bind(channel_id)
    .bind(min_id)
    .bind(max_id)
    .bind(query_pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
}
