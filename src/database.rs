use poise::serenity_prelude as serenity;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;

#[derive(sqlx::FromRow, Debug)]
pub struct SearchResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub guild_id: i64,
    pub _author_id: i64,
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
            _author_id: msg.author.id.get() as i64,
            author_name: msg.author.name.clone(),
            content: msg.content.clone(),
            created_at: msg.timestamp.timestamp(),
        }
    }
}

pub async fn init_db() -> Result<SqlitePool, sqlx::Error> {
    let database_url = "sqlite://discord_bot.db?mode=rwc"; // rwc: read, write, create

    let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

// Config Helpers

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

// Message Helpers

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

// like query
pub async fn search_messages_like(
    pool: &SqlitePool,
    guild_id: i64,
    channel_id: i64,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    let query_pattern = format!("%{}%", query);

    sqlx::query_as::<_, SearchResult>(
        r#"
        SELECT message_id, channel_id, guild_id, author_id, author_name, content, created_at
        FROM messages
        WHERE guild_id = ?
          AND channel_id = ?
          AND content LIKE ?
        ORDER BY created_at DESC
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
        ORDER BY m.created_at DESC
        "#,
    )
    .bind(guild_id)
    .bind(channel_id)
    .bind(min_id)
    .bind(max_id)
    .bind(query_pattern)
    .fetch_all(pool)
    .await
}

pub async fn get_earliest_message_id(
    pool: &SqlitePool,
    channel_id: i64,
) -> Result<Option<i64>, sqlx::Error> {
    let row = sqlx::query("SELECT MIN(message_id) as min_id FROM messages WHERE channel_id = ?")
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        let id: Option<i64> = row.try_get("min_id")?;
        Ok(id)
    } else {
        Ok(None)
    }
}
